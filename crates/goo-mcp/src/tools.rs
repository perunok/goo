//! The tools goo offers AI assistants. They work on the notes on disk (through goo-core, inside folders the user
//! has opened in goo) and report every write to the window through [`Host`], so open notes update at once.
//!
//! Safety rules:
//! - Only folders from goo's recent list (picked by the user in the app) are reachable; paths are workspace-relative
//!   and checked with goo-core's path guard (no `..`, symlinks resolved).
//! - Notes with unsaved edits in the window are never written; the assistant is told to ask the user to save first.
//! - Deleting only moves a note to the system trash (recoverable), and is marked destructive so clients ask first.

use goo_core::CoreError;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

/// What the tools need from the running app.
pub trait Host: Send + Sync {
    /// Canonical root of the folder open in the window, if any.
    fn open_root(&self) -> Option<PathBuf>;
    /// Folders the user opened in goo, most recent first: the only ones tools may touch.
    fn workspaces(&self) -> Vec<PathBuf>;
    /// Notes with unsaved edits in the window (canonical paths).
    fn unsaved(&self) -> Vec<PathBuf>;
    /// Notes were created or changed on disk.
    fn changed(&self, root: &Path, files: &[PathBuf]);
    /// Show these in the window: the first path decides the folder (a folder, or a note's own folder), the notes
    /// among them open as tabs.
    fn open_in_app(&self, files: Vec<PathBuf>);
    /// A tool is running (for the status bar).
    fn activity(&self, _tool: &str) {}
    /// The note on screen as the user sees it: `{path, text, dirty, selection}` (path absolute), or null when no note
    /// is open. Answered by the window.
    fn open_note(&self) -> Result<Value, String> {
        Err("goo's window is not available".into())
    }
    /// For each Mermaid source: None when it renders, else the error. Answered by the window (goo's own renderer).
    fn check_mermaid(&self, _sources: &[String]) -> Result<Vec<Option<String>>, String> {
        Err("goo's window is not available".into())
    }
    /// goo cloud state of a folder (`{linked: false}` when it does not sync).
    fn sync_status(&self, _root: &Path) -> Result<Value, String> {
        Ok(json!({ "linked": false }))
    }
    /// Moves a file to the system trash (the app); tests remove it.
    fn trash(&self, _path: &Path) -> std::io::Result<()> {
        Err(std::io::Error::other("deleting is not available"))
    }
    /// A note was renamed or moved: open tabs follow it.
    fn moved(&self, root: &Path, from: &Path, to: &Path) {
        self.changed(root, &[from.to_path_buf(), to.to_path_buf()]);
    }
    /// Syncs a linked folder now and waits for the result (same shape as `sync_status`).
    fn sync_now(&self, _root: &Path) -> Result<Value, String> {
        Err("This folder does not sync with goo cloud.".into())
    }
}

/// Longest note text returned by `read_note` (characters).
pub const READ_LIMIT: usize = 200_000;

pub struct ToolSpec {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub read_only: bool,
    pub destructive: bool,
    /// Talks to something outside this computer (goo cloud).
    pub open_world: bool,
    pub input: fn() -> Value,
}

const WORKSPACE_PROP: &str = "Folder to use: its name or full path, as listed by list_workspaces. Default: the folder open in goo.";
const PATH_PROP: &str = "Note path relative to the folder, with / separators, e.g. \"projects/plan.md\".";

fn schema(props: Value, required: &[&str]) -> Value {
    let mut props = props;
    props.as_object_mut().unwrap().insert("workspace".into(), json!({ "type": "string", "description": WORKSPACE_PROP }));
    json!({ "type": "object", "properties": props, "required": required, "additionalProperties": false })
}

