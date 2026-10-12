//! AI assistant access (MCP): runs goo-mcp's endpoint inside the app while it is turned on, implements its `Host`
//! with the app's state, and connects Claude Code. Off by default; Settings > AI assistants turns it on.

use crate::AppState;
use goo_core::Recents;
use goo_mcp::{files, http, protocol, Host};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

pub struct Mcp {
    dir: PathBuf,
    settings: Mutex<files::Settings>,
    token: Arc<RwLock<String>>,
    server: Mutex<Option<http::RunningServer>>,
    error: Mutex<Option<String>>,
}

/// The app as seen by the MCP tools.
struct AppHost(AppHandle);

impl Host for AppHost {
    fn open_root(&self) -> Option<PathBuf> {
        self.0.state::<AppState>().root.lock().unwrap().clone()
    }

    fn workspaces(&self) -> Vec<PathBuf> {
        let state = self.0.state::<AppState>();
        Recents::load(&state.recents_file).workspaces.iter().filter_map(|w| std::fs::canonicalize(w).ok()).filter(|p| p.is_dir()).collect()
    }

    fn unsaved(&self) -> Vec<PathBuf> {
        self.0.state::<AppState>().unsaved.lock().unwrap().clone()
    }

    fn changed(&self, root: &Path, files: &[PathBuf]) {
        let files: Vec<String> = files.iter().map(|f| f.to_string_lossy().into_owned()).collect();
        let _ = self.0.emit("files-changed", json!({ "root": root.to_string_lossy(), "files": files }));
        // a synced folder sends assistant edits to goo cloud right away
        self.0.state::<crate::cloud::Cloud>().trigger(Some(root.to_string_lossy().into_owned()));
    }

    fn open_in_app(&self, files: Vec<PathBuf>) {
        crate::request_open(&self.0, files);
    }

    fn activity(&self, tool: &str) {
        let _ = self.0.emit("mcp-activity", tool);
    }

    fn trash(&self, path: &Path) -> std::io::Result<()> {
        crate::to_trash(path)
    }

    fn moved(&self, root: &Path, from: &Path, to: &Path) {
        let _ = self.0.emit("files-moved", json!({ "root": root.to_string_lossy(), "from": from.to_string_lossy(), "to": to.to_string_lossy() }));
        self.changed(root, &[to.to_path_buf()]);
    }

    fn open_note(&self) -> Result<Value, String> {
        crate::uibridge::ask(&self.0, "open_note", Value::Null, Duration::from_secs(5))
    }

    fn check_mermaid(&self, sources: &[String]) -> Result<Vec<Option<String>>, String> {
        let answer = crate::uibridge::ask(&self.0, "check_mermaid", json!({ "sources": sources }), Duration::from_secs(30))?;
        let list = answer.as_array().ok_or("unexpected answer from the window")?;
        Ok(list.iter().map(|v| v.as_str().map(String::from)).collect())
    }

    fn sync_status(&self, root: &Path) -> Result<Value, String> {
        Ok(self.0.state::<crate::cloud::Cloud>().folder_json(root))
    }

