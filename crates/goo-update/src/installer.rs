//! In-app updates: download the latest release's installer for this installation, verify it, install it, restart.
//!
//! What is trusted: the release JSON comes from GitHub's latest-release endpoint over HTTPS, like the update check. The
//! installer must be the asset with the exact expected name, listed at github.com/perunok/goo/releases/download (or,
//! for a test feed, on the feed's own origin), and its bytes must match the asset's size and GitHub's SHA-256 `digest`:
//! no digest, a different size or a different hash means the file is deleted and nothing is installed.
//!
//! Installing (after the user chose "Restart to update"):
//! - AppImage: the file is replaced in place (same folder, atomic rename).
//! - .deb: `pkexec apt-get install` (the desktop asks for the administrator password; cancelling it cancels the
//!   update and goo keeps running). apt, not dpkg, so that a new dependency is installed too.
//! - Windows: the NSIS installer runs silently (`/S /UPDATE`) and starts goo again (`/R`).
//! - macOS: after goo quits, a helper mounts the disk image and replaces the app bundle.
//!
//! Linux and macOS restart through a detached helper that waits for this process to exit first (the single-instance
//! lock would otherwise hand the new start back to this process).

use crate::check::{agent, fetch_release_json};
use crate::detect::{asset_name, InstallMethod, InstallTarget};
use crate::release::{self, DOWNLOAD_PREFIX, LATEST_RELEASE_API};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Progress is pushed at most this often while downloading.
const PROGRESS_EVERY: Duration = Duration::from_millis(200);
/// A whole download may take this long (a stalled connection ends it eventually).
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(60 * 60);
const CHUNK: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Idle,
    Downloading,
    Verifying,
    Ready,
    Installing,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateState {
    /// How this installation updates itself; None: it cannot (see `unsupported_reason`).
    pub method: Option<InstallMethod>,
    pub unsupported_reason: Option<String>,
    pub phase: Phase,
    pub version: Option<String>,
    pub received: u64,
    pub total: Option<u64>,
    pub bytes_per_sec: Option<u64>,
    pub error: Option<String>,
}

/// Starting other programs, so tests can watch instead of running them.
pub trait Spawner: Send + Sync {
    /// Starts a process that outlives goo.
    fn spawn_detached(&self, program: &str, args: &[String]) -> io::Result<()>;
    /// Runs a process and waits for it; its exit code.
    fn run(&self, program: &str, args: &[String]) -> io::Result<i32>;
}

pub struct SystemSpawner;

impl Spawner for SystemSpawner {
    fn spawn_detached(&self, program: &str, args: &[String]) -> io::Result<()> {
        let mut cmd = std::process::Command::new(program);
        cmd.args(args).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const DETACHED_PROCESS: u32 = 0x0000_0008;
            const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
            cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
        }
        cmd.spawn().map(|_| ())
    }

    fn run(&self, program: &str, args: &[String]) -> io::Result<i32> {
        let status = std::process::Command::new(program).args(args).status()?;
        Ok(status.code().unwrap_or(1))
    }
}

pub struct Deps {
    pub current_version: String,
    /// GitHub's latest-release endpoint, or a test feed.
    pub feed_url: String,
    pub install: InstallTarget,
    /// `std::env::consts::ARCH`
    pub arch: String,
    /// Where installers are kept (`<app data>/updates`).
    pub download_dir: PathBuf,
    pub emit: Arc<dyn Fn(&UpdateState) + Send + Sync>,
    pub spawner: Arc<dyn Spawner>,
    /// This process and its executable (restart after quitting).
    pub pid: u32,
    pub exec_path: PathBuf,
    /// Quits goo through its normal quit path.
    pub quit: Arc<dyn Fn() + Send + Sync>,
}

struct Inner {
    deps: Deps,
    state: Mutex<UpdateState>,
    /// Bumped by every download and every cancel: results of an older worker are ignored.
    generation: AtomicU64,
    cancel: Mutex<Arc<AtomicBool>>,
    file: Mutex<Option<PathBuf>>,
}

#[derive(Clone)]
pub struct Installer(Arc<Inner>);

