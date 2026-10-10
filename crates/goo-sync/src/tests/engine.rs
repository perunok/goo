//! Engine scenarios: two "devices" (folder + state dir each) syncing through the in-process fake server.

use super::fake::{FakeServer, Note};
use crate::api::*;
use crate::engine::check_remote_path;
use crate::state::{StateStore, STATE_FILE};
use crate::{sync, Report, SyncFailure, SyncOptions};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const WS: &str = "ws-1";

struct Device {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    state: PathBuf,
    name: &'static str,
    unsaved: RefCell<Vec<String>>,
    read_only: bool,
}

impl Device {
    fn new(name: &'static str) -> Device {
        let tmp = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(tmp.path()).unwrap();
        let root = base.join("notes");
        fs::create_dir_all(&root).unwrap();
        Device { _tmp: tmp, root, state: base.join("state"), name, unsaved: RefCell::new(Vec::new()), read_only: false }
    }

    fn try_sync(&self, server: &FakeServer) -> Result<Report, SyncFailure> {
        let unsaved: Vec<PathBuf> = self.unsaved.borrow().iter().map(|r| self.abs(r)).collect();
        let may_write = move |p: &Path| !unsaved.iter().any(|u| u == p);
        let client = ClientInfo { device_name: self.name.into(), platform: "linux".into(), app_version: "0.1.0".into() };
        let opts = SyncOptions { may_write: &may_write, read_only: self.read_only, ..SyncOptions::new(WS, client) };
        sync(&self.root, &self.state, server, &opts)
    }

    fn sync(&self, server: &FakeServer) -> Report {
        self.try_sync(server).unwrap_or_else(|e| panic!("{} sync failed: {e}", self.name))
    }

    fn abs(&self, rel: &str) -> PathBuf {
        rel.split('/').fold(self.root.clone(), |p, s| p.join(s))
    }

    fn write(&self, rel: &str, text: &str) {
        let p = self.abs(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }

    fn read(&self, rel: &str) -> Option<String> {
        fs::read_to_string(self.abs(rel)).ok()
    }

    fn remove(&self, rel: &str) {
        fs::remove_file(self.abs(rel)).unwrap();
    }

    fn rename(&self, from: &str, to: &str) {
        let t = self.abs(to);
        fs::create_dir_all(t.parent().unwrap()).unwrap();
        fs::rename(self.abs(from), t).unwrap();
    }

    /// Every note in the folder, as the app shows them.
    fn notes(&self) -> BTreeMap<String, String> {
        let (paths, _) = goo_core::list_notes(&self.root).unwrap();
        paths.iter().map(|p| (goo_core::relative_string(&self.root, p), fs::read_to_string(p).unwrap())).collect()
    }
}

/// Syncs every device twice and checks that all folders equal the server's live notes.
fn converge(server: &FakeServer, devices: &[&Device]) -> BTreeMap<String, String> {
    for _ in 0..2 {
        for d in devices {
            d.sync(server);
        }
    }
    let live = server.live();
    for d in devices {
        assert_eq!(d.notes(), live, "{} differs from the server", d.name);
    }
    live
}

fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
}

fn note_id(server: &FakeServer, path: &str) -> String {
    server.with(|s| s.notes.iter().find(|(_, n)| n.path == path && !n.deleted).map(|(id, _)| id.clone()).expect("note on server"))
}

// ---------------------------------------------------------------------------
// basic flows
// ---------------------------------------------------------------------------

