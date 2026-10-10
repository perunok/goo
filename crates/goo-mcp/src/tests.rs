use crate::bridge::{Bridge, SendError, Sent, Transport};
use crate::files::{self, Endpoint, Launch};
use crate::{http, protocol, tools::Host};
use serde_json::{json, Value};
use std::cell::{Cell, RefCell};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

#[derive(Default)]
struct FakeHost {
    /// What the window would answer for get_open_note (None: window not available).
    screen: Option<Value>,
    /// Mermaid sources containing "broken" fail.
    mermaid: bool,
    open: Option<PathBuf>,
    recents: Vec<PathBuf>,
    unsaved: Vec<PathBuf>,
    changed: Mutex<Vec<PathBuf>>,
    opened: Mutex<Vec<PathBuf>>,
    activity: Mutex<Vec<String>>,
}

impl Host for FakeHost {
    fn open_root(&self) -> Option<PathBuf> {
        self.open.clone()
    }
    fn workspaces(&self) -> Vec<PathBuf> {
        self.recents.clone()
    }
    fn unsaved(&self) -> Vec<PathBuf> {
        self.unsaved.clone()
    }
    fn changed(&self, _root: &Path, files: &[PathBuf]) {
        self.changed.lock().unwrap().extend_from_slice(files);
    }
    fn open_in_app(&self, files: Vec<PathBuf>) {
        self.opened.lock().unwrap().extend(files);
    }
    fn activity(&self, tool: &str) {
        self.activity.lock().unwrap().push(tool.into());
    }
    fn open_note(&self) -> Result<Value, String> {
        self.screen.clone().ok_or_else(|| "goo's window is not available".into())
    }
    fn check_mermaid(&self, sources: &[String]) -> Result<Vec<Option<String>>, String> {
        if !self.mermaid {
            return Err("goo's window is not available".into());
        }
        Ok(sources.iter().map(|s| s.contains("broken").then(|| "Parse error on line 2".to_string())).collect())
    }
}

fn workspace() -> (tempfile::TempDir, PathBuf) {
    let d = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(d.path()).unwrap().join("notes");
    std::fs::create_dir_all(root.join("projects")).unwrap();
    std::fs::write(root.join("todo.md"), "# Todo\n- milk\n").unwrap();
    std::fs::write(root.join("projects/plan.md"), "# Plan\nstep one\nstep two\n").unwrap();
    (d, root)
}

fn host_for(root: &Path) -> FakeHost {
    FakeHost { open: Some(root.to_path_buf()), recents: vec![root.to_path_buf()], ..Default::default() }
}

fn call(host: &dyn Host, name: &str, args: Value) -> (bool, String, Value) {
    let msg = json!({ "jsonrpc": "2.0", "id": 7, "method": "tools/call", "params": { "name": name, "arguments": args } });
    let r = protocol::handle(host, msg).unwrap();
    let result = &r["result"];
    (
        result["isError"].as_bool().unwrap(),
        result["content"][0]["text"].as_str().unwrap().to_string(),
        result.get("structuredContent").cloned().unwrap_or(Value::Null),
    )
}

// ---------------------------------------------------------------------------
// protocol
// ---------------------------------------------------------------------------

#[test]
fn initialize_negotiates_the_version_and_explains_goo() {
    let host = FakeHost::default();
    let r = protocol::handle(&host, json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}})).unwrap();
    assert_eq!(r["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(r["result"]["serverInfo"]["name"], "goo");
    assert!(r["result"]["instructions"].as_str().unwrap().contains("list_workspaces"));
    let r = protocol::handle(&host, json!({"jsonrpc":"2.0","id":2,"method":"initialize","params":{"protocolVersion":"1999-01-01"}})).unwrap();
    assert_eq!(r["result"]["protocolVersion"], protocol::PROTOCOL_VERSIONS[0]);
}

#[test]
fn notifications_get_no_answer_and_unknown_methods_an_error() {
    let host = FakeHost::default();
    assert!(protocol::handle(&host, json!({"jsonrpc":"2.0","method":"notifications/initialized"})).is_none());
    let r = protocol::handle(&host, json!({"jsonrpc":"2.0","id":"x","method":"resources/list"})).unwrap();
    assert_eq!(r["error"]["code"], protocol::METHOD_NOT_FOUND);
    assert_eq!(r["id"], "x");
    let r = protocol::handle(&host, json!({"jsonrpc":"2.0","id":3,"method":"ping"})).unwrap();
    assert_eq!(r["result"], json!({}));
}

#[test]
fn tools_are_listed_with_schemas_and_hints() {
    let list = protocol::tool_list();
    let tools = list["tools"].as_array().unwrap();
    let names: Vec<_> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        [
            "list_workspaces", "list_notes", "read_note", "search_notes", "create_note", "write_note", "edit_note", "append_to_note",
            "open_in_app", "get_open_note", "check_mermaid", "sync_status", "sync_now"
        ]
    );
    for t in tools {
        assert_eq!(t["inputSchema"]["type"], "object", "{}", t["name"]);
        // only sync_now reaches beyond this computer
        assert_eq!(t["annotations"]["openWorldHint"], t["name"] == "sync_now", "{}", t["name"]);
    }
    let write = tools.iter().find(|t| t["name"] == "write_note").unwrap();
    assert_eq!(write["annotations"]["destructiveHint"], true);
    assert_eq!(write["inputSchema"]["required"], json!(["path", "content"]));
}

