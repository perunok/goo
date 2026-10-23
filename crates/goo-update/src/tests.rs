use crate::check::{self, CheckError};
use crate::detect::{asset_name, detect_install, DetectInput, InstallMethod, InstallTarget, Probe};
use crate::installer::{Deps, Installer, Phase, Spawner, UpdateState};
use crate::release::{self, parse_assets, parse_iso8601, parse_release};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// a fake GitHub
// ---------------------------------------------------------------------------

enum Reply {
    Json(String),
    Bytes(Vec<u8>),
    Status(u16),
    /// Sends the bytes slowly (chunk by chunk) so a download can be cancelled midway.
    Slow(Vec<u8>),
}

struct SlowReader {
    data: Vec<u8>,
    pos: usize,
}

impl Read for SlowReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        std::thread::sleep(Duration::from_millis(40));
        let n = buf.len().min(4096).min(self.data.len() - self.pos);
        buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

struct TestServer {
    base: String,
    routes: Arc<Mutex<HashMap<String, Reply>>>,
    server: Arc<tiny_http::Server>,
    thread: Option<JoinHandle<()>>,
}

impl TestServer {
    fn start() -> TestServer {
        let server = Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
        let base = format!("http://127.0.0.1:{}", server.server_addr().to_ip().unwrap().port());
        let routes: Arc<Mutex<HashMap<String, Reply>>> = Arc::default();
        let (s, r) = (server.clone(), routes.clone());
        let thread = std::thread::spawn(move || {
            for req in s.incoming_requests() {
                let r = r.clone();
                std::thread::spawn(move || {
                    let url = req.url().to_string();
                    let reply = {
                        let routes = r.lock().unwrap();
                        match routes.get(&url) {
                            None => Reply::Status(404),
                            Some(Reply::Json(j)) => Reply::Json(j.clone()),
                            Some(Reply::Bytes(b)) => Reply::Bytes(b.clone()),
                            Some(Reply::Status(c)) => Reply::Status(*c),
                            Some(Reply::Slow(b)) => Reply::Slow(b.clone()),
                        }
                    };
                    let _ = match reply {
                        Reply::Json(j) => req.respond(tiny_http::Response::from_string(j).with_header(tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap())),
                        Reply::Bytes(b) => req.respond(tiny_http::Response::from_data(b)),
                        Reply::Status(c) => req.respond(tiny_http::Response::from_string("x").with_status_code(c)),
                        Reply::Slow(b) => {
                            let len = b.len();
                            req.respond(tiny_http::Response::new(tiny_http::StatusCode(200), vec![], SlowReader { data: b, pos: 0 }, Some(len), None))
                        }
                    };
                });
            }
        });
        TestServer { base, routes, server, thread: Some(thread) }
    }

    fn feed(&self) -> String {
        format!("{}/latest", self.base)
    }

    fn set(&self, path: &str, reply: Reply) {
        self.routes.lock().unwrap().insert(path.to_string(), reply);
    }

