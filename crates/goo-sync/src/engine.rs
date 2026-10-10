//! One sync cycle of a linked folder (DESIGN §4.2).
//!
//! 1. Remote changes deferred last time (the note had unsaved edits in the app) are applied if the note is writable now.
//! 2. Local renames are recognised (so a moved note is not mistaken for a delete), then pages after the checkpoint are
//!    pulled and applied; the checkpoint is saved after each page, never taken from a push answer.
//! 3. The folder is scanned and the local changes are pushed in batches (deletes, then edits/renames, then creates), op
//!    ids persisted before sending; the answers are applied per change.
//! 4. Pull/push repeat (up to 4 rounds) while a rejection asks for another round and something moved.
//!
//! Text is never discarded: concurrent edits are merged, overlapping ones keep the local file and add the remote text as
//! a conflict copy, an edit beats a delete, and a file in the way of an incoming note is renamed, not overwritten. Every
//! remote path is checked before anything is written, and every write stays inside the folder (goo-core path guard).

use goo_core::{canonical_root, is_hidden, is_markdown, read_text, relative_parts, resolve_inside, write_atomic, CoreError, SKIPPED_DIRS};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use crate::api::*;
use crate::content_hash;
use crate::merge::{merge, Merge};
use crate::scan::{detect_changes, detect_renames, scan, FileInfo, LocalChange, Scan};
use crate::state::{file_stat, is_note_id, nfc, path_key, trusted_stat, NoteState, PendingCreate, PendingOp, Quarantine, State, StateStore};

/// Pull/push rounds per cycle at most.
pub const MAX_ROUNDS: usize = 4;

fn always(_: &Path) -> bool {
    true
}

pub struct SyncOptions<'a> {
    /// The goo cloud workspace linked to the folder.
    pub workspace_id: String,
    /// This installation; registered when no client id is known yet. `device_name` also names conflict copies.
    pub client: ClientInfo,
    /// The client id the app registered (kept in the state). None: use the stored one, or register.
    pub client_id: Option<String>,
    /// The caller is a viewer: pull only, report local changes as not uploaded.
    pub read_only: bool,
    /// "May sync write this file now?" The app answers no for notes with unsaved edits in its window; their remote
    /// changes wait for a later cycle.
    pub may_write: &'a dyn Fn(&Path) -> bool,
    /// Local time offset for the date in conflict copy names.
    pub utc_offset_minutes: i32,
}