// ---------------------------------------------------------------------------
// tools
// ---------------------------------------------------------------------------

#[test]
fn reads_lists_and_searches_the_open_folder() {
    let (_d, root) = workspace();
    let host = host_for(&root);
    let (e, _, data) = call(&host, "list_notes", json!({}));
    assert!(!e);
    assert_eq!(data["notes"], json!(["projects/plan.md", "todo.md"]));
    let (e, text, _) = call(&host, "read_note", json!({"path": "todo.md"}));
    assert!(!e);
    assert_eq!(text, "# Todo\n- milk\n");
    let (_, _, data) = call(&host, "search_notes", json!({"query": "STEP"}));
    assert_eq!(data["hits"].as_array().unwrap().len(), 2);
    let (_, _, data) = call(&host, "list_workspaces", json!({}));
    assert_eq!(data["workspaces"][0]["name"], "notes");
    assert_eq!(data["workspaces"][0]["open"], true);
    assert_eq!(host.activity.lock().unwrap().len(), 4);
}

#[test]
fn writes_report_changes_and_respect_unsaved_notes() {
    let (_d, root) = workspace();
    let mut host = host_for(&root);

    let (e, _, data) = call(&host, "create_note", json!({"path": "ideas/new"}));
    assert!(!e);
    assert_eq!(data["created"], "ideas/new.md");
    assert_eq!(std::fs::read_to_string(root.join("ideas/new.md")).unwrap(), "# new\n\n");

    let (e, _, _) = call(&host, "edit_note", json!({"path": "projects/plan.md", "old_text": "step one", "new_text": "step 1"}));
    assert!(!e);
    let (e, msg, _) = call(&host, "edit_note", json!({"path": "projects/plan.md", "old_text": "step", "new_text": "x"}));
    assert!(e);
    assert!(msg.contains("appears 2 times"), "{msg}");
    let (e, _, _) = call(&host, "append_to_note", json!({"path": "todo.md", "content": "- eggs"}));
    assert!(!e);
    assert_eq!(std::fs::read_to_string(root.join("todo.md")).unwrap(), "# Todo\n- milk\n- eggs\n");
    assert_eq!(host.changed.lock().unwrap().len(), 3);

    // a note with unsaved edits in the window is left alone
    host.unsaved = vec![root.join("todo.md")];
    let (e, msg, _) = call(&host, "write_note", json!({"path": "todo.md", "content": "gone"}));
    assert!(e);
    assert!(msg.contains("unsaved changes"), "{msg}");
    assert!(std::fs::read_to_string(root.join("todo.md")).unwrap().contains("eggs"));

    let (e, _, _) = call(&host, "open_in_app", json!({"path": "projects/plan.md"}));
    assert!(!e);
    assert_eq!(host.opened.lock().unwrap()[0], root.join("projects/plan.md"));
}