    fn sync_now(&self, root: &Path) -> Result<Value, String> {
        self.0.state::<crate::cloud::Cloud>().sync_and_wait(root, Duration::from_secs(90))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpStatus {
    enabled: bool,
    running: bool,
    port: u16,
    url: Option<String>,
    error: Option<String>,
    token: String,
    /// What assistants run: `<command> --mcp-stdio`.
    command: String,
    args: Vec<String>,
    /// A development build (target/debug): the command only works while `npm run app:dev` serves the UI.
    dev_build: bool,
    claude_code: ClaudeCode,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeCode {
    /// The `claude` command was found.
    found: bool,
    /// "no", "yes", or "outdated" (registered with a different command, e.g. an older AppImage path).
    connected: &'static str,
}

/// The executable assistants should run. An AppImage's own path changes on every launch, so the AppImage is used.
fn command_path() -> String {
    std::env::var("APPIMAGE")
        .ok()
        .or_else(|| std::env::current_exe().ok().map(|p| p.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "goo".into())
}

impl Mcp {
    pub fn new(app_config_dir: &Path) -> Mcp {
        let dir = files::mcp_dir(app_config_dir);
        let settings = files::load_settings(&dir);
        // persist a freshly generated token so connected clients keep working across restarts
        let _ = files::save_settings(&dir, &settings);
        let _ = files::write_launch(&dir, files::Launch { enabled: settings.enabled });
        let token = Arc::new(RwLock::new(settings.token.clone()));
        Mcp { dir, settings: Mutex::new(settings), token, server: Mutex::new(None), error: Mutex::new(None) }
    }

    /// Starts or stops the endpoint to match the settings.
    pub fn apply(&self, app: &AppHandle) {
        let settings = self.settings.lock().unwrap().clone();
        let mut server = self.server.lock().unwrap();
        if let Some(s) = server.take() {
            s.stop();
        }
        files::remove_endpoint(&self.dir);
        *self.error.lock().unwrap() = None;
        if !settings.enabled {
            return;
        }
        let host = AppHost(app.clone());
        let handler: http::Handler = Arc::new(move |msg: Value| protocol::handle(&host, msg));
        match http::start(settings.port, self.token.clone(), handler) {
            Ok(s) => {
                let endpoint = files::Endpoint { url: s.url(), token: settings.token.clone() };
                if let Err(e) = files::write_endpoint(&self.dir, &endpoint) {
                    *self.error.lock().unwrap() = Some(format!("Could not write {}: {e}", self.dir.display()));
                }
                *server = Some(s);
            }
            Err(e) => *self.error.lock().unwrap() = Some(e.to_string()),
        }
    }

    pub fn shutdown(&self) {
        if let Some(s) = self.server.lock().unwrap().take() {
            s.stop();
        }
        files::remove_endpoint(&self.dir);
    }

    fn update(&self, change: impl FnOnce(&mut files::Settings)) -> Result<(), String> {
        let mut s = self.settings.lock().unwrap();
        change(&mut s);
        *self.token.write().unwrap() = s.token.clone();
        files::save_settings(&self.dir, &s).map_err(|e| e.to_string())?;
        files::write_launch(&self.dir, files::Launch { enabled: s.enabled }).map_err(|e| e.to_string())
    }

    fn status(&self) -> McpStatus {
        let s = self.settings.lock().unwrap().clone();
        let server = self.server.lock().unwrap();
        let command = command_path();
        McpStatus {
            enabled: s.enabled,
            running: server.is_some(),
            port: server.as_ref().map_or(s.port, |r| r.port),
            url: server.as_ref().map(|r| r.url()),
            error: self.error.lock().unwrap().clone(),
            token: s.token,
            claude_code: claude_code_status(&command),
            command,
            args: vec![goo_mcp::STDIO_FLAG.into()],
            dev_build: cfg!(debug_assertions),
        }
    }
}

// ---------------------------------------------------------------------------
// Claude Code
// ---------------------------------------------------------------------------

fn home() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

/// The `claude` CLI: PATH, the usual install places, then a login shell (GUI apps often get a short PATH).
fn find_claude() -> Option<PathBuf> {
    let names: &[&str] = if cfg!(windows) { &["claude.exe", "claude.cmd"] } else { &["claude"] };
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            for n in names {
                let p = dir.join(n);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    if let Some(h) = home() {
        for rel in [".local/bin/claude", ".claude/local/claude", ".npm-global/bin/claude", ".bun/bin/claude"] {
            let p = h.join(rel);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    for p in ["/usr/local/bin/claude", "/opt/homebrew/bin/claude"] {
        if Path::new(p).is_file() {
            return Some(PathBuf::from(p));
        }
    }
    #[cfg(unix)]
    {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
        if let Ok(out) = Command::new(shell).args(["-lc", "command -v claude"]).output() {
            let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if out.status.success() && Path::new(&p).is_file() {
                return Some(PathBuf::from(p));
            }
        }
    }
    None
}

/// Reads Claude Code's user-scope MCP servers from ~/.claude.json.
fn claude_code_status(command: &str) -> ClaudeCode {
    let found = find_claude().is_some();
    let entry = home()
        .and_then(|h| std::fs::read_to_string(h.join(".claude.json")).ok())
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| v.pointer("/mcpServers/goo").cloned());
    let connected = match entry {
        None => "no",
        Some(e) if e["command"] == command && e["args"] == json!([goo_mcp::STDIO_FLAG]) => "yes",
        Some(_) => "outdated",
    };
    ClaudeCode { found, connected }
}

fn run_claude(args: &[&str]) -> Result<(), String> {
    let claude = find_claude().ok_or("Claude Code (the `claude` command) was not found.")?;
    let out = Command::new(claude).args(args).output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        let msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if msg.is_empty() { format!("claude {} failed", args.join(" ")) } else { msg })
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn mcp_status(mcp: State<'_, Mcp>) -> Result<McpStatus, String> {
    Ok(mcp.status())
}

#[tauri::command]
pub async fn mcp_set_enabled(enabled: bool, app: AppHandle, mcp: State<'_, Mcp>) -> Result<McpStatus, String> {
    mcp.update(|s| s.enabled = enabled)?;
    mcp.apply(&app);
    Ok(mcp.status())
}

/// A new token: clients holding the old one are locked out (the bridge reads the new one from endpoint.json).
#[tauri::command]
pub async fn mcp_new_token(app: AppHandle, mcp: State<'_, Mcp>) -> Result<McpStatus, String> {
    mcp.update(|s| s.token = files::new_token())?;
    mcp.apply(&app);
    Ok(mcp.status())
}

#[tauri::command]
pub async fn mcp_connect_claude_code(mcp: State<'_, Mcp>) -> Result<McpStatus, String> {
    let config = json!({ "type": "stdio", "command": command_path(), "args": [goo_mcp::STDIO_FLAG] }).to_string();
    let _ = run_claude(&["mcp", "remove", "--scope", "user", "goo"]);
    // add-json, not `add -e ...`: -e is variadic and would swallow the server name
    run_claude(&["mcp", "add-json", "--scope", "user", "goo", &config])?;
    Ok(mcp.status())
}

#[tauri::command]
pub async fn mcp_disconnect_claude_code(mcp: State<'_, Mcp>) -> Result<McpStatus, String> {
    run_claude(&["mcp", "remove", "--scope", "user", "goo"])?;
    Ok(mcp.status())
}

/// The UI reports which notes have unsaved edits; MCP writes to them are refused.
#[tauri::command]
pub fn set_unsaved(paths: Vec<String>, state: State<'_, AppState>) {
    *state.unsaved.lock().unwrap() = paths.iter().filter_map(|p| std::fs::canonicalize(p).ok()).collect();
}