#[test]
fn create_edit_rename_delete_in_both_directions() {
    let server = FakeServer::new();
    let a = Device::new("devA");
    let b = Device::new("devB");

    a.write("todo.md", "# Todo\n- milk\n");
    a.write("projects/plan.md", "# Plan\n");
    let r = a.sync(&server);
    assert_eq!(r.pushed, 2);
    assert_eq!(r.reset.as_deref(), Some("first sync of this folder"));
    let r = b.sync(&server);
    assert_eq!(r.pulled, 2);
    assert_eq!(r.written, 2);
    assert!(r.changed_files.contains(&b.abs("projects/plan.md")));
    converge(&server, &[&a, &b]);

    // edit on B, rename on A (different notes)
    b.write("todo.md", "# Todo\n- milk\n- eggs\n");
    a.rename("projects/plan.md", "archive/plan.md");
    b.sync(&server);
    let r = a.sync(&server);
    assert_eq!(r.pushed, 1, "the rename is pushed as a path-only change");
    let pushed = server.with(|s| s.pushes.last().unwrap()[0].clone());
    assert_eq!(pushed.path.as_deref(), Some("archive/plan.md"));
    assert_eq!(pushed.content, None, "a rename does not resend the text");
    let r = b.sync(&server);
    assert_eq!(r.renamed, 1);
    assert!(!b.abs("projects").exists(), "the emptied folder is removed");
    let live = converge(&server, &[&a, &b]);
    assert_eq!(live, map(&[("archive/plan.md", "# Plan\n"), ("todo.md", "# Todo\n- milk\n- eggs\n")]));

    // delete on B
    b.remove("archive/plan.md");
    b.sync(&server);
    let r = a.sync(&server);
    assert_eq!(r.deleted, 1);
    assert!(a.read("archive/plan.md").is_none());
    assert_eq!(converge(&server, &[&a, &b]), map(&[("todo.md", "# Todo\n- milk\n- eggs\n")]));
}

#[test]
fn nothing_to_do_is_one_quiet_round() {
    let server = FakeServer::new();
    let a = Device::new("devA");
    a.write("a.md", "a\n");
    a.sync(&server);
    let pushes = server.with(|s| s.pushes.len());
    let r = a.sync(&server);
    assert_eq!((r.pulled, r.pushed, r.written, r.rounds), (0, 0, 0, 1));
    assert_eq!(server.with(|s| s.pushes.len()), pushes, "no empty pushes");
}

#[test]
fn many_notes_are_batched_and_paged() {
    let server = FakeServer::new();
    server.with(|s| s.page_bytes = Some(2000));
    let a = Device::new("devA");
    let b = Device::new("devB");
    for i in 0..250 {
        a.write(&format!("n/{i:03}.md"), &format!("note {i}\n{}\n", "x".repeat(40)));
    }
    let r = a.sync(&server);
    assert_eq!(r.pushed, 250);
    let sizes: Vec<usize> = server.with(|s| s.pushes.iter().map(|p| p.len()).collect());
    assert_eq!(sizes, vec![200, 50]);
    let pulls_before = server.with(|s| s.pulls);
    let r = b.sync(&server);
    assert_eq!(r.pulled, 250);
    assert!(server.with(|s| s.pulls) - pulls_before > 5, "small pages: several pulls");
    converge(&server, &[&a, &b]);
}

// ---------------------------------------------------------------------------
// concurrent changes
// ---------------------------------------------------------------------------

#[test]
fn concurrent_edits_on_different_lines_merge() {
    let server = FakeServer::new();
    let a = Device::new("devA");
    let b = Device::new("devB");
    a.write("plan.md", "one\ntwo\nthree\nfour\nfive\n");
    converge(&server, &[&a, &b]);

    a.write("plan.md", "ONE\ntwo\nthree\nfour\nfive\n");
    b.write("plan.md", "one\ntwo\nthree\nfour\nFIVE\n");
    a.sync(&server);
    let r = b.sync(&server);
    assert!(r.conflicts.is_empty());
    assert_eq!(b.read("plan.md").unwrap(), "ONE\ntwo\nthree\nfour\nFIVE\n");
    assert_eq!(r.pushed, 1, "the merge is pushed");
    let live = converge(&server, &[&a, &b]);
    assert_eq!(live["plan.md"], "ONE\ntwo\nthree\nfour\nFIVE\n");
}

#[test]
fn overlapping_edits_keep_both_texts() {
    let server = FakeServer::new();
    let a = Device::new("devA");
    let b = Device::new("devB");
    a.write("plan.md", "title\nbody\n");
    converge(&server, &[&a, &b]);

    a.write("plan.md", "title\nbody from A\n");
    b.write("plan.md", "title\nbody from B\n");
    a.sync(&server);
    let r = b.sync(&server);
    assert_eq!(r.conflicts.len(), 1);
    let c = &r.conflicts[0];
    assert_eq!(c.path, "plan.md");
    assert!(c.conflict_path.starts_with("plan (conflict devB 20") && c.conflict_path.ends_with(").md"), "{}", c.conflict_path);
    assert_eq!(b.read("plan.md").unwrap(), "title\nbody from B\n", "the local file is kept");
    assert_eq!(b.read(&c.conflict_path).unwrap(), "title\nbody from A\n", "the remote text is next to it");
    let live = converge(&server, &[&a, &b]);
    assert_eq!(live.len(), 2);
    assert_eq!(live["plan.md"], "title\nbody from B\n");
    assert_eq!(live[&c.conflict_path], "title\nbody from A\n");
}