    /// Publishes release JSON for `tag` with these assets: (name, bytes, digest override). The asset URL is on this server.
    fn publish(&self, tag: &str, assets: &[(&str, Vec<u8>, Option<String>)]) {
        let list: Vec<Value> = assets
            .iter()
            .map(|(name, bytes, digest)| {
                let mut a = json!({
                    "name": name,
                    "browser_download_url": format!("{}/download/{name}", self.base),
                    "size": bytes.len(),
                });
                match digest {
                    Some(d) if d.is_empty() => {}
                    Some(d) => a["digest"] = json!(d),
                    None => a["digest"] = json!(format!("sha256:{}", sha_hex(bytes))),
                }
                a
            })
            .collect();
        self.set("/latest", Reply::Json(json!({ "tag_name": tag, "published_at": "2026-10-12T13:16:19Z", "assets": list }).to_string()));
        for (name, bytes, _) in assets {
            self.set(&format!("/download/{name}"), Reply::Bytes(bytes.clone()));
        }
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.server.unblock();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn sha_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

// ---------------------------------------------------------------------------
// release JSON and versions
// ---------------------------------------------------------------------------

#[test]
fn releases_are_read_defensively() {
    let r = parse_release(&json!({ "tag_name": "v0.5.0", "published_at": "2026-10-12T13:16:19Z", "html_url": "https://evil.example/x" })).unwrap();
    assert_eq!(r.version, "0.5.0");
    assert_eq!(r.url, "https://github.com/perunok/goo/releases/tag/v0.5.0"); // built from the version, not from the answer
    assert_eq!(r.published_at, Some(1_791_810_979));
    assert_eq!(parse_release(&json!({ "tag_name": "0.5.0" })).unwrap().published_at, None);
    for bad in [
        json!({ "tag_name": "v0.5.0-beta.1" }),
        json!({ "tag_name": "v0.5.0+build7" }),
        json!({ "tag_name": "latest" }),
        json!({ "tag_name": 5 }),
        json!({ "tag_name": "v0.5.0", "draft": true }),
        json!({ "tag_name": "v0.5.0", "prerelease": true }),
        json!([]),
        json!("v0.5.0"),
    ] {
        assert!(parse_release(&bad).is_none(), "{bad}");
    }
}

#[test]
fn timestamps_and_versions() {
    assert_eq!(parse_iso8601("2000-02-29T00:00:00Z"), Some(951_782_400));
    assert_eq!(parse_iso8601("1970-01-01T00:00:01Z"), Some(1));
    for bad in ["", "2026-13-01T00:00:00Z", "2026-10-12 13:16:19", "2026-10-12T13:16:19+02:00", "yesterday"] {
        assert_eq!(parse_iso8601(bad), None, "{bad}");
    }
    assert!(release::is_newer("0.10.0", "0.9.0")); // numeric, not textual
    assert!(release::is_newer("v1.0.0", "0.99.99"));
    assert!(!release::is_newer("0.4.0", "0.4.0"));
    assert!(!release::is_newer("0.3.9", "0.4.0"));
    assert!(!release::is_newer("0.5.0-rc.1", "0.4.0")); // pre-releases are never offered
    assert!(!release::is_newer("0.5.0", "dev")); // an unreadable running version offers nothing
}

#[test]
fn assets_need_a_name_url_size_and_a_valid_digest() {
    let hash = "a".repeat(64);
    let assets = parse_assets(&json!({ "assets": [
        { "name": "ok.deb", "browser_download_url": "https://x/ok.deb", "size": 10, "digest": format!("sha256:{}", hash.to_uppercase()) },
        { "name": "nodigest.deb", "browser_download_url": "https://x/n.deb", "size": 10 },
        { "name": "shortdigest.deb", "browser_download_url": "https://x/s.deb", "size": 10, "digest": "sha256:abc" },
        { "name": "md5.deb", "browser_download_url": "https://x/m.deb", "size": 10, "digest": format!("md5:{hash}") },
        { "name": "nourl.deb", "size": 10 },
        { "browser_download_url": "https://x/noname", "size": 10 },
        { "name": "nosize.deb", "browser_download_url": "https://x/ns.deb" },
    ] }));
    let digests: Vec<_> = assets.iter().map(|a| (a.name.as_str(), a.sha256.clone())).collect();
    assert_eq!(digests, [("ok.deb", Some(hash)), ("nodigest.deb", None), ("shortdigest.deb", None), ("md5.deb", None)]);
    assert!(parse_assets(&json!({})).is_empty());
}

#[test]
fn feed_urls() {
    assert_eq!(check::feed_url(None, false).unwrap().as_deref(), Some(release::LATEST_RELEASE_API));
    assert_eq!(check::feed_url(None, true).unwrap(), None); // checks turned off
    assert_eq!(check::feed_url(Some(" http://127.0.0.1:9/latest "), true).unwrap().as_deref(), Some("http://127.0.0.1:9/latest"));
    assert_eq!(check::feed_url(Some(""), false).unwrap().as_deref(), Some(release::LATEST_RELEASE_API));
    assert!(check::feed_url(Some("file:///etc/passwd"), false).is_err());
    assert!(check::feed_url(Some("ftp://x"), false).is_err());
}

// ---------------------------------------------------------------------------
// the check
// ---------------------------------------------------------------------------

#[test]
fn check_answers_newer_same_older_and_no_release_yet() {
    let s = TestServer::start();
    s.publish("v0.5.0", &[]);
    let r = check::check("0.4.0", &s.feed()).unwrap();
    assert!(r.update_available);
    assert_eq!(r.latest.as_ref().unwrap().version, "0.5.0");
    assert_eq!(r.current_version, "0.4.0");
    assert!(r.checked_at > 1_700_000_000);
    assert!(!check::check("0.5.0", &s.feed()).unwrap().update_available);
    assert!(!check::check("0.6.0", &s.feed()).unwrap().update_available);
    s.set("/latest", Reply::Status(404));
    let none = check::check("0.4.0", &s.feed()).unwrap();
    assert!(none.latest.is_none() && !none.update_available);
}

#[test]
fn check_errors_are_told_apart() {
    let s = TestServer::start();
    s.set("/latest", Reply::Status(403)); // GitHub's rate limit
    assert!(matches!(check::check("0.4.0", &s.feed()), Err(CheckError::Network(m)) if m.contains("403")));
    s.set("/latest", Reply::Json("not json".into()));
    assert!(matches!(check::check("0.4.0", &s.feed()), Err(CheckError::Unreadable(_))));
    s.set("/latest", Reply::Json(json!({ "tag_name": "nightly" }).to_string()));
    assert!(matches!(check::check("0.4.0", &s.feed()), Err(CheckError::Unreadable(_))));
    s.set("/latest", Reply::Json(json!({ "tag_name": "v0.5.0", "body": "x".repeat(1_100_000) }).to_string()));
    assert!(matches!(check::check("0.4.0", &s.feed()), Err(CheckError::Unreadable(m)) if m.contains("too large")));
    // nothing listens there
    let dead = format!("http://127.0.0.1:{}/latest", { std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port() });
    assert!(matches!(check::check("0.4.0", &dead), Err(CheckError::Network(_))));
}

// ---------------------------------------------------------------------------
// how an installation updates itself
// ---------------------------------------------------------------------------

#[derive(Default)]
struct FakeProbe {
    writable: bool,
    existing: Vec<&'static str>,
    commands: Vec<&'static str>,
}

impl Probe for FakeProbe {
    fn is_writable(&self, _dir: &Path) -> bool {
        self.writable
    }
    fn exists(&self, path: &Path) -> bool {
        self.existing.iter().any(|p| Path::new(p) == path)
    }
    fn has_command(&self, name: &str) -> bool {
        self.commands.contains(&name)
    }
}

fn detect(os: &str, arch: &str, packaged: bool, appimage: Option<&str>, exec: &str, probe: &FakeProbe) -> InstallTarget {
    let appimage = appimage.map(PathBuf::from);
    let exec = PathBuf::from(exec);
    detect_install(&DetectInput { os, arch, is_packaged: packaged, appimage: appimage.as_deref(), exec_path: &exec, probe })
}

#[test]
fn installations_that_can_update_themselves() {
    let ok = FakeProbe { writable: true, existing: vec!["/var/lib/dpkg/info/goo-notes.list", "/opt/goo/uninstall.exe"], commands: vec!["pkexec", "apt-get"] };
    let appimage = detect("linux", "x86_64", true, Some("/home/a/goo.AppImage"), "/tmp/.mount_goo/usr/bin/goo", &ok);
    assert_eq!((appimage.method, appimage.target.as_deref()), (Some(InstallMethod::AppImage), Some(Path::new("/home/a/goo.AppImage"))));
    assert_eq!(detect("linux", "aarch64", true, None, "/usr/bin/goo-notes", &ok).method, Some(InstallMethod::Deb));
    // (host-neutral paths: backslashes only separate folders on Windows itself)
    assert_eq!(detect("windows", "x86_64", true, None, "/opt/goo/goo.exe", &ok).method, Some(InstallMethod::Nsis));
    let mac = detect("macos", "aarch64", true, None, "/Applications/goo.app/Contents/MacOS/goo", &ok);
    assert_eq!((mac.method, mac.target.as_deref()), (Some(InstallMethod::Dmg), Some(Path::new("/Applications/goo.app"))));
}

#[test]
fn installations_that_cannot_say_why() {
    let ok = FakeProbe { writable: true, existing: vec!["/var/lib/dpkg/info/goo-notes.list"], commands: vec!["pkexec", "apt-get"] };
    let read_only = FakeProbe { writable: false, ..FakeProbe { existing: vec![], commands: vec![], writable: false } };
    let no_pkexec = FakeProbe { writable: true, existing: vec!["/var/lib/dpkg/info/goo-notes.list"], commands: vec!["apt-get"] };
    let why = |t: InstallTarget| {
        assert_eq!(t.method, None);
        t.reason.unwrap()
    };
    assert!(why(detect("linux", "x86_64", false, None, "/usr/bin/goo-notes", &ok)).contains("development build"));
    assert!(why(detect("linux", "x86_64", true, Some("/opt/goo.AppImage"), "/x", &read_only)).contains("not writable"));
    assert!(why(detect("linux", "x86_64", true, None, "/usr/bin/goo-notes", &no_pkexec)).contains("pkexec"));
    // an .rpm, a hand-built binary, or Ubuntu's unrelated `goo` package: never touched
    assert!(why(detect("linux", "x86_64", true, None, "/usr/bin/goo-notes", &read_only)).contains(".rpm"));
    assert!(why(detect("linux", "x86_64", true, None, "/usr/bin/goo", &ok)).contains("not installed from"));
    assert!(why(detect("linux", "riscv64", true, None, "/usr/bin/goo-notes", &ok)).contains("Linux only"));
    // uninstall.exe exists for /opt/goo but not next to this goo.exe: an .msi install
    assert!(why(detect("windows", "x86_64", true, None, "/opt/other/goo.exe", &ok)).contains(".msi"));
    assert!(why(detect("windows", "aarch64", true, None, "/opt/goo/goo.exe", &ok)).contains("x64"));
    assert!(why(detect("macos", "x86_64", true, None, "/usr/local/bin/goo", &ok)).contains("app bundle"));
    assert!(why(detect("macos", "x86_64", true, None, "/Applications/goo.app/Contents/MacOS/goo", &read_only)).contains("not writable"));
    assert!(why(detect("freebsd", "x86_64", true, None, "/usr/bin/goo", &ok)).contains("freebsd"));
}

#[test]
fn asset_names_match_the_release_workflow() {
    use InstallMethod::*;
    assert_eq!(asset_name(Deb, "0.5.0", "x86_64"), "goo-0.5.0-linux-amd64.deb");
    assert_eq!(asset_name(Deb, "0.5.0", "aarch64"), "goo-0.5.0-linux-arm64.deb");
    assert_eq!(asset_name(AppImage, "0.5.0", "x86_64"), "goo-0.5.0-linux-x86_64.AppImage");
    assert_eq!(asset_name(AppImage, "0.5.0", "aarch64"), "goo-0.5.0-linux-aarch64.AppImage");
    assert_eq!(asset_name(Nsis, "0.5.0", "x86_64"), "goo-0.5.0-win-x64.exe");
    assert_eq!(asset_name(Dmg, "0.5.0", "aarch64"), "goo-0.5.0-mac-arm64.dmg");
    assert_eq!(asset_name(Dmg, "0.5.0", "x86_64"), "goo-0.5.0-mac-x64.dmg");
}

// ---------------------------------------------------------------------------
// downloading and installing
// ---------------------------------------------------------------------------

#[derive(Default)]
struct FakeSpawner {
    detached: Mutex<Vec<(String, Vec<String>)>>,
    runs: Mutex<Vec<(String, Vec<String>)>>,
    run_code: AtomicI32,
}

impl Spawner for FakeSpawner {
    fn spawn_detached(&self, program: &str, args: &[String]) -> io::Result<()> {
        self.detached.lock().unwrap().push((program.into(), args.into()));
        Ok(())
    }
    fn run(&self, program: &str, args: &[String]) -> io::Result<i32> {
        self.runs.lock().unwrap().push((program.into(), args.into()));
        Ok(self.run_code.load(Ordering::SeqCst))
    }
}

struct Rig {
    installer: Installer,
    spawner: Arc<FakeSpawner>,
    quit: Arc<AtomicBool>,
    events: Arc<Mutex<Vec<UpdateState>>>,
    dir: tempfile::TempDir,
}

fn rig(feed: &str, method: Option<InstallMethod>, target: Option<PathBuf>) -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let spawner = Arc::new(FakeSpawner::default());
    let quit = Arc::new(AtomicBool::new(false));
    let events: Arc<Mutex<Vec<UpdateState>>> = Arc::default();
    let (ev, q) = (events.clone(), quit.clone());
    let installer = Installer::new(Deps {
        current_version: "0.4.0".into(),
        feed_url: feed.into(),
        install: InstallTarget { method, reason: method.is_none().then(|| "not here".into()), target },
        arch: "x86_64".into(),
        download_dir: dir.path().join("updates"),
        emit: Arc::new(move |s| ev.lock().unwrap().push(s.clone())),
        spawner: spawner.clone(),
        pid: 4242,
        exec_path: PathBuf::from("/usr/bin/goo-notes"),
        quit: Arc::new(move || q.store(true, Ordering::SeqCst)),
    });
    Rig { installer, spawner, quit, events, dir }
}

fn wait_until(installer: &Installer, done: impl Fn(&UpdateState) -> bool) -> UpdateState {
    let start = Instant::now();
    loop {
        let s = installer.state();
        if done(&s) {
            return s;
        }
        assert!(start.elapsed() < Duration::from_secs(20), "timed out in {:?}", s.phase);
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn finished(s: &UpdateState) -> bool {
    matches!(s.phase, Phase::Ready | Phase::Error)
}

const DEB: &str = "goo-0.5.0-linux-amd64.deb";

fn payload(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i * 31 % 251) as u8).collect()
}

#[test]
fn downloads_verifies_and_keeps_only_the_new_installer() {
    let s = TestServer::start();
    let bytes = payload(300_000);
    s.publish("v0.5.0", &[(DEB, bytes.clone(), None), ("goo-0.5.0-linux-arm64.deb", payload(10), None)]);
    let r = rig(&s.feed(), Some(InstallMethod::Deb), None);
    let dir = r.dir.path().join("updates");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("goo-0.3.0-linux-amd64.deb"), b"old").unwrap(); // an older download
    std::fs::write(dir.join("junk.part"), b"half").unwrap();

