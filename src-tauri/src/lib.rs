//! The Tauri shell: window, plugins and the commands the UI calls (`src/backend/tauri.ts` is the other side).
//! All file logic lives in `goo-core`; this file only keeps track of which folder is open and checks that every
//! path the UI sends stays inside it.
//!
//! Opening from outside ("Open with goo" in a file manager, `goo notes.md`, a macOS open event, or a second launch
//! handed over by the single-instance plugin) stores the path as a pending request and tells the UI with an
//! `open-request` event; the UI asks about unsaved changes if needed, then claims it with `take_open_request`.

mod cloud;
mod mcp;
mod uibridge;
mod watch;

use goo_core::{CoreError, ErrorCode, Recents, Tree};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

type CmdResult<T> = Result<T, CoreError>;

struct AppState {
    /// Canonical root of the open folder.
    root: Mutex<Option<PathBuf>>,
    recents_file: PathBuf,
    /// Files or a folder to open that came from outside the app, waiting for the UI to take them.
    pending_open: Mutex<Vec<PathBuf>>,
    /// A folder picker is on screen (only one at a time).
    picking: AtomicBool,
    /// Notes with unsaved edits in the window (reported by the UI); AI assistants may not write them.
    unsaved: Mutex<Vec<PathBuf>>,
}

#[derive(Serialize)]
struct WorkspaceInfo {
    root: String,
    name: String,
}

fn info(root: &Path) -> WorkspaceInfo {
    WorkspaceInfo { root: root.to_string_lossy().into_owned(), name: goo_core::workspace_name(root) }
}

fn current_root(state: &AppState) -> CmdResult<PathBuf> {
    state
        .root
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| CoreError::new(ErrorCode::NoWorkspace, "No folder is open"))
}

/// Makes `path` the open folder and moves it to the top of the recent list.
fn activate(app: &AppHandle, state: &AppState, path: &Path) -> CmdResult<WorkspaceInfo> {
    let root = goo_core::canonical_root(path)?;
    let mut recents = Recents::load(&state.recents_file);
    recents.push(&root.to_string_lossy());
    recents.save(&state.recents_file)?;
    *state.root.lock().unwrap() = Some(root.clone());
    app.state::<watch::FolderWatch>().follow(app, &root);
    Ok(info(&root))
}

/// Clears `AppState::picking` when the picker closes, however the command ends.
struct PickingGuard<'a>(&'a AtomicBool);

impl Drop for PickingGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// Native folder picker, attached to the goo window (modal) and never more than one at a time: a second request
/// while one is open is ignored. The dialog blocks, so this runs as an async command (off the main thread).
#[tauri::command]
async fn pick_workspace(app: AppHandle, state: State<'_, AppState>) -> CmdResult<Option<WorkspaceInfo>> {
    if state.picking.swap(true, Ordering::SeqCst) {
        return Ok(None);
    }
    let _guard = PickingGuard(&state.picking);
    let mut dialog = app.dialog().file().set_title("Open a folder of notes");
    #[cfg(desktop)]
    if let Some(window) = app.get_webview_window("main") {
        dialog = dialog.set_parent(&window);
    }
    let Some(picked) = dialog.blocking_pick_folder() else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|e| CoreError::new(ErrorCode::Io, e.to_string()))?;
    activate(&app, &state, &path).map(Some)
}

/// Reopens a folder from the recent list. Only folders the user picked before are accepted, so the UI cannot widen
/// its own file access.
#[tauri::command]
fn open_workspace(root: String, app: AppHandle, state: State<'_, AppState>) -> CmdResult<WorkspaceInfo> {
    if !Recents::load(&state.recents_file).contains(&root) {
        return Err(CoreError::new(ErrorCode::OutsideWorkspace, "Pick this folder with Open Folder first"));
    }
    activate(&app, &state, Path::new(&root))
}

/// Recent folders that still exist, most recent first.
#[tauri::command]
fn recent_workspaces(state: State<'_, AppState>) -> Vec<WorkspaceInfo> {
    Recents::load(&state.recents_file)
        .workspaces
        .iter()
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .map(|p| info(&p))
        .collect()
}

#[tauri::command]
fn forget_workspace(root: String, state: State<'_, AppState>) -> CmdResult<()> {
    let mut recents = Recents::load(&state.recents_file);
    recents.remove(&root);
    recents.save(&state.recents_file)
}

