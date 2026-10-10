//! Two devices syncing through the REAL goo cloud server (`goo-cloud/server`), over HTTP.
//!
//! Skipped unless `GOO_SYNC_IT_SERVER_DIR` points at the server folder (with `npm install` done):
//!
//! ```sh
//! GOO_SYNC_IT_SERVER_DIR=/path/to/goo-cloud/server cargo test -p goo-sync --test real_server -- --nocapture
//! ```
//!
//! It starts a throwaway PostgreSQL in Docker (host networking, port 55434, data on tmpfs), applies the migrations,
//! starts the server on 127.0.0.1:18089 with a bootstrap admin, signs two devices in through the device flow (approved
//! on the server's `/device` page, as a user would), creates a workspace and runs a convergence scenario. The server and
//! the container are stopped at the end, also when the test fails.

use goo_sync::api::{ClientInfo, Remote, Role};
use goo_sync::http::{DeviceSignIn, HttpRemote, MemoryTokenStore, TokenStore};
use goo_sync::{sync, Report, SyncOptions};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const CONTAINER: &str = "goo-sync-it-pg";
const PG_PORT: u16 = 55434;
const PORT: u16 = 18089;
const EMAIL: &str = "it-admin@example.com";
const PASSWORD: &str = "it-only-passphrase-1";

/// Stops the server and the database container whatever happens.
struct Cleanup {
    server: Option<Child>,
    log: Option<PathBuf>,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        if thread::panicking() {
            if let Some(log) = &self.log {
                eprintln!("--- server log ---\n{}", fs::read_to_string(log).unwrap_or_default());
            }
        }
        if let Some(mut s) = self.server.take() {
            let _ = s.kill();
            let _ = s.wait();
        }
        let _ = Command::new("docker").args(["stop", CONTAINER]).stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
}

fn wait_until(what: &str, timeout: Duration, mut ok: impl FnMut() -> bool) {
    let end = Instant::now() + timeout;
    while Instant::now() < end {
        if ok() {
            return;
        }
        thread::sleep(Duration::from_millis(300));
    }
    panic!("timed out waiting for {what}");
}

fn form_encode(pairs: &[(&str, &str)]) -> String {
    let enc = |s: &str| {
        s.bytes()
            .map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
            .collect::<String>()
    };
    pairs.iter().map(|(k, v)| format!("{}={}", enc(k), enc(v))).collect::<Vec<_>>().join("&")
}

/// Device sign-in, approved on the `/device` page with email + password.
fn sign_in(base: &str, device: &str) -> Arc<MemoryTokenStore> {
    let flow = DeviceSignIn::start(base, "goo", device).unwrap();
    let agent: ureq::Agent = ureq::Agent::config_builder().http_status_as_error(false).build().into();
    let body = form_encode(&[("user_code", &flow.start.user_code), ("email", EMAIL), ("password", PASSWORD)]);
    let resp = agent
        .post(&format!("{base}/device"))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .send(body)
        .unwrap();
    assert_eq!(resp.status().as_u16(), 200, "device approval page");
    let store = Arc::new(MemoryTokenStore::default());
    let user = flow.wait(store.as_ref(), &|| false).unwrap();
    assert_eq!(user.email, EMAIL);
    store
}

struct Device {
    root: PathBuf,
    state: PathBuf,
    name: &'static str,
    remote: HttpRemote,
}

impl Device {
    fn sync(&self, workspace: &str) -> Report {
        let client = ClientInfo { device_name: self.name.into(), platform: "linux".into(), app_version: "it".into() };
        let opts = SyncOptions::new(workspace, client);
        sync(&self.root, &self.state, &self.remote, &opts).unwrap_or_else(|e| panic!("{} sync failed: {e}", self.name))
    }

    fn write(&self, rel: &str, text: &str) {
        let p = self.root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }

    fn read(&self, rel: &str) -> String {
        fs::read_to_string(self.root.join(rel)).unwrap()
    }

    fn notes(&self) -> BTreeMap<String, String> {
        let (paths, _) = goo_core::list_notes(&self.root).unwrap();
        paths.iter().map(|p| (goo_core::relative_string(&self.root, p), fs::read_to_string(p).unwrap())).collect()
    }
}

