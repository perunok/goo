//! goo cloud: sign-in, linking folders to cloud workspaces, and the background sync that runs goo-sync's engine.
//!
//! - The account (server URL, who is signed in, this installation's client id) is in `<config>/cloud/account.json`;
//!   the tokens are in the OS keychain (`goo-cloud` / server URL), never on disk.
//! - Links (local folder <-> workspace) are in `<config>/cloud/links.json`; each workspace's sync state is in
//!   `<config>/cloud/sync/<workspace id>/` (goo-sync's state store), never inside the notes folder.
//! - One scheduler thread syncs every linked folder: soon after a save or a "Sync now", every 60 s otherwise, with
//!   backoff after network errors. Only one sync runs at a time.

use crate::AppState;
use goo_sync::api::{ClientInfo, Remote, RemoteError, Role, TokenPair};
use goo_sync::http::{DeviceSignIn, HttpRemote, SignInError, TokenStore};
use goo_sync::state::StateStore;
use goo_sync::{Conflict, Quarantined, SyncError, SyncOptions};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

const KEYCHAIN_SERVICE: &str = "goo-cloud";
const INTERVAL: Duration = Duration::from_secs(60);
/// After a save or "Sync now": wait a moment so a burst of saves becomes one sync.
const DEBOUNCE: Duration = Duration::from_millis(1500);
const MAX_BACKOFF: Duration = Duration::from_secs(300);

