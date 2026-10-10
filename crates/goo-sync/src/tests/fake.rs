//! An in-process goo cloud with the sync semantics of `goo-cloud/server/src/services/sync.ts` and `routes/sync.ts`:
//! a workspace checkpoint, one `seq` per accepted change stored on the note (so pulls are coalesced), tombstones (left
//! out of `after=0`), case-insensitive path uniqueness among live notes, idempotency by `op_id` (rejections replay
//! with the note's current state), the six rejection reasons, viewer `forbidden` (not stored), paging by count and
//! content size, and the ~8 MiB budget for `current` content in a push answer. Faults can be injected.

use crate::api::*;
use crate::content_hash;
use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

pub const PULL_PAGE_BYTES: usize = 8 * 1024 * 1024;
const MAX_PATH_CHARS: usize = 1024;
const MAX_SEGMENT_CHARS: usize = 200;
const NOTE_EXTENSIONS: &[&str] = &[".md", ".markdown", ".mdown", ".mkd"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub path: String,
    pub content: String,
    pub version: u64,
    pub deleted: bool,
    pub seq: u64,
}

#[derive(Debug, Clone)]
enum Outcome {
    Accepted { note_id: String, version: u64, seq: u64 },
    Rejected { note_id: String, reason: Reason, message: String },
}

#[derive(Default)]
pub struct Inner {
    pub checkpoint: u64,
    pub notes: BTreeMap<String, Note>,
    ops: HashMap<String, Outcome>,
    pub clients: Vec<String>,
    /// Every push received (also failed ones), for assertions on op ids.
    pub pushes: Vec<Vec<PushChange>>,
    pub pulls: usize,
    /// The caller is a viewer: every change is `forbidden`.
    pub viewer: bool,
    /// The next push is applied but its answer is lost (a network error is returned).
    pub lose_next_answer: bool,
    /// The next push fails before reaching the server.
    pub fail_next_push: bool,
    /// Content budget of a pull page (the real server's is 8 MiB).
    pub page_bytes: Option<usize>,
    /// Changes of "another device" applied right before the next push (a race between a pull and a push).
    pub before_next_push: Vec<PushChange>,
}

#[derive(Default)]
pub struct FakeServer {
    pub inner: Mutex<Inner>,
}

fn path_key(path: &str) -> String {
    // the server's pathKey: NFC, then lowercase
    crate::state::path_key(path)
}

fn is_uuid(id: &str) -> bool {
    crate::state::is_note_id(id)
}

/// `pathProblem` of the server.
pub fn path_problem(path: &str) -> Option<String> {
    if path.is_empty() {
        return Some("path is empty".into());
    }
    if path.chars().count() > MAX_PATH_CHARS {
        return Some(format!("path is longer than {MAX_PATH_CHARS} characters"));
    }
    if path.contains('\\') {
        return Some("path must use / as the separator (no backslash)".into());
    }
    if path.chars().any(|c| ('\u{0}'..='\u{1f}').contains(&c) || ('\u{7f}'..='\u{9f}').contains(&c)) {
        return Some("path must not contain control characters".into());
    }
    if path.starts_with('/') {
        return Some("path must be relative (no leading /)".into());
    }
    for s in path.split('/') {
        if s.is_empty() {
            return Some("path must not contain empty segments (// or a trailing /)".into());
        }
        if s == "." || s == ".." {
            return Some("path must not contain . or .. segments".into());
        }
        if s.chars().count() > MAX_SEGMENT_CHARS {
            return Some(format!("a path segment is longer than {MAX_SEGMENT_CHARS} characters"));
        }
    }
    let name = path.rsplit('/').next().unwrap().to_lowercase();
    if !NOTE_EXTENSIONS.iter().any(|e| name.ends_with(e)) {
        return Some("path must end in .md, .markdown, .mdown, .mkd".into());
    }
    None
}

fn current_of(n: &Note) -> Current {
    Current { version: n.version, path: n.path.clone(), deleted: n.deleted, content: if n.deleted { None } else { Some(n.content.clone()) } }
}

impl FakeServer {
    pub fn new() -> FakeServer {
        FakeServer::default()
    }

    pub fn with<R>(&self, f: impl FnOnce(&mut Inner) -> R) -> R {
        f(&mut self.inner.lock().unwrap())
    }