fn converge(ws: &str, a: &Device, b: &Device) -> BTreeMap<String, String> {
    for _ in 0..2 {
        a.sync(ws);
        b.sync(ws);
    }
    let notes = a.notes();
    assert_eq!(notes, b.notes(), "the two folders differ");
    notes
}

#[test]
fn two_devices_converge_through_the_real_server() {
    let Ok(server_dir) = std::env::var("GOO_SYNC_IT_SERVER_DIR") else {
        eprintln!("skipped: set GOO_SYNC_IT_SERVER_DIR to the goo-cloud/server folder to run against the real server");
        return;
    };
    let server_dir = PathBuf::from(server_dir);
    let _ = Command::new("docker").args(["rm", "-f", CONTAINER]).stdout(Stdio::null()).stderr(Stdio::null()).status();
    let started = Command::new("docker")
        .args(["run", "-d", "--rm", "--name", CONTAINER, "--network", "host", "-e", "POSTGRES_PASSWORD=test", "-e", "POSTGRES_DB=goo_it"])
        .args(["--tmpfs", "/var/lib/postgresql/data", "postgres:16-alpine", "-c", &format!("port={PG_PORT}"), "-c", "listen_addresses=127.0.0.1"])
        .stdout(Stdio::null())
        .status()
        .unwrap();
    assert!(started.success(), "docker run");
    let mut cleanup = Cleanup { server: None, log: None };
    wait_until("PostgreSQL", Duration::from_secs(60), || {
        Command::new("docker")
            .args(["exec", CONTAINER, "pg_isready", "-h", "127.0.0.1", "-p", &PG_PORT.to_string(), "-U", "postgres", "-d", "goo_it"])
            .stdout(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    });

    let database_url = format!("postgresql://postgres:test@127.0.0.1:{PG_PORT}/goo_it");
    let migrate = Command::new(server_dir.join("node_modules/.bin/prisma"))
        .args(["migrate", "deploy"])
        .current_dir(&server_dir)
        .env("DATABASE_URL", &database_url)
        .output()
        .unwrap();
    assert!(migrate.status.success(), "prisma migrate deploy: {}", String::from_utf8_lossy(&migrate.stderr));

    let tmp = tempfile::tempdir().unwrap();
    let base_dir = fs::canonicalize(tmp.path()).unwrap();
    let log = fs::File::create(base_dir.join("server.log")).unwrap();
    let base = format!("http://127.0.0.1:{PORT}");
    let bootstrap = format!(r#"[{{"email":"{EMAIL}","password":"{PASSWORD}","display_name":"IT Admin","platform_role":"super_admin"}}]"#);
    // node itself (not the tsx launcher) so killing it stops the server
    let server = Command::new("node")
        .args(["--import", "tsx", "src/index.ts"])
        .current_dir(&server_dir)
        .env("DATABASE_URL", &database_url)
        .env("GOO_SIGNING_SECRET", "goo-sync-integration-test-signing-secret-0123456789")
        .env("GOO_ADMIN_BOOTSTRAP", bootstrap)
        .env("PORT", PORT.to_string())
        .env("GOO_HOST", "127.0.0.1")
        .env("GOO_BASE_URL", &base)
        .env("GOO_SYNC_RATE_LIMIT_PER_MINUTE", "100000")
        .env("GOO_LOG_LEVEL", "warn")
        .stdout(log.try_clone().unwrap())
        .stderr(log)
        .spawn()
        .unwrap();
    cleanup.server = Some(server);
    cleanup.log = Some(base_dir.join("server.log"));
    let health = ureq::Agent::new_with_defaults();
    wait_until("the server", Duration::from_secs(90), || {
        health.get(&format!("{base}/healthz")).call().is_ok_and(|r| r.status().as_u16() == 200)
    });

    // two sign-ins of the same account (two installations)
    let store_a = sign_in(&base, "laptop A");
    let store_b = sign_in(&base, "desktop B");
    let remote_a = HttpRemote::new(&base, store_a.clone());
    let remote_b = HttpRemote::new(&base, store_b.clone());
    assert_eq!(remote_a.me().unwrap().user.email, EMAIL);
    let ws = remote_a.create_workspace("integration notes", "").unwrap();
    let listed = remote_b.list_workspaces().unwrap();
    assert_eq!(listed.iter().find(|w| w.id == ws.id).and_then(|w| w.role), Some(Role::Owner));
    assert_eq!(remote_b.get_workspace(&ws.id).unwrap().role, Some(Role::Owner));

    let dev = |name: &'static str, remote: HttpRemote| {
        let root = base_dir.join(name).join("notes");
        fs::create_dir_all(&root).unwrap();
        Device { root, state: base_dir.join(name).join("state"), name, remote }
    };
    let a = dev("devA", remote_a);
    let b = dev("devB", remote_b);

    // create on A, download on B
    a.write("plan.md", "one\ntwo\nthree\nfour\nfive\n");
    a.write("sub/folder/deep note.md", "deep\n");
    a.write("Ünïcödé ☕.md", "café\n");
    let r = a.sync(&ws.id);
    assert_eq!(r.pushed, 3);
    let r = b.sync(&ws.id);
    assert_eq!(r.pulled, 3);
    converge(&ws.id, &a, &b);

    // concurrent edits on different lines merge
    a.write("plan.md", "ONE\ntwo\nthree\nfour\nfive\n");
    b.write("plan.md", "one\ntwo\nthree\nfour\nFIVE\n");
    a.sync(&ws.id);
    let r = b.sync(&ws.id);
    assert!(r.conflicts.is_empty());
    assert_eq!(converge(&ws.id, &a, &b)["plan.md"], "ONE\ntwo\nthree\nfour\nFIVE\n");

    // overlapping edits: conflict copy, nothing lost
    a.write("plan.md", "ONE from A\ntwo\nthree\nfour\nFIVE\n");
    b.write("plan.md", "ONE from B\ntwo\nthree\nfour\nFIVE\n");
    a.sync(&ws.id);
    let r = b.sync(&ws.id);
    assert_eq!(r.conflicts.len(), 1);
    let copy = r.conflicts[0].conflict_path.clone();
    let notes = converge(&ws.id, &a, &b);
    assert_eq!(notes["plan.md"], "ONE from B\ntwo\nthree\nfour\nFIVE\n");
    assert_eq!(notes[&copy], "ONE from A\ntwo\nthree\nfour\nFIVE\n");

    // rename on B + edit on A of the same note; delete on A; the same new path on both
    fs::create_dir_all(b.root.join("archive")).unwrap();
    fs::rename(b.root.join("sub/folder/deep note.md"), b.root.join("archive/deep note.md")).unwrap();
    a.write("sub/folder/deep note.md", "deep\nedited on A\n");
    fs::remove_file(a.root.join("Ünïcödé ☕.md")).unwrap();
    fs::remove_file(a.root.join(&copy)).unwrap();
    a.write("ideas.md", "from A\n");
    b.write("ideas.md", "from B\n");
    a.sync(&ws.id);
    b.sync(&ws.id);
    let notes = converge(&ws.id, &a, &b);
    assert_eq!(
        notes,
        [
            ("archive/deep note.md", "deep\nedited on A\n"),
            ("ideas (local).md", "from B\n"),
            ("ideas.md", "from A\n"),
            ("plan.md", "ONE from B\ntwo\nthree\nfour\nFIVE\n"),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect::<BTreeMap<_, _>>()
    );
    assert!(!a.root.join("sub").exists(), "folders emptied by a remote rename are removed");

    // token refresh against the real server: the access token is about to expire
    let before = store_a.load().unwrap();
    store_a.save(&goo_sync::api::TokenPair { expires_at: 0, ..before.clone() }).unwrap();
    a.write("after-refresh.md", "ok\n");
    a.sync(&ws.id);
    let after = store_a.load().unwrap();
    assert_ne!(after.refresh_token, before.refresh_token, "rotated");
    assert_eq!(b.sync(&ws.id).pulled, 1);
    assert_eq!(b.read("after-refresh.md"), "ok\n");

    // the server's view matches the folders
    let page = a.remote.pull(&ws.id, &state_client_id(&a.state), 0, 500).unwrap();
    let server_notes: BTreeMap<String, String> = page.changes.into_iter().map(|c| (c.path, c.content.unwrap())).collect();
    assert_eq!(server_notes, a.notes());
    eprintln!("real server: {} notes converged on two devices", server_notes.len());
}

fn state_client_id(state_dir: &Path) -> String {
    let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(state_dir.join("state.json")).unwrap()).unwrap();
    v["client_id"].as_str().unwrap().to_string()
}