// ---------------------------------------------------------------------------
// Persisted data
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Account {
    server_url: String,
    email: String,
    display_name: String,
    /// This installation, registered once per sign-in (shared by all linked folders).
    client_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Link {
    /// Canonical folder path.
    root: String,
    workspace_id: String,
    workspace_name: String,
    /// "viewer" pulls only.
    role: String,
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn write_json<T: Serialize>(path: &Path, v: &T) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(v).map_err(|e| e.to_string())?;
    goo_core::write_atomic(path, &text).map_err(|e| e.to_string())
}

/// Token pairs in the OS keychain, one entry per server.
struct KeychainTokens {
    server_url: String,
}

impl KeychainTokens {
    fn entry(&self) -> Result<keyring::Entry, String> {
        keyring::Entry::new(KEYCHAIN_SERVICE, &self.server_url).map_err(|e| format!("keychain: {e}"))
    }

    fn clear(&self) {
        if let Ok(e) = self.entry() {
            let _ = e.delete_credential();
        }
    }
}

impl TokenStore for KeychainTokens {
    fn load(&self) -> Option<TokenPair> {
        let secret = self.entry().ok()?.get_password().ok()?;
        serde_json::from_str(&secret).ok()
    }

    fn save(&self, pair: &TokenPair) -> Result<(), String> {
        let json = serde_json::to_string(pair).map_err(|e| e.to_string())?;
        self.entry()?.set_password(&json).map_err(|e| format!("Could not store the sign-in in the system keychain: {e}"))
    }
}

// ---------------------------------------------------------------------------
// Status shown in the UI
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FolderStatus {
    /// "idle", "syncing", "offline", "error" or "signed_out".
    state: String,
    last_synced_at: Option<u64>,
    message: Option<String>,
    conflicts: Vec<Conflict>,
    quarantined: Vec<Quarantined>,
    deferred: Vec<String>,
    not_uploaded: Vec<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct LinkStatus {
    root: String,
    workspace_id: String,
    workspace_name: String,
    role: String,
    status: FolderStatus,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AccountInfo {
    server_url: String,
    email: String,
    display_name: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SignInInfo {
    server_url: String,
    user_code: String,
    verification_uri: String,
    verification_uri_complete: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CloudStatus {
    account: Option<AccountInfo>,
    sign_in: Option<SignInInfo>,
    /// The last sign-in failure, shown until the next attempt.
    sign_in_error: Option<String>,
    links: Vec<LinkStatus>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteWorkspace {
    id: String,
    name: String,
    role: String,
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

struct SignInJob {
    info: SignInInfo,
    cancelled: Arc<AtomicBool>,
}

enum Trigger {
    /// Sync this folder soon (after a save or "Sync now"); None = every linked folder.
    Soon(Option<String>),
}

pub struct Cloud {
    dir: PathBuf,
    account: Mutex<Option<Account>>,
    links: Mutex<Vec<Link>>,
    status: Mutex<std::collections::HashMap<String, FolderStatus>>,
    sign_in: Mutex<Option<SignInJob>>,
    sign_in_error: Mutex<Option<String>>,
    utc_offset_minutes: AtomicI32,
    trigger: Mutex<Option<Sender<Trigger>>>,
    /// Serialises syncs (the scheduler and nothing else runs them, but be explicit).
    running: Mutex<()>,
}

fn role_name(r: Role) -> String {
    serde_json::to_value(r).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_else(|| "viewer".into())
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn device_name() -> String {
    let from_env = std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).ok();
    let from_file = || std::fs::read_to_string("/etc/hostname").ok().map(|s| s.trim().to_string());
    from_env.or_else(from_file).filter(|s| !s.is_empty()).unwrap_or_else(|| "goo desktop".into())
}

/// `https://goo.example.com/` -> `https://goo.example.com`. Plain http only for this computer (testing).
fn normalise_server_url(raw: &str) -> Result<String, String> {
    let url = raw.trim().trim_end_matches('/').to_string();
    let lower = url.to_ascii_lowercase();
    let host_of = |rest: &str| {
        let authority = rest.split('/').next().unwrap_or("");
        // "[::1]:8080" -> "[::1]", "localhost:8080" -> "localhost"
        if authority.starts_with('[') { authority.split(']').next().map(|h| format!("{h}]")).unwrap_or_default() } else { authority.split(':').next().unwrap_or("").to_string() }
    };
    let ok = match lower.split_once("://") {
        Some(("https", rest)) => !host_of(rest).is_empty(),
        Some(("http", rest)) => ["localhost", "127.0.0.1", "[::1]"].contains(&host_of(rest).as_str()),
        _ => false,
    };
    if ok {
        Ok(url)
    } else {
        Err("Enter the server address, starting with https:// (e.g. https://goo.example.com).".into())
    }
}

#[cfg(test)]
mod tests {
    use super::normalise_server_url as n;

    #[test]
    fn server_urls_need_https_except_on_this_computer() {
        assert_eq!(n(" https://goo.example.com/ ").unwrap(), "https://goo.example.com");
        assert!(n("http://localhost:8080").is_ok());
        assert!(n("http://127.0.0.1").is_ok());
        assert!(n("http://[::1]:3000/").is_ok());
        for bad in ["http://goo.example.com", "http://localhost.evil.com", "http://127.0.0.1.evil.com", "goo.example.com", "https://", "ftp://x"] {
            assert!(n(bad).is_err(), "{bad}");
        }
    }
}

fn describe(e: &RemoteError) -> String {
    match e {
        RemoteError::Network(m) => format!("Can't reach the server ({m})."),
        RemoteError::SignInRequired(_) => "Your sign-in expired. Sign in again.".into(),
        RemoteError::RateLimited { .. } => "The server is busy; trying again later.".into(),
        RemoteError::Api { code, message, .. } if code == "workspace_access_denied" => {
            format!("You no longer have access to this workspace ({message}).")
        }
        RemoteError::Api { message, .. } => message.clone(),
        RemoteError::Protocol(m) => format!("Unexpected answer from the server: {m}"),
        RemoteError::TokenStore(m) => m.clone(),
    }
}

impl Cloud {
    pub fn new(app_config_dir: &Path) -> Cloud {
        let dir = app_config_dir.join("cloud");
        Cloud {
            account: Mutex::new(read_json(&dir.join("account.json"))),
            links: Mutex::new(read_json(&dir.join("links.json")).unwrap_or_default()),
            dir,
            status: Mutex::new(Default::default()),
            sign_in: Mutex::new(None),
            sign_in_error: Mutex::new(None),
            utc_offset_minutes: AtomicI32::new(0),
            trigger: Mutex::new(None),
            running: Mutex::new(()),
        }
    }

    fn save_account(&self) -> Result<(), String> {
        match &*self.account.lock().unwrap() {
            Some(a) => write_json(&self.dir.join("account.json"), a),
            None => {
                let _ = std::fs::remove_file(self.dir.join("account.json"));
                Ok(())
            }
        }
    }

    fn save_links(&self) -> Result<(), String> {
        write_json(&self.dir.join("links.json"), &*self.links.lock().unwrap())
    }

    fn state_dir(&self, workspace_id: &str) -> PathBuf {
        self.dir.join("sync").join(workspace_id)
    }

    fn remote(&self) -> Result<(Account, HttpRemote), String> {
        let account = self.account.lock().unwrap().clone().ok_or("Not signed in to goo cloud.")?;
        let store: Arc<dyn TokenStore> = Arc::new(KeychainTokens { server_url: account.server_url.clone() });
        let remote = HttpRemote::new(&account.server_url, store);
        Ok((account, remote))
    }

    pub fn status(&self) -> CloudStatus {
        let statuses = self.status.lock().unwrap();
        let signed_in = self.account.lock().unwrap().is_some();
        CloudStatus {
            account: self.account.lock().unwrap().as_ref().map(|a| AccountInfo {
                server_url: a.server_url.clone(),
                email: a.email.clone(),
                display_name: a.display_name.clone(),
            }),
            sign_in: self.sign_in.lock().unwrap().as_ref().map(|j| j.info.clone()),
            sign_in_error: self.sign_in_error.lock().unwrap().clone(),
            links: self
                .links
                .lock()
                .unwrap()
                .iter()
                .map(|l| LinkStatus {
                    root: l.root.clone(),
                    workspace_id: l.workspace_id.clone(),
                    workspace_name: l.workspace_name.clone(),
                    role: l.role.clone(),
                    status: statuses.get(&l.root).cloned().unwrap_or_else(|| FolderStatus {
                        state: if signed_in { "idle".into() } else { "signed_out".into() },
                        ..Default::default()
                    }),
                })
                .collect(),
        }
    }

    fn emit(&self, app: &AppHandle) {
        let _ = app.emit("cloud-status", self.status());
    }

    fn set_status(&self, app: &AppHandle, root: &str, change: impl FnOnce(&mut FolderStatus)) {
        {
            let mut all = self.status.lock().unwrap();
            change(all.entry(root.to_string()).or_default());
        }
        self.emit(app);
    }

    /// A folder's goo cloud state for AI assistants (MCP `sync_status`).
    pub fn folder_json(&self, root: &Path) -> serde_json::Value {
        let root_s = root.to_string_lossy().into_owned();
        let Some(link) = self.links.lock().unwrap().iter().find(|l| l.root == root_s).cloned() else {
            return json!({ "linked": false, "message": "This folder does not sync with goo cloud." });
        };
        let signed_in = self.account.lock().unwrap().is_some();
        let s = self.status.lock().unwrap().get(&root_s).cloned().unwrap_or_else(|| FolderStatus {
            state: if signed_in { "idle".into() } else { "signed_out".into() },
            ..Default::default()
        });
        json!({
            "linked": true,
            "workspace": link.workspace_name,
            "role": link.role,
            "state": s.state,
            "last_synced_at": s.last_synced_at,
            "message": s.message,
            "conflicts": s.conflicts,
            "not_synced": s.quarantined,
            "waiting_for_save": s.deferred,
            "not_uploaded_read_only": s.not_uploaded,
        })
    }

    /// Syncs a linked folder now and waits until that sync has finished (or `timeout`).
    pub fn sync_and_wait(&self, root: &Path, timeout: Duration) -> Result<serde_json::Value, String> {
        let root_s = root.to_string_lossy().into_owned();
        if !self.links.lock().unwrap().iter().any(|l| l.root == root_s) {
            return Err("This folder does not sync with goo cloud.".into());
        }
        let snapshot = || self.status.lock().unwrap().get(&root_s).map(|s| (s.state.clone(), s.last_synced_at));
        let before = snapshot();
        self.trigger(Some(root_s.clone()));
        let started = Instant::now();
        let mut saw_syncing = false;
        while started.elapsed() < timeout {
            std::thread::sleep(Duration::from_millis(250));
            let now = snapshot();
            match &now {
                Some((state, _)) if state == "syncing" => saw_syncing = true,
                // finished: we saw it run, or it ran between two looks (new time, or a new state such as "offline")
                Some(_) if saw_syncing || now != before => return Ok(self.folder_json(root)),
                _ => {}
            }
        }
        Err("The sync did not finish in time; it goes on in the background (see sync_status).".into())
    }

    pub fn trigger(&self, root: Option<String>) {
        if let Some(tx) = &*self.trigger.lock().unwrap() {
            let _ = tx.send(Trigger::Soon(root));
        }
    }

    /// Starts the scheduler thread (once, at app start).
    pub fn start(app: &AppHandle) {
        let (tx, rx) = mpsc::channel();
        *app.state::<Cloud>().trigger.lock().unwrap() = Some(tx);
        let app = app.clone();
        std::thread::Builder::new()
            .name("goo-cloud-sync".into())
            .spawn(move || scheduler(app, rx))
            .expect("spawn the sync thread");
    }

    /// Syncs one linked folder now (called by the scheduler only).
    fn sync_link(&self, app: &AppHandle, link: &Link) -> Result<(), RemoteError> {
        let _one_at_a_time = self.running.lock().unwrap();
        let (mut account, remote) = match self.remote() {
            Ok(r) => r,
            Err(message) => {
                self.set_status(app, &link.root, |s| {
                    s.state = "signed_out".into();
                    s.message = Some(message);
                });
                return Ok(());
            }
        };
        self.set_status(app, &link.root, |s| s.state = "syncing".into());
        let client = ClientInfo {
            device_name: device_name(),
            platform: std::env::consts::OS.into(),
            app_version: app.package_info().version.to_string(),
        };
        if account.client_id.is_none() {
            let id = remote.register_client(&client)?;
            account.client_id = Some(id);
            *self.account.lock().unwrap() = Some(account.clone());
            let _ = self.save_account();
        }
        let unsaved = app.state::<AppState>().unsaved.lock().unwrap().clone();
        let may_write = move |p: &Path| {
            let p = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
            !unsaved.contains(&p)
        };
        let opts = SyncOptions {
            workspace_id: link.workspace_id.clone(),
            client,
            client_id: account.client_id.clone(),
            read_only: link.role == "viewer",
            may_write: &may_write,
            utc_offset_minutes: self.utc_offset_minutes.load(Ordering::Relaxed),
        };
        let root = PathBuf::from(&link.root);
        let result = goo_sync::sync(&root, &self.state_dir(&link.workspace_id), &remote, &opts);
        let (report, error) = match result {
            Ok(r) => (r, None),
            Err(f) => (f.report, Some(f.error)),
        };
        if !report.changed_files.is_empty() {
            let files: Vec<String> = report.changed_files.iter().map(|p| p.to_string_lossy().into_owned()).collect();
            let _ = app.emit("files-changed", json!({ "root": link.root, "files": files }));
        }
        let remote_error = match &error {
            Some(SyncError::Remote(e)) => Some(e.clone()),
            _ => None,
        };
        self.set_status(app, &link.root, |s| {
            s.conflicts = report.conflicts.clone();
            s.quarantined = report.quarantined.clone();
            s.deferred = report.deferred.clone();
            s.not_uploaded = report.not_uploaded.clone();
            match &error {
                None => {
                    s.state = "idle".into();
                    s.message = report.reset.clone();
                    s.last_synced_at = Some(now_secs());
                }
                Some(SyncError::Remote(e @ RemoteError::Network(_))) => {
                    s.state = "offline".into();
                    s.message = Some(describe(e));
                }
                Some(SyncError::Remote(e @ RemoteError::SignInRequired(_))) => {
                    s.state = "signed_out".into();
                    s.message = Some(describe(e));
                }
                Some(SyncError::Remote(e)) => {
                    s.state = "error".into();
                    s.message = Some(describe(e));
                }
                Some(SyncError::Local(e)) => {
                    s.state = "error".into();
                    s.message = Some(e.message.clone());
                }
            }
        });
        match remote_error {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

/// Syncs every linked folder on a timer, sooner when triggered; backs off after network trouble.
fn scheduler(app: AppHandle, rx: Receiver<Trigger>) {
    let cloud = app.state::<Cloud>();
    let mut next_full = Instant::now() + Duration::from_secs(3);
    let mut backoff = Duration::ZERO;
    loop {
        let wait = next_full.saturating_duration_since(Instant::now());
        let mut only: Option<Option<String>> = None;
        match rx.recv_timeout(wait) {
            Ok(Trigger::Soon(root)) => {
                // collect a burst of triggers into one sync
                std::thread::sleep(DEBOUNCE);
                let mut roots = vec![root];
                while let Ok(Trigger::Soon(r)) = rx.try_recv() {
                    roots.push(r);
                }
                only = Some(if roots.iter().any(Option::is_none) { None } else { roots[0].clone() });
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        let links: Vec<Link> = cloud.links.lock().unwrap().clone();
        let mut network_trouble = false;
        for link in &links {
            if let Some(Some(root)) = &only {
                if root != &link.root {
                    continue;
                }
            }
            if !Path::new(&link.root).is_dir() {
                continue;
            }
            match cloud.sync_link(&app, link) {
                Err(RemoteError::Network(_)) | Err(RemoteError::RateLimited { .. }) => network_trouble = true,
                _ => {}
            }
        }
        backoff = if network_trouble { (backoff * 2).clamp(Duration::from_secs(5), MAX_BACKOFF) } else { Duration::ZERO };
        if only.is_none() || network_trouble {
            next_full = Instant::now() + if network_trouble { backoff } else { INTERVAL };
        }
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn cloud_status(cloud: State<'_, Cloud>) -> CloudStatus {
    cloud.status()
}

/// Local time offset for conflict copy names (the UI knows it; Rust has no time zone database here).
#[tauri::command]
pub fn cloud_set_utc_offset(minutes: i32, cloud: State<'_, Cloud>) {
    cloud.utc_offset_minutes.store(minutes, Ordering::Relaxed);
}

/// Starts device sign-in: shows a code, opens the browser on the approval page and waits in the background.
#[tauri::command]
pub async fn cloud_sign_in(server_url: String, app: AppHandle, cloud: State<'_, Cloud>) -> Result<CloudStatus, String> {
    let url = normalise_server_url(&server_url)?;
    if let Some(job) = cloud.sign_in.lock().unwrap().take() {
        job.cancelled.store(true, Ordering::SeqCst);
    }
    *cloud.sign_in_error.lock().unwrap() = None;
    let flow = DeviceSignIn::start(&url, "goo", &device_name()).map_err(|e| describe(&e))?;
    let info = SignInInfo {
        server_url: url.clone(),
        user_code: flow.start.user_code.clone(),
        verification_uri: flow.start.verification_uri.clone(),
        verification_uri_complete: flow.start.verification_uri_complete.clone(),
    };
    let cancelled = Arc::new(AtomicBool::new(false));
    *cloud.sign_in.lock().unwrap() = Some(SignInJob { info: info.clone(), cancelled: cancelled.clone() });
    let _ = app.opener().open_url(info.verification_uri_complete.clone(), None::<&str>);

    let app2 = app.clone();
    std::thread::spawn(move || {
        let cloud = app2.state::<Cloud>();
        let store = KeychainTokens { server_url: url.clone() };
        let result = flow.wait(&store, &|| cancelled.load(Ordering::SeqCst));
        let still_current = cloud.sign_in.lock().unwrap().as_ref().is_some_and(|j| Arc::ptr_eq(&j.cancelled, &cancelled));
        if !still_current {
            return; // replaced by a newer attempt
        }
        *cloud.sign_in.lock().unwrap() = None;
        match result {
            Ok(user) => {
                let previous = cloud.account.lock().unwrap().clone();
                if previous.as_ref().is_some_and(|p| p.server_url != url || p.email != user.email) {
                    // another account or server: the old links belong to the old one
                    unlink_all(&cloud);
                }
                *cloud.account.lock().unwrap() =
                    Some(Account { server_url: url, email: user.email, display_name: user.display_name, client_id: None });
                let _ = cloud.save_account();
                cloud.trigger(None);
            }
            Err(SignInError::Cancelled) => {}
            Err(e) => *cloud.sign_in_error.lock().unwrap() = Some(e.to_string()),
        }
        cloud.emit(&app2);
    });
    Ok(cloud.status())
}

#[tauri::command]
pub fn cloud_cancel_sign_in(app: AppHandle, cloud: State<'_, Cloud>) -> CloudStatus {
    if let Some(job) = cloud.sign_in.lock().unwrap().take() {
        job.cancelled.store(true, Ordering::SeqCst);
    }
    cloud.emit(&app);
    cloud.status()
}

fn unlink_all(cloud: &Cloud) {
    let links = std::mem::take(&mut *cloud.links.lock().unwrap());
    for l in links {
        let _ = StateStore::new(cloud.state_dir(&l.workspace_id)).remove();
    }
    cloud.status.lock().unwrap().clear();
    let _ = cloud.save_links();
}

/// Signs out on the server, forgets the tokens and unlinks every folder. The notes stay on disk.
#[tauri::command]
pub async fn cloud_sign_out(app: AppHandle, cloud: State<'_, Cloud>) -> Result<CloudStatus, String> {
    if let Ok((account, remote)) = cloud.remote() {
        let _ = remote.sign_out();
        KeychainTokens { server_url: account.server_url }.clear();
    }
    *cloud.account.lock().unwrap() = None;
    cloud.save_account()?;
    unlink_all(&cloud);
    cloud.emit(&app);
    Ok(cloud.status())
}

#[tauri::command]
pub async fn cloud_workspaces(cloud: State<'_, Cloud>) -> Result<Vec<RemoteWorkspace>, String> {
    let (_, remote) = cloud.remote()?;
    let list = remote.list_workspaces().map_err(|e| describe(&e))?;
    Ok(list
        .into_iter()
        .map(|w| RemoteWorkspace { id: w.id, name: w.name, role: w.role.map(role_name).unwrap_or_else(|| "viewer".into()) })
        .collect())
}

/// Links the folder open in goo to a workspace: an existing one (`workspace_id`) or a new one (`new_name`).
#[tauri::command]
pub async fn cloud_link(
    workspace_id: Option<String>,
    new_name: Option<String>,
    app: AppHandle,
    cloud: State<'_, Cloud>,
    state: State<'_, AppState>,
) -> Result<CloudStatus, String> {
    let root = state.root.lock().unwrap().clone().ok_or("Open a folder first.")?;
    let root_s = root.to_string_lossy().into_owned();
    let (_, remote) = cloud.remote()?;
    let (id, name, role) = match (workspace_id, new_name) {
        (Some(id), _) => {
            let ws = remote.get_workspace(&id).map_err(|e| describe(&e))?;
            (ws.id, ws.name, ws.role.map(role_name).unwrap_or_else(|| "viewer".into()))
        }
        (None, Some(name)) if !name.trim().is_empty() => {
            let ws = remote.create_workspace(name.trim(), "").map_err(|e| describe(&e))?;
            (ws.id, ws.name, "owner".into())
        }
        _ => return Err("Choose a workspace or enter a name for a new one.".into()),
    };
    {
        let mut links = cloud.links.lock().unwrap();
        if links.iter().any(|l| l.workspace_id == id && l.root != root_s) {
            return Err("That workspace is already linked to another folder on this computer.".into());
        }
        links.retain(|l| l.root != root_s);
        links.push(Link { root: root_s.clone(), workspace_id: id, workspace_name: name, role });
    }
    cloud.save_links()?;
    cloud.trigger(Some(root_s));
    cloud.emit(&app);
    Ok(cloud.status())
}

/// Stops syncing a folder. Its notes stay on disk and on the server.
#[tauri::command]
pub fn cloud_unlink(root: String, app: AppHandle, cloud: State<'_, Cloud>) -> Result<CloudStatus, String> {
    let removed: Vec<Link> = {
        let mut links = cloud.links.lock().unwrap();
        let (gone, kept): (Vec<_>, Vec<_>) = links.drain(..).partition(|l| l.root == root);
        *links = kept;
        gone
    };
    for l in removed {
        let _ = StateStore::new(cloud.state_dir(&l.workspace_id)).remove();
    }
    cloud.status.lock().unwrap().remove(&root);
    cloud.save_links()?;
    cloud.emit(&app);
    Ok(cloud.status())
}

/// Sync soon: after a save (`root` = that folder) or the "Sync now" button.
#[tauri::command]
pub fn cloud_sync_now(root: Option<String>, cloud: State<'_, Cloud>) {
    cloud.trigger(root);
}