impl Installer {
    pub fn new(deps: Deps) -> Installer {
        let state = UpdateState {
            method: deps.install.method,
            unsupported_reason: deps.install.reason.clone(),
            phase: Phase::Idle,
            version: None,
            received: 0,
            total: None,
            bytes_per_sec: None,
            error: None,
        };
        Installer(Arc::new(Inner {
            deps,
            state: Mutex::new(state),
            generation: AtomicU64::new(0),
            cancel: Mutex::new(Arc::new(AtomicBool::new(false))),
            file: Mutex::new(None),
        }))
    }

    pub fn state(&self) -> UpdateState {
        self.0.state.lock().unwrap().clone()
    }

    /// Deletes installers left by an earlier run (nothing remembers a downloaded update across restarts).
    pub fn cleanup(&self) {
        let _ = fs::remove_dir_all(&self.0.deps.download_dir);
    }

    /// Applies `change` (when `generation` is still current) and tells the listener.
    fn set(&self, generation: Option<u64>, change: impl FnOnce(&mut UpdateState)) {
        let snapshot = {
            let mut state = self.0.state.lock().unwrap();
            if generation.is_some_and(|g| g != self.0.generation.load(Ordering::SeqCst)) {
                return;
            }
            change(&mut state);
            state.clone()
        };
        (self.0.deps.emit)(&snapshot);
    }

    /// Starts the download in the background; progress is pushed. Returns at once with the new state.
    pub fn download(&self) -> Result<UpdateState, String> {
        let Some(method) = self.0.deps.install.method else {
            return Err(self.0.deps.install.reason.clone().unwrap_or_else(|| "This installation cannot update itself.".into()));
        };
        if matches!(self.state().phase, Phase::Downloading | Phase::Verifying | Phase::Installing) {
            return Ok(self.state());
        }
        let generation = self.0.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let cancel = Arc::new(AtomicBool::new(false));
        *self.0.cancel.lock().unwrap() = cancel.clone();
        *self.0.file.lock().unwrap() = None;
        self.set(None, |s| {
            s.phase = Phase::Downloading;
            s.version = None;
            s.received = 0;
            s.total = None;
            s.bytes_per_sec = None;
            s.error = None;
        });
        let me = self.clone();
        std::thread::Builder::new()
            .name("goo-update-download".into())
            .spawn(move || {
                if let Err(message) = me.run_download(method, generation, &cancel) {
                    if !cancel.load(Ordering::SeqCst) {
                        me.set(Some(generation), |s| {
                            s.phase = Phase::Error;
                            s.bytes_per_sec = None;
                            s.error = Some(message);
                        });
                    }
                }
            })
            .map_err(|e| format!("Could not start the download: {e}"))?;
        Ok(self.state())
    }

    /// Stops a running download (not while verifying or installing).
    pub fn cancel(&self) -> UpdateState {
        if self.state().phase == Phase::Downloading {
            self.0.cancel.lock().unwrap().store(true, Ordering::SeqCst);
            self.0.generation.fetch_add(1, Ordering::SeqCst);
            self.set(None, |s| {
                s.phase = Phase::Idle;
                s.received = 0;
                s.total = None;
                s.bytes_per_sec = None;
                s.error = None;
            });
        }
        self.state()
    }