    assert_eq!(r.installer.download().unwrap().phase, Phase::Downloading);
    let end = wait_until(&r.installer, finished);
    assert_eq!(end.phase, Phase::Ready, "{:?}", end.error);
    assert_eq!((end.version.as_deref(), end.received, end.total), (Some("0.5.0"), 300_000, Some(300_000)));
    assert_eq!(std::fs::read(dir.join(DEB)).unwrap(), bytes);
    let mut left: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    left.sort();
    assert_eq!(left, [DEB]); // old versions and parts are gone
    // the listener saw it go through the phases, with growing byte counts
    let events = r.events.lock().unwrap();
    let phases: Vec<Phase> = events.iter().map(|e| e.phase).collect();
    let (d, v, ready) = (phases.iter().position(|p| *p == Phase::Downloading), phases.iter().position(|p| *p == Phase::Verifying), phases.iter().position(|p| *p == Phase::Ready));
    assert!(d < v && v < ready, "{phases:?}");
    let mut last = 0;
    for e in events.iter() {
        assert!(e.received >= last);
        last = e.received;
    }
    assert!(!r.quit.load(Ordering::SeqCst), "downloading never quits");
}

fn expect_refusal(s: &TestServer, assets: &[(&str, Vec<u8>, Option<String>)], message: &str) {
    s.publish("v0.5.0", assets);
    let r = rig(&s.feed(), Some(InstallMethod::Deb), None);
    r.installer.download().unwrap();
    let end = wait_until(&r.installer, finished);
    assert_eq!(end.phase, Phase::Error, "expected a refusal containing {message:?}");
    assert!(end.error.as_deref().unwrap().contains(message), "{:?} should contain {message:?}", end.error);
    let dir = r.dir.path().join("updates");
    let leftovers: Vec<_> = std::fs::read_dir(&dir).map(|d| d.map(|e| e.unwrap().file_name()).collect()).unwrap_or_default();
    assert!(leftovers.is_empty(), "nothing may stay behind: {leftovers:?}");
    assert!(r.installer.install().is_err(), "a refused download can never be installed");
    assert!(!r.quit.load(Ordering::SeqCst));
}