#[test]
fn an_edit_beats_a_delete_either_way() {
    let server = FakeServer::new();
    let a = Device::new("devA");
    let b = Device::new("devB");
    a.write("x.md", "x\n");
    a.write("y.md", "y\n");
    converge(&server, &[&a, &b]);

    // the delete arrives first: the edited copy is pushed again as a new note
    a.remove("x.md");
    b.write("x.md", "x edited\n");
    a.sync(&server);
    b.sync(&server);
    // the edit arrives first: the delete is refused and the note comes back
    b.write("y.md", "y edited\n");
    a.remove("y.md");
    b.sync(&server);
    let r = a.sync(&server);
    assert!(r.written >= 2);
    let live = converge(&server, &[&a, &b]);
    assert_eq!(live, map(&[("x.md", "x edited\n"), ("y.md", "y edited\n")]));
}

#[test]
fn remote_delete_removes_untouched_files_and_keeps_edited_ones() {
    let server = FakeServer::new();
    let a = Device::new("devA");
    let b = Device::new("devB");
    a.write("keep.md", "k\n");
    a.write("gone.md", "g\n");
    converge(&server, &[&a, &b]);
    let keep_id = note_id(&server, "keep.md");

    a.remove("keep.md");
    a.remove("gone.md");
    a.sync(&server);
    b.write("keep.md", "k, edited offline\n");
    let r = b.sync(&server);
    assert_eq!(r.deleted, 1);
    assert!(b.read("gone.md").is_none());
    assert_eq!(b.read("keep.md").unwrap(), "k, edited offline\n");
    assert_ne!(note_id(&server, "keep.md"), keep_id, "pushed as a new note");
    converge(&server, &[&a, &b]);
}

#[test]
fn remote_rename_and_local_edit_combine() {
    let server = FakeServer::new();
    let a = Device::new("devA");
    let b = Device::new("devB");
    a.write("draft.md", "line 1\nline 2\n");
    converge(&server, &[&a, &b]);

    a.rename("draft.md", "posts/final.md");
    b.write("draft.md", "line 1\nline 2\nline 3 from B\n");
    a.sync(&server);
    let r = b.sync(&server);
    assert!(r.conflicts.is_empty());
    let live = converge(&server, &[&a, &b]);
    assert_eq!(live, map(&[("posts/final.md", "line 1\nline 2\nline 3 from B\n")]));
}

#[test]
fn the_same_new_path_on_both_devices_keeps_both() {
    let server = FakeServer::new();
    let a = Device::new("devA");
    let b = Device::new("devB");
    a.write("ideas.md", "from A\n");
    b.write("ideas.md", "from B\n");
    a.sync(&server);
    let r = b.sync(&server);
    assert_eq!(r.renamed, 1);
    assert_eq!(b.read("ideas (local).md").unwrap(), "from B\n");
    let live = converge(&server, &[&a, &b]);
    assert_eq!(live, map(&[("ideas (local).md", "from B\n"), ("ideas.md", "from A\n")]));

    // identical text on both sides is simply adopted
    let c = Device::new("devC");
    c.write("ideas.md", "from A\n");
    let r = c.sync(&server);
    assert_eq!((r.renamed, r.written), (0, 1));
    assert_eq!(c.notes(), live);
}

#[test]
fn a_case_variant_of_a_remote_path_is_moved_aside() {
    let server = FakeServer::new();
    let a = Device::new("devA");
    let b = Device::new("devB");
    a.write("plan.md", "A\n");
    a.sync(&server);
    // case-sensitive file system: B has a different file whose name differs only in case
    b.write("Plan.md", "B\n");
    let r = b.sync(&server);
    assert_eq!(b.read("plan.md").unwrap(), "A\n");
    assert_eq!(b.read("Plan (local).md").unwrap(), "B\n");
    assert_eq!(r.rounds, 1, "recognised as the same path while pulling: no path_taken round trip");
    let live = converge(&server, &[&a, &b]);
    assert_eq!(live, map(&[("Plan (local).md", "B\n"), ("plan.md", "A\n")]));
}