#[tauri::command]
async fn list_tree(state: State<'_, AppState>) -> CmdResult<Tree> {
    goo_core::list_tree(&current_root(&state)?)
}

#[tauri::command]
async fn read_file(path: String, state: State<'_, AppState>) -> CmdResult<String> {
    let root = current_root(&state)?;
    goo_core::read_text(&goo_core::resolve_inside(&root, Path::new(&path))?)
}

#[tauri::command]
async fn write_file(path: String, contents: String, state: State<'_, AppState>) -> CmdResult<()> {
    let root = current_root(&state)?;
    goo_core::write_atomic(&goo_core::resolve_inside(&root, Path::new(&path))?, &contents)
}

/// Creates `name` (".md" appended if needed) in `dir` and returns its path.
#[tauri::command]
async fn create_file(dir: String, name: String, contents: String, state: State<'_, AppState>) -> CmdResult<String> {
    let root = current_root(&state)?;
    let file = goo_core::new_file_path(&root, Path::new(&dir), &name)?;
    goo_core::create_new(&file, &contents)?;
    Ok(file.to_string_lossy().into_owned())
}

fn path_out(p: PathBuf) -> String {
    p.to_string_lossy().into_owned()
}

/// Moves a note or folder to the system trash (recoverable from there).
pub(crate) fn to_trash(path: &Path) -> std::io::Result<()> {
    trash::delete(path).map_err(|e| std::io::Error::other(e.to_string()))
}

/// Renames a note or folder where it is; returns the new path.
#[tauri::command]
async fn rename_entry(path: String, new_name: String, state: State<'_, AppState>) -> CmdResult<String> {
    let root = current_root(&state)?;
    goo_core::rename_entry(&root, Path::new(&path), &new_name).map(path_out)
}

/// Moves a note or folder into another folder; returns the new path.
#[tauri::command]
async fn move_entry(path: String, dest_dir: String, state: State<'_, AppState>) -> CmdResult<String> {
    let root = current_root(&state)?;
    goo_core::move_entry(&root, Path::new(&path), Path::new(&dest_dir)).map(path_out)
}

#[tauri::command]
async fn create_folder(dir: String, name: String, state: State<'_, AppState>) -> CmdResult<String> {
    let root = current_root(&state)?;
    goo_core::create_folder(&root, Path::new(&dir), &name).map(path_out)
}

/// Moves a note or folder to the system trash.
#[tauri::command]
async fn delete_entry(path: String, state: State<'_, AppState>) -> CmdResult<()> {
    let root = current_root(&state)?;
    goo_core::delete_entry(&root, Path::new(&path), to_trash).map(|_| ())
}

/// Shows a note or folder in the system file manager.
#[tauri::command]
fn reveal_entry(path: String, app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let root = current_root(&state)?;
    let entry = if Path::new(&path) == root { root } else { goo_core::entry_inside(&root, Path::new(&path))? };
    app.opener().reveal_item_in_dir(entry).map_err(|e| CoreError::new(ErrorCode::Io, e.to_string()))
}

#[derive(Serialize)]
struct OpenRequest {
    workspace: WorkspaceInfo,
    /// The notes to open (empty when a folder was opened).
    files: Vec<String>,
    /// Paths that were left out (not Markdown, or in another folder than the rest of the selection).
    skipped: Vec<String>,
    /// True when the open folder changed.
    switched: bool,
}

/// Payload of the `open-request` event: what is about to be opened, and whether it leaves the current folder (so the
/// UI knows to ask about unsaved notes first).
#[derive(Clone, Serialize)]
struct OpenEvent {
    path: String,
    switch: bool,
}