#[test]
fn only_folders_opened_in_goo_are_reachable() {
    let (_d, root) = workspace();
    let other = tempfile::tempdir().unwrap();
    std::fs::write(other.path().join("secret.md"), "no").unwrap();
    let host = host_for(&root);

    let (e, msg, _) = call(&host, "read_note", json!({"path": "secret.md", "workspace": other.path().to_string_lossy()}));
    assert!(e);
    assert!(msg.contains("not one of the folders"), "{msg}");
    for path in ["../secret.md", "/etc/passwd", "projects/../../x.md"] {
        let (e, _, _) = call(&host, "read_note", json!({ "path": path }));
        assert!(e, "{path}");
    }
    // by name works too
    let (e, _, _) = call(&host, "read_note", json!({"path": "todo.md", "workspace": "NOTES"}));
    assert!(!e);
    // nothing open and no workspace given
    let closed = FakeHost { recents: vec![root.clone()], ..Default::default() };
    let (e, msg, _) = call(&closed, "list_notes", json!({}));
    assert!(e);
    assert!(msg.contains("No folder is open"), "{msg}");
}

#[test]
fn bad_arguments_are_tool_errors_not_crashes() {
    let (_d, root) = workspace();
    let host = host_for(&root);
    for (tool, args) in [
        ("read_note", json!({})),
        ("read_note", json!({"path": 5})),
        ("search_notes", json!({"query": "x", "limit": 0})),
        ("edit_note", json!({"path": "todo.md", "old_text": "", "new_text": "x"})),
        ("create_note", json!({"path": "todo.md"})),
        ("read_note", json!("nope")),
    ] {
        let (e, _, _) = call(&host, tool, args.clone());
        assert!(e, "{tool} {args}");
    }
    let r = protocol::handle(&host, json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"rm_rf"}})).unwrap();
    assert_eq!(r["error"]["code"], protocol::INVALID_PARAMS);
}

// ---------------------------------------------------------------------------
// http endpoint
// ---------------------------------------------------------------------------

fn serve(host: Arc<FakeHost>, token: &str) -> (http::RunningServer, Arc<RwLock<String>>) {
    let token = Arc::new(RwLock::new(token.to_string()));
    let handler: http::Handler = Arc::new(move |msg| protocol::handle(host.as_ref(), msg));
    (http::start(0, token.clone(), handler).unwrap(), token)
}

/// A raw HTTP/1.1 request, so headers (Host, Origin) are exactly what the test says.
fn raw(port: u16, request: &str) -> (u16, String) {
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    s.write_all(request.as_bytes()).unwrap();
    let mut out = String::new();
    let _ = s.read_to_string(&mut out);
    let status = out.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    let body = out.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
    (status, body)
}

fn post(port: u16, token: &str, extra: &str, body: &str) -> (u16, String) {
    raw(
        port,
        &format!(
            "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        ),
    )
}

#[test]
fn endpoint_answers_only_authorised_local_requests() {
    let (_d, root) = workspace();
    let (server, token) = serve(Arc::new(host_for(&root)), "goo_secret");
    let port = server.port;
    let ping = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;

    let (status, body) = post(port, "goo_secret", "", ping);
    assert_eq!(status, 200);
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap()["result"], json!({}));

    assert_eq!(post(port, "wrong", "", ping).0, 401);
    assert_eq!(post(port, "goo_secret", "Origin: https://evil.example\r\n", ping).0, 403);
    let (status, _) = raw(port, &format!("POST /mcp HTTP/1.1\r\nHost: evil.example:{port}\r\nAuthorization: Bearer goo_secret\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"));
    assert_eq!(status, 403);
    let (status, _) = raw(port, &format!("GET /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer goo_secret\r\nConnection: close\r\n\r\n"));
    assert_eq!(status, 405);
    let (status, _) = raw(port, &format!("POST /other HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"));
    assert_eq!(status, 404);

    // notifications: accepted, no body; broken JSON: a parse error
    assert_eq!(post(port, "goo_secret", "", r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).0, 202);
    let (status, body) = post(port, "goo_secret", "", "{nope");
    assert_eq!(status, 200);
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap()["error"]["code"], protocol::PARSE_ERROR);

    // a new token locks the old one out at once
    *token.write().unwrap() = "goo_new".into();
    assert_eq!(post(port, "goo_secret", "", ping).0, 401);
    assert_eq!(post(port, "goo_new", "", ping).0, 200);
    server.stop();
}

#[test]
fn a_busy_port_moves_to_the_next_one() {
    let blocker = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let busy = blocker.local_addr().unwrap().port();
    let handler: http::Handler = Arc::new(|_| None);
    let server = http::start(busy, Arc::new(RwLock::new("t".into())), handler).unwrap();
    assert_ne!(server.port, busy);
    assert!(server.port > busy && server.port < busy + http::PORT_ATTEMPTS);
}

// ---------------------------------------------------------------------------
// stdio bridge
// ---------------------------------------------------------------------------