#[test]
fn path_taken_in_a_race_renames_the_local_file() {
    let server = FakeServer::new();
    let a = Device::new("devA");
    a.write("race.md", "A\n");
    // another device creates the same path between A's pull and A's push
    server.with(|s| {
        s.before_next_push = vec![PushChange {
            op_id: "race-op".into(),
            note_id: "11111111-1111-4111-8111-111111111111".into(),
            op: Op::Upsert,
            base_version: 0,
            path: Some("RACE.md".into()),
            content: Some("other\n".into()),
        }]
    });
    let r = a.sync(&server);
    assert_eq!(r.rounds, 2, "path_taken asks for another round");
    assert_eq!(a.read("race (local).md").unwrap(), "A\n");
    assert_eq!(a.read("RACE.md").unwrap(), "other\n");
    assert_eq!(server.live(), map(&[("RACE.md", "other\n"), ("race (local).md", "A\n")]));
}

// ---------------------------------------------------------------------------
// Unicode normalisation (the server's pathKey is NFC + lowercase; macOS may report NFD names)
// ---------------------------------------------------------------------------

const CAFE_NFC: &str = "caf\u{e9}";
const CAFE_NFD: &str = "cafe\u{301}";
const DIR_NFC: &str = "\u{dc}ber";
const DIR_NFD: &str = "U\u{308}ber";

#[test]
fn an_nfd_local_name_of_an_nfc_remote_path_is_unchanged() {
    let server = FakeServer::new();
    let a = Device::new("devA");
    let b = Device::new("devB");
    let nfc_path = format!("{DIR_NFC}/{CAFE_NFC}.md");
    let nfd_path = format!("{DIR_NFD}/{CAFE_NFD}.md");
    a.write(&nfc_path, "1\n2\n3\n");
    converge(&server, &[&a, &b]);

    // B's file system reports the names decomposed (as macOS may)
    b.rename(&nfc_path, &nfd_path);
    fs::remove_dir(b.abs(DIR_NFC)).unwrap();
    let pushes = server.with(|s| s.pushes.len());
    let r = b.sync(&server);
    assert_eq!((r.pushed, r.renamed, r.written), (0, 0, 0));
    assert_eq!(server.with(|s| s.pushes.len()), pushes, "no rename is pushed");

    // remote edits and new notes land in the NFD file and folder, not in NFC twins
    a.write(&nfc_path, "1\n2\n3 from A\n");
    a.write(&format!("{DIR_NFC}/new.md"), "n\n");
    a.sync(&server);
    let r = b.sync(&server);
    assert_eq!(r.renamed, 0);
    assert_eq!(b.read(&nfd_path).unwrap(), "1\n2\n3 from A\n");
    assert_eq!(b.read(&format!("{DIR_NFD}/new.md")).unwrap(), "n\n");
    assert!(!b.abs(DIR_NFC).exists());

    // a local edit is pushed as content only; the server keeps its path
    b.write(&nfd_path, "1 from B\n2\n3 from A\n");
    b.sync(&server);
    let last = server.with(|s| s.pushes.last().unwrap().clone());
    assert_eq!((last.len(), last[0].path.clone()), (1, None));
    assert!(server.live().contains_key(&nfc_path));
    a.sync(&server);
    assert_eq!(a.read(&nfc_path).unwrap(), "1 from B\n2\n3 from A\n");

    // the edit and the re-encoding of the name may happen together: still the same note, no rename
    b.rename(&nfd_path, &format!("{DIR_NFD}/{CAFE_NFC}.md"));
    b.write(&format!("{DIR_NFD}/{CAFE_NFC}.md"), "edited again\n");
    b.sync(&server);
    let last = server.with(|s| s.pushes.last().unwrap().clone());
    assert_eq!((last.len(), last[0].path.clone(), last[0].base_version), (1, None, 3));
}