/// Queues `paths` and tells the UI. Called for the arguments of a second launch and for macOS open events.
fn request_open(app: &AppHandle, paths: Vec<PathBuf>) {
    let state = app.state::<AppState>();
    let current = state.root.lock().unwrap().clone();
    match goo_core::plan_open(current.as_deref(), &paths) {
        Ok(plan) => {
            let event = OpenEvent { path: plan.root.to_string_lossy().into_owned(), switch: plan.switch };
            *state.pending_open.lock().unwrap() = paths;
            let _ = app.emit("open-request", event);
        }
        Err(e) => {
            let _ = app.emit("open-error", e.message);
        }
    }
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Opens the pending request (if any): a note opens in the current folder when it is inside it, otherwise its folder
/// becomes the open folder (and joins the recent list).
#[tauri::command]
fn take_open_request(app: AppHandle, state: State<'_, AppState>) -> CmdResult<Option<OpenRequest>> {
    let paths = std::mem::take(&mut *state.pending_open.lock().unwrap());
    if paths.is_empty() {
        return Ok(None);
    }
    let current = state.root.lock().unwrap().clone();
    let plan = goo_core::plan_open(current.as_deref(), &paths)?;
    let workspace = if plan.switch { activate(&app, &state, &plan.root)? } else { info(&plan.root) };
    let strings = |v: Vec<PathBuf>| v.into_iter().map(|p| p.to_string_lossy().into_owned()).collect();
    Ok(Some(OpenRequest { workspace, files: strings(plan.files), skipped: strings(plan.skipped), switched: plan.switch }))
}

/// The user kept the current folder (e.g. cancelled at "unsaved changes").
#[tauri::command]
fn drop_open_request(state: State<'_, AppState>) {
    state.pending_open.lock().unwrap().clear();
}

/// Opens a link from a rendered note in the system browser or mail client. Nothing else is opened.
#[tauri::command]
fn open_external(url: String, app: AppHandle) -> CmdResult<()> {
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://") || lower.starts_with("mailto:")) {
        return Err(CoreError::new(ErrorCode::InvalidName, "Only web and mail links can be opened"));
    }
    app.opener().open_url(url, None::<&str>).map_err(|e| CoreError::new(ErrorCode::Io, e.to_string()))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();

    // Must be the first plugin: a second launch exits right away and passes its arguments to this instance.
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
        let paths = goo_core::launch_paths(&argv, Path::new(&cwd));
        if paths.is_empty() {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        } else {
            request_open(app, paths);
        }
    }));

    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let config = app.path().app_config_dir()?;
            // `goo notes.md` / "Open with goo": the UI takes this at startup.
            let args: Vec<String> = std::env::args().collect();
            let cwd = std::env::current_dir().unwrap_or_default();
            app.manage(AppState {
                root: Mutex::new(None),
                recents_file: config.join("recent-workspaces.json"),
                pending_open: Mutex::new(goo_core::launch_paths(&args, &cwd)),
                picking: AtomicBool::new(false),
                unsaved: Mutex::new(Vec::new()),
            });
            if Some(&config) != goo_mcp::files::app_config_dir().as_ref() {
                eprintln!("goo: config dir {} differs from the MCP bridge's guess; `goo --mcp-stdio` may not find the app", config.display());
            }
            goo_mcp::set_app_version(&app.package_info().version.to_string());
            let mcp = mcp::Mcp::new(&config);
            mcp.apply(app.handle());
            app.manage(mcp);
            app.manage(watch::FolderWatch::default());
            app.manage(uibridge::UiBridge::default());
            app.manage(cloud::Cloud::new(&config));
            cloud::Cloud::start(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            pick_workspace,
            open_workspace,
            recent_workspaces,
            forget_workspace,
            list_tree,
            read_file,
            write_file,
            create_file,
            open_external,
            take_open_request,
            drop_open_request,
            rename_entry,
            move_entry,
            create_folder,
            delete_entry,
            reveal_entry,
            mcp::mcp_status,
            mcp::mcp_set_enabled,
            mcp::mcp_new_token,
            mcp::mcp_connect_claude_code,
            mcp::mcp_disconnect_claude_code,
            mcp::set_unsaved,
            cloud::cloud_status,
            cloud::cloud_set_utc_offset,
            cloud::cloud_sign_in,
            cloud::cloud_cancel_sign_in,
            cloud::cloud_sign_out,
            cloud::cloud_workspaces,
            cloud::cloud_link,
            cloud::cloud_unlink,
            cloud::cloud_sync_now,
            uibridge::ui_answer,
        ])
        .build(tauri::generate_context!())
        .expect("error while building goo")
        .run(|_app, _event| {
            if let tauri::RunEvent::Exit = &_event {
                // assistants must not find a stale endpoint.json
                _app.state::<mcp::Mcp>().shutdown();
            }
            // macOS hands files from Finder ("Open With", double click) to the running app as an event, not as
            // arguments.
            #[cfg(target_os = "macos")]
            {
                if let tauri::RunEvent::Opened { urls } = &_event {
                    let paths: Vec<PathBuf> = urls.iter().filter_map(|u| u.to_file_path().ok()).collect();
                    if !paths.is_empty() {
                        request_open(_app, paths);
                    }
                }
            }
        });
}