    fn run_download(&self, method: InstallMethod, generation: u64, cancel: &AtomicBool) -> Result<(), String> {
        let deps = &self.0.deps;
        let current = &deps.current_version;
        let body = fetch_release_json(&deps.feed_url, current)
            .map_err(|e| format!("Could not get the release information from GitHub: {e}."))?
            .ok_or("No release has been published yet.")?;
        let release = release::parse_release(&body).ok_or("GitHub sent release information goo does not understand.")?;
        if !release::is_newer(&release.version, current) {
            return Err(format!("goo {current} is already the latest release."));
        }
        let name = asset_name(method, &release.version, &deps.arch);
        let asset = release::parse_assets(&body)
            .into_iter()
            .find(|a| a.name == name)
            .ok_or_else(|| format!("Release {} has no {name}.", release.version))?;
        if !self.allowed_url(&asset.url) {
            return Err(format!("Refusing to download the update from {}.", asset.url));
        }
        let expected = asset.sha256.clone().ok_or("GitHub gave no checksum for the installer, so it cannot be verified.")?;
        self.set(Some(generation), |s| {
            s.version = Some(release.version.clone());
            s.total = Some(asset.size);
        });

        fs::create_dir_all(&deps.download_dir).map_err(|e| format!("Could not prepare the download folder: {e}"))?;
        // Earlier downloads (other versions, broken parts) are not needed any more.
        if let Ok(entries) = fs::read_dir(&deps.download_dir) {
            for entry in entries.flatten() {
                if entry.file_name().to_string_lossy() != name {
                    let path = entry.path();
                    let _ = if path.is_dir() { fs::remove_dir_all(&path) } else { fs::remove_file(&path) };
                }
            }
        }
        let file = deps.download_dir.join(&name);
        let part = deps.download_dir.join(format!("{name}.part"));

        let allow_http = deps.feed_url != LATEST_RELEASE_API;
        let mut response = agent(allow_http, Some(DOWNLOAD_TIMEOUT))
            .get(&asset.url)
            .header("User-Agent", &format!("goo/{current}"))
            .call()
            .map_err(|_| "The download failed: could not reach GitHub.".to_string())?;
        let code = response.status().as_u16();
        if code != 200 {
            return Err(format!("The download failed: HTTP {code}."));
        }

        let result = self.stream_to(&mut response.body_mut().with_config().limit(asset.size + 1).reader(), &part, asset.size, generation, cancel);
        let (received, digest) = match result {
            Ok(v) => v,
            Err(e) => {
                let _ = fs::remove_file(&part);
                return Err(e);
            }
        };
        if cancel.load(Ordering::SeqCst) {
            let _ = fs::remove_file(&part);
            return Ok(());
        }
        self.set(Some(generation), |s| {
            s.phase = Phase::Verifying;
            s.received = received;
            s.bytes_per_sec = None;
        });
        if received != asset.size || digest != expected {
            let _ = fs::remove_file(&part);
            return Err("The downloaded installer does not match GitHub's checksum; it was discarded. Try again.".into());
        }
        fs::rename(&part, &file).map_err(|e| format!("Could not save the installer: {e}"))?;
        *self.0.file.lock().unwrap() = Some(file);
        self.set(Some(generation), |s| s.phase = Phase::Ready);
        Ok(())
    }

    /// Copies `reader` to `part`, hashing as it goes and reporting progress. Returns the bytes and their SHA-256.
    fn stream_to(&self, reader: &mut dyn Read, part: &Path, expected_size: u64, generation: u64, cancel: &AtomicBool) -> Result<(u64, String), String> {
        let mut out = fs::File::create(part).map_err(|e| format!("Could not write the installer: {e}"))?;
        let mut hash = Sha256::new();
        let mut buf = vec![0u8; CHUNK];
        let mut received = 0u64;
        let mut last_push = Instant::now() - PROGRESS_EVERY;
        let mut window_start = Instant::now();
        let mut window_bytes = 0u64;
        let mut speed: Option<u64> = None;
        loop {
            if cancel.load(Ordering::SeqCst) {
                return Ok((received, String::new()));
            }
            let n = reader.read(&mut buf).map_err(|_| "The download was interrupted.".to_string())?;
            if n == 0 {
                break;
            }
            received += n as u64;
            if received > expected_size {
                return Err("The download is larger than GitHub said; it was discarded.".into());
            }
            hash.update(&buf[..n]);
            out.write_all(&buf[..n]).map_err(|e| format!("Could not write the installer: {e}"))?;
            window_bytes += n as u64;
            let now = Instant::now();
            if now.duration_since(window_start) >= Duration::from_secs(1) {
                speed = Some((window_bytes as f64 / now.duration_since(window_start).as_secs_f64()) as u64);
                window_start = now;
                window_bytes = 0;
            }
            if now.duration_since(last_push) >= PROGRESS_EVERY {
                last_push = now;
                self.set(Some(generation), |s| {
                    s.received = received;
                    s.bytes_per_sec = speed;
                });
            }
        }
        out.sync_all().map_err(|e| format!("Could not write the installer: {e}"))?;
        let digest: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
        Ok((received, digest))
    }

    fn allowed_url(&self, url: &str) -> bool {
        if url.starts_with(DOWNLOAD_PREFIX) {
            return true;
        }
        // A test feed (GOO_UPDATE_FEED_URL) may serve its installers itself.
        let feed = &self.0.deps.feed_url;
        feed != LATEST_RELEASE_API && origin(url).is_some_and(|o| Some(o) == origin(feed))
    }