#[test]
fn a_case_change_with_another_normalisation_is_a_rename() {
    let server = FakeServer::new();
    let a = Device::new("devA");
    let b = Device::new("devB");
    a.write(&format!("{CAFE_NFC}.md"), "c\n");
    converge(&server, &[&a, &b]);
    let id = note_id(&server, &format!("{CAFE_NFC}.md"));

    // decomposed and capitalised: a real (case) rename, sent with the local name as it is
    let upper_nfd = "CAFE\u{301}.md";
    b.rename(&format!("{CAFE_NFC}.md"), upper_nfd);
    b.sync(&server);
    assert_eq!(note_id(&server, upper_nfd), id);
    let r = a.sync(&server);
    assert_eq!(r.renamed, 1);
    assert_eq!(a.notes(), map(&[(upper_nfd, "c\n")]));

    // only the encoding changes back (case kept): no rename travels
    let upper_nfc = "CAF\u{c9}.md";
    a.rename(upper_nfd, upper_nfc);
    let pushes = server.with(|s| s.pushes.len());
    let r = a.sync(&server);
    assert_eq!((r.pushed, server.with(|s| s.pushes.len())), (0, pushes));
    // a remote change of just the encoding does not move B's file either
    server.push_as_other(vec![PushChange {
        op_id: "reencode".into(),
        note_id: id.clone(),
        op: Op::Upsert,
        base_version: 2,
        path: Some(upper_nfc.into()),
        content: None,
    }]);
    let r = b.sync(&server);
    assert_eq!(r.renamed, 0);
    assert_eq!(b.notes(), map(&[(upper_nfd, "c\n")]));
}

#[test]
fn names_that_differ_only_in_normalisation_are_one_path() {
    // two different local files whose names differ only in normalisation: the second is moved aside before sending
    let server = FakeServer::new();
    let a = Device::new("devA");
    a.write(&format!("{CAFE_NFC}.md"), "composed\n");
    a.write(&format!("{CAFE_NFD}.md"), "decomposed\n");
    let r = a.sync(&server);
    assert_eq!((r.rounds, r.pushed), (1, 2), "no path_taken round trip");
    let live = server.live();
    assert_eq!(live.len(), 2);
    assert_eq!(live.values().filter(|t| t.as_str() == "composed\n" || t.as_str() == "decomposed\n").count(), 2);
    assert!(live.keys().any(|k| k.contains("(local)")));

    // a remote note and an untracked local file of the same key but other text: moved aside while pulling
    let b = Device::new("devB");
    b.write(&format!("{CAFE_NFD}.md"), "mine\n");
    let r = b.sync(&server);
    assert_eq!(r.rounds, 1);
    assert!(b.notes().values().any(|t| t == "mine\n"));
    let live = converge(&server, &[&a, &b]);
    assert_eq!(live.len(), 3);

    // same key and the same text: adopted under the local name
    let c = Device::new("devC");
    let text = live.iter().find(|(k, _)| !k.contains("(local)")).map(|(_, v)| v.clone()).unwrap();
    let plain = live.keys().find(|k| !k.contains("(local)")).unwrap().clone();
    let other_form = if plain.contains(CAFE_NFC) { plain.replace(CAFE_NFC, CAFE_NFD) } else { plain.replace(CAFE_NFD, CAFE_NFC) };
    c.write(&other_form, &text);
    let r = c.sync(&server);
    assert_eq!(r.pushed, 0);
    assert!(c.read(&other_form).is_some());
    assert_eq!(c.notes().len(), 3);
}

#[test]
fn case_only_rename_travels() {
    let server = FakeServer::new();
    let a = Device::new("devA");
    let b = Device::new("devB");
    a.write("notes/readme.md", "hi\n");
    converge(&server, &[&a, &b]);
    let id = note_id(&server, "notes/readme.md");
    a.rename("notes/readme.md", "notes/README.md");
    a.sync(&server);
    assert_eq!(note_id(&server, "notes/README.md"), id, "same note, new path");
    let r = b.sync(&server);
    assert_eq!(r.renamed, 1);
    assert_eq!(converge(&server, &[&a, &b]), map(&[("notes/README.md", "hi\n")]));
}