#[test]
fn a_tampered_or_unverifiable_installer_is_discarded() {
    let s = TestServer::start();
    let bytes = payload(5_000);
    // GitHub's digest is for other bytes
    expect_refusal(&s, &[(DEB, bytes.clone(), Some(format!("sha256:{}", sha_hex(b"something else"))))], "does not match GitHub's checksum");
    // no digest at all
    expect_refusal(&s, &[(DEB, bytes.clone(), Some(String::new()))], "no checksum");
    // the server sends fewer bytes than the asset's size
    s.publish("v0.5.0", &[(DEB, bytes.clone(), None)]);
    s.set(&format!("/download/{DEB}"), Reply::Bytes(bytes[..4_000].to_vec()));
    let r = rig(&s.feed(), Some(InstallMethod::Deb), None);
    r.installer.download().unwrap();
    let end = wait_until(&r.installer, finished);
    assert_eq!(end.phase, Phase::Error);
    assert!(!r.dir.path().join("updates").join(DEB).exists());
    // the server sends more than the asset's size
    s.set(&format!("/download/{DEB}"), Reply::Bytes(payload(9_000)));
    let r = rig(&s.feed(), Some(InstallMethod::Deb), None);
    r.installer.download().unwrap();
    let end = wait_until(&r.installer, finished);
    assert!(end.error.unwrap().contains("larger than GitHub said"));
    assert!(std::fs::read_dir(r.dir.path().join("updates")).unwrap().next().is_none());
}