/// Stands in for goo: `running` decides whether posts get through; starting the app writes endpoint.json.
struct FakeTransport {
    dir: PathBuf,
    running: Cell<bool>,
    token: RefCell<String>,
    starts: Cell<u32>,
}

impl Transport for FakeTransport {
    fn post(&self, ep: &Endpoint, _pv: Option<&str>, body: &str) -> Result<Sent, SendError> {
        if !self.running.get() {
            return Err(SendError::Unreachable);
        }
        if ep.token != *self.token.borrow() {
            return Err(SendError::Unauthorized);
        }
        let msg: Value = serde_json::from_str(body).unwrap();
        Ok(match protocol::handle(&FakeHost::default(), msg) {
            Some(v) => Sent::Answer(v.to_string()),
            None => Sent::Accepted,
        })
    }
    fn start_app(&self) -> std::io::Result<()> {
        self.starts.set(self.starts.get() + 1);
        self.running.set(true);
        files::write_endpoint(&self.dir, &Endpoint { url: "http://127.0.0.1:1/mcp".into(), token: self.token.borrow().clone() })
    }
    fn sleep(&self, _d: Duration) {}
}

fn bridge(running: bool) -> (tempfile::TempDir, Bridge<FakeTransport>) {
    let d = tempfile::tempdir().unwrap();
    let dir = d.path().join("mcp");
    let t = FakeTransport { dir: dir.clone(), running: Cell::new(running), token: RefCell::new("goo_a".into()), starts: Cell::new(0) };
    if running {
        files::write_endpoint(&dir, &Endpoint { url: "http://127.0.0.1:1/mcp".into(), token: "goo_a".into() }).unwrap();
    }
    (d, Bridge::new(dir, t))
}

const INIT: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#;