#[test]
fn paths_with_spaces_unicode_and_subfolders() {
    let server = FakeServer::new();
    let a = Device::new("devA");
    let b = Device::new("devB");
    a.write("My Notes/Ünïcödé café ☕.md", "über\n");
    a.write("deep/er/still deeper/x y.markdown", "deep\n");
    a.write("日本語/メモ.md", "こんにちは\n");
    let live = converge(&server, &[&a, &b]);
    assert_eq!(live.len(), 3);
    assert_eq!(b.read("My Notes/Ünïcödé café ☕.md").unwrap(), "über\n");
    b.rename("日本語/メモ.md", "日本語/メモ 2.md");
    b.sync(&server);
    a.sync(&server);
    assert_eq!(a.read("日本語/メモ 2.md").unwrap(), "こんにちは\n");
}

// ---------------------------------------------------------------------------
// failures and recovery
// ---------------------------------------------------------------------------

#[test]
fn a_lost_push_answer_converges_without_duplicates() {
    let server = FakeServer::new();
    let a = Device::new("devA");
    let b = Device::new("devB");
    a.write("new.md", "new\n");
    a.write("old.md", "old\n");
    a.sync(&server);

    a.write("old.md", "old, edited\n");
    a.write("fresh.md", "fresh\n");
    server.with(|s| s.lose_next_answer = true);
    let err = a.try_sync(&server).unwrap_err();
    assert!(matches!(err.error, crate::SyncError::Remote(RemoteError::Network(_))));
    assert_eq!(server.live().len(), 3, "the server applied the push");

    let r = a.sync(&server);
    assert!(r.conflicts.is_empty());
    assert_eq!(r.renamed, 0, "the own note is recognised, not moved aside");
    let live = converge(&server, &[&a, &b]);
    assert_eq!(live, map(&[("fresh.md", "fresh\n"), ("new.md", "new\n"), ("old.md", "old, edited\n")]));
    let versions: Vec<u64> = server.with(|s| s.notes.values().map(|n| n.version).collect());
    assert_eq!(versions.iter().max(), Some(&2), "no extra versions");
}

#[test]
fn a_lost_create_answer_followed_by_a_remote_delete_settles() {
    let server = FakeServer::new();
    let a = Device::new("devA");
    let b = Device::new("devB");
    a.write("first.md", "1\n");
    a.sync(&server);
    a.write("short-lived.md", "x\n");
    server.with(|s| s.lose_next_answer = true);
    assert!(a.try_sync(&server).is_err());
    b.sync(&server);
    b.remove("short-lived.md");
    b.sync(&server);
    // A only ever sees the tombstone of its own note: recognised, and the untouched file goes
    let r = a.sync(&server);
    assert_eq!(r.deleted, 1);
    assert_eq!(r.rounds, 1, "no version_mismatch loop");
    assert_eq!(converge(&server, &[&a, &b]), map(&[("first.md", "1\n")]));

    // first push to an empty workspace: the checkpoint is still 0, and the initial download has no tombstones, so the
    // deleted note cannot be recognised; it is sent again under a new id (kept, not lost, and no divergence)
    let server = FakeServer::new();
    let c = Device::new("devC");
    let d = Device::new("devD");
    c.write("again.md", "y\n");
    server.with(|s| s.lose_next_answer = true);
    assert!(c.try_sync(&server).is_err());
    d.sync(&server);
    d.remove("again.md");
    d.sync(&server);
    assert!(server.live().is_empty());
    let r = c.sync(&server);
    assert_eq!(r.pushed, 1);
    assert_eq!(converge(&server, &[&c, &d]), map(&[("again.md", "y\n")]));
}

#[test]
fn a_failed_push_is_resent_with_the_same_op_ids() {
    let server = FakeServer::new();
    let a = Device::new("devA");
    a.write("a.md", "a\n");
    a.sync(&server);
    a.write("a.md", "a2\n");
    a.write("b.md", "b\n");
    server.with(|s| s.fail_next_push = true);
    assert!(a.try_sync(&server).is_err());
    a.sync(&server);
    let (failed, resent) = server.with(|s| {
        let n = s.pushes.len();
        (s.pushes[n - 2].clone(), s.pushes[n - 1].clone())
    });
    let ids = |p: &[PushChange]| p.iter().map(|c| (c.op_id.clone(), c.note_id.clone())).collect::<Vec<_>>();
    assert_eq!(ids(&failed), ids(&resent));

    // the server replays an op id it has seen: same answer, nothing applied twice
    let replay = server.push_as_other(resent.clone());
    assert_eq!(replay.accepted.len(), 2);
    assert_eq!(server.with(|s| s.checkpoint), 3);
}

