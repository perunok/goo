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

const MARKDOWN_EXTENSIONS: &[&str] = &["md", "markdown", "mdown", "mkd"];
const SKIPPED_DIRS: &[&str] = &["node_modules", "target", "dist", "build", "__pycache__"];

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

fn is_hidden(name: &str) -> bool {
    name.starts_with('.')
}

/// Markdown files under `root`, folders first, both sorted case-insensitively. Hidden entries, build/dependency
/// folders and folders without any Markdown inside are left out. Symlinked folders are not followed (no cycles).
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
            if !children.is_empty() {
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

/// Display name of a workspace: its folder name.
pub fn workspace_name(root: &Path) -> String {
    root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path_string(root))
}

#[cfg(test)]
mod tests;