    /// Live notes as `path -> content`.
    pub fn live(&self) -> BTreeMap<String, String> {
        self.with(|s| s.notes.values().filter(|n| !n.deleted).map(|n| (n.path.clone(), n.content.clone())).collect())
    }

    /// A change as another device would push it (test setup). Returns the outcome as a push answer.
    pub fn push_as_other(&self, changes: Vec<PushChange>) -> PushResponse {
        self.with(|s| s.apply_all(&changes))
    }
}

impl Inner {
    fn path_owner(&self, note_id: &str, path: &str) -> bool {
        let key = path_key(path);
        self.notes.iter().any(|(id, n)| id != note_id && !n.deleted && path_key(&n.path) == key)
    }

    fn next_seq(&mut self) -> u64 {
        self.checkpoint += 1;
        self.checkpoint
    }

    fn decide(&mut self, ch: &PushChange) -> Outcome {
        let note_id = ch.note_id.clone();
        let rejected = |reason: Reason, message: &str| Outcome::Rejected { note_id: note_id.clone(), reason, message: message.into() };
        if !is_uuid(&ch.note_id) {
            return rejected(Reason::Invalid, "note_id must be a UUID");
        }
        if ch.op == Op::Upsert {
            if let Some(p) = &ch.path {
                if let Some(problem) = path_problem(p) {
                    return rejected(Reason::Invalid, &problem);
                }
            }
            if let Some(c) = &ch.content {
                if c.len() as u64 > MAX_NOTE_BYTES {
                    return rejected(Reason::TooLarge, &format!("content is {} bytes; the limit is {MAX_NOTE_BYTES}", c.len()));
                }
                if c.contains('\u{0}') {
                    return rejected(Reason::Invalid, "content must not contain NUL characters");
                }
            }
        }
        let Some(cur) = self.notes.get(&ch.note_id).cloned() else {
            if ch.op == Op::Delete {
                return rejected(Reason::NotFound, "note does not exist on the server");
            }
            if ch.base_version > 0 {
                return rejected(Reason::NotFound, "note does not exist on the server (push it as new with base_version 0)");
            }
            let (Some(path), Some(content)) = (&ch.path, &ch.content) else {
                return rejected(Reason::Invalid, "a new note needs both path and content");
            };
            if self.path_owner(&ch.note_id, path) {
                return rejected(Reason::PathTaken, &format!("another note already uses the path {path}"));
            }
            let seq = self.next_seq();
            self.notes.insert(ch.note_id.clone(), Note { path: path.clone(), content: content.clone(), version: 1, deleted: false, seq });
            return Outcome::Accepted { note_id, version: 1, seq };
        };
        if ch.base_version != cur.version {
            return rejected(Reason::VersionMismatch, "base_version does not match the server version; pull and merge");
        }
        if ch.op == Op::Delete {
            if cur.deleted {
                return Outcome::Accepted { note_id, version: cur.version, seq: cur.seq };
            }
            let seq = self.next_seq();
            let version = cur.version + 1;
            self.notes.insert(ch.note_id.clone(), Note { path: cur.path, content: String::new(), version, deleted: true, seq });
            return Outcome::Accepted { note_id, version, seq };
        }
        let path = ch.path.clone().unwrap_or(cur.path.clone());
        let content = ch.content.clone().unwrap_or(cur.content.clone());
        if !cur.deleted && path == cur.path && content == cur.content {
            return Outcome::Accepted { note_id, version: cur.version, seq: cur.seq };
        }
        if (cur.deleted || path_key(&path) != path_key(&cur.path)) && self.path_owner(&ch.note_id, &path) {
            return rejected(Reason::PathTaken, &format!("another note already uses the path {path}"));
        }
        let seq = self.next_seq();
        let version = cur.version + 1;
        self.notes.insert(ch.note_id.clone(), Note { path, content, version, deleted: false, seq });
        Outcome::Accepted { note_id, version, seq }
    }