impl SyncOptions<'static> {
    pub fn new(workspace_id: &str, client: ClientInfo) -> SyncOptions<'static> {
        SyncOptions { workspace_id: workspace_id.into(), client, client_id: None, read_only: false, may_write: &always, utc_offset_minutes: 0 }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Report {
    /// Remote changes applied (written, moved, deleted or merged).
    pub pulled: usize,
    /// Local changes the server accepted.
    pub pushed: usize,
    /// Files written with remote or merged text (conflict copies included).
    pub written: usize,
    /// Files deleted because they were deleted remotely.
    pub deleted: usize,
    /// Files moved: remote renames, and local files moved out of the way of a remote note.
    pub renamed: usize,
    pub conflicts: Vec<Conflict>,
    /// Every file currently held back (until it changes).
    pub quarantined: Vec<Quarantined>,
    /// Local paths whose remote changes wait because the note had unsaved edits.
    pub deferred: Vec<String>,
    /// Read-only workspaces: local changes that were not uploaded.
    pub not_uploaded: Vec<String>,
    /// Remote changes refused because their path or id was unsafe.
    pub refused: Vec<Refused>,
    /// Absolute paths written, moved or deleted on disk (the app reloads open notes among them).
    pub changed_files: Vec<PathBuf>,
    pub rounds: usize,
    /// Set when the state was missing or unusable and the cycle started as an initial sync.
    pub reset: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Conflict {
    /// The note, holding the local text (pushed as the next version).
    pub path: String,
    /// The copy holding the remote text (a new note).
    pub conflict_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Quarantined {
    pub path: String,
    pub reason: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Refused {
    pub note_id: String,
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub enum SyncError {
    Remote(RemoteError),
    Local(CoreError),
}

impl fmt::Display for SyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SyncError::Remote(e) => e.fmt(f),
            SyncError::Local(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for SyncError {}

impl From<RemoteError> for SyncError {
    fn from(e: RemoteError) -> Self {
        SyncError::Remote(e)
    }
}

impl From<CoreError> for SyncError {
    fn from(e: CoreError) -> Self {
        SyncError::Local(e)
    }
}

impl From<std::io::Error> for SyncError {
    fn from(e: std::io::Error) -> Self {
        SyncError::Local(e.into())
    }
}

/// A cycle that stopped early. What was applied before the error is saved and listed in `report` (so the app can
/// still reload changed notes); the next cycle continues from there.
#[derive(Debug, Clone)]
pub struct SyncFailure {
    pub error: SyncError,
    pub report: Report,
}

impl fmt::Display for SyncFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(f)
    }
}

impl std::error::Error for SyncFailure {}

type Result<T> = std::result::Result<T, SyncError>;

/// Syncs `folder` with the workspace in `opts`, keeping the engine's state in `state_dir` (outside the folder).
pub fn sync(folder: &Path, state_dir: &Path, remote: &dyn Remote, opts: &SyncOptions) -> std::result::Result<Report, SyncFailure> {
    let root = canonical_root(folder).map_err(|e| SyncFailure { error: e.into(), report: Report::default() })?;
    let store = StateStore::new(state_dir);
    let (state, reset) = store.load(&opts.workspace_id, &root);
    let mut engine = Engine { root, store, state, remote, opts, client_id: String::new(), report: Report { reset, ..Default::default() } };
    let result = engine.run();
    engine.finish();
    let saved = engine.save();
    match result.and(saved) {
        Ok(()) => Ok(engine.report),
        Err(error) => Err(SyncFailure { error, report: engine.report }),
    }
}

/// What the file of a note looks like right now.
enum Local {
    Missing,
    Unreadable,
    Hash(String),
}

/// One change ready to send, with what is needed to apply its answer.
struct Prepared {
    change: PushChange,
    kind: Kind,
    /// Local path of the file (for renames on `path_taken` and quarantine).
    path: String,
    /// Hash of the file when sent ("" for deletes).
    hash: String,
    json_len: usize,
}

enum Kind {
    Delete,
    Update,
    Create,
}

#[derive(Default)]
struct PushOutcome {
    /// A rejection asks for another round (pull and merge, or retry after a local rename).
    retry: bool,
    /// Something moved: changes accepted, the server reported a newer version, a file was renamed or an id forgotten.
    progress: bool,
}

struct Engine<'a> {
    root: PathBuf,
    store: StateStore,
    state: State,
    remote: &'a dyn Remote,
    opts: &'a SyncOptions<'a>,
    client_id: String,
    report: Report,
}

impl Engine<'_> {
    fn run(&mut self) -> Result<()> {
        self.client_id = match self.opts.client_id.clone().or_else(|| self.state.client_id.clone()) {
            Some(id) => id,
            None => self.remote.register_client(&self.opts.client)?,
        };
        self.state.client_id = Some(self.client_id.clone());
        self.save()?;

        self.apply_deferred()?;
        for round in 1..=MAX_ROUNDS {
            self.report.rounds = round;
            self.observe()?;
            let pulled = self.pull()?;
            let outcome = self.push()?;
            if !outcome.retry || !(outcome.progress || pulled > 0) {
                break;
            }
        }
        Ok(())
    }

    fn save(&self) -> Result<()> {
        Ok(self.store.save(&self.state)?)
    }

    /// Fills the report's lists from the state, and de-duplicates the changed files.
    fn finish(&mut self) {
        self.report.quarantined = self
            .state
            .quarantined
            .iter()
            .map(|(path, q)| Quarantined { path: path.clone(), reason: q.reason.clone(), message: q.message.clone() })
            .collect();
        self.report.deferred = self
            .state
            .deferred
            .iter()
            .map(|(id, c)| self.state.notes.get(id).map(|n| n.path.clone()).unwrap_or_else(|| c.path.clone()))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let mut seen = BTreeSet::new();
        self.report.changed_files.retain(|p| seen.insert(p.clone()));
    }

    // -----------------------------------------------------------------------
    // Paths and files
    // -----------------------------------------------------------------------

    fn abs(&self, rel: &str) -> PathBuf {
        rel.split('/').fold(self.root.clone(), |p, part| p.join(part))
    }

    fn may_write(&self, rel: &str) -> bool {
        (self.opts.may_write)(&self.abs(rel))
    }

    /// The absolute path for writing `rel`: missing folders are created, every folder (and an existing file, which may
    /// be a symlink) must resolve inside the root.
    fn target(&self, rel: &str) -> Result<PathBuf> {
        let parts = relative_parts(rel)?;
        let (name, dirs) = parts.split_last().expect("relative_parts is never empty");
        let mut dir = self.root.clone();
        for d in dirs {
            dir.push(d);
            if fs::symlink_metadata(&dir).is_err() {
                fs::create_dir(&dir)?;
            }
            dir = resolve_inside(&self.root, &dir)?;
        }
        let file = dir.join(name);
        if fs::symlink_metadata(&file).is_ok() {
            resolve_inside(&self.root, &file)?;
        }
        Ok(file)
    }

    fn write_file(&mut self, rel: &str, text: &str) -> Result<()> {
        let path = self.target(rel)?;
        write_atomic(&path, text)?;
        self.report.written += 1;
        self.report.changed_files.push(self.abs(rel));
        Ok(())
    }

    fn delete_file(&mut self, rel: &str) -> Result<()> {
        let abs = self.abs(rel);
        // the link itself is removed, never what it points to; it must still point inside the folder
        resolve_inside(&self.root, &abs)?;
        fs::remove_file(&abs)?;
        self.report.deleted += 1;
        self.report.changed_files.push(abs);
        self.prune_dirs(rel);
        Ok(())
    }

    /// Moves a file inside the folder; `to` must be free (see [`Self::free_variant`]).
    fn move_file(&mut self, from: &str, to: &str) -> Result<()> {
        let from_abs = self.abs(from);
        resolve_inside(&self.root, &from_abs)?;
        let to_abs = self.target(to)?;
        fs::rename(&from_abs, &to_abs)?;
        self.report.renamed += 1;
        self.report.changed_files.push(from_abs);
        self.report.changed_files.push(self.abs(to));
        self.prune_dirs(from);
        Ok(())
    }

    /// Removes the folders `rel` leaves empty (never the root). Folders are implicit in a workspace.
    fn prune_dirs(&self, rel: &str) {
        let mut parts: Vec<&str> = rel.split('/').collect();
        parts.pop();
        while !parts.is_empty() {
            if fs::remove_dir(self.abs(&parts.join("/"))).is_err() {
                break;
            }
            parts.pop();
        }
    }

    /// The entry of `dir` (absolute) whose name has the same key as `name`, the exact name first.
    fn entry_like(dir: &Path, name: &str) -> Option<String> {
        if fs::symlink_metadata(dir.join(name)).is_ok() && fs::read_dir(dir).ok()?.flatten().any(|e| e.file_name().to_str() == Some(name)) {
            return Some(name.to_string());
        }
        let key = path_key(name);
        fs::read_dir(dir).ok()?.flatten().filter_map(|e| e.file_name().to_str().map(String::from)).find(|n| path_key(n) == key)
    }

    /// `rel` with its folders spelled as they already are on disk when a folder of the same key exists (a remote
    /// `Café/x.md` goes into a local NFD `Café` folder, not a second one). The file name is kept.
    fn localize(&self, rel: &str) -> String {
        let mut parts: Vec<String> = rel.split('/').map(String::from).collect();
        let name = parts.pop().unwrap_or_default();
        let mut dir = self.root.clone();
        for part in parts.iter_mut() {
            if let Some(existing) = Self::entry_like(&dir, part) {
                *part = existing;
            }
            dir.push(&*part);
        }
        parts.push(name);
        parts.join("/")
    }

    /// The file on disk whose path has the same key as `rel` (the server's pathKey), as its local path.
    fn on_disk(&self, rel: &str) -> Option<String> {
        let rel = self.localize(rel);
        let (dir, name) = rel.rsplit_once('/').map_or((None, rel.as_str()), |(d, n)| (Some(d), n));
        let found = Self::entry_like(&dir.map_or(self.root.clone(), |d| self.abs(d)), name)?;
        Some(dir.map_or(found.clone(), |d| format!("{d}/{found}")))
    }

    /// Whether `rel` is taken (compared by the server's key) by a file on disk or by another tracked note. The file of
    /// `except_note` itself does not count (a case-only rename moves a note onto its own key).
    fn occupied(&self, rel: &str, except_note: Option<&str>) -> bool {
        let own = except_note.and_then(|id| self.state.notes.get(id)).map(|n| n.path.as_str());
        let key = path_key(rel);
        self.on_disk(rel).is_some_and(|found| Some(found.as_str()) != own)
            || self.state.notes.iter().any(|(id, n)| Some(id.as_str()) != except_note && path_key(&n.path) == key)
    }

    /// A free sibling of `rel`: `name (label).md`, `name (label 2).md`, ... or, without a label, `name (2).md`, ...
    fn free_variant(&self, rel: &str, label: Option<&str>) -> String {
        let (dir, name) = match rel.rsplit_once('/') {
            Some((d, n)) => (format!("{d}/"), n),
            None => (String::new(), rel),
        };
        let (stem, ext) = match name.rsplit_once('.') {
            Some((s, e)) if !s.is_empty() => (s, format!(".{e}")),
            _ => (name, String::new()),
        };
        for n in 1.. {
            let suffix = match label {
                Some(l) if n == 1 => format!(" ({l})"),
                Some(l) => format!(" ({l} {n})"),
                None => format!(" ({})", n + 1),
            };
            // keep the name within goo-core's 200 characters
            let room = 200usize.saturating_sub(suffix.chars().count() + ext.chars().count());
            let stem: String = stem.chars().take(room).collect();
            let candidate = format!("{dir}{stem}{suffix}{ext}");
            if !self.occupied(&candidate, None) {
                return candidate;
            }
        }
        unreachable!()
    }

    fn local(&self, rel: &str, note: Option<&NoteState>) -> Local {
        let abs = self.abs(rel);
        match fs::symlink_metadata(&abs) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Local::Missing,
            Err(_) => return Local::Unreadable,
            Ok(_) => {}
        }
        if let (Some(n), Some((size, mtime))) = (note, file_stat(&abs)) {
            if n.size == Some(size) && n.mtime == Some(mtime) {
                return Local::Hash(n.base_hash.clone());
            }
        }
        match fs::read(&abs) {
            Ok(bytes) => Local::Hash(content_hash(&bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Local::Missing,
            Err(_) => Local::Unreadable,
        }
    }

    /// Records `text` as the agreed text of a note (base file, hash, version, paths). `file_holds_base`: the file at
    /// `path` holds exactly this text, so its size/mtime can vouch for it next time.
    fn agree(&mut self, id: &str, path: &str, remote_path: &str, version: u64, text: &str, file_holds_base: bool) -> Result<()> {
        self.store.write_base(id, text)?;
        let stat = if file_holds_base { trusted_stat(&self.abs(path)) } else { None };
        let note = NoteState {
            path: path.into(),
            remote_path: remote_path.into(),
            version,
            base_hash: content_hash(text.as_bytes()),
            size: stat.map(|s| s.0),
            mtime: stat.map(|s| s.1),
            pending: None,
        };
        self.state.notes.insert(id.into(), note);
        Ok(())
    }

    fn forget(&mut self, id: &str) {
        self.state.notes.remove(id);
        self.store.remove_base(id);
    }

    fn conflict_label(&self) -> String {
        let device: String = self
            .opts
            .client
            .device_name
            .chars()
            .map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '(' | ')') || c.is_control() { '-' } else { c })
            .take(40)
            .collect();
        let device = device.trim();
        let device = if device.is_empty() { "device" } else { device };
        format!("conflict {device} {}", local_stamp(crate::now_secs() as i64 + self.opts.utc_offset_minutes as i64 * 60))
    }

    // -----------------------------------------------------------------------
    // Local renames (before pulling, so a moved note is not taken for a deleted one)
    // -----------------------------------------------------------------------

    /// Scans the folder, applies detected renames to the state and remembers size/mtime of files that hold their base.
    fn observe(&mut self) -> Result<Scan> {
        let scan = scan(&self.root, &self.state)?;
        for (id, path) in detect_renames(&self.state, &scan) {
            self.state.pending_creates.remove(&path);
            if let Some(n) = self.state.notes.get_mut(&id) {
                n.path = path;
                n.size = None;
                n.mtime = None;
            }
        }
        for n in self.state.notes.values_mut() {
            if let Some(f) = scan.files.get(&n.path) {
                if f.hash == n.base_hash && (n.size, n.mtime) != (Some(f.size), Some(f.mtime)) {
                    let stat = trusted_stat(&f.abs);
                    n.size = stat.map(|s| s.0);
                    n.mtime = stat.map(|s| s.1);
                }
            }
        }
        self.save()?;
        Ok(scan)
    }

    // -----------------------------------------------------------------------
    // Pull
    // -----------------------------------------------------------------------

    fn apply_deferred(&mut self) -> Result<()> {
        let mut waiting: Vec<PullChange> = self.state.deferred.values().cloned().collect();
        waiting.sort_by_key(|c| c.seq);
        for c in waiting {
            if self.apply_remote(c)? {
                self.report.pulled += 1;
            }
        }
        self.save()
    }

    fn pull(&mut self) -> Result<usize> {
        let mut applied = 0;
        let initial = self.state.checkpoint == 0;
        loop {
            let page = self.remote.pull(&self.opts.workspace_id, &self.client_id, self.state.checkpoint, PULL_LIMIT)?;
            let empty = page.changes.is_empty();
            for c in page.changes {
                if self.apply_remote(c)? {
                    applied += 1;
                }
            }
            self.state.checkpoint = page.checkpoint;
            self.save()?;
            if !page.has_more || empty {
                break;
            }
        }
        if initial {
            // The initial download leaves tombstones out. A new note still pending after it (its push answer was lost)
            // is not live on the server: either it never arrived, or it was deleted since. A replay of its op id would
            // claim it exists, so it is sent again under a new id.
            let pending: Vec<String> = self.state.pending_creates.keys().cloned().collect();
            for path in pending {
                self.drop_pending_create(&path);
            }
            self.save()?;
        }
        self.report.pulled += applied;
        Ok(applied)
    }

    fn refuse(&mut self, c: &PullChange, message: impl Into<String>) {
        self.report.refused.push(Refused { note_id: c.note_id.clone(), path: c.path.clone(), message: message.into() });
    }

    fn defer(&mut self, c: PullChange) {
        self.state.deferred.insert(c.note_id.clone(), c);
    }

    /// Applies one remote change; false when it was skipped (already known, unknown tombstone, refused or deferred).
    fn apply_remote(&mut self, c: PullChange) -> Result<bool> {
        // a newer change replaces a deferred one; it is deferred again below if the note is still not writable
        self.state.deferred.remove(&c.note_id);
        if !is_note_id(&c.note_id) {
            self.refuse(&c, "the note id is not a UUID");
            return Ok(false);
        }
        // our own new note whose push answer was lost: it is the file we sent, at version 1
        if !self.state.notes.contains_key(&c.note_id) {
            let own = self.state.pending_creates.iter().find(|(_, p)| p.note_id == c.note_id).map(|(path, p)| (path.clone(), p.clone()));
            if let Some((path, p)) = own {
                self.state.pending_creates.remove(&path);
                let note = NoteState { path: path.clone(), remote_path: path, version: 1, base_hash: p.hash, size: None, mtime: None, pending: None };
                self.state.notes.insert(c.note_id.clone(), note);
            }
        }
        if c.deleted {
            return self.apply_delete(c);
        }
        if let Err(message) = check_remote_path(&c.path) {
            self.refuse(&c, message);
            return Ok(false);
        }
        let Some(content) = c.content.clone() else {
            self.refuse(&c, "the change has no content");
            return Ok(false);
        };
        match self.state.notes.get(&c.note_id).cloned() {
            None => self.apply_new(c, content),
            Some(note) => self.apply_update(c, note, content),
        }
    }

    fn apply_delete(&mut self, c: PullChange) -> Result<bool> {
        let Some(note) = self.state.notes.get(&c.note_id).cloned() else { return Ok(false) };
        if c.version <= note.version {
            return Ok(false);
        }
        if !self.may_write(&note.path) {
            self.defer(c);
            return Ok(false);
        }
        match self.local(&note.path, Some(&note)) {
            // untouched: the delete applies
            Local::Hash(h) if h == note.base_hash => self.delete_file(&note.path)?,
            // edited (or unreadable): kept, and pushed as a new note once the id is forgotten
            Local::Hash(_) | Local::Unreadable => {}
            Local::Missing => {}
        }
        self.forget(&c.note_id);
        Ok(true)
    }

    fn apply_new(&mut self, c: PullChange, content: String) -> Result<bool> {
        let mut path = self.localize(&c.path);
        // an untracked file with the same key (another case or Unicode normalisation) is "the file at that path"
        let existing = self.on_disk(&path);
        if !self.may_write(&path) || existing.as_ref().is_some_and(|e| !self.may_write(e)) {
            self.defer(c);
            return Ok(false);
        }
        let hash = content_hash(content.as_bytes());
        if self.state.note_at(&c.path).is_some() {
            // another tracked note holds the path here (its own move comes later): keep this one next to it
            path = self.free_variant(&path, None);
        } else if let Some(existing) = existing {
            match self.local(&existing, None) {
                Local::Missing => {}
                Local::Hash(h) if h == hash => {
                    // the same text is already there (e.g. a re-link after lost state): adopt it under its local name
                    self.state.pending_creates.remove(&existing);
                    self.agree(&c.note_id, &existing, &c.path, c.version, &content, true)?;
                    return Ok(true);
                }
                Local::Hash(_) | Local::Unreadable => {
                    // a different local file is in the way: move it aside, it is pushed as a new note
                    let aside = self.free_variant(&existing, Some("local"));
                    self.state.pending_creates.remove(&existing);
                    self.move_file(&existing, &aside)?;
                }
            }
        }
        self.write_file(&path, &content)?;
        self.agree(&c.note_id, &path, &c.path, c.version, &content, true)?;
        Ok(true)
    }

    fn apply_update(&mut self, c: PullChange, note: NoteState, content: String) -> Result<bool> {
        if c.version <= note.version {
            return Ok(false);
        }
        let id = c.note_id.clone();
        let locally_moved = nfc(&note.path) != nfc(&note.remote_path);
        // a local rename not pushed yet wins over a remote one (it is pushed against the new version)
        let wanted = if locally_moved { note.path.clone() } else { self.localize(&c.path) };
        if !self.may_write(&note.path) || !self.may_write(&wanted) {
            self.defer(c);
            return Ok(false);
        }
        let remote_hash = content_hash(content.as_bytes());
        match self.local(&note.path, Some(&note)) {
            Local::Unreadable => {
                self.defer(c);
                return Ok(false);
            }
            Local::Missing if remote_hash == note.base_hash => {
                // deleted here, only renamed (or re-saved) remotely: the delete stands and is pushed against this version
                let path = wanted.clone();
                let n = self.state.notes.get_mut(&id).expect("tracked");
                n.path = path;
                n.remote_path = c.path.clone();
                n.version = c.version;
                n.pending = None;
            }
            Local::Missing => {
                // deleted here, edited remotely: the edit wins, the note comes back
                let path = if self.occupied(&wanted, Some(&id)) { self.free_variant(&wanted, None) } else { wanted.clone() };
                self.write_file(&path, &content)?;
                self.agree(&id, &path, &c.path, c.version, &content, true)?;
            }
            Local::Hash(h) if h == note.base_hash => {
                let path = self.move_note(&id, &note.path, &wanted)?;
                if remote_hash != h {
                    self.write_file(&path, &content)?;
                }
                self.agree(&id, &path, &c.path, c.version, &content, true)?;
            }
            Local::Hash(_) => {
                let local_text = match read_text(&self.abs(&note.path)) {
                    Ok(t) => t,
                    Err(_) => {
                        self.defer(c);
                        return Ok(false);
                    }
                };
                let merged = match self.store.read_base(&id, &note.base_hash) {
                    Some(base) => merge(&base, &local_text, &content),
                    None if local_text == content => Merge::Clean(local_text.clone()),
                    None => Merge::Conflict,
                };
                let path = self.move_note(&id, &note.path, &wanted)?;
                match merged {
                    Merge::Clean(text) => {
                        if text != local_text {
                            self.write_file(&path, &text)?;
                        }
                        // the base is the remote text: the merge (if it differs) is pushed as the next version
                        self.agree(&id, &path, &c.path, c.version, &content, text == content)?;
                    }
                    Merge::Conflict => {
                        let copy = self.free_variant(&path, Some(&self.conflict_label()));
                        self.write_file(&copy, &content)?;
                        self.agree(&id, &path, &c.path, c.version, &content, false)?;
                        self.report.conflicts.push(Conflict { path, conflict_path: copy });
                    }
                }
            }
        }
        Ok(true)
    }

    /// Moves a tracked note's file to `to` (or a free variant when another file holds it); returns where it ended up.
    fn move_note(&mut self, id: &str, from: &str, to: &str) -> Result<String> {
        // the same name in another Unicode normalisation is not a move: the local name stays
        if nfc(from) == nfc(to) {
            return Ok(from.to_string());
        }
        // a case-only rename lands on the note's own key, which `occupied` does not count
        let to = if self.occupied(to, Some(id)) { self.free_variant(to, None) } else { to.to_string() };
        self.move_file(from, &to)?;
        if let Some(n) = self.state.notes.get_mut(id) {
            n.path = to.clone();
        }
        Ok(to)
    }

    // -----------------------------------------------------------------------
    // Push
    // -----------------------------------------------------------------------

    fn push(&mut self) -> Result<PushOutcome> {
        let mut scan = self.observe()?;
        let mut changes = detect_changes(&self.state, &scan);
        self.avoid_path_clashes(&mut changes, &mut scan)?;
        // pending creates whose file is gone or became tracked are stale
        let creates: BTreeSet<&str> =
            changes.iter().filter_map(|c| if let LocalChange::Create { path } = c { Some(path.as_str()) } else { None }).collect();
        let stale: Vec<(String, String)> = self
            .state
            .pending_creates
            .iter()
            .filter(|(p, _)| !creates.contains(p.as_str()))
            .map(|(p, c)| (p.clone(), c.note_id.clone()))
            .collect();
        for (path, id) in stale {
            self.state.pending_creates.remove(&path);
            if !self.state.notes.contains_key(&id) {
                self.store.remove_base(&id);
            }
        }
        // quarantined files that changed or are gone are re-armed
        let rearm: Vec<String> = self
            .state
            .quarantined
            .iter()
            .filter(|(p, q)| scan.files.get(p.as_str()).map(|f| f.hash.as_str()).unwrap_or("") != q.file_hash)
            .map(|(p, _)| p.clone())
            .collect();
        for p in rearm {
            self.state.quarantined.remove(&p);
        }

        if self.opts.read_only {
            self.report.not_uploaded = changes.iter().map(|c| self.change_path(c)).collect();
            return Ok(PushOutcome::default());
        }

        let mut outcome = PushOutcome::default();
        let mut batch: Vec<Prepared> = Vec::new();
        let mut content_bytes = 0usize;
        let mut json_bytes = 0usize;
        for change in &changes {
            let Some(p) = self.prepare(change, &scan)? else { continue };
            let len = p.change.content.as_ref().map_or(0, |c| c.len());
            let full = batch.len() >= MAX_PUSH_CHANGES
                || content_bytes + len > PUSH_BATCH_BYTES
                || json_bytes + p.json_len > PUSH_BATCH_JSON_BYTES;
            if !batch.is_empty() && full {
                self.send(std::mem::take(&mut batch), &mut outcome)?;
                content_bytes = 0;
                json_bytes = 0;
            }
            content_bytes += len;
            json_bytes += p.json_len;
            batch.push(p);
        }
        if !batch.is_empty() {
            self.send(batch, &mut outcome)?;
        }
        Ok(outcome)
    }

    /// New notes whose path has the key of a tracked note or of another new note (they differ only in case or Unicode
    /// normalisation) would be refused `path_taken`: they are moved aside before sending. Paths of notes deleted in
    /// this push are free (deletes go first).
    fn avoid_path_clashes(&mut self, changes: &mut [LocalChange], scan: &mut Scan) -> Result<()> {
        let deleted: BTreeSet<&str> =
            changes.iter().filter_map(|c| if let LocalChange::Delete { note_id } = c { Some(note_id.as_str()) } else { None }).collect();
        let mut claimed: BTreeSet<String> =
            self.state.notes.iter().filter(|(id, _)| !deleted.contains(id.as_str())).map(|(_, n)| path_key(&n.path)).collect();
        for change in changes.iter_mut() {
            let LocalChange::Create { path } = change else { continue };
            if !claimed.contains(&path_key(path)) || !self.may_write(path) {
                claimed.insert(path_key(path));
                continue;
            }
            let aside = self.free_variant(path, Some("local"));
            self.move_file(path, &aside)?;
            self.drop_pending_create(path);
            if let Some(f) = scan.files.remove(path.as_str()) {
                scan.files.insert(aside.clone(), FileInfo { abs: self.abs(&aside), ..f });
            }
            claimed.insert(path_key(&aside));
            *path = aside;
        }
        Ok(())
    }

    fn change_path(&self, change: &LocalChange) -> String {
        match change {
            LocalChange::Create { path } => path.clone(),
            LocalChange::Update { note_id, .. } | LocalChange::Delete { note_id } => {
                self.state.notes.get(note_id).map(|n| n.path.clone()).unwrap_or_default()
            }
        }
    }

    fn quarantine(&mut self, path: &str, reason: &str, message: &str, file_hash: &str) {
        let q = Quarantine { reason: reason.into(), message: message.into(), file_hash: file_hash.into() };
        self.state.quarantined.insert(path.into(), q);
    }

    /// Reads the text of a note to push; None (quarantined or skipped) when it cannot be sent.
    fn text_to_send(&mut self, path: &str, size: u64, scanned_hash: &str) -> Option<String> {
        if size > MAX_NOTE_BYTES {
            let message = format!("{path} is larger than 4 MB and is not synced");
            self.quarantine(path, "too_large", &message, scanned_hash);
            return None;
        }
        match read_text(&self.abs(path)) {
            Ok(text) if text.len() as u64 > MAX_NOTE_BYTES => {
                let message = format!("{path} is larger than 4 MB and is not synced");
                self.quarantine(path, "too_large", &message, &content_hash(text.as_bytes()));
                None
            }
            Ok(text) => Some(text),
            Err(e) if e.code == goo_core::ErrorCode::NotText => {
                self.quarantine(path, "invalid", &e.message, scanned_hash);
                None
            }
            // vanished or unreadable since the scan: next cycle
            Err(_) => None,
        }
    }

    fn prepare(&mut self, change: &LocalChange, scan: &Scan) -> Result<Option<Prepared>> {
        let held = |s: &Self, path: &str, hash: &str| s.state.quarantined.get(path).is_some_and(|q| q.file_hash == hash);
        let prepared = match change {
            LocalChange::Delete { note_id } => {
                let note = &self.state.notes[note_id];
                if self.state.deferred.contains_key(note_id) || held(self, &note.path, "") {
                    return Ok(None);
                }
                let fingerprint = format!("delete|{}", note.version);
                let change = PushChange {
                    op_id: reuse_op(note.pending.as_ref(), &fingerprint),
                    note_id: note_id.clone(),
                    op: Op::Delete,
                    base_version: note.version,
                    path: None,
                    content: None,
                };
                let path = note.path.clone();
                self.state.notes.get_mut(note_id).expect("tracked").pending = Some(PendingOp { op_id: change.op_id.clone(), fingerprint });
                Prepared { change, kind: Kind::Delete, path, hash: String::new(), json_len: 0 }
            }
            LocalChange::Update { note_id, content, rename } => {
                let note = self.state.notes[note_id].clone();
                let Some(file) = scan.files.get(&note.path) else { return Ok(None) };
                if self.state.deferred.contains_key(note_id) || held(self, &note.path, &file.hash) {
                    return Ok(None);
                }
                let mut text = None;
                if *content {
                    let Some(t) = self.text_to_send(&note.path, file.size, &file.hash) else { return Ok(None) };
                    // changed back to the base since the scan: nothing to send
                    if content_hash(t.as_bytes()) != note.base_hash {
                        text = Some(t);
                    }
                }
                let path = rename.then(|| note.path.clone());
                if text.is_none() && path.is_none() {
                    return Ok(None);
                }
                let hash = text.as_ref().map(|t| content_hash(t.as_bytes())).unwrap_or_else(|| note.base_hash.clone());
                let fingerprint = format!("upsert|{}|{:?}|{:?}", note.version, path, text.as_ref().map(|_| &hash));
                let change = PushChange {
                    op_id: reuse_op(note.pending.as_ref(), &fingerprint),
                    note_id: note_id.clone(),
                    op: Op::Upsert,
                    base_version: note.version,
                    path,
                    content: text,
                };
                self.state.notes.get_mut(note_id).expect("tracked").pending = Some(PendingOp { op_id: change.op_id.clone(), fingerprint });
                Prepared { change, kind: Kind::Update, path: note.path, hash, json_len: 0 }
            }
            LocalChange::Create { path } => {
                let Some(file) = scan.files.get(path) else { return Ok(None) };
                let deferred_here = self.state.deferred.values().any(|c| !c.deleted && path_key(&c.path) == path_key(path));
                if deferred_here || held(self, path, &file.hash) {
                    return Ok(None);
                }
                let Some(text) = self.text_to_send(path, file.size, &file.hash) else { return Ok(None) };
                let hash = content_hash(text.as_bytes());
                let pending = self.state.pending_creates.get(path).cloned();
                let note_id = pending.as_ref().map(|p| p.note_id.clone()).unwrap_or_else(new_id);
                let op_id = match &pending {
                    Some(p) if p.hash == hash => p.op_id.clone(),
                    _ => new_id(),
                };
                // the text sent is the base if the answer is lost and the note comes back in a pull
                self.store.write_base(&note_id, &text)?;
                self.state.pending_creates.insert(path.clone(), PendingCreate { note_id: note_id.clone(), op_id: op_id.clone(), hash: hash.clone() });
                let change =
                    PushChange { op_id, note_id, op: Op::Upsert, base_version: 0, path: Some(path.clone()), content: Some(text) };
                Prepared { change, kind: Kind::Create, path: path.clone(), hash, json_len: 0 }
            }
        };
        let json_len = serde_json::to_string(&prepared.change).map(|s| s.len()).unwrap_or(0);
        Ok(Some(Prepared { json_len, ..prepared }))
    }

    /// Sends one batch. The op ids are already in the state; it is saved before sending, so a lost answer is replayed
    /// with the same ids.
    fn send(&mut self, batch: Vec<Prepared>, outcome: &mut PushOutcome) -> Result<()> {
        self.save()?;
        let changes: Vec<PushChange> = batch.iter().map(|p| p.change.clone()).collect();
        let answer = self.remote.push(&self.opts.workspace_id, &self.client_id, &changes)?;
        let mut by_op: BTreeMap<String, Prepared> = batch.into_iter().map(|p| (p.change.op_id.clone(), p)).collect();
        for a in answer.accepted {
            if let Some(p) = by_op.remove(&a.op_id) {
                self.accepted(p, &a)?;
                outcome.progress = true;
            }
        }
        for r in answer.rejected {
            if let Some(p) = by_op.remove(&r.op_id) {
                self.rejected(p, &r, outcome)?;
            }
        }
        // changes without an answer keep their op ids for the next attempt
        self.save()
    }

    fn accepted(&mut self, p: Prepared, a: &Accepted) -> Result<()> {
        let id = p.change.note_id.clone();
        match p.kind {
            Kind::Delete => self.forget(&id),
            Kind::Update => {
                if let Some(text) = &p.change.content {
                    self.store.write_base(&id, text)?;
                }
                if let Some(n) = self.state.notes.get_mut(&id) {
                    n.version = a.version;
                    n.pending = None;
                    if let Some(path) = &p.change.path {
                        n.remote_path = path.clone();
                    }
                    if p.change.content.is_some() {
                        n.base_hash = p.hash.clone();
                        n.size = None;
                        n.mtime = None;
                    }
                }
            }
            Kind::Create => {
                self.state.pending_creates.remove(&p.path);
                let note =
                    NoteState { path: p.path.clone(), remote_path: p.path, version: a.version, base_hash: p.hash, size: None, mtime: None, pending: None };
                self.state.notes.insert(id, note);
            }
        }
        self.report.pushed += 1;
        Ok(())
    }

    fn rejected(&mut self, p: Prepared, r: &Rejected, outcome: &mut PushOutcome) -> Result<()> {
        let id = p.change.note_id.clone();
        if let Some(n) = self.state.notes.get_mut(&id) {
            n.pending = None;
        }
        match r.reason {
            // our own new note exists already (an earlier answer was lost and its pull went by): take the server's
            // state as the base, so the next round pushes only what differs
            Reason::VersionMismatch if matches!(p.kind, Kind::Create) => {
                self.state.pending_creates.remove(&p.path);
                match &r.current {
                    Some(Current { deleted: false, content: Some(text), version, path }) => {
                        let holds = content_hash(text.as_bytes()) == p.hash;
                        self.agree(&id, &p.path, path, *version, text, holds)?;
                    }
                    // a tombstone or no text: the file is pushed under a new id
                    _ => self.store.remove_base(&id),
                }
                outcome.retry = true;
                outcome.progress = true;
            }
            // the server has a newer version: the next round pulls and merges it
            Reason::VersionMismatch => {
                outcome.retry = true;
                outcome.progress = true;
            }
            Reason::PathTaken => {
                if self.may_write(&p.path) {
                    let aside = self.free_variant(&p.path, Some("local"));
                    self.move_file(&p.path, &aside)?;
                    match p.kind {
                        Kind::Create => self.drop_pending_create(&p.path),
                        _ => {
                            if let Some(n) = self.state.notes.get_mut(&id) {
                                n.path = aside;
                            }
                        }
                    }
                    outcome.retry = true;
                    outcome.progress = true;
                }
            }
            Reason::NotFound => {
                // gone on the server (or never there): the file is pushed as a new note
                self.forget(&id);
                outcome.retry = true;
                outcome.progress = true;
            }
            Reason::TooLarge | Reason::Invalid | Reason::Forbidden | Reason::Unknown => {
                if let Kind::Create = p.kind {
                    self.drop_pending_create(&p.path);
                }
                let message = if r.message.is_empty() { r.reason.as_str().to_string() } else { r.message.clone() };
                self.quarantine(&p.path, r.reason.as_str(), &message, &p.hash);
            }
        }
        Ok(())
    }

    fn drop_pending_create(&mut self, path: &str) {
        if let Some(pc) = self.state.pending_creates.remove(path) {
            if !self.state.notes.contains_key(&pc.note_id) {
                self.store.remove_base(&pc.note_id);
            }
        }
    }
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// The pending op id when the same change is sent again, else a new one.
fn reuse_op(pending: Option<&PendingOp>, fingerprint: &str) -> String {
    match pending {
        Some(p) if p.fingerprint == fingerprint => p.op_id.clone(),
        _ => new_id(),
    }
}

/// Why a remote path must not be written, or Ok: relative, `/`-separated, no `..`, valid names (goo-core rules), a
/// Markdown file, and visible in the app's tree (no hidden or build folders), else the next scan would miss it and
/// push a delete.
pub fn check_remote_path(path: &str) -> std::result::Result<(), String> {
    let parts = relative_parts(path).map_err(|e| e.message)?;
    if parts.join("/") != path {
        return Err(format!("\"{path}\" is not a plain relative path"));
    }
    if !is_markdown(Path::new(path)) {
        return Err(format!("\"{path}\" is not a Markdown file"));
    }
    let (_, dirs) = parts.split_last().expect("relative_parts is never empty");
    if parts.iter().any(|p| is_hidden(p)) || dirs.iter().any(|d| SKIPPED_DIRS.contains(d)) {
        return Err(format!("\"{path}\" is in a hidden or skipped folder"));
    }
    Ok(())
}

/// `YYYY-MM-DD HHMM` for Unix seconds (already shifted to local time).
pub fn local_stamp(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // civil-from-days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}{:02}", rem / 3600, rem % 3600 / 60)
}