pub fn tools() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "list_workspaces",
            title: "List folders",
            description: "Folders of notes the user has opened in goo (only these are reachable), and which one is open now.",
            read_only: true,
            destructive: false,
            open_world: false,
            input: || json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        },
        ToolSpec {
            name: "list_notes",
            title: "List notes",
            description: "Every Markdown note in a folder, as relative paths.",
            read_only: true,
            destructive: false,
            open_world: false,
            input: || schema(json!({}), &[]),
        },
        ToolSpec {
            name: "read_note",
            title: "Read a note",
            description: "The Markdown text of a note as saved on disk.",
            read_only: true,
            destructive: false,
            open_world: false,
            input: || schema(json!({ "path": { "type": "string", "description": PATH_PROP } }), &["path"]),
        },
        ToolSpec {
            name: "search_notes",
            title: "Search notes",
            description: "Case-insensitive search in note names and text. Returns matching lines with line numbers (line 0 = the file name matched).",
            read_only: true,
            destructive: false,
            open_world: false,
            input: || {
                schema(
                    json!({
                        "query": { "type": "string", "description": "Text to look for." },
                        "limit": { "type": "integer", "minimum": 1, "maximum": 200, "description": "Most hits to return (default 50)." }
                    }),
                    &["query"],
                )
            },
        },
        ToolSpec {
            name: "create_note",
            title: "Create a note",
            description: "Creates a new note (\".md\" is added if missing; missing folders are created). Fails if it already exists. Mermaid diagrams go in ```mermaid code blocks.",
            read_only: false,
            destructive: false,
            open_world: false,
            input: || {
                schema(
                    json!({
                        "path": { "type": "string", "description": PATH_PROP },
                        "content": { "type": "string", "description": "Markdown text. Default: a heading with the note's name." }
                    }),
                    &["path"],
                )
            },
        },
        ToolSpec {
            name: "write_note",
            title: "Replace a note",
            description: "Replaces the whole text of an existing note. Prefer edit_note for small changes. Refused while the note has unsaved edits in goo.",
            read_only: false,
            destructive: true,
            open_world: false,
            input: || {
                schema(
                    json!({
                        "path": { "type": "string", "description": PATH_PROP },
                        "content": { "type": "string", "description": "The new Markdown text." }
                    }),
                    &["path", "content"],
                )
            },
        },
        ToolSpec {
            name: "edit_note",
            title: "Edit a note",
            description: "Replaces an exact piece of text in a note. old_text must appear exactly once unless replace_all is true. Refused while the note has unsaved edits in goo.",
            read_only: false,
            destructive: false,
            open_world: false,
            input: || {
                schema(
                    json!({
                        "path": { "type": "string", "description": PATH_PROP },
                        "old_text": { "type": "string", "description": "Text to find (exact, including whitespace)." },
                        "new_text": { "type": "string", "description": "Replacement text." },
                        "replace_all": { "type": "boolean", "description": "Replace every occurrence (default false)." }
                    }),
                    &["path", "old_text", "new_text"],
                )
            },
        },
        ToolSpec {
            name: "append_to_note",
            title: "Append to a note",
            description: "Adds text at the end of an existing note (on a new line). Refused while the note has unsaved edits in goo.",
            read_only: false,
            destructive: false,
            open_world: false,
            input: || {
                schema(
                    json!({
                        "path": { "type": "string", "description": PATH_PROP },
                        "content": { "type": "string", "description": "Markdown text to add." }
                    }),
                    &["path", "content"],
                )
            },
        },
        ToolSpec {
            name: "open_in_app",
            title: "Show a note in goo",
            description: "Opens a note in the goo window so the user can see it (switching folder if needed).",
            read_only: false,
            destructive: false,
            open_world: false,
            input: || schema(json!({ "path": { "type": "string", "description": PATH_PROP } }), &["path"]),
        },
        ToolSpec {
            name: "move_note",
            title: "Rename or move a note",
            description: "Renames and/or moves a note to a new path in the same folder (missing folders are created; \".md\" is added if missing). Fails if the target exists. Refused while the note has unsaved edits in goo.",
            read_only: false,
            destructive: false,
            open_world: false,
            input: || {
                schema(
                    json!({
                        "path": { "type": "string", "description": PATH_PROP },
                        "to": { "type": "string", "description": "The new path relative to the folder, e.g. \"archive/2026/plan.md\"." }
                    }),
                    &["path", "to"],
                )
            },
        },
        ToolSpec {
            name: "delete_note",
            title: "Delete a note (to the trash)",
            description: "Moves a note to the system trash, where the user can restore it. Ask the user before deleting. Refused while the note has unsaved edits in goo.",
            read_only: false,
            destructive: true,
            open_world: false,
            input: || schema(json!({ "path": { "type": "string", "description": PATH_PROP } }), &["path"]),
        },
        ToolSpec {
            name: "get_open_note",
            title: "The note on screen",
            description: "The note open in the goo window as the user sees it, including edits not saved yet, and the selected text (with line numbers) if any. Use it when the user says \"this note\" or \"the selection\".",
            read_only: true,
            destructive: false,
            open_world: false,
            input: || json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        },
        ToolSpec {
            name: "check_mermaid",
            title: "Check Mermaid diagrams",
            description: "Checks that Mermaid diagrams render in goo (with goo's own renderer): either the ```mermaid blocks of a note, or a diagram passed as code. Returns each diagram's line and error, if any. Use it after writing or changing diagrams.",
            read_only: true,
            destructive: false,
            open_world: false,
            input: || {
                schema(
                    json!({
                        "path": { "type": "string", "description": "A note whose ```mermaid blocks to check (saved text)." },
                        "code": { "type": "string", "description": "A Mermaid diagram to check, without the ``` fence." }
                    }),
                    &[],
                )
            },
        },
        ToolSpec {
            name: "sync_status",
            title: "goo cloud sync status",
            description: "Whether the folder syncs with goo cloud, when it last synced, and notes that need attention: conflicts (edited on two devices; both versions kept as two notes), notes that could not be synced, and changes waiting for the user to save.",
            read_only: true,
            destructive: false,
            open_world: false,
            input: || schema(json!({}), &[]),
        },
        ToolSpec {
            name: "sync_now",
            title: "Sync with goo cloud now",
            description: "Syncs the folder with goo cloud now and waits for the result (up to about a minute). Notes written with these tools are synced soon anyway; use this when the user wants it done before continuing.",
            read_only: false,
            destructive: false,
            open_world: true,
            input: || schema(json!({}), &[]),
        },
    ]
}