    fn apply_all(&mut self, changes: &[PushChange]) -> PushResponse {
        let mut accepted = Vec::new();
        let mut rejected = Vec::new();
        let mut current_bytes = 0usize;
        for ch in changes {
            let outcome = if self.viewer {
                Outcome::Rejected { note_id: ch.note_id.clone(), reason: Reason::Forbidden, message: "this workspace is read-only for your role (viewer)".into() }
            } else if let Some(prior) = self.ops.get(&ch.op_id).cloned() {
                prior
            } else {
                let o = self.decide(ch);
                self.ops.insert(ch.op_id.clone(), o.clone());
                o
            };
            match outcome {
                Outcome::Accepted { note_id, version, seq } => accepted.push(Accepted { op_id: ch.op_id.clone(), note_id, version, seq }),
                Outcome::Rejected { note_id, reason, message } => {
                    let mut current = match reason {
                        Reason::VersionMismatch | Reason::PathTaken => self.notes.get(&note_id).map(current_of),
                        _ => None,
                    };
                    if let Some(Current { content: Some(c), .. }) = &current {
                        current_bytes += c.len();
                        if current_bytes > PULL_PAGE_BYTES {
                            current.as_mut().unwrap().content = None;
                        }
                    }
                    rejected.push(Rejected { op_id: ch.op_id.clone(), note_id, reason, message, current });
                }
            }
        }
        PushResponse { accepted, rejected, checkpoint: self.checkpoint }
    }

    fn page(&self, after: u64, limit: u32) -> PullPage {
        let head = self.checkpoint;
        let mut meta: Vec<(&String, &Note)> =
            self.notes.iter().filter(|(_, n)| n.seq > after && n.seq <= head && (after > 0 || !n.deleted)).collect();
        meta.sort_by_key(|(_, n)| n.seq);
        let limit = limit as usize;
        let mut has_more = meta.len() > limit;
        meta.truncate(limit);
        let budget = self.page_bytes.unwrap_or(PULL_PAGE_BYTES);
        let mut bytes = 0;
        for i in 0..meta.len() {
            bytes += meta[i].1.content.len();
            if bytes > budget && i > 0 {
                meta.truncate(i);
                has_more = true;
                break;
            }
        }
        let changes: Vec<PullChange> = meta
            .iter()
            .map(|(id, n)| PullChange {
                note_id: (*id).clone(),
                path: n.path.clone(),
                version: n.version,
                deleted: n.deleted,
                content_hash: (!n.deleted).then(|| content_hash(n.content.as_bytes())),
                content: (!n.deleted).then(|| n.content.clone()),
                seq: n.seq,
            })
            .collect();
        let checkpoint = if has_more { changes.last().map(|c| c.seq).unwrap_or(after) } else { head };
        PullPage { changes, checkpoint, has_more }
    }
}

impl Remote for FakeServer {
    fn register_client(&self, _info: &ClientInfo) -> Result<String, RemoteError> {
        Ok(self.with(|s| {
            let id = uuid::Uuid::new_v4().to_string();
            s.clients.push(id.clone());
            id
        }))
    }

    fn pull(&self, _workspace_id: &str, client_id: &str, after: u64, limit: u32) -> Result<PullPage, RemoteError> {
        self.with(|s| {
            if !s.clients.iter().any(|c| c == client_id) {
                return Err(RemoteError::Api { status: 400, code: "invalid_request".into(), message: "unknown client_id".into() });
            }
            s.pulls += 1;
            Ok(s.page(after, limit))
        })
    }

    fn push(&self, _workspace_id: &str, client_id: &str, changes: &[PushChange]) -> Result<PushResponse, RemoteError> {
        self.with(|s| {
            if !s.clients.iter().any(|c| c == client_id) {
                return Err(RemoteError::Api { status: 400, code: "invalid_request".into(), message: "unknown client_id".into() });
            }
            assert!(changes.len() <= MAX_PUSH_CHANGES, "more than 200 changes in one push");
            s.pushes.push(changes.to_vec());
            let race = std::mem::take(&mut s.before_next_push);
            s.apply_all(&race);
            if s.fail_next_push {
                s.fail_next_push = false;
                return Err(RemoteError::Network("connection reset".into()));
            }
            let answer = s.apply_all(changes);
            if s.lose_next_answer {
                s.lose_next_answer = false;
                return Err(RemoteError::Network("connection reset after the request was sent".into()));
            }
            Ok(answer)
        })
    }
}
