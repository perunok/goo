//! The Tauri shell: window, plugins and the commands the UI calls (`src/backend/tauri.ts` is the other side).
//! All file logic lives in `goo-core`; this file only keeps track of which folder is open and checks that every
//! path the UI sends stays inside it.

use goo_core::{CoreError, ErrorCode, Recents, Tree};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

type CmdResult<T> = Result<T, CoreError>;

struct AppState {
    /// Canonical root of the open folder.
    root: Mutex<Option<PathBuf>>,
    recents_file: PathBuf,
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
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let config = app.path().app_config_dir()?;
            app.manage(AppState { root: Mutex::new(None), recents_file: config.join("recent-workspaces.json") });
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running goo");
}
