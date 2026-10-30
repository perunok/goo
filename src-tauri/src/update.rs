//! In-app updates: the commands the UI calls (check, download, cancel, install) and the `update-state` event that
//! reports the download. All the logic is in `goo-update`; this wires it to the app (data folder, quitting, events).
//!
//! `GOO_UPDATE_FEED_URL` (http/https) replaces GitHub's latest-release URL (tests, e2e); `GOO_UPDATES=off` turns update
//! checks off entirely (automated runs, packagers that update goo themselves).

use goo_core::{CoreError, ErrorCode};
use goo_update::{CheckResult, DetectInput, Deps, Installer, SystemProbe, SystemSpawner, UpdateState};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

pub struct Updates {
    installer: Installer,
    /// None: update checks are turned off.
    feed_url: Option<String>,
    current_version: String,
}

impl Updates {
    pub fn new(app: &AppHandle) -> Result<Updates, String> {
        let current_version = app.package_info().version.to_string();
        let off = std::env::var("GOO_UPDATES").is_ok_and(|v| v.eq_ignore_ascii_case("off") || v == "0");
        let feed = goo_update::feed_url(std::env::var("GOO_UPDATE_FEED_URL").ok().as_deref(), off)?;

        let exec_path = std::env::current_exe().unwrap_or_default();
        let appimage = std::env::var_os("APPIMAGE").map(PathBuf::from);
        let install = goo_update::detect_install(&DetectInput {
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            is_packaged: !cfg!(debug_assertions),
            appimage: appimage.as_deref(),
            exec_path: &exec_path,
            probe: &SystemProbe,
        });
        let download_dir = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("updates");

        let emitter = app.clone();
        let quitter = app.clone();
        let installer = Installer::new(Deps {
            current_version: current_version.clone(),
            feed_url: feed.clone().unwrap_or_else(|| goo_update::LATEST_RELEASE_API.to_string()),
            install,
            arch: std::env::consts::ARCH.to_string(),
            download_dir,
            emit: Arc::new(move |state| {
                let _ = emitter.emit("update-state", state);
            }),
            spawner: Arc::new(SystemSpawner),
            pid: std::process::id(),
            exec_path,
            // the normal quit: RunEvent::Exit still stops the MCP endpoint
            quit: Arc::new(move || quitter.exit(0)),
        });
        // an installer left by an earlier run is of no use (nothing remembers a download across restarts)
        installer.cleanup();
        Ok(Updates { installer, feed_url: feed, current_version })
    }

    /// For a failed setup (a bad GOO_UPDATE_FEED_URL): checks answer with the reason.
    pub fn disabled(app: &AppHandle, reason: &str) -> Updates {
        let installer = Installer::new(Deps {
            current_version: app.package_info().version.to_string(),
            feed_url: goo_update::LATEST_RELEASE_API.to_string(),
            install: goo_update::InstallTarget { method: None, reason: Some(reason.to_string()), target: None },
            arch: std::env::consts::ARCH.to_string(),
            download_dir: PathBuf::new(),
            emit: Arc::new(|_| {}),
            spawner: Arc::new(SystemSpawner),
            pid: std::process::id(),
            exec_path: PathBuf::new(),
            quit: Arc::new(|| {}),
        });
        Updates { installer, feed_url: None, current_version: app.package_info().version.to_string() }
    }
}

fn io(message: impl Into<String>) -> CoreError {
    CoreError::new(ErrorCode::Io, message)
}

type CmdResult<T> = Result<T, CoreError>;

/// One GET to GitHub's latest-release endpoint; downloads nothing.
#[tauri::command]
pub async fn check_for_updates(updates: State<'_, Updates>) -> CmdResult<CheckResult> {
    let Some(feed) = updates.feed_url.clone() else {
        return Err(io("Update checks are turned off (GOO_UPDATES=off)."));
    };
    let current = updates.current_version.clone();
    tauri::async_runtime::spawn_blocking(move || goo_update::check(&current, &feed))
        .await
        .map_err(|e| io(e.to_string()))?
        .map_err(|e| io(e.to_string()))
}

#[tauri::command]
pub fn update_state(updates: State<'_, Updates>) -> UpdateState {
    updates.installer.state()
}

/// Starts downloading the installer for this installation; progress arrives as `update-state` events.
#[tauri::command]
pub fn download_update(updates: State<'_, Updates>) -> CmdResult<UpdateState> {
    if updates.feed_url.is_none() {
        return Err(io("Update checks are turned off (GOO_UPDATES=off)."));
    }
    updates.installer.download().map_err(io)
}

#[tauri::command]
pub fn cancel_update_download(updates: State<'_, Updates>) -> UpdateState {
    updates.installer.cancel()
}

/// Installs the downloaded update and quits; a helper starts the new version. The UI saves unsaved notes first.
#[tauri::command]
pub async fn install_update(updates: State<'_, Updates>) -> CmdResult<UpdateState> {
    let installer = updates.installer.clone();
    // pkexec waits for the password: not on the UI's thread
    tauri::async_runtime::spawn_blocking(move || installer.install())
        .await
        .map_err(|e| io(e.to_string()))?
        .map_err(io)
}