/// The ```mermaid blocks of a note: (1-based line of the opening fence, diagram source).
pub fn mermaid_blocks(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut current: Option<(usize, String, String)> = None; // (line, fence, body)
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        match &mut current {
            None => {
                let fence: String = trimmed.chars().take_while(|c| *c == '`' || *c == '~').collect();
                let same = fence.chars().all(|c| c == fence.chars().next().unwrap_or('`'));
                if fence.len() >= 3 && same && trimmed[fence.len()..].trim().split_whitespace().next().is_some_and(|l| l.eq_ignore_ascii_case("mermaid")) {
                    current = Some((i + 1, fence, String::new()));
                }
            }
            Some((start, fence, body)) => {
                if trimmed.starts_with(fence.as_str()) && trimmed[fence.len()..].trim().is_empty() {
                    out.push((*start, std::mem::take(body)));
                    current = None;
                } else {
                    body.push_str(line);
                    body.push('\n');
                }
            }
        }
    }
    out
}

/// A tool's answer: text for the model plus, when useful, a structured copy.
pub struct ToolOutput {
    pub text: String,
    pub structured: Option<Value>,
}

impl ToolOutput {
    fn data(v: Value) -> ToolOutput {
        ToolOutput { text: serde_json::to_string_pretty(&v).unwrap_or_default(), structured: Some(v) }
    }
}

pub type ToolResult = Result<ToolOutput, String>;

fn err(e: CoreError) -> String {
    e.message
}

fn obj(args: &Value) -> Result<&Map<String, Value>, String> {
    match args {
        Value::Object(m) => Ok(m),
        Value::Null => {
            static EMPTY: std::sync::OnceLock<Map<String, Value>> = std::sync::OnceLock::new();
            Ok(EMPTY.get_or_init(Map::new))
        }
        _ => Err("arguments must be an object".into()),
    }
}

fn opt_str<'a>(a: &'a Map<String, Value>, key: &str) -> Result<Option<&'a str>, String> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(_) => Err(format!("\"{key}\" must be a string")),
    }
}

fn req_str<'a>(a: &'a Map<String, Value>, key: &str) -> Result<&'a str, String> {
    opt_str(a, key)?.ok_or_else(|| format!("\"{key}\" is required"))
}

/// The folder a call works in: the named one from the recent list, or the open one.
fn workspace(host: &dyn Host, a: &Map<String, Value>) -> Result<PathBuf, String> {
    let known = host.workspaces();
    match opt_str(a, "workspace")? {
        None => host.open_root().ok_or_else(|| "No folder is open in goo. Pass \"workspace\" (see list_workspaces) or ask the user to open one.".to_string()),
        Some(w) => {
            if let Some(p) = known.iter().find(|p| p.to_string_lossy() == w) {
                return Ok(p.clone());
            }
            let by_name: Vec<_> = known.iter().filter(|p| goo_core::workspace_name(p).eq_ignore_ascii_case(w)).collect();
            match by_name.as_slice() {
                [one] => Ok((*one).clone()),
                [] => Err(format!("\"{w}\" is not one of the folders opened in goo (see list_workspaces). Ask the user to open it in goo first.")),
                _ => Err(format!("Several folders are called \"{w}\"; pass the full path from list_workspaces.")),
            }
        }
    }
}

fn refuse_unsaved(host: &dyn Host, root: &Path, file: &Path) -> Result<(), String> {
    if host.unsaved().iter().any(|p| p == file) {
        return Err(format!(
            "{} has unsaved changes in goo. Ask the user to save or discard them first.",
            goo_core::relative_string(root, file)
        ));
    }
    Ok(())
}

