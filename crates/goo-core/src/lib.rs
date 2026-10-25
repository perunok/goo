//! goo's file logic. Everything the app does on disk goes through here, and every path coming from the UI is
//! checked against the open workspace root first (`resolve_inside`), so a bug in the UI cannot read or write
//! outside the folder the user picked.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};

/// Files larger than this are refused (goo is a notes editor, not a log viewer).
pub const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
/// The tree walk stops after this many entries so a huge folder (a home directory) cannot freeze the app.
pub const MAX_TREE_ENTRIES: usize = 20_000;
/// How many recent workspaces are remembered.
pub const MAX_RECENTS: usize = 12;

/// What counts as a note. Public for the file picker's filter.
pub const MARKDOWN_EXTENSIONS: &[&str] = &["md", "markdown", "mdown", "mkd"];
/// Folders the tree leaves out (with hidden entries). Public so sync can refuse remote paths the tree would not show.
pub const SKIPPED_DIRS: &[&str] = &["node_modules", "target", "dist", "build", "__pycache__"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    NotFound,
    OutsideWorkspace,
    NoWorkspace,
    InvalidName,
    AlreadyExists,
    TooLarge,
    NotText,
    Io,
}

#[derive(Debug, Clone, Serialize)]
pub struct CoreError {
    pub code: ErrorCode,
    pub message: String,
}

impl CoreError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CoreError {}