#[test]
fn bridge_relays_answers_and_swallows_notification_acks() {
    let (_d, mut b) = bridge(true);
    let answer: Value = serde_json::from_str(&b.forward(INIT).unwrap()).unwrap();
    assert_eq!(answer["result"]["protocolVersion"], "2025-06-18");
    assert!(b.forward(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_none());
    assert_eq!(b.transport.starts.get(), 0);
}

#[test]
fn bridge_refuses_to_start_goo_when_access_is_off() {
    let (_d, mut b) = bridge(false);
    files::write_launch(&b.dir, Launch { enabled: false }).unwrap();
    let answer: Value = serde_json::from_str(&b.forward(INIT).unwrap()).unwrap();
    assert_eq!(answer["id"], 1);
    assert!(answer["error"]["message"].as_str().unwrap().contains("turned off"));
    assert_eq!(b.transport.starts.get(), 0);
}

#[test]
fn bridge_starts_goo_when_it_is_closed() {
    let (_d, mut b) = bridge(false);
    files::write_launch(&b.dir, Launch { enabled: true }).unwrap();
    let answer: Value = serde_json::from_str(&b.forward(INIT).unwrap()).unwrap();
    assert_eq!(answer["result"]["serverInfo"]["name"], "goo");
    assert_eq!(b.transport.starts.get(), 1);
}

#[test]
fn bridge_picks_up_a_new_token() {
    let (_d, mut b) = bridge(true);
    *b.transport.token.borrow_mut() = "goo_b".into();
    files::write_endpoint(&b.dir, &Endpoint { url: "http://127.0.0.1:1/mcp".into(), token: "goo_b".into() }).unwrap();
    assert!(b.forward(INIT).unwrap().contains("protocolVersion"));
}

#[test]
fn bridge_works_end_to_end_over_http() {
    let (_d, root) = workspace();
    let (server, _token) = serve(Arc::new(host_for(&root)), "goo_e2e");
    let cfg = tempfile::tempdir().unwrap();
    let dir = cfg.path().join("mcp");
    files::write_endpoint(&dir, &Endpoint { url: server.url(), token: "goo_e2e".into() }).unwrap();
    let mut b = Bridge::new(dir, crate::bridge::Http::default());
    assert!(b.forward(INIT).unwrap().contains("\"goo\""));
    let r: Value = serde_json::from_str(
        &b.forward(r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"read_note","arguments":{"path":"todo.md"}}}"#).unwrap(),
    )
    .unwrap();
    assert_eq!(r["result"]["content"][0]["text"], "# Todo\n- milk\n");
    server.stop();
    // goo closed and access off: a clean JSON-RPC error, not a hang
    let r: Value = serde_json::from_str(&b.forward(r#"{"jsonrpc":"2.0","id":3,"method":"ping"}"#).unwrap()).unwrap();
    assert!(r["error"]["message"].as_str().unwrap().contains("turned off"), "{r}");
}

// ---------------------------------------------------------------------------
// files
// ---------------------------------------------------------------------------

#[test]
fn settings_start_off_and_secrets_are_private() {
    let d = tempfile::tempdir().unwrap();
    let s = files::load_settings(d.path());
    assert!(!s.enabled);
    assert!(s.token.starts_with("goo_") && s.token.len() == 68);
    assert_ne!(s.token, files::new_token());
    files::save_settings(d.path(), &s).unwrap();
    assert_eq!(files::load_settings(d.path()), s);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(d.path().join("settings.json")).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    assert!(!files::read_launch(d.path()).enabled);
}

// ---------------------------------------------------------------------------
// window-backed and goo cloud tools
// ---------------------------------------------------------------------------

#[test]
fn mermaid_blocks_are_found_with_their_lines() {
    let text = "# T\n\n```mermaid\nflowchart LR\n  A --> B\n```\n\n```js\nx\n```\n~~~~ Mermaid title\nbroken\n~~~~\n```mermaid\nnot closed\n";
    let blocks = crate::tools::mermaid_blocks(text);
    assert_eq!(blocks, vec![(3, "flowchart LR\n  A --> B\n".to_string()), (11, "broken\n".to_string())]);
}

#[test]
fn check_mermaid_reports_each_diagram_of_a_note() {
    let (_d, root) = workspace();
    std::fs::write(root.join("d.md"), "x\n```mermaid\ngraph TD\n```\ntext\n```mermaid\nbroken\n```\n").unwrap();
    let host = FakeHost { mermaid: true, ..host_for(&root) };
    let (e, _, data) = call(&host, "check_mermaid", json!({"path": "d.md"}));
    assert!(!e);
    assert_eq!(data["all_ok"], false);
    assert_eq!(data["diagrams"][0], json!({"line": 2, "ok": true, "error": null}));
    assert_eq!(data["diagrams"][1]["line"], 6);
    assert_eq!(data["diagrams"][1]["error"], "Parse error on line 2");
    let (e, _, data) = call(&host, "check_mermaid", json!({"code": "graph TD; A-->B"}));
    assert!(!e);
    assert_eq!(data["all_ok"], true);
    let (e, msg, _) = call(&host, "check_mermaid", json!({}));
    assert!(e && msg.contains("either"), "{msg}");
    // without the window: a clear error
    let (e, msg, _) = call(&host_for(&root), "check_mermaid", json!({"code": "graph TD"}));
    assert!(e && msg.contains("window"), "{msg}");
}

#[test]
fn get_open_note_gives_the_screen_with_a_relative_path() {
    let (_d, root) = workspace();
    let screen = json!({"path": root.join("projects/plan.md").to_string_lossy(), "text": "# Plan\nunsaved", "dirty": true,
                        "selection": {"text": "unsaved", "from_line": 2, "to_line": 2}});
    let host = FakeHost { screen: Some(screen), ..host_for(&root) };
    let (e, _, data) = call(&host, "get_open_note", json!({}));
    assert!(!e);
    assert_eq!(data["path"], "projects/plan.md");
    assert_eq!(data["workspace"], "notes");
    assert_eq!(data["dirty"], true);
    assert_eq!(data["selection"]["text"], "unsaved");

    let nothing = FakeHost { screen: Some(json!({"path": null})), ..host_for(&root) };
    let (e, _, data) = call(&nothing, "get_open_note", json!({}));
    assert!(!e);
    assert_eq!(data["open"], false);

    // a note outside the folders opened in goo is not handed out
    let outside = FakeHost { screen: Some(json!({"path": "/etc/hosts.md", "text": "x"})), ..host_for(&root) };
    let (e, _, _) = call(&outside, "get_open_note", json!({}));
    assert!(e);
}

#[test]
fn sync_tools_default_to_not_linked() {
    let (_d, root) = workspace();
    let host = host_for(&root);
    let (e, _, data) = call(&host, "sync_status", json!({}));
    assert!(!e);
    assert_eq!(data["linked"], false);
    let (e, msg, _) = call(&host, "sync_now", json!({}));
    assert!(e && msg.contains("does not sync"), "{msg}");
}