#[test]
fn notes_with_unsaved_edits_wait() {
    let server = FakeServer::new();
    let a = Device::new("devA");
    let b = Device::new("devB");
    a.write("open.md", "1\n2\n3\n4\n");
    a.write("other.md", "o\n");
    converge(&server, &[&a, &b]);

    a.write("open.md", "1\n2\n3\n4 from A\n");
    a.write("other.md", "o2\n");
    a.write("brand new.md", "n\n");
    a.sync(&server);
    b.unsaved.borrow_mut().extend(["open.md".to_string(), "brand new.md".to_string()]);
    // B also has an unsaved new file in the way of an incoming note
    b.write("brand new.md", "B's draft\n");
    let r = b.sync(&server);
    assert_eq!(r.deferred, vec!["brand new.md".to_string(), "open.md".to_string()]);
    assert_eq!(b.read("open.md").unwrap(), "1\n2\n3\n4\n", "not written while unsaved");
    assert_eq!(b.read("other.md").unwrap(), "o2\n");
    assert_eq!(b.read("brand new.md").unwrap(), "B's draft\n");
    let checkpoint = server.with(|s| s.checkpoint);
    let state = StateStore::new(&b.state).load(WS, &b.root).0;
    assert_eq!(state.checkpoint, checkpoint, "the checkpoint advances anyway");

    // the user saves an edit of their own, then the deferred change is merged in
    b.write("open.md", "1 from B\n2\n3\n4\n");
    b.unsaved.borrow_mut().clear();
    let r = b.sync(&server);
    assert!(r.deferred.is_empty());
    assert_eq!(b.read("open.md").unwrap(), "1 from B\n2\n3\n4 from A\n");
    assert_eq!(b.read("brand new (local).md").unwrap(), "B's draft\n");
    converge(&server, &[&a, &b]);
}

#[test]
fn lost_state_relinks_without_deleting_anything() {
    let server = FakeServer::new();
    let a = Device::new("devA");
    let b = Device::new("devB");
    a.write("a.md", "a\n");
    a.write("sub/b.md", "b\n");
    converge(&server, &[&a, &b]);

    // B loses its state, meanwhile edits a note, adds one; A deletes one and edits another
    fs::remove_dir_all(&b.state).unwrap();
    b.write("a.md", "a, edited on B\n");
    b.write("c.md", "c\n");
    a.remove("sub/b.md");
    a.sync(&server);
    let r = b.sync(&server);
    assert!(r.reset.is_some());
    assert_eq!(r.deleted, 0);
    assert_eq!(b.read("sub/b.md").unwrap(), "b\n", "an unknown tombstone deletes nothing");
    assert_eq!(b.read("a.md").unwrap(), "a\n");
    assert_eq!(b.read("a (local).md").unwrap(), "a, edited on B\n");
    let live = converge(&server, &[&a, &b]);
    assert_eq!(live.len(), 4, "nothing lost: {live:?}");

    // a corrupt state file is set aside the same way
    fs::write(b.state.join(STATE_FILE), "{ not json").unwrap();
    let r = b.sync(&server);
    assert!(r.reset.unwrap().contains("damaged"));
    assert!(b.state.join("state.json.corrupt").exists());
    assert_eq!((r.deleted, r.renamed), (0, 0));
    assert_eq!(converge(&server, &[&a, &b]), live);
}

#[test]
fn not_found_pushes_the_note_again_as_new() {
    let server = FakeServer::new();
    let a = Device::new("devA");
    a.write("x.md", "x\n");
    a.sync(&server);
    let id = note_id(&server, "x.md");
    server.with(|s| s.notes.clear());
    a.write("x.md", "x2\n");
    let r = a.sync(&server);
    assert_eq!(r.rounds, 2);
    assert_ne!(note_id(&server, "x.md"), id);
    assert_eq!(server.live(), map(&[("x.md", "x2\n")]));
}

// ---------------------------------------------------------------------------
// read-only, quarantine, hostile servers
// ---------------------------------------------------------------------------