impl From<io::Error> for CoreError {
    fn from(e: io::Error) -> Self {
        let code = match e.kind() {
            io::ErrorKind::NotFound => ErrorCode::NotFound,
            io::ErrorKind::AlreadyExists => ErrorCode::AlreadyExists,
            _ => ErrorCode::Io,
        };
        Self::new(code, e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, CoreError>;

// ---------------------------------------------------------------------------
// Workspace tree
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    Dir,
    File,
}

#[derive(Debug, Clone, Serialize)]
pub struct TreeNode {
    pub name: String,
    pub path: String,
    pub kind: NodeKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<TreeNode>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Tree {
    pub nodes: Vec<TreeNode>,
    /// True when the walk stopped at `MAX_TREE_ENTRIES`.
    pub truncated: bool,
}

pub fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| MARKDOWN_EXTENSIONS.iter().any(|m| m.eq_ignore_ascii_case(e)))
        .unwrap_or(false)
}

pub fn is_hidden(name: &str) -> bool {
    name.starts_with('.')
}

/// Markdown files under `root`, folders first, both sorted case-insensitively. Hidden entries, build/dependency
/// folders and folders that hold only other kinds of files are left out (empty folders are listed). Symlinked folders are not followed (no cycles).
pub fn list_tree(root: &Path) -> Result<Tree> {
    let mut budget = MAX_TREE_ENTRIES;
    let nodes = walk(root, &mut budget)?;
    Ok(Tree { nodes, truncated: budget == 0 })
}

fn walk(dir: &Path, budget: &mut usize) -> Result<Vec<TreeNode>> {
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    for entry in fs::read_dir(dir)? {
        if *budget == 0 {
            break;
        }
        let Ok(entry) = entry else { continue };
        let name = entry.file_name().to_string_lossy().into_owned();
        if is_hidden(&name) {
            continue;
        }
        let Ok(ft) = entry.file_type() else { continue };
        let path = entry.path();
        *budget -= 1;
        if ft.is_dir() {
            if SKIPPED_DIRS.contains(&name.as_str()) {
                continue;
            }
            // An unreadable sub-folder should not hide the rest of the tree.
            let children = walk(&path, budget).unwrap_or_default();
            // Folders with notes, and truly empty folders (just made in goo, waiting for a first note). Folders holding
            // only other files (images, PDFs) stay hidden.
            let empty = fs::read_dir(&path).map(|mut d| d.next().is_none()).unwrap_or(false);
            if !children.is_empty() || empty {
                dirs.push(TreeNode { name, path: path_string(&path), kind: NodeKind::Dir, children: Some(children) });
            }
        } else if (ft.is_file() || ft.is_symlink()) && is_markdown(&path) {
            files.push(TreeNode { name, path: path_string(&path), kind: NodeKind::File, children: None });
        }
    }
    let key = |n: &TreeNode| n.name.to_lowercase();
    dirs.sort_by_key(key);
    files.sort_by_key(key);
    dirs.extend(files);
    Ok(dirs)
}

fn path_string(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// What a file-system change under `root` means for the app (folder watching). Only changes the tree could show
/// matter: Markdown files and folders, not inside hidden or build/dependency folders. goo's own temp files
/// (`.name.goo-<pid>.tmp`) are hidden and so ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchedChange {
    /// A note changed (or appeared, or went away): reload it if open, refresh the tree.
    Note,
    /// Something else that may change the tree (a folder appeared or went away).
    Tree,
    /// Not shown by goo.
    Ignore,
}

pub fn classify_change(root: &Path, path: &Path) -> WatchedChange {
    let Ok(rel) = path.strip_prefix(root) else { return WatchedChange::Ignore };
    let parts: Vec<String> = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    let Some((last, dirs)) = parts.split_last() else { return WatchedChange::Ignore };
    if dirs.iter().any(|d| is_hidden(d) || SKIPPED_DIRS.contains(&d.as_str())) || is_hidden(last) {
        return WatchedChange::Ignore;
    }
    if is_markdown(path) {
        WatchedChange::Note
    } else if path.is_dir() || path.extension().is_none() {
        // a folder (or one that was just removed, so it can no longer be checked)
        if SKIPPED_DIRS.contains(&last.as_str()) { WatchedChange::Ignore } else { WatchedChange::Tree }
    } else {
        WatchedChange::Ignore
    }
}

// ---------------------------------------------------------------------------
// Path guarding
// ---------------------------------------------------------------------------

/// The canonical form of a workspace root; it must be an existing directory.
pub fn canonical_root(root: &Path) -> Result<PathBuf> {
    let c = fs::canonicalize(root)?;
    if !c.is_dir() {
        return Err(CoreError::new(ErrorCode::NotFound, format!("{} is not a folder", root.display())));
    }
    Ok(c)
}

/// Canonicalises an existing `path` and checks it lies inside `root` (which must already be canonical).
/// Symlinks are resolved first, so a link pointing outside the workspace is refused too.
pub fn resolve_inside(root: &Path, path: &Path) -> Result<PathBuf> {
    let c = fs::canonicalize(path)?;
    if c.starts_with(root) {
        Ok(c)
    } else {
        Err(CoreError::new(ErrorCode::OutsideWorkspace, format!("{} is outside the open folder", path.display())))
    }
}

/// A single path component a user may type as a file or folder name.
pub fn validate_name(name: &str) -> Result<&str> {
    let n = name.trim();
    let bad = n.is_empty()
        || n.len() > 200
        || n == "."
        || n == ".."
        || n.chars().any(|c| matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control());
    if bad || Path::new(n).components().count() != 1 || !matches!(Path::new(n).components().next(), Some(Component::Normal(_))) {
        return Err(CoreError::new(ErrorCode::InvalidName, format!("\"{name}\" is not a valid name")));
    }
    Ok(n)
}

/// Where a new note called `name` would go in `dir` (inside `root`). `.md` is appended unless the name already has
/// a Markdown extension. Fails if the file exists.
pub fn new_file_path(root: &Path, dir: &Path, name: &str) -> Result<PathBuf> {
    let name = validate_name(name)?;
    let dir = resolve_inside(root, dir)?;
    let mut file = dir.join(name);
    if !is_markdown(&file) {
        file = dir.join(format!("{name}.md"));
    }
    if file.exists() {
        return Err(CoreError::new(ErrorCode::AlreadyExists, format!("{} already exists", file.display())));
    }
    Ok(file)
}

// ---------------------------------------------------------------------------
// Workspace-relative paths (what AI assistants use)
// ---------------------------------------------------------------------------

/// Checks a workspace-relative path like `projects/plan.md`: relative, every part a valid name (no `..`, no
/// separators tricks). Accepts `/` and `\\` as separators.
pub fn relative_parts(rel: &str) -> Result<Vec<&str>> {
    let bad = || CoreError::new(ErrorCode::InvalidName, format!("\"{rel}\" is not a path inside the folder (use e.g. notes/plan.md)"));
    let trimmed = rel.trim();
    if trimmed.is_empty() || trimmed.starts_with('/') || trimmed.starts_with('\\') || Path::new(trimmed).is_absolute() {
        return Err(bad());
    }
    let parts: Vec<&str> = trimmed.split(['/', '\\']).filter(|p| !p.is_empty() && *p != ".").collect();
    if parts.is_empty() {
        return Err(bad());
    }
    for p in &parts {
        validate_name(p).map_err(|_| bad())?;
    }
    Ok(parts)
}

/// An existing note inside `root`, from a workspace-relative path.
pub fn resolve_note(root: &Path, rel: &str) -> Result<PathBuf> {
    let parts = relative_parts(rel)?;
    let path = parts.iter().fold(root.to_path_buf(), |p, part| p.join(part));
    let resolved = resolve_inside(root, &path)?;
    if !resolved.is_file() {
        return Err(CoreError::new(ErrorCode::NotFound, format!("{rel} is not a file")));
    }
    Ok(resolved)
}

/// `path` relative to `root` with `/` separators (for tool results).
pub fn relative_string(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// Creates a note at a workspace-relative path, making missing folders on the way. `.md` is appended unless the name
/// already has a Markdown extension. Never overwrites.
pub fn create_note_at(root: &Path, rel: &str, contents: &str) -> Result<PathBuf> {
    let mut parts = relative_parts(rel)?;
    let file_name = parts.pop().unwrap();
    let mut dir = root.to_path_buf();
    for part in parts {
        dir.push(part);
        if !dir.exists() {
            fs::create_dir(&dir)?;
        }
        // a symlinked folder must still point inside the workspace
        dir = resolve_inside(root, &dir)?;
    }
    let file = new_file_path(root, &dir, file_name)?;
    create_new(&file, contents)?;
    Ok(file)
}

/// Every note under `root` (same filters as the tree), as paths.
pub fn list_notes(root: &Path) -> Result<(Vec<PathBuf>, bool)> {
    fn collect(nodes: &[TreeNode], out: &mut Vec<PathBuf>) {
        for n in nodes {
            match &n.children {
                Some(c) => collect(c, out),
                None => out.push(PathBuf::from(&n.path)),
            }
        }
    }
    let tree = list_tree(root)?;
    let mut out = Vec::new();
    collect(&tree.nodes, &mut out);
    Ok((out, tree.truncated))
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SearchHit {
    /// Workspace-relative path.
    pub path: String,
    /// 1-based line number.
    pub line: usize,
    pub text: String,
}

/// Case-insensitive search over the notes' names and text. At most `limit` hits, at most 5 per note; a match in the
/// file name is reported as line 0.
pub fn search_notes(root: &Path, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Err(CoreError::new(ErrorCode::InvalidName, "the search text is empty"));
    }
    let (notes, _) = list_notes(root)?;
    let mut hits = Vec::new();
    for note in notes {
        let rel = relative_string(root, &note);
        let mut per_note = 0;
        if rel.to_lowercase().contains(&q) {
            hits.push(SearchHit { path: rel.clone(), line: 0, text: rel.clone() });
            per_note += 1;
        }
        let Ok(text) = read_text(&note) else { continue };
        for (i, line) in text.lines().enumerate() {
            if hits.len() >= limit || per_note >= 5 {
                break;
            }
            if line.to_lowercase().contains(&q) {
                hits.push(SearchHit { path: rel.clone(), line: i + 1, text: snippet(line) });
                per_note += 1;
            }
        }
        if hits.len() >= limit {
            break;
        }
    }
    Ok(hits)
}

/// A line shortened for a search result.
fn snippet(line: &str) -> String {
    let mut t: String = line.trim().chars().take(240).collect();
    if line.trim().chars().count() > 240 {
        t.push('…');
    }
    t
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LineHit {
    /// 1-based line number.
    pub line: usize,
    pub text: String,
}

/// A note found by `find_notes`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct NoteMatch {
    /// Absolute path.
    pub path: String,
    /// The first lines with any of the keywords (at most 3; none when only the name matched).
    pub lines: Vec<LineHit>,
}

/// Keyword search for the sidebar: the notes (in tree order, at most `limit`) whose name or text contains every
/// whitespace-separated word of `query`, case-insensitively, each anywhere in the note.
pub fn find_notes(root: &Path, query: &str, limit: usize) -> Result<Vec<NoteMatch>> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if words.is_empty() {
        return Err(CoreError::new(ErrorCode::InvalidName, "the search text is empty"));
    }
    let (notes, _) = list_notes(root)?;
    let mut found = Vec::new();
    for note in notes {
        if found.len() >= limit {
            break;
        }
        let Ok(text) = read_text(&note) else { continue };
        let name = relative_string(root, &note).to_lowercase();
        let lower = text.to_lowercase();
        if !words.iter().all(|w| name.contains(w.as_str()) || lower.contains(w.as_str())) {
            continue;
        }
        let lines = text
            .lines()
            .enumerate()
            .filter(|(_, l)| {
                let l = l.to_lowercase();
                words.iter().any(|w| l.contains(w.as_str()))
            })
            .take(3)
            .map(|(i, l)| LineHit { line: i + 1, text: snippet(l) })
            .collect();
        found.push(NoteMatch { path: path_string(&note), lines });
    }
    Ok(found)
}

// ---------------------------------------------------------------------------
// File management: rename, move, new folder, delete
// ---------------------------------------------------------------------------

/// An existing note or folder inside `root`, as a path whose parent is canonical. The entry itself is not resolved, so a
/// symlink is renamed/moved/deleted as a link, never what it points to. The root itself is refused.
pub fn entry_inside(root: &Path, path: &Path) -> Result<PathBuf> {
    let name = path.file_name().ok_or_else(|| CoreError::new(ErrorCode::InvalidName, "that is the folder itself"))?;
    let parent = path.parent().ok_or_else(|| CoreError::new(ErrorCode::InvalidName, "that is the folder itself"))?;
    let entry = resolve_inside(root, parent)?.join(name);
    if entry == root || fs::symlink_metadata(&entry).is_err() {
        return Err(CoreError::new(ErrorCode::NotFound, format!("{} does not exist", path.display())));
    }
    Ok(entry)
}

fn same_entry(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// Renames `from` to `to` (both checked by the caller). `to` must be free, except when it is the same entry spelled
/// differently (a case-only rename on a case-insensitive disk), which goes through a temporary name.
fn rename_checked(from: &Path, to: &Path) -> Result<()> {
    if fs::symlink_metadata(to).is_ok() {
        if !same_entry(from, to) {
            return Err(CoreError::new(ErrorCode::AlreadyExists, format!("{} already exists", to.display())));
        }
        if from.file_name() == to.file_name() {
            return Ok(());
        }
        let tmp = from.with_file_name(format!(".goo-rename-{}", std::process::id()));
        fs::rename(from, &tmp)?;
        fs::rename(&tmp, to)?;
        return Ok(());
    }
    fs::rename(from, to)?;
    Ok(())
}

/// Renames a note or folder where it is. A note keeps a Markdown extension (".md" is added when the new name has
/// none). Returns the new path.
pub fn rename_entry(root: &Path, path: &Path, new_name: &str) -> Result<PathBuf> {
    let from = entry_inside(root, path)?;
    let name = validate_name(new_name)?;
    let mut to = from.with_file_name(name);
    if !from.is_dir() && !is_markdown(&to) {
        to = from.with_file_name(format!("{name}.md"));
    }
    rename_checked(&from, &to)?;
    Ok(to)
}

/// Moves a note or folder into another folder of the workspace (keeping its name). Returns the new path.
pub fn move_entry(root: &Path, path: &Path, dest_dir: &Path) -> Result<PathBuf> {
    let from = entry_inside(root, path)?;
    let dest = resolve_inside(root, dest_dir)?;
    if !dest.is_dir() {
        return Err(CoreError::new(ErrorCode::InvalidName, format!("{} is not a folder", dest_dir.display())));
    }
    if from.is_dir() && dest.starts_with(fs::canonicalize(&from)?) {
        return Err(CoreError::new(ErrorCode::InvalidName, "a folder cannot move into itself"));
    }
    let to = dest.join(from.file_name().expect("entry_inside has a name"));
    if to == from {
        return Ok(to);
    }
    rename_checked(&from, &to)?;
    Ok(to)
}

/// Moves (and/or renames) a note to a workspace-relative path, creating missing folders. Used by AI assistants.
pub fn move_note_to(root: &Path, path: &Path, new_rel: &str) -> Result<PathBuf> {
    let from = entry_inside(root, path)?;
    if from.is_dir() {
        return Err(CoreError::new(ErrorCode::InvalidName, "only notes can be moved this way"));
    }
    let mut parts = relative_parts(new_rel)?;
    let name = parts.pop().expect("relative_parts is never empty");
    let mut dir = root.to_path_buf();
    for part in parts {
        dir.push(part);
        if !dir.exists() {
            fs::create_dir(&dir)?;
        }
        dir = resolve_inside(root, &dir)?;
    }
    let mut to = dir.join(name);
    if !is_markdown(&to) {
        to = dir.join(format!("{name}.md"));
    }
    rename_checked(&from, &to)?;
    Ok(to)
}

/// Makes a new, empty folder in `parent`. Returns its path.
pub fn create_folder(root: &Path, parent: &Path, name: &str) -> Result<PathBuf> {
    let parent = resolve_inside(root, parent)?;
    let dir = parent.join(validate_name(name)?);
    if fs::symlink_metadata(&dir).is_ok() {
        return Err(CoreError::new(ErrorCode::AlreadyExists, format!("{} already exists", dir.display())));
    }
    fs::create_dir(&dir)?;
    Ok(dir)
}

/// Deletes a note or folder of the workspace with `remove` (the app passes the system trash). Never the root.
pub fn delete_entry(root: &Path, path: &Path, remove: impl FnOnce(&Path) -> io::Result<()>) -> Result<PathBuf> {
    let entry = entry_inside(root, path)?;
    remove(&entry).map_err(|e| CoreError::new(ErrorCode::Io, format!("Could not delete {}: {e}", entry.display())))?;
    Ok(entry)
}

// ---------------------------------------------------------------------------
// Reading and writing
// ---------------------------------------------------------------------------

pub fn read_text(path: &Path) -> Result<String> {
    let meta = fs::metadata(path)?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(CoreError::new(ErrorCode::TooLarge, format!("{} is larger than 16 MB", path.display())));
    }
    let bytes = fs::read(path)?;
    String::from_utf8(bytes).map_err(|_| CoreError::new(ErrorCode::NotText, format!("{} is not UTF-8 text", path.display())))
}

/// Writes through a temporary file in the same folder and renames it over the target, so a crash or a full disk
/// never leaves a half-written note behind.
pub fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    let dir = path.parent().ok_or_else(|| CoreError::new(ErrorCode::Io, "file has no parent folder"))?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dir.join(format!(".{name}.goo-{}.tmp", std::process::id()));
    let result = (|| -> io::Result<()> {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(contents.as_bytes())?;
        f.sync_all()?;
        if let Ok(meta) = fs::metadata(path) {
            // keep the original permissions (e.g. a read-only group bit)
            fs::set_permissions(&tmp, meta.permissions())?;
        }
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    Ok(result?)
}

/// Creates a new, empty note; fails if it already exists (no silent overwrite).
pub fn create_new(path: &Path, contents: &str) -> Result<()> {
    let mut f = fs::OpenOptions::new().write(true).create_new(true).open(path)?;
    f.write_all(contents.as_bytes())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Recent workspaces
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Recents {
    pub workspaces: Vec<String>,
}

impl Recents {
    pub fn load(file: &Path) -> Recents {
        fs::read_to_string(file).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save(&self, file: &Path) -> Result<()> {
        if let Some(dir) = file.parent() {
            fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| CoreError::new(ErrorCode::Io, e.to_string()))?;
        write_atomic(file, &json)
    }

    /// Moves `root` to the front (most recent first) and caps the list.
    pub fn push(&mut self, root: &str) {
        self.workspaces.retain(|w| w != root);
        self.workspaces.insert(0, root.to_string());
        self.workspaces.truncate(MAX_RECENTS);
    }

    pub fn contains(&self, root: &str) -> bool {
        self.workspaces.iter().any(|w| w == root)
    }

    pub fn remove(&mut self, root: &str) {
        self.workspaces.retain(|w| w != root);
    }
}

// ---------------------------------------------------------------------------
// Opening from outside ("Open with" in a file manager, `goo notes.md`, macOS open events)
// ---------------------------------------------------------------------------

/// The paths to open from a command line (`argv[0]` is the program): several when a file manager opens a selection
/// ("Open with" passes them all). Flags are skipped, `file://` URIs (what some file managers pass) are decoded,
/// relative paths are resolved against `cwd`, and paths that do not exist are dropped.
pub fn launch_paths(args: &[String], cwd: &Path) -> Vec<PathBuf> {
    args.iter()
        .skip(1)
        .filter(|a| !a.starts_with('-'))
        .filter_map(|a| {
            let p = file_uri_to_path(a).unwrap_or_else(|| PathBuf::from(a));
            let p = if p.is_absolute() { p } else { cwd.join(p) };
            p.exists().then_some(p)
        })
        .collect()
}

/// `file:///home/a/My%20Notes.md` -> `/home/a/My Notes.md`. None for anything that is not a local file URI.
pub fn file_uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    // `file://localhost/x` and `file:///x` are local; `file://host/x` is not.
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    if !rest.starts_with('/') {
        return None;
    }
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = |b: u8| (b as char).to_digit(16);
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    let decoded = String::from_utf8(out).ok()?;
    // Windows: file:///C:/notes/a.md -> C:/notes/a.md
    let decoded = match decoded.as_bytes() {
        [b'/', d, b':', ..] if d.is_ascii_alphabetic() => decoded[1..].to_string(),
        _ => decoded,
    };
    Some(PathBuf::from(decoded))
}

/// What opening paths from outside means for the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenPlan {
    /// The folder to show (canonical).
    pub root: PathBuf,
    /// The notes to open in it (none when a folder was opened).
    pub files: Vec<PathBuf>,
    /// Paths left out: not Markdown, or outside `root` (a selection spread over several folders).
    pub skipped: Vec<PathBuf>,
    /// False when `root` is the folder already open (no switch, no "unsaved changes" question needed).
    pub switch: bool,
}

/// The first path decides the folder: a folder opens as the workspace; a note opens in the current workspace when it
/// lies inside it, otherwise its own folder becomes the workspace. The other paths join as notes when they are
/// Markdown files inside that folder.
pub fn plan_open(current_root: Option<&Path>, paths: &[PathBuf]) -> Result<OpenPlan> {
    let mut canon = Vec::new();
    let mut skipped = Vec::new();
    for p in paths {
        match fs::canonicalize(p) {
            Ok(c) if c.is_dir() || is_markdown(&c) => canon.push(c),
            _ => skipped.push(p.clone()),
        }
    }
    let Some(first) = canon.first().cloned() else {
        let what = paths.first().map(|p| p.display().to_string()).unwrap_or_default();
        return Err(CoreError::new(ErrorCode::InvalidName, format!("{what} is not a Markdown file or a folder")));
    };
    let root = if first.is_dir() {
        first.clone()
    } else if let Some(r) = current_root.filter(|r| first.starts_with(r)) {
        r.to_path_buf()
    } else {
        first.parent().ok_or_else(|| CoreError::new(ErrorCode::NotFound, "file has no parent folder"))?.to_path_buf()
    };
    let mut files = Vec::new();
    for c in canon {
        if c.is_file() && c.starts_with(&root) {
            if !files.contains(&c) {
                files.push(c);
            }
        } else if c != root {
            skipped.push(c);
        }
    }
    let switch = current_root != Some(root.as_path());
    Ok(OpenPlan { root, files, skipped, switch })
}

/// Display name of a workspace: its folder name.
pub fn workspace_name(root: &Path) -> String {
    root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path_string(root))
}

#[cfg(test)]
mod tests;
