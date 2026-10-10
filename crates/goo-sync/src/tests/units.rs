//! Merge, scan and state on their own.

use crate::content_hash;
use crate::engine::local_stamp;
use crate::merge::{merge, Merge};
use crate::scan::{detect_changes, detect_renames, scan, LocalChange};
use crate::state::{is_note_id, NoteState, State, StateStore};
use std::fs;
use std::path::PathBuf;

#[test]
fn merges_by_lines() {
    let base = "a\nb\nc\nd\ne\n";
    assert_eq!(merge(base, "A\nb\nc\nd\ne\n", "a\nb\nc\nd\nE\n"), Merge::Clean("A\nb\nc\nd\nE\n".into()));
    assert_eq!(merge(base, "a\nB1\nc\nd\ne\n", "a\nB2\nc\nd\ne\n"), Merge::Conflict);
    assert_eq!(merge(base, "same\n", "same\n"), Merge::Clean("same\n".into()));
    assert_eq!(merge(base, base, "theirs\n"), Merge::Clean("theirs\n".into()));
    assert_eq!(merge(base, "ours\n", base), Merge::Clean("ours\n".into()));
    // edits on adjacent lines overlap (diff3); CRLF and a missing final newline are kept as they are
    assert_eq!(merge("x\r\ny", "x!\r\ny", "x\r\ny!"), Merge::Conflict);
    assert_eq!(merge("x\r\nm\r\ny", "x!\r\nm\r\ny", "x\r\nm\r\ny!"), Merge::Clean("x!\r\nm\r\ny!".into()));
}

fn folder() -> (tempfile::TempDir, PathBuf) {
    let d = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(d.path()).unwrap();
    (d, root)
}

fn tracked(path: &str, text: &str) -> NoteState {
    NoteState {
        path: path.into(),
        remote_path: path.into(),
        version: 1,
        base_hash: content_hash(text.as_bytes()),
        size: None,
        mtime: None,
        pending: None,
    }
}

#[test]
fn scan_derives_creates_edits_renames_and_deletes() {
    let (_d, root) = folder();
    fs::create_dir_all(root.join("sub")).unwrap();
    fs::create_dir_all(root.join(".hidden")).unwrap();
    fs::write(root.join("same.md"), "same").unwrap();
    fs::write(root.join("edited.md"), "new text").unwrap();
    fs::write(root.join("sub/moved.md"), "moved").unwrap();
    fs::write(root.join("new.md"), "new").unwrap();
    fs::write(root.join(".hidden/x.md"), "hidden").unwrap();
    fs::write(root.join("not-a-note.txt"), "x").unwrap();

    let mut state = State::fresh("w", &root);
    state.notes.insert("1".into(), tracked("same.md", "same"));
    state.notes.insert("2".into(), tracked("edited.md", "old text"));
    state.notes.insert("3".into(), tracked("moved.md", "moved"));
    state.notes.insert("4".into(), tracked("gone.md", "gone"));

    let s = scan(&root, &state).unwrap();
    assert_eq!(s.files.keys().collect::<Vec<_>>(), ["edited.md", "new.md", "same.md", "sub/moved.md"]);
    let renames = detect_renames(&state, &s);
    assert_eq!(renames, vec![("3".to_string(), "sub/moved.md".to_string())]);
    state.notes.get_mut("3").unwrap().path = "sub/moved.md".into();
    let changes = detect_changes(&state, &s);
    assert_eq!(
        changes,
        vec![
            LocalChange::Delete { note_id: "4".into() },
            LocalChange::Update { note_id: "2".into(), content: true, rename: false },
            LocalChange::Update { note_id: "3".into(), content: false, rename: true },
            LocalChange::Create { path: "new.md".into() },
        ]
    );
}

#[test]
fn scan_trusts_size_and_mtime_of_unchanged_files() {
    let (_d, root) = folder();
    fs::write(root.join("a.md"), "real text").unwrap();
    let (size, mtime) = crate::state::file_stat(&root.join("a.md")).unwrap();
    let mut state = State::fresh("w", &root);
    // the stored stat matches: the (deliberately wrong) base hash is taken without reading the file
    let mut n = tracked("a.md", "something else");
    n.size = Some(size);
    n.mtime = Some(mtime);
    state.notes.insert("1".into(), n);
    let s = scan(&root, &state).unwrap();
    assert_eq!(s.files["a.md"].hash, content_hash(b"something else"));
    // another mtime: hashed
    state.notes.get_mut("1").unwrap().mtime = Some(mtime + 1);
    let s = scan(&root, &state).unwrap();
    assert_eq!(s.files["a.md"].hash, content_hash(b"real text"));
}

#[test]
fn a_rename_prefers_the_same_file_name() {
    let (_d, root) = folder();
    fs::create_dir_all(root.join("b")).unwrap();
    fs::write(root.join("copy.md"), "t").unwrap();
    fs::write(root.join("b/note.md"), "t").unwrap();
    let mut state = State::fresh("w", &root);
    state.notes.insert("1".into(), tracked("a/note.md", "t"));
    let s = scan(&root, &state).unwrap();
    assert_eq!(detect_renames(&state, &s), vec![("1".to_string(), "b/note.md".to_string())]);
}

#[test]
fn state_survives_round_trips_and_recovers_from_damage() {
    let (_d, dir) = folder();
    let root = dir.join("notes");
    fs::create_dir_all(&root).unwrap();
    let store = StateStore::new(dir.join("state"));
    let (mut state, why) = store.load("w1", &root);
    assert!(why.is_some());
    state.checkpoint = 42;
    state.notes.insert("0e5c6a1e-6f3b-4c1e-9d55-2a3c4b5d6e7f".into(), tracked("a.md", "a"));
    store.save(&state).unwrap();
    store.write_base("0e5c6a1e-6f3b-4c1e-9d55-2a3c4b5d6e7f", "a").unwrap();
    assert_eq!(store.load("w1", &root), (state.clone(), None));
    assert_eq!(store.read_base("0e5c6a1e-6f3b-4c1e-9d55-2a3c4b5d6e7f", &content_hash(b"a")).as_deref(), Some("a"));
    assert_eq!(store.read_base("0e5c6a1e-6f3b-4c1e-9d55-2a3c4b5d6e7f", &content_hash(b"other")), None, "a stale base is not used");
    assert!(store.write_base("../../evil", "x").is_err());

    // another workspace or folder starts fresh
    assert_eq!(store.load("w2", &root).0.checkpoint, 0);
    assert_eq!(store.load("w1", &dir).0.checkpoint, 0);
    // damage
    fs::write(dir.join("state/state.json"), "{\"format\": 1, \"work").unwrap();
    let (fresh, why) = store.load("w1", &root);
    assert_eq!(fresh.checkpoint, 0);
    assert!(why.unwrap().contains("damaged"));
    assert!(dir.join("state/state.json.corrupt").exists());
    assert!(root.read_dir().unwrap().next().is_none(), "the notes folder is never touched");
}

#[test]
fn note_ids_and_stamps() {
    assert!(is_note_id("0e5c6a1e-6f3b-4c1e-9d55-2a3c4b5d6e7f"));
    assert!(!is_note_id("0e5c6a1e-6f3b-4c1e-9d55-2a3c4b5d6e7"));
    assert!(!is_note_id("../../0e5c6a1e-6f3b-4c1e-9d55-2a3c4b5"));
    assert_eq!(local_stamp(0), "1970-01-01 0000");
    assert_eq!(local_stamp(1_791_639_000), "2026-10-10 1330");
    assert_eq!(local_stamp(951_782_400 + 86_399), "2000-02-29 2359");
}
