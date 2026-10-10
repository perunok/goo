//! Local changes, derived from the folder as it is now against the stored state (no op log): many offline edits
//! become one change and an edit that was undone is no change at all.
//!
//! The scan sees exactly the notes the app shows (goo-core's tree filters). A tracked file whose size and mtime are
//! unchanged is not read again. A tracked file that is gone while an untracked file holds its base text is a rename
//! (same note id); a tracked file that is gone otherwise is a delete.

use goo_core::{list_notes, relative_string, resolve_inside};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::content_hash;
use crate::state::{file_stat, nfc, State};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileInfo {
    pub abs: PathBuf,
    pub size: u64,
    pub mtime: u128,
    pub hash: String,
}

#[derive(Debug, Clone, Default)]
pub struct Scan {
    /// Notes in the folder by workspace-relative path (`/`-separated).
    pub files: BTreeMap<String, FileInfo>,
    /// Notes that exist but could not be read this time (never treated as deleted).
    pub unreadable: BTreeSet<String>,
    /// The walk stopped at goo-core's entry limit: files may be missing from `files`, so nothing counts as deleted.
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalChange {
    /// An untracked note.
    Create { path: String },
    /// A tracked note whose text and/or path differ from what the server has.
    Update { note_id: String, content: bool, rename: bool },
    /// A tracked note whose file is gone.
    Delete { note_id: String },
}

/// Walks `root` (canonical) and hashes every note, reusing the base hash of tracked files whose size and mtime are
/// unchanged. Symlinked notes pointing outside the folder are left out.
pub fn scan(root: &Path, state: &State) -> goo_core::Result<Scan> {
    let (paths, truncated) = list_notes(root)?;
    let tracked: BTreeMap<&str, _> = state.notes.values().map(|n| (n.path.as_str(), n)).collect();
    let mut out = Scan { truncated, ..Default::default() };
    for abs in paths {
        let rel = relative_string(root, &abs);
        if resolve_inside(root, &abs).is_err() {
            continue;
        }
        let Some((size, mtime)) = file_stat(&abs) else {
            out.unreadable.insert(rel);
            continue;
        };
        let known = tracked.get(rel.as_str()).filter(|n| n.size == Some(size) && n.mtime == Some(mtime));
        let hash = match known {
            Some(n) => n.base_hash.clone(),
            None => match fs::read(&abs) {
                Ok(bytes) => content_hash(&bytes),
                Err(_) => {
                    out.unreadable.insert(rel);
                    continue;
                }
            },
        };
        out.files.insert(rel, FileInfo { abs, size, mtime, hash });
    }
    Ok(out)
}

/// Tracked notes whose file moved: `(note_id, new path)`. A missing tracked file is matched first with an untracked file
/// of the same name in another Unicode normalisation (not a rename at all, whatever the text), then with an untracked
/// file holding exactly its base text, preferring one with the same file name (a move between folders), then by path.
pub fn detect_renames(state: &State, scan: &Scan) -> Vec<(String, String)> {
    let tracked: BTreeSet<&str> = state.notes.values().map(|n| n.path.as_str()).collect();
    let mut untracked: Vec<(&String, &FileInfo)> = scan.files.iter().filter(|(p, _)| !tracked.contains(p.as_str())).collect();
    let mut missing: Vec<(&String, &crate::state::NoteState)> = state
        .notes
        .iter()
        .filter(|(_, n)| !scan.files.contains_key(&n.path) && !scan.unreadable.contains(&n.path))
        .collect();
    missing.sort_by(|a, b| a.1.path.cmp(&b.1.path));
    let mut out = Vec::new();
    let mut left = Vec::new();
    for (id, note) in missing {
        let key = nfc(&note.path);
        match untracked.iter().position(|(p, _)| nfc(p) == key) {
            Some(i) => {
                let (path, _) = untracked.remove(i);
                out.push((id.clone(), path.clone()));
            }
            None => left.push((id, note)),
        }
    }
    for (id, note) in left {
        let name = file_name(&note.path);
        let candidates = || untracked.iter().enumerate().filter(|(_, (_, f))| f.hash == note.base_hash);
        let pick = candidates().find(|(_, (p, _))| file_name(p) == name).or_else(|| candidates().next()).map(|(i, _)| i);
        if let Some(i) = pick {
            let (path, _) = untracked.remove(i);
            out.push((id.clone(), path.clone()));
        }
    }
    out
}

/// The changes to push, after renames were applied to `state`: deletes first (they free paths), then updates, then
/// creates. Deletes are left out when the walk was truncated. Paths compare exactly here: an untracked `Plan.md` next
/// to a tracked `plan.md` (possible on case-sensitive file systems) is a create; the server answers `path_taken` and
/// the engine renames it.
pub fn detect_changes(state: &State, scan: &Scan) -> Vec<LocalChange> {
    let mut deletes = Vec::new();
    let mut updates = Vec::new();
    let mut tracked = BTreeSet::new();
    for (id, note) in &state.notes {
        tracked.insert(note.path.as_str());
        match scan.files.get(&note.path) {
            Some(file) => {
                let content = file.hash != note.base_hash;
                // the local name is sent as it is; only a change beyond the Unicode normalisation is a rename
                let rename = nfc(&note.path) != nfc(&note.remote_path);
                if content || rename {
                    updates.push(LocalChange::Update { note_id: id.clone(), content, rename });
                }
            }
            None if scan.unreadable.contains(&note.path) || scan.truncated => {}
            None => deletes.push(LocalChange::Delete { note_id: id.clone() }),
        }
    }
    let creates = scan
        .files
        .keys()
        .filter(|p| !tracked.contains(p.as_str()))
        .map(|p| LocalChange::Create { path: p.clone() });
    deletes.into_iter().chain(updates).chain(creates).collect()
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}
