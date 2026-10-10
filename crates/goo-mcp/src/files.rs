//! Files shared between the app (which runs the endpoint) and the stdio bridge (which assistants launch), in
//! `<app config dir>/mcp/`:
//!
//! - `settings.json` (0600): `{enabled, port, token}`; the app's MCP settings, token included.
//! - `endpoint.json` (0600): `{url, token}`; present only while the endpoint runs, so the bridge can find it.
//! - `launch.json`: `{enabled}`; no secrets. Lets the bridge start goo when it is not running, and refuse when MCP
//!   access is turned off.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Must equal `identifier` in src-tauri/tauri.conf.json: Tauri's app config dir is `<config dir>/<identifier>`.
pub const APP_IDENTIFIER: &str = "io.github.perunok.goo";
pub const DEFAULT_PORT: u16 = 7355;

/// goo's app config dir, computed the way Tauri does (the bridge runs without Tauri).
pub fn app_config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join(APP_IDENTIFIER))
}

pub fn mcp_dir(app_config_dir: &Path) -> PathBuf {
    app_config_dir.join("mcp")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub enabled: bool,
    pub port: u16,
    pub token: String,
}

impl Settings {
    /// MCP access is off until the user turns it on.
    pub fn new() -> Settings {
        Settings { enabled: false, port: DEFAULT_PORT, token: new_token() }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endpoint {
    pub url: String,
    pub token: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Launch {
    pub enabled: bool,
}

/// `goo_` + 64 hex characters (32 random bytes).
pub fn new_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the OS random number generator failed");
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("goo_{hex}")
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

/// Writes through a temp file created owner-only (0600 on Unix) and renamed over the target, so the token is never
/// readable by other users, not even briefly.
pub fn write_private(path: &Path, contents: &str) -> io::Result<()> {
    let dir = path.parent().ok_or_else(|| io::Error::other("no parent folder"))?;
    fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".{}.tmp", path.file_name().unwrap_or_default().to_string_lossy()));
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let result = (|| {
        let mut f = opts.open(&tmp)?;
        f.write_all(contents.as_bytes())?;
        f.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn to_json<T: Serialize>(v: &T) -> String {
    serde_json::to_string_pretty(v).expect("plain structs serialise")
}

pub fn load_settings(dir: &Path) -> Settings {
    match read_json::<Settings>(&dir.join("settings.json")) {
        Some(s) if s.token.starts_with("goo_") && s.port > 0 => s,
        _ => Settings::new(),
    }
}

pub fn save_settings(dir: &Path, s: &Settings) -> io::Result<()> {
    write_private(&dir.join("settings.json"), &to_json(s))
}

pub fn read_endpoint(dir: &Path) -> Option<Endpoint> {
    read_json(&dir.join("endpoint.json"))
}

pub fn write_endpoint(dir: &Path, e: &Endpoint) -> io::Result<()> {
    write_private(&dir.join("endpoint.json"), &to_json(e))
}

pub fn remove_endpoint(dir: &Path) {
    let _ = fs::remove_file(dir.join("endpoint.json"));
}

pub fn read_launch(dir: &Path) -> Launch {
    read_json(&dir.join("launch.json")).unwrap_or(Launch { enabled: false })
}

pub fn write_launch(dir: &Path, l: Launch) -> io::Result<()> {
    write_private(&dir.join("launch.json"), &to_json(&l))
}