fn check_size(text: &str) -> Result<(), String> {
    if text.len() as u64 > goo_core::MAX_FILE_BYTES {
        return Err("The text is larger than 16 MB.".into());
    }
    Ok(())
}

pub fn call(host: &dyn Host, name: &str, args: &Value) -> ToolResult {
    let a = obj(args)?;
    host.activity(name);
    match name {
        "list_workspaces" => {
            let open = host.open_root();
            let list: Vec<Value> = host
                .workspaces()
                .iter()
                .map(|p| json!({ "name": goo_core::workspace_name(p), "path": p.to_string_lossy(), "open": open.as_deref() == Some(p.as_path()) }))
                .collect();
            Ok(ToolOutput::data(json!({ "workspaces": list })))
        }
        "list_notes" => {
            let root = workspace(host, a)?;
            let (notes, truncated) = goo_core::list_notes(&root).map_err(err)?;
            let notes: Vec<String> = notes.iter().map(|p| goo_core::relative_string(&root, p)).collect();
            Ok(ToolOutput::data(json!({ "workspace": goo_core::workspace_name(&root), "notes": notes, "truncated": truncated })))
        }
        "read_note" => {
            let root = workspace(host, a)?;
            let file = goo_core::resolve_note(&root, req_str(a, "path")?).map_err(err)?;
            let text = goo_core::read_text(&file).map_err(err)?;
            let truncated = text.chars().count() > READ_LIMIT;
            let shown: String = if truncated { text.chars().take(READ_LIMIT).collect() } else { text };
            let rel = goo_core::relative_string(&root, &file);
            let note = if truncated { format!("{shown}\n\n[goo: the note is longer; the rest was cut off]") } else { shown.clone() };
            Ok(ToolOutput { text: note, structured: Some(json!({ "path": rel, "content": shown, "truncated": truncated })) })
        }
        "search_notes" => {
            let root = workspace(host, a)?;
            let limit = match a.get("limit") {
                None | Some(Value::Null) => 50,
                Some(v) => v.as_u64().filter(|n| (1..=200).contains(n)).ok_or("\"limit\" must be 1 to 200")? as usize,
            };
            let hits = goo_core::search_notes(&root, req_str(a, "query")?, limit).map_err(err)?;
            Ok(ToolOutput::data(json!({ "hits": hits })))
        }
        "create_note" => {
            let root = workspace(host, a)?;
            let rel = req_str(a, "path")?;
            let content = match opt_str(a, "content")? {
                Some(c) => c.to_string(),
                None => {
                    let name = rel.rsplit(['/', '\\']).next().unwrap_or(rel);
                    let stem = name.strip_suffix(".md").unwrap_or(name);
                    format!("# {stem}\n\n")
                }
            };
            check_size(&content)?;
            let file = goo_core::create_note_at(&root, rel, &content).map_err(err)?;
            host.changed(&root, std::slice::from_ref(&file));
            Ok(ToolOutput::data(json!({ "created": goo_core::relative_string(&root, &file) })))
        }
        "write_note" => {
            let root = workspace(host, a)?;
            let file = goo_core::resolve_note(&root, req_str(a, "path")?).map_err(err)?;
            let content = req_str(a, "content")?;
            check_size(content)?;
            refuse_unsaved(host, &root, &file)?;
            goo_core::write_atomic(&file, content).map_err(err)?;
            host.changed(&root, std::slice::from_ref(&file));
            Ok(ToolOutput::data(json!({ "written": goo_core::relative_string(&root, &file), "bytes": content.len() })))
        }
        "edit_note" => {
            let root = workspace(host, a)?;
            let file = goo_core::resolve_note(&root, req_str(a, "path")?).map_err(err)?;
            let old = req_str(a, "old_text")?;
            let new = req_str(a, "new_text")?;
            let all = a.get("replace_all").and_then(Value::as_bool).unwrap_or(false);
            if old.is_empty() {
                return Err("\"old_text\" is empty".into());
            }
            refuse_unsaved(host, &root, &file)?;
            let text = goo_core::read_text(&file).map_err(err)?;
            let count = text.matches(old).count();
            let updated = match (count, all) {
                (0, _) => return Err("old_text was not found in the note (it must match exactly, including whitespace).".into()),
                (1, _) | (_, true) => text.replace(old, new),
                (n, false) => return Err(format!("old_text appears {n} times; add surrounding text to make it unique, or set replace_all.")),
            };
            check_size(&updated)?;
            goo_core::write_atomic(&file, &updated).map_err(err)?;
            host.changed(&root, std::slice::from_ref(&file));
            Ok(ToolOutput::data(json!({ "edited": goo_core::relative_string(&root, &file), "replacements": count })))
        }
        "append_to_note" => {
            let root = workspace(host, a)?;
            let file = goo_core::resolve_note(&root, req_str(a, "path")?).map_err(err)?;
            let addition = req_str(a, "content")?;
            refuse_unsaved(host, &root, &file)?;
            let mut text = goo_core::read_text(&file).map_err(err)?;
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(addition);
            if !text.ends_with('\n') {
                text.push('\n');
            }
            check_size(&text)?;
            goo_core::write_atomic(&file, &text).map_err(err)?;
            host.changed(&root, std::slice::from_ref(&file));
            Ok(ToolOutput::data(json!({ "appended": goo_core::relative_string(&root, &file) })))
        }
        "open_in_app" => {
            let root = workspace(host, a)?;
            let file = goo_core::resolve_note(&root, req_str(a, "path")?).map_err(err)?;
            // the folder first: goo opens that folder (not the note's own subfolder) and the note in it
            host.open_in_app(vec![root.clone(), file.clone()]);
            Ok(ToolOutput::data(json!({ "opened": goo_core::relative_string(&root, &file) })))
        }
"move_note" => {
            let root = workspace(host, a)?;
            let file = goo_core::resolve_note(&root, req_str(a, "path")?).map_err(err)?;
            refuse_unsaved(host, &root, &file)?;
            let to = goo_core::move_note_to(&root, &file, req_str(a, "to")?).map_err(err)?;
            host.moved(&root, &file, &to);
            Ok(ToolOutput::data(json!({
                "moved": goo_core::relative_string(&root, &file),
                "to": goo_core::relative_string(&root, &to)
            })))
        }
        "delete_note" => {
            let root = workspace(host, a)?;
            let file = goo_core::resolve_note(&root, req_str(a, "path")?).map_err(err)?;
            refuse_unsaved(host, &root, &file)?;
            let gone = goo_core::delete_entry(&root, &file, |p| host.trash(p)).map_err(err)?;
            host.changed(&root, std::slice::from_ref(&gone));
            Ok(ToolOutput::data(json!({ "deleted": goo_core::relative_string(&root, &gone), "where": "the system trash" })))
        }
        "get_open_note" => {
            let note = host.open_note()?;
            let Some(path) = note.get("path").and_then(Value::as_str).map(PathBuf::from) else {
                return Ok(ToolOutput::data(json!({ "open": false, "message": "No note is open in goo." })));
            };
            // only notes inside folders the user opened in goo
            let known = host.workspaces();
            let Some(root) = known.iter().find(|r| path.starts_with(r)) else {
                return Err("The open note is outside the folders opened in goo.".into());
            };
            let mut data = note.clone();
            data["open"] = json!(true);
            data["path"] = json!(goo_core::relative_string(root, &path));
            data["workspace"] = json!(goo_core::workspace_name(root));
            Ok(ToolOutput::data(data))
        }
        "check_mermaid" => {
            let (sources, lines): (Vec<String>, Vec<Option<usize>>) = match (opt_str(a, "code")?, opt_str(a, "path")?) {
                (Some(code), None) => (vec![code.to_string()], vec![None]),
                (None, Some(path)) => {
                    let root = workspace(host, a)?;
                    let file = goo_core::resolve_note(&root, path).map_err(err)?;
                    let text = goo_core::read_text(&file).map_err(err)?;
                    let blocks = mermaid_blocks(&text);
                    if blocks.is_empty() {
                        return Ok(ToolOutput::data(json!({ "diagrams": [], "message": "The note has no ```mermaid blocks." })));
                    }
                    blocks.into_iter().map(|(line, src)| (src, Some(line))).unzip()
                }
                _ => return Err("Pass either \"path\" (a note) or \"code\" (a diagram).".into()),
            };
            let results = host.check_mermaid(&sources)?;
            let diagrams: Vec<Value> = results
                .iter()
                .zip(lines)
                .map(|(r, line)| json!({ "line": line, "ok": r.is_none(), "error": r }))
                .collect();
            let all_ok = results.iter().all(Option::is_none);
            Ok(ToolOutput::data(json!({ "all_ok": all_ok, "diagrams": diagrams })))
        }
        "sync_status" => {
            let root = workspace(host, a)?;
            Ok(ToolOutput::data(host.sync_status(&root)?))
        }
        "sync_now" => {
            let root = workspace(host, a)?;
            Ok(ToolOutput::data(host.sync_now(&root)?))
        }
        other => Err(format!("Unknown tool \"{other}\"")),
    }
}
