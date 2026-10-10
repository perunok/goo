//! What the engine remembers about one linked workspace, kept outside the notes folder in a directory the app picks
//! (`<config>/sync/<workspace-id>/`):
//!
//! - `state.json`: the client id, the pull checkpoint and, per note, its id, local and remote path, version and the hash
//!   of the last agreed text (plus size/mtime so unchanged files are not re-hashed), pending op ids, quarantined files
//!   and remote changes waiting for a note with unsaved edits.
//! - `base/<note_id>`: the last agreed text of each note, the base of the 3-way merge.
//!
//! Every write is atomic. A missing, unreadable or foreign `state.json` is not an error: the engine starts a fresh
//! initial sync, which never deletes a user's file (nothing is tracked, so nothing can look deleted).

use goo_core::{write_atomic, CoreError, ErrorCode};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::api::PullChange;
use crate::content_hash;

pub const STATE_FILE: &str = "state.json";
pub const BASE_DIR: &str = "base";
/// Bumped when the layout changes incompatibly; an unknown format starts a fresh sync.
pub const STATE_FORMAT: u32 = 1;
/// A file modified this recently is not trusted by size/mtime alone (an edit within the same timestamp tick would be
/// missed on file systems with coarse timestamps), so it is hashed again on the next scan.
const RACY_WINDOW_NS: u128 = 2_000_000_000;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub format: u32,
    pub workspace_id: String,
    /// The canonical folder this state belongs to; another folder starts fresh.
    pub root: String,
    #[serde(default)]
    pub client_id: Option<String>,
    /// The pull checkpoint: every change up to it has been applied (or deferred).
    #[serde(default)]
    pub checkpoint: u64,
    /// Tracked notes by note id.
    #[serde(default)]
    pub notes: BTreeMap<String, NoteState>,
    /// New notes being pushed, by local path: the id and op id are kept until the server answers, so a lost answer is
    /// replayed with the same ids (and the note is recognised when it comes back in a pull).
    #[serde(default)]
    pub pending_creates: BTreeMap<String, PendingCreate>,
    /// Files the server refused for a permanent reason, by local path. Not retried until the file changes.
    #[serde(default)]
    pub quarantined: BTreeMap<String, Quarantine>,
    /// Remote changes not written yet because the note had unsaved edits in the app, by note id.
    #[serde(default)]
    pub deferred: BTreeMap<String, PullChange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteState {
    /// Local path, workspace-relative with `/`.
    pub path: String,
    /// The path the server has. Differs from `path` after a local rename (or a remote rename into an occupied path)
    /// until the rename is pushed.
    pub remote_path: String,
    /// The server version the base text belongs to.
    pub version: u64,
    /// SHA-256 of the last agreed text (`base/<note_id>`).
    pub base_hash: String,
    /// Size and mtime (ns) of the file when it was last seen holding exactly the base text.
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub mtime: Option<u128>,
    /// The op id of a change sent but not answered yet.
    #[serde(default)]
    pub pending: Option<PendingOp>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingOp {
    pub op_id: String,
    /// What the change was (op, base version, path, content hash): the same change reuses the op id.
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingCreate {
    pub note_id: String,
    pub op_id: String,
    /// Hash of the content sent (also stored as `base/<note_id>`).
    pub hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Quarantine {
    pub reason: String,
    pub message: String,
    /// Hash of the file when it was refused ("" for a delete); another hash re-arms it.
    pub file_hash: String,
}

impl State {
    pub fn fresh(workspace_id: &str, root: &Path) -> State {
        State { format: STATE_FORMAT, workspace_id: workspace_id.into(), root: root.to_string_lossy().into_owned(), ..Default::default() }
    }

    /// The tracked note whose local path is `path`, compared case-insensitively (the server's rule).
    pub fn note_at(&self, path: &str) -> Option<(&String, &NoteState)> {
        let key = path_key(path);
        self.notes.iter().find(|(_, n)| path_key(&n.path) == key)
    }
}

/// The server's path comparison (`pathKey`): Unicode NFC, then lowercase. Two paths with the same key are the same
/// note path for the server (and on macOS/Windows file systems).
pub fn path_key(path: &str) -> String {
    nfc(path).to_lowercase()
}

/// `path` in Unicode NFC. Paths equal in NFC differ only in how accents are encoded (macOS may report NFD names): the
/// same name, not a rename. Case still counts here (a case-only change is a real rename).
pub fn nfc(path: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    path.nfc().collect()
}

/// Server-made note ids are used as file names under `base/`, so only real UUIDs are accepted.
pub fn is_note_id(id: &str) -> bool {
    let b = id.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, c)| if matches!(i, 8 | 13 | 18 | 23) { *c == b'-' } else { c.is_ascii_hexdigit() })
}

/// Size and modification time (ns since the epoch) of a file, the fast "unchanged?" check.
pub fn file_stat(path: &Path) -> Option<(u64, u128)> {
    let meta = fs::metadata(path).ok()?;
    let mtime = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_nanos();
    Some((meta.len(), mtime))
}

/// The stat to remember for a file that holds the base text, or None when it was modified too recently to trust.
pub fn trusted_stat(path: &Path) -> Option<(u64, u128)> {
    let (size, mtime) = file_stat(path)?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_nanos();
    (now.saturating_sub(mtime) >= RACY_WINDOW_NS).then_some((size, mtime))
}

/// Where the state of one linked workspace lives.
#[derive(Debug, Clone)]
pub struct StateStore {
    dir: PathBuf,
}

impl StateStore {
    pub fn new(dir: impl Into<PathBuf>) -> StateStore {
        StateStore { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Loads the state for `workspace_id` linked to `root` (canonical). Returns a fresh state, and why, when there is
    /// none or it cannot be used: missing, corrupt (kept as `state.json.corrupt` for inspection), another format,
    /// another workspace or another folder.
    pub fn load(&self, workspace_id: &str, root: &Path) -> (State, Option<String>) {
        let file = self.dir.join(STATE_FILE);
        let fresh = |why: &str| (State::fresh(workspace_id, root), Some(why.to_string()));
        let text = match fs::read_to_string(&file) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                self.clear_bases();
                return fresh("first sync of this folder");
            }
            Err(_) => return fresh("the sync state could not be read; starting a fresh sync"),
        };
        let state = match serde_json::from_str::<State>(&text) {
            Ok(s) => s,
            Err(_) => {
                let _ = fs::rename(&file, self.dir.join(format!("{STATE_FILE}.corrupt")));
                self.clear_bases();
                return fresh("the sync state was damaged; starting a fresh sync");
            }
        };
        if state.format != STATE_FORMAT || state.workspace_id != workspace_id || Path::new(&state.root) != root {
            self.clear_bases();
            return fresh("the sync state belongs to another workspace or folder; starting a fresh sync");
        }
        (state, None)
    }

    pub fn save(&self, state: &State) -> goo_core::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let json = serde_json::to_string(state).map_err(|e| CoreError::new(ErrorCode::Io, e.to_string()))?;
        write_atomic(&self.dir.join(STATE_FILE), &json)
    }

    fn base_path(&self, note_id: &str) -> goo_core::Result<PathBuf> {
        if !is_note_id(note_id) {
            return Err(CoreError::new(ErrorCode::InvalidName, format!("{note_id} is not a note id")));
        }
        Ok(self.dir.join(BASE_DIR).join(note_id.to_ascii_lowercase()))
    }

    /// The base text of a note, if present and still matching `hash` (a crash between writing the base and saving the
    /// state leaves a newer base behind; it is then treated as missing).
    pub fn read_base(&self, note_id: &str, hash: &str) -> Option<String> {
        let text = fs::read_to_string(self.base_path(note_id).ok()?).ok()?;
        (content_hash(text.as_bytes()) == hash).then_some(text)
    }

    pub fn write_base(&self, note_id: &str, text: &str) -> goo_core::Result<()> {
        let path = self.base_path(note_id)?;
        fs::create_dir_all(self.dir.join(BASE_DIR))?;
        write_atomic(&path, text)
    }

    pub fn remove_base(&self, note_id: &str) {
        if let Ok(p) = self.base_path(note_id) {
            let _ = fs::remove_file(p);
        }
    }

    fn clear_bases(&self) {
        let _ = fs::remove_dir_all(self.dir.join(BASE_DIR));
    }

    /// Forgets the link entirely (the app's "unlink"); the notes folder is not touched.
    pub fn remove(&self) -> goo_core::Result<()> {
        match fs::remove_dir_all(&self.dir) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }
}
