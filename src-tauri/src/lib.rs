//! The Tauri shell: window, plugins and the commands the UI calls (`src/backend/tauri.ts` is the other side).
//! All file logic lives in `goo-core`; this file only keeps track of which folder is open and checks that every
//! path the UI sends stays inside it.
//!
//! Opening from outside ("Open with goo" in a file manager, `goo notes.md`, a macOS open event, or a second launch
//! handed over by the single-instance plugin) stores the path as a pending request and tells the UI with an
//! `open-request` event; the UI asks about unsaved changes if needed, then claims it with `take_open_request`.

use goo_core::{CoreError, ErrorCode, Recents, Tree};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

type CmdResult<T> = Result<T, CoreError>;

struct AppState {
    /// Canonical root of the open folder.
    root: Mutex<Option<PathBuf>>,
    recents_file: PathBuf,
    /// A file or folder to open that came from outside the app, waiting for the UI to take it.
    pending_open: Mutex<Option<PathBuf>>,
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
fn activate(state: &AppState, path: &Path) -> CmdResult<WorkspaceInfo> {
    let root = goo_core::canonical_root(path)?;
    let mut recents = Recents::load(&state.recents_file);
    recents.push(&root.to_string_lossy());
    recents.save(&state.recents_file)?;
    *state.root.lock().unwrap() = Some(root.clone());
    Ok(info(&root))
}

/// Native folder picker. The dialog blocks, so this runs as an async command (off the main thread).
#[tauri::command]
async fn pick_workspace(app: AppHandle, state: State<'_, AppState>) -> CmdResult<Option<WorkspaceInfo>> {
    let Some(picked) = app.dialog().file().set_title("Open a folder of notes").blocking_pick_folder() else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|e| CoreError::new(ErrorCode::Io, e.to_string()))?;
    activate(&state, &path).map(Some)
}

/// Reopens a folder from the recent list. Only folders the user picked before are accepted, so the UI cannot widen
/// its own file access.
#[tauri::command]
fn open_workspace(root: String, state: State<'_, AppState>) -> CmdResult<WorkspaceInfo> {
    if !Recents::load(&state.recents_file).contains(&root) {
        return Err(CoreError::new(ErrorCode::OutsideWorkspace, "Pick this folder with Open Folder first"));
    }
    activate(&state, Path::new(&root))
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

#[derive(Serialize)]
struct OpenRequest {
    workspace: WorkspaceInfo,
    /// The note to open, if a file (not a folder) was opened.
    file: Option<String>,
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

/// Queues `path` and tells the UI. Called for launch arguments of a second instance and macOS open events.
fn request_open(app: &AppHandle, path: PathBuf) {
    let state = app.state::<AppState>();
    let current = state.root.lock().unwrap().clone();
    match goo_core::plan_open(current.as_deref(), &path) {
        Ok(plan) => {
            let event = OpenEvent { path: path.to_string_lossy().into_owned(), switch: plan.switch };
            *state.pending_open.lock().unwrap() = Some(path);
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
fn take_open_request(state: State<'_, AppState>) -> CmdResult<Option<OpenRequest>> {
    let Some(path) = state.pending_open.lock().unwrap().take() else {
        return Ok(None);
    };
    let current = state.root.lock().unwrap().clone();
    let plan = goo_core::plan_open(current.as_deref(), &path)?;
    let workspace = if plan.switch { activate(&state, &plan.root)? } else { info(&plan.root) };
    Ok(Some(OpenRequest {
        workspace,
        file: plan.file.map(|f| f.to_string_lossy().into_owned()),
        switched: plan.switch,
    }))
}

/// The user kept the current folder (e.g. cancelled at "unsaved changes").
#[tauri::command]
fn drop_open_request(state: State<'_, AppState>) {
    state.pending_open.lock().unwrap().take();
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
        match goo_core::launch_path(&argv, Path::new(&cwd)) {
            Some(path) => request_open(app, path),
            None => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
            }
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
                pending_open: Mutex::new(goo_core::launch_path(&args, &cwd)),
            });
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
        ])
        .build(tauri::generate_context!())
        .expect("error while building goo")
        .run(|_app, _event| {
            // macOS hands files from Finder ("Open With", double click) to the running app as an event, not as
            // arguments.
            #[cfg(target_os = "macos")]
            {
                if let tauri::RunEvent::Opened { urls } = &_event {
                    if let Some(path) = urls.iter().find_map(|u| u.to_file_path().ok()) {
                        request_open(_app, path);
                    }
                }
            }
        });
}