#[test]
fn only_the_right_release_asset_from_the_right_place() {
    let s = TestServer::start();
    // no installer for this platform
    expect_refusal(&s, &[("goo-0.5.0-linux-arm64.deb", payload(10), None)], &format!("has no {DEB}"));
    // an asset hosted somewhere else than the feed (or GitHub's release downloads)
    s.publish("v0.5.0", &[(DEB, payload(10), None)]);
    let body = json!({ "tag_name": "v0.5.0", "assets": [{ "name": DEB, "browser_download_url": "http://127.0.0.1:1/evil.deb", "size": 10,
        "digest": format!("sha256:{}", sha_hex(&payload(10))) }] });
    s.set("/latest", Reply::Json(body.to_string()));
    let r = rig(&s.feed(), Some(InstallMethod::Deb), None);
    r.installer.download().unwrap();
    assert!(wait_until(&r.installer, finished).error.unwrap().contains("Refusing to download"));
    // already up to date
    s.publish("v0.4.0", &[(DEB, payload(10), None)]);
    let r = rig(&s.feed(), Some(InstallMethod::Deb), None);
    r.installer.download().unwrap();
    assert!(wait_until(&r.installer, finished).error.unwrap().contains("already the latest"));
    // GitHub down
    s.set("/latest", Reply::Status(500));
    let r = rig(&s.feed(), Some(InstallMethod::Deb), None);
    r.installer.download().unwrap();
    assert!(wait_until(&r.installer, finished).error.unwrap().contains("500"));
}