    /// Installs the downloaded update and quits (a helper starts the new version). On failure the state says why and
    /// goo keeps running.
    pub fn install(&self) -> Result<UpdateState, String> {
        let method = self.0.deps.install.method;
        let file = self.0.file.lock().unwrap().clone();
        let (Some(method), Some(file)) = (method, file) else {
            return Err("No downloaded update is ready to install.".into());
        };
        if self.state().phase != Phase::Ready || !file.exists() {
            return Err("No downloaded update is ready to install.".into());
        }
        self.set(None, |s| {
            s.phase = Phase::Installing;
            s.error = None;
        });
        match self.run_install(method, &file) {
            Ok(()) => {
                (self.0.deps.quit)();
                Ok(self.state())
            }
            Err(message) => {
                self.set(None, |s| {
                    s.phase = Phase::Error;
                    s.error = Some(message.clone());
                });
                Err(message)
            }
        }
    }

    fn run_install(&self, method: InstallMethod, file: &Path) -> Result<(), String> {
        let deps = &self.0.deps;
        let file_str = file.to_string_lossy().into_owned();
        let target = deps.install.target.as_deref();
        match method {
            InstallMethod::AppImage => {
                let target = target.ok_or("The AppImage to replace is not known.")?;
                let next = PathBuf::from(format!("{}.new", target.display()));
                fs::copy(file, &next).map_err(|e| format!("Could not copy the update next to the AppImage: {e}"))?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&next, fs::Permissions::from_mode(0o755)).map_err(|e| format!("Could not make the update executable: {e}"))?;
                }
                fs::rename(&next, target).map_err(|e| {
                    let _ = fs::remove_file(&next);
                    format!("Could not replace the AppImage: {e}")
                })?;
                self.restart_after_quit("exec \"$0\"", &[target.to_string_lossy().into_owned()])
            }
            InstallMethod::Deb => {
                let args = ["env", "DEBIAN_FRONTEND=noninteractive", "apt-get", "install", "-y", &file_str].map(String::from);
                let code = deps.spawner.run("pkexec", &args).map_err(|e| format!("Could not start pkexec: {e}"))?;
                if code == 126 || code == 127 {
                    return Err("Installing was cancelled: the administrator password was not given.".into());
                }
                if code != 0 {
                    return Err(format!("apt could not install the update (exit code {code})."));
                }
                self.restart_after_quit("exec \"$0\"", &[deps.exec_path.to_string_lossy().into_owned()])
            }
            InstallMethod::Nsis => deps
                .spawner
                .spawn_detached(&file_str, &["/S".into(), "/UPDATE".into(), "/R".into()])
                .map_err(|e| format!("Could not start the installer: {e}")),
            InstallMethod::Dmg => {
                let bundle = target.ok_or("The app bundle to replace is not known.")?;
                let script = [
                    "mnt=$(mktemp -d)",
                    "hdiutil attach -nobrowse -quiet -mountpoint \"$mnt\" \"$0\" || exit 1",
                    "app=$(ls -d \"$mnt\"/*.app | head -n 1)",
                    "[ -n \"$app\" ] && ditto \"$app\" \"$1.new\" && rm -rf \"$1\" && mv \"$1.new\" \"$1\"",
                    "hdiutil detach -quiet \"$mnt\"",
                    "open \"$1\"",
                ]
                .join("; ");
                self.restart_after_quit(&script, &[file_str, bundle.to_string_lossy().into_owned()])
            }
        }
    }

    /// A detached shell waits until this process has exited, then runs `script` (`$0`, `$1` = `args`).
    fn restart_after_quit(&self, script: &str, args: &[String]) -> Result<(), String> {
        let mut sh_args = vec!["-c".to_string(), format!("while kill -0 {} 2>/dev/null; do sleep 0.3; done; {script}", self.0.deps.pid)];
        sh_args.extend_from_slice(args);
        self.0.deps.spawner.spawn_detached("/bin/sh", &sh_args).map_err(|e| format!("Could not prepare the restart: {e}"))
    }
}

/// `scheme://host:port` of a URL (what must match for a test feed's own downloads).
fn origin(url: &str) -> Option<&str> {
    let (scheme, rest) = url.split_once("://")?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let host = &rest[..end];
    (!host.is_empty() && !host.contains('@')).then(|| &url[..scheme.len() + 3 + end])
}
