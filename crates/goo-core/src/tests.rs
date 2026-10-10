use super::*;
use std::fs;
use tempfile::TempDir;

fn ws() -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = canonical_root(dir.path()).unwrap();
    (dir, root)
}

fn touch(p: &Path, s: &str) {
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, s).unwrap();
}

#[test]
fn tree_lists_markdown_only_folders_first_and_skips_noise() {
    let (_d, root) = ws();
    touch(&root.join("b.md"), "");
    touch(&root.join("A.markdown"), "");
    touch(&root.join("notes.txt"), "");
    touch(&root.join("zeta/inner.md"), "");
    touch(&root.join("alpha/deep/x.md"), "");
    touch(&root.join("empty/readme.txt"), "");
    touch(&root.join(".git/x.md"), "");
    touch(&root.join("node_modules/pkg/README.md"), "");
    touch(&root.join(".hidden.md"), "");

    let tree = list_tree(&root).unwrap();
    assert!(!tree.truncated);
    let names: Vec<_> = tree.nodes.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(names, ["alpha", "zeta", "A.markdown", "b.md"]);
    let alpha = &tree.nodes[0];
    assert_eq!(alpha.kind, NodeKind::Dir);
    let deep = &alpha.children.as_ref().unwrap()[0];
    assert_eq!(deep.name, "deep");
    assert_eq!(deep.children.as_ref().unwrap()[0].name, "x.md");
}

#[test]
fn resolve_inside_refuses_paths_outside_the_root() {
    let (_d, root) = ws();
    touch(&root.join("in.md"), "hi");
    assert!(resolve_inside(&root, &root.join("in.md")).is_ok());

    let outside = tempfile::tempdir().unwrap();
    let secret = outside.path().join("secret.md");
    touch(&secret, "nope");
    let err = resolve_inside(&root, &secret).unwrap_err();
    assert_eq!(err.code, ErrorCode::OutsideWorkspace);
    let err = resolve_inside(&root, &root.join("..").join(outside.path().file_name().unwrap()).join("secret.md"));
    assert!(err.is_err());
}

#[cfg(unix)]
#[test]
fn resolve_inside_refuses_symlinks_that_escape() {
    let (_d, root) = ws();
    let outside = tempfile::tempdir().unwrap();
    let secret = outside.path().join("secret.md");
    touch(&secret, "nope");
    std::os::unix::fs::symlink(&secret, root.join("link.md")).unwrap();
    assert_eq!(resolve_inside(&root, &root.join("link.md")).unwrap_err().code, ErrorCode::OutsideWorkspace);
}

#[test]
fn names_are_single_components() {
    for ok in ["note", "My note.md", "über", "2026-10-10"] {
        assert!(validate_name(ok).is_ok(), "{ok}");
    }
    for bad in ["", "  ", ".", "..", "a/b", "a\\b", "../x", "x:y", "a\nb", "con?"] {
        assert!(validate_name(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn new_file_path_appends_md_and_refuses_existing() {
    let (_d, root) = ws();
    assert_eq!(new_file_path(&root, &root, "todo").unwrap(), root.join("todo.md"));
    assert_eq!(new_file_path(&root, &root, "plan.markdown").unwrap(), root.join("plan.markdown"));
    touch(&root.join("todo.md"), "");
    assert_eq!(new_file_path(&root, &root, "todo").unwrap_err().code, ErrorCode::AlreadyExists);
    assert!(new_file_path(&root, &root, "../escape").is_err());
}

#[test]
fn atomic_write_replaces_content_and_leaves_no_temp_file() {
    let (_d, root) = ws();
    let f = root.join("n.md");
    touch(&f, "old");
    write_atomic(&f, "new ✓").unwrap();
    assert_eq!(read_text(&f).unwrap(), "new ✓");
    let leftovers: Vec<_> = fs::read_dir(&root).unwrap().filter_map(|e| e.ok()).filter(|e| e.file_name() != "n.md").collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn create_new_never_overwrites() {
    let (_d, root) = ws();
    let f = root.join("n.md");
    create_new(&f, "# hi").unwrap();
    assert_eq!(create_new(&f, "again").unwrap_err().code, ErrorCode::AlreadyExists);
    assert_eq!(read_text(&f).unwrap(), "# hi");
}

#[test]
fn read_text_rejects_binary() {
    let (_d, root) = ws();
    let f = root.join("bin.md");
    fs::write(&f, [0xff, 0xfe, 0x00]).unwrap();
    assert_eq!(read_text(&f).unwrap_err().code, ErrorCode::NotText);
    assert_eq!(read_text(&root.join("missing.md")).unwrap_err().code, ErrorCode::NotFound);
}

#[test]
fn recents_are_most_recent_first_unique_and_capped() {
    let (_d, root) = ws();
    let file = root.join("cfg/recent.json");
    let mut r = Recents::default();
    for i in 0..20 {
        r.push(&format!("/w/{i}"));
    }
    r.push("/w/15");
    assert_eq!(r.workspaces.len(), MAX_RECENTS);
    assert_eq!(r.workspaces[0], "/w/15");
    assert_eq!(r.workspaces.iter().filter(|w| *w == "/w/15").count(), 1);
    r.save(&file).unwrap();
    let back = Recents::load(&file);
    assert_eq!(back.workspaces, r.workspaces);
    assert!(Recents::load(&root.join("missing.json")).workspaces.is_empty());
}