#[test]
fn an_installation_that_cannot_update_does_not_download() {
    let s = TestServer::start();
    s.publish("v0.5.0", &[(DEB, payload(10), None)]);
    let r = rig(&s.feed(), None, None);
    assert_eq!(r.installer.download().unwrap_err(), "not here");
    assert_eq!(r.installer.state().unsupported_reason.as_deref(), Some("not here"));
    assert!(r.installer.install().is_err());
}

#[test]
fn a_download_can_be_cancelled_and_started_again() {
    let s = TestServer::start();
    let bytes = payload(400_000); // ~4 s at the slow server's pace
    s.publish("v0.5.0", &[(DEB, bytes.clone(), None)]);
    s.set(&format!("/download/{DEB}"), Reply::Slow(bytes.clone()));
    let r = rig(&s.feed(), Some(InstallMethod::Deb), None);
    r.installer.download().unwrap();
    wait_until(&r.installer, |s| s.received > 0);
    let cancelled = r.installer.cancel();
    assert_eq!((cancelled.phase, cancelled.received, cancelled.error), (Phase::Idle, 0, None));
    std::thread::sleep(Duration::from_millis(400)); // the abandoned worker must not change the state any more
    assert_eq!(r.installer.state().phase, Phase::Idle);
    assert!(!r.dir.path().join("updates").join(DEB).exists());
    // starting again works (this time at normal speed)
    s.set(&format!("/download/{DEB}"), Reply::Bytes(bytes.clone()));
    r.installer.download().unwrap();
    assert_eq!(wait_until(&r.installer, finished).phase, Phase::Ready);
    assert_eq!(std::fs::read(r.dir.path().join("updates").join(DEB)).unwrap(), bytes);
    // cancelling when nothing is downloading changes nothing
    assert_eq!(r.installer.cancel().phase, Phase::Ready);
}

fn ready(method: InstallMethod, name: &str, target: Option<PathBuf>) -> (TestServer, Rig, Vec<u8>) {
    let s = TestServer::start();
    let bytes = payload(20_000);
    s.publish("v0.5.0", &[(name, bytes.clone(), None)]);
    let r = rig(&s.feed(), Some(method), target);
    r.installer.download().unwrap();
    assert_eq!(wait_until(&r.installer, finished).phase, Phase::Ready);
    (s, r, bytes)
}

#[test]
fn installing_nothing_that_is_not_ready() {
    let s = TestServer::start();
    let r = rig(&s.feed(), Some(InstallMethod::Deb), None);
    assert!(r.installer.install().unwrap_err().contains("No downloaded update"));
    assert!(!r.quit.load(Ordering::SeqCst));
    assert!(r.spawner.runs.lock().unwrap().is_empty() && r.spawner.detached.lock().unwrap().is_empty());
}

