//! Watches the open folder, so changes made outside goo (another editor, git, Claude Code, a sync tool) show up:
//! open notes reload (or warn when they have unsaved edits) and the tree refreshes. Same `files-changed` event as
//! MCP writes and goo cloud, with `tree: true` when only the tree needs refreshing.

use goo_core::{classify_change, WatchedChange};
use notify_debouncer_mini::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, Debouncer};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/// Events closer together than this become one `files-changed`.
const DEBOUNCE: Duration = Duration::from_millis(300);

#[derive(Default)]
pub struct FolderWatch {
    current: Mutex<Option<(PathBuf, Debouncer<RecommendedWatcher>)>>,
}

impl FolderWatch {
    /// Watches `root` (replacing the previous folder). Failure is not fatal: goo works without watching.
    pub fn follow(&self, app: &AppHandle, root: &Path) {
        let mut current = self.current.lock().unwrap();
        if current.as_ref().is_some_and(|(r, _)| r == root) {
            return;
        }
        *current = None; // stop watching the old folder first
        let app = app.clone();
        let watched = root.to_path_buf();
        let handler = move |res: DebounceEventResult| {
            let Ok(events) = res else { return };
            let mut notes: Vec<String> = Vec::new();
            let mut tree = false;
            for e in events {
                match classify_change(&watched, &e.path) {
                    WatchedChange::Note => notes.push(e.path.to_string_lossy().into_owned()),
                    WatchedChange::Tree => tree = true,
                    WatchedChange::Ignore => {}
                }
            }
            if !notes.is_empty() || tree {
                notes.sort();
                notes.dedup();
                let _ = app.emit("files-changed", json!({ "root": watched.to_string_lossy(), "files": notes, "tree": true }));
            }
        };
        match new_debouncer(DEBOUNCE, handler) {
            Ok(mut debouncer) => match debouncer.watcher().watch(root, RecursiveMode::Recursive) {
                Ok(()) => *current = Some((root.to_path_buf(), debouncer)),
                Err(e) => eprintln!("goo: not watching {}: {e}", root.display()),
            },
            Err(e) => eprintln!("goo: folder watching unavailable: {e}"),
        }
    }
}