#[test]
fn a_viewer_pulls_but_does_not_push() {
    let server = FakeServer::new();
    let a = Device::new("devA");
    let mut v = Device::new("viewer");
    v.read_only = true;
    a.write("shared.md", "s\n");
    a.sync(&server);
    v.sync(&server);
    assert_eq!(v.read("shared.md").unwrap(), "s\n");

    v.write("shared.md", "s, edited by the viewer\n");
    v.write("mine.md", "m\n");
    let pushes = server.with(|s| s.pushes.len());
    let r = v.sync(&server);
    assert_eq!(r.not_uploaded, vec!["shared.md".to_string(), "mine.md".to_string()]);
    assert_eq!(server.with(|s| s.pushes.len()), pushes, "nothing sent");
    assert_eq!(server.live(), map(&[("shared.md", "s\n")]));
}

#[test]
fn forbidden_changes_are_quarantined_until_the_file_changes() {
    let server = FakeServer::new();
    let a = Device::new("devA");
    server.with(|s| s.viewer = true);
    a.write("x.md", "x\n");
    let r = a.sync(&server);
    assert_eq!(r.quarantined.len(), 1);
    assert_eq!(r.quarantined[0].reason, "forbidden");
    assert_eq!(r.rounds, 1, "not retried in a loop");
    let pushes = server.with(|s| s.pushes.len());
    let r = a.sync(&server);
    assert_eq!(server.with(|s| s.pushes.len()), pushes, "not retried while unchanged");
    assert_eq!(r.quarantined.len(), 1);

    server.with(|s| s.viewer = false);
    a.write("x.md", "x2\n");
    let r = a.sync(&server);
    assert!(r.quarantined.is_empty());
    assert_eq!(server.live(), map(&[("x.md", "x2\n")]));
}

#[test]
fn notes_over_4_mib_are_quarantined_locally() {
    let server = FakeServer::new();
    let a = Device::new("devA");
    let big = "x".repeat(MAX_NOTE_BYTES as usize + 1);
    a.write("big.md", &big);
    a.write("small.md", "s\n");
    let r = a.sync(&server);
    assert_eq!(r.pushed, 1);
    assert_eq!(r.quarantined.len(), 1);
    assert_eq!((r.quarantined[0].path.as_str(), r.quarantined[0].reason.as_str()), ("big.md", "too_large"));
    let sent: usize = server.with(|s| s.pushes.iter().flatten().filter(|c| c.path.as_deref() == Some("big.md")).count());
    assert_eq!(sent, 0, "never sent");

    a.write("big.md", "small now\n");
    let r = a.sync(&server);
    assert!(r.quarantined.is_empty());
    assert_eq!(server.live().len(), 2);
}

#[test]
fn hostile_remote_paths_are_refused() {
    let server = FakeServer::new();
    let a = Device::new("devA");
    let bad = ["../x.md", "/etc/x.md", "a/../../x.md", "sub/../x.md", ".git/x.md", "node_modules/x.md", "a\\..\\x.md", "x.txt", "a//b.md"];
    server.with(|s| {
        for (i, p) in bad.iter().enumerate() {
            let id = format!("00000000-0000-4000-8000-{i:012}");
            s.checkpoint += 1;
            let seq = s.checkpoint;
            s.notes.insert(id, Note { path: p.to_string(), content: "pwned\n".into(), version: 1, deleted: false, seq });
        }
        s.checkpoint += 1;
        let seq = s.checkpoint;
        s.notes.insert("../../escape".into(), Note { path: "ok.md".into(), content: "x".into(), version: 1, deleted: false, seq });
    });
    let r = a.sync(&server);
    assert_eq!(r.refused.len(), bad.len() + 1);
    assert_eq!(r.written, 0);
    assert!(a.notes().is_empty());
    let parent = a.root.parent().unwrap();
    assert!(!parent.join("x.md").exists());
    assert!(!a.state.join("escape").exists() && !a.state.parent().unwrap().join("escape").exists());
}

#[test]
fn remote_path_rules() {
    for ok in ["a.md", "My Notes/x y.md", "ü/ß.markdown", "a/b/c.MD"] {
        assert!(check_remote_path(ok).is_ok(), "{ok}");
    }
    for bad in ["", "../a.md", "/a.md", "a/./b.md", "a/.hidden.md", "build/a.md", "a.txt", "a:b.md", "C:\\a.md"] {
        assert!(check_remote_path(bad).is_err(), "{bad}");
    }
}