#[test]
fn deb_goes_through_pkexec_apt_and_restarts_after_quitting() {
    let (_s, r, _) = ready(InstallMethod::Deb, DEB, None);
    let file = r.dir.path().join("updates").join(DEB);
    let end = r.installer.install().unwrap();
    assert_eq!(end.phase, Phase::Installing);
    assert!(r.quit.load(Ordering::SeqCst));
    let runs = r.spawner.runs.lock().unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].0, "pkexec");
    assert_eq!(runs[0].1, ["env", "DEBIAN_FRONTEND=noninteractive", "apt-get", "install", "-y", file.to_str().unwrap()]);
    let detached = r.spawner.detached.lock().unwrap();
    assert_eq!(detached.len(), 1);
    assert_eq!(detached[0].0, "/bin/sh");
    assert_eq!(detached[0].1[0], "-c");
    assert!(detached[0].1[1].starts_with("while kill -0 4242 2>/dev/null; do sleep 0.3; done; exec \"$0\""), "{}", detached[0].1[1]);
    assert_eq!(detached[0].1[2], "/usr/bin/goo-notes");
}

#[test]
fn a_refused_password_or_a_failed_apt_keeps_goo_running() {
    for (code, message) in [(126, "administrator password"), (127, "administrator password"), (100, "exit code 100")] {
        let (_s, r, _) = ready(InstallMethod::Deb, DEB, None);
        r.spawner.run_code.store(code, Ordering::SeqCst);
        let err = r.installer.install().unwrap_err();
        assert!(err.contains(message), "{err}");
        assert!(!r.quit.load(Ordering::SeqCst), "goo must keep running");
        assert!(r.spawner.detached.lock().unwrap().is_empty(), "no restart helper");
        let s = r.installer.state();
        assert_eq!((s.phase, s.error.is_some()), (Phase::Error, true));
    }
}

#[cfg(unix)]
#[test]
fn an_appimage_is_replaced_in_place() {
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::tempdir().unwrap();
    let target = home.path().join("goo.AppImage");
    std::fs::write(&target, b"old version").unwrap();
    let name = "goo-0.5.0-linux-x86_64.AppImage";
    let (_s, r, bytes) = ready(InstallMethod::AppImage, name, Some(target.clone()));
    r.installer.install().unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), bytes);
    assert_eq!(std::fs::metadata(&target).unwrap().permissions().mode() & 0o777, 0o755);
    assert!(!home.path().join("goo.AppImage.new").exists());
    assert!(r.quit.load(Ordering::SeqCst));
    let detached = r.spawner.detached.lock().unwrap();
    assert_eq!(detached[0].1[2], target.to_str().unwrap());
    assert!(r.spawner.runs.lock().unwrap().is_empty());
}

#[test]
fn windows_runs_the_setup_silently_and_lets_it_relaunch_goo() {
    let (_s, r, _) = ready(InstallMethod::Nsis, "goo-0.5.0-win-x64.exe", None);
    let file = r.dir.path().join("updates").join("goo-0.5.0-win-x64.exe");
    r.installer.install().unwrap();
    assert!(r.quit.load(Ordering::SeqCst));
    let detached = r.spawner.detached.lock().unwrap();
    assert_eq!(*detached, [(file.to_str().unwrap().to_string(), vec!["/S".to_string(), "/UPDATE".into(), "/R".into()])]);
}

#[test]
fn macos_replaces_the_bundle_after_quitting() {
    let bundle = PathBuf::from("/Applications/goo.app");
    let (_s, r, _) = ready(InstallMethod::Dmg, "goo-0.5.0-mac-x64.dmg", Some(bundle));
    let file = r.dir.path().join("updates").join("goo-0.5.0-mac-x64.dmg");
    r.installer.install().unwrap();
    assert!(r.quit.load(Ordering::SeqCst));
    let detached = r.spawner.detached.lock().unwrap();
    let args = &detached[0].1;
    assert_eq!(args[0], "-c");
    let script = &args[1];
    assert!(script.starts_with("while kill -0 4242 2>/dev/null; do sleep 0.3; done; mnt=$(mktemp -d)"), "{script}");
    assert!(script.contains("hdiutil attach") && script.contains("ditto") && script.ends_with("open \"$1\""), "{script}");
    assert_eq!((args[2].as_str(), args[3].as_str()), (file.to_str().unwrap(), "/Applications/goo.app"));
}

#[test]
fn cleanup_removes_leftover_installers() {
    let s = TestServer::start();
    let r = rig(&s.feed(), Some(InstallMethod::Deb), None);
    let dir = r.dir.path().join("updates");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(DEB), b"x").unwrap();
    r.installer.cleanup();
    assert!(!dir.exists());
    r.installer.cleanup(); // nothing there: fine
}

// ---------------------------------------------------------------------------
// the real thing (Linux and macOS): no fake spawner
// ---------------------------------------------------------------------------

/// An AppImage update end to end with the real spawner: the installer is downloaded from a local feed, the AppImage file
/// is replaced, and a detached helper starts the new version only after the old process has exited.
#[cfg(unix)]
#[test]
fn an_appimage_update_really_restarts_into_the_new_version() {
    use crate::installer::SystemSpawner;
    use std::os::unix::fs::PermissionsExt;

    let home = tempfile::tempdir().unwrap();
    let marker = home.path().join("started-by-update");
    let target = home.path().join("goo.AppImage");
    std::fs::write(&target, "#!/bin/sh\necho old > /dev/null\n").unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();

    // the "new version" is a script that proves it was started
    let new_version = format!("#!/bin/sh\necho started > '{}'\n", marker.display()).into_bytes();
    let s = TestServer::start();
    s.publish("v0.5.0", &[("goo-0.5.0-linux-x86_64.AppImage", new_version.clone(), None)]);

    // the "old goo": a process that lives for two more seconds, then exits (and is reaped, like a real parent would)
    let mut old = std::process::Command::new("sleep").arg("2").spawn().unwrap();
    let old_pid = old.id();
    let reaper = std::thread::spawn(move || old.wait().unwrap());

    let quit = Arc::new(AtomicBool::new(false));
    let q = quit.clone();
    let installer = Installer::new(Deps {
        current_version: "0.4.0".into(),
        feed_url: s.feed(),
        install: InstallTarget { method: Some(InstallMethod::AppImage), reason: None, target: Some(target.clone()) },
        arch: "x86_64".into(),
        download_dir: home.path().join("updates"),
        emit: Arc::new(|_| {}),
        spawner: Arc::new(SystemSpawner),
        pid: old_pid,
        exec_path: PathBuf::from("/does/not/matter"),
        quit: Arc::new(move || q.store(true, Ordering::SeqCst)),
    });
    installer.download().unwrap();
    assert_eq!(wait_until(&installer, finished).phase, Phase::Ready);
    installer.install().unwrap();

    assert!(quit.load(Ordering::SeqCst), "goo quits after starting the helper");
    assert_eq!(std::fs::read(&target).unwrap(), new_version, "the AppImage file is now the new version");
    assert_eq!(std::fs::metadata(&target).unwrap().permissions().mode() & 0o777, 0o755);
    assert!(!marker.exists(), "the new version must not start while the old process still runs (single-instance lock)");

    reaper.join().unwrap(); // the old process is gone
    let start = Instant::now();
    while !marker.exists() {
        assert!(start.elapsed() < Duration::from_secs(10), "the new version was never started");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(std::fs::read_to_string(&marker).unwrap().trim(), "started");
}

// ---------------------------------------------------------------------------
// the contract with the UI (src/backend/types.ts: UpdateCheckResult, UpdateInstallState)
// ---------------------------------------------------------------------------

#[test]
fn what_the_ui_receives() {
    let state = UpdateState {
        method: Some(InstallMethod::AppImage),
        unsupported_reason: None,
        phase: Phase::Downloading,
        version: Some("0.5.0".into()),
        received: 10,
        total: Some(100),
        bytes_per_sec: Some(5),
        error: None,
    };
    assert_eq!(
        serde_json::to_value(&state).unwrap(),
        json!({ "method": "appimage", "unsupportedReason": null, "phase": "downloading", "version": "0.5.0", "received": 10, "total": 100, "bytesPerSec": 5, "error": null })
    );
    for (method, name) in [(InstallMethod::Deb, "deb"), (InstallMethod::Nsis, "nsis"), (InstallMethod::Dmg, "dmg")] {
        assert_eq!(serde_json::to_value(method).unwrap(), name);
    }
    for (phase, name) in [(Phase::Idle, "idle"), (Phase::Verifying, "verifying"), (Phase::Ready, "ready"), (Phase::Installing, "installing"), (Phase::Error, "error")] {
        assert_eq!(serde_json::to_value(phase).unwrap(), name);
    }
    let result = crate::check::CheckResult {
        current_version: "0.4.0".into(),
        latest: parse_release(&json!({ "tag_name": "v0.5.0", "published_at": "2026-10-12T13:16:19Z" })),
        update_available: true,
        checked_at: 1,
    };
    assert_eq!(
        serde_json::to_value(&result).unwrap(),
        json!({ "currentVersion": "0.4.0", "latest": { "version": "0.5.0", "url": "https://github.com/perunok/goo/releases/tag/v0.5.0", "publishedAt": 1_791_810_979 }, "updateAvailable": true, "checkedAt": 1 })
    );
}
