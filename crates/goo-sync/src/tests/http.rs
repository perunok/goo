//! `HttpRemote` and device sign-in against a tiny local HTTP server.

use crate::api::*;
use crate::http::{DeviceSignIn, HttpRemote, MemoryTokenStore, SignInError, TokenStore};
use crate::now_secs;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

struct Req {
    method: String,
    url: String,
    auth: Option<String>,
    body: String,
}

type Handler = dyn Fn(&Req) -> (u16, Value) + Send + Sync;

struct TestServer {
    url: String,
    server: Arc<tiny_http::Server>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.server.unblock();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Serves every request on its own thread (so concurrent calls really overlap).
fn serve(handler: impl Fn(&Req) -> (u16, Value) + Send + Sync + 'static) -> TestServer {
    let handler: Arc<Handler> = Arc::new(handler);
    let server = Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
    let port = server.server_addr().to_ip().unwrap().port();
    let s = server.clone();
    let thread = thread::spawn(move || {
        for mut req in s.incoming_requests() {
            let handler = handler.clone();
            thread::spawn(move || {
                let mut body = String::new();
                req.as_reader().read_to_string(&mut body).unwrap();
                let auth = req.headers().iter().find(|h| h.field.equiv("Authorization")).map(|h| h.value.as_str().to_string());
                let r = Req { method: req.method().as_str().to_string(), url: req.url().to_string(), auth, body };
                let (status, value) = handler(&r);
                let mut resp = tiny_http::Response::from_string(value.to_string()).with_status_code(status);
                if status == 429 {
                    resp = resp.with_header(tiny_http::Header::from_bytes("Retry-After", "7").unwrap());
                }
                if !value.is_null() {
                    resp = resp.with_header(tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap());
                }
                let _ = req.respond(resp);
            });
        }
    });
    TestServer { url: format!("http://127.0.0.1:{port}"), server, thread: Some(thread) }
}

fn error(code: &str) -> Value {
    json!({ "error": { "code": code, "message": format!("{code}!"), "details": {}, "request_id": "r" } })
}

fn me_body() -> Value {
    json!({
        "user": { "id": "u1", "email": "a@example.com", "display_name": "A", "platform_role": "user", "must_change_password": false },
        "workspace_memberships": [{ "workspace_id": "w1", "role": "editor" }]
    })
}

fn pair(access: &str, refresh: &str, expires_in: i64) -> TokenPair {
    TokenPair { access_token: access.into(), refresh_token: refresh.into(), expires_at: (now_secs() as i64 + expires_in) as u64 }
}

/// A token store that logs saves into the same event log as the server, to check persist-before-use.
struct LoggingStore {
    inner: MemoryTokenStore,
    log: Arc<Mutex<Vec<String>>>,
}

impl TokenStore for LoggingStore {
    fn load(&self) -> Option<TokenPair> {
        self.inner.load()
    }
    fn save(&self, pair: &TokenPair) -> Result<(), String> {
        self.log.lock().unwrap().push(format!("save {}", pair.access_token));
        self.inner.save(pair)
    }
}

/// A server where `a2` is the only valid access token and `r1` rotates to `a2`/`r2`.
fn auth_server(log: Arc<Mutex<Vec<String>>>, refreshes: Arc<AtomicUsize>, refresh_delay: Duration) -> TestServer {
    serve(move |r| {
        if r.url == "/v1/auth/refresh" {
            refreshes.fetch_add(1, Ordering::SeqCst);
            thread::sleep(refresh_delay);
            let body: Value = serde_json::from_str(&r.body).unwrap();
            log.lock().unwrap().push(format!("refresh {}", body["refresh_token"].as_str().unwrap()));
            if body["refresh_token"] != "r1" {
                return (401, error("unauthenticated"));
            }
            return (200, json!({ "access_token": "a2", "refresh_token": "r2", "expires_in": 3600, "token_type": "Bearer" }));
        }
        log.lock().unwrap().push(format!("{} {} {}", r.method, r.url, r.auth.clone().unwrap_or_default()));
        if r.auth.as_deref() != Some("Bearer a2") {
            return (401, error("unauthenticated"));
        }
        (200, me_body())
    })
}

#[test]
fn sends_the_bearer_token_and_refreshes_ahead_of_expiry() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let refreshes = Arc::new(AtomicUsize::new(0));
    let server = auth_server(log.clone(), refreshes.clone(), Duration::ZERO);
    // valid for 30 more seconds: inside the 60 s margin
    let store = Arc::new(LoggingStore { inner: MemoryTokenStore::new(Some(pair("a1", "r1", 30))), log: log.clone() });
    let remote = HttpRemote::new(&server.url, store.clone());
    let me = remote.me().unwrap();
    assert_eq!(me.workspace_memberships[0].role, Role::Editor);
    assert_eq!(*log.lock().unwrap(), ["refresh r1", "save a2", "GET /v1/me Bearer a2"]);
    let saved = store.load().unwrap();
    assert_eq!((saved.access_token.as_str(), saved.refresh_token.as_str()), ("a2", "r2"));
    assert!(saved.expires_at >= now_secs() + 3599);
    // the fresh token is reused without another refresh
    remote.me().unwrap();
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
}

#[test]
fn refreshes_once_on_unauthenticated_and_persists_before_retrying() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let refreshes = Arc::new(AtomicUsize::new(0));
    let server = auth_server(log.clone(), refreshes.clone(), Duration::ZERO);
    // not expired by the clock, but refused by the server (e.g. signed out elsewhere)
    let store = Arc::new(LoggingStore { inner: MemoryTokenStore::new(Some(pair("a1", "r1", 3600))), log: log.clone() });
    let remote = HttpRemote::new(&server.url, store);
    remote.me().unwrap();
    assert_eq!(*log.lock().unwrap(), ["GET /v1/me Bearer a1", "refresh r1", "save a2", "GET /v1/me Bearer a2"]);
}

#[test]
fn concurrent_calls_share_a_single_refresh() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let refreshes = Arc::new(AtomicUsize::new(0));
    let server = auth_server(log.clone(), refreshes.clone(), Duration::from_millis(300));
    let store: Arc<dyn TokenStore> = Arc::new(MemoryTokenStore::new(Some(pair("a1", "r1", 3600))));
    let remote = Arc::new(HttpRemote::new(&server.url, store));
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let remote = remote.clone();
            thread::spawn(move || remote.me().map(|m| m.user.id))
        })
        .collect();
    for t in threads {
        assert_eq!(t.join().unwrap().unwrap(), "u1");
    }
    assert_eq!(refreshes.load(Ordering::SeqCst), 1, "one refresh for all callers (r1 is single-use)");
}

#[test]
fn a_refused_refresh_asks_to_sign_in_again() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let server = auth_server(log.clone(), Arc::new(AtomicUsize::new(0)), Duration::ZERO);
    let store = Arc::new(MemoryTokenStore::new(Some(pair("a0", "r0-reused", -10))));
    let remote = HttpRemote::new(&server.url, store.clone());
    assert!(matches!(remote.me(), Err(RemoteError::SignInRequired(_))));
    assert_eq!(store.load().unwrap().refresh_token, "r0-reused", "nothing stored");
    let empty = HttpRemote::new(&server.url, Arc::new(MemoryTokenStore::default()));
    assert!(matches!(empty.me(), Err(RemoteError::SignInRequired(_))));
}

#[test]
fn a_refresh_that_fails_on_the_network_is_not_retried() {
    // accepts connections and closes them without an answer
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let connections = Arc::new(AtomicUsize::new(0));
    let c = connections.clone();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            c.fetch_add(1, Ordering::SeqCst);
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let _ = stream.flush();
            drop(stream);
        }
    });
    let store = Arc::new(MemoryTokenStore::new(Some(pair("a1", "r1", -10))));
    let remote = HttpRemote::new(&url, store.clone());
    let err = remote.me().unwrap_err();
    assert!(matches!(err, RemoteError::Network(_)), "{err:?}");
    thread::sleep(Duration::from_millis(100));
    assert_eq!(connections.load(Ordering::SeqCst), 1, "exactly one attempt");
    let kept = store.load().unwrap();
    assert_eq!((kept.access_token.as_str(), kept.refresh_token.as_str()), ("a1", "r1"), "the old pair is kept");
}

#[test]
fn errors_are_told_apart_by_their_code() {
    let server = serve(|r| match r.url.as_str() {
        "/v1/me" => (429, error("rate_limited")),
        "/v1/workspaces/w%2F1" => (403, error("workspace_access_denied")),
        "/v1/workspaces/gateway" => (502, Value::Null),
        _ => (404, error("not_found")),
    });
    let remote = HttpRemote::new(&server.url, Arc::new(MemoryTokenStore::new(Some(pair("a", "r", 3600)))));
    assert_eq!(remote.me().unwrap_err(), RemoteError::RateLimited { retry_after: Some(7) });
    let e = remote.get_workspace("w/1").unwrap_err();
    assert_eq!(e.code(), Some("workspace_access_denied"), "ids are escaped into one path segment");
    assert!(matches!(remote.get_workspace("gateway"), Err(RemoteError::Network(_))));
}

#[test]
fn workspaces_sync_calls_and_their_wire_format() {
    let seen = Arc::new(Mutex::new(Vec::<(String, String, String)>::new()));
    let s = seen.clone();
    let ws = |id: &str| {
        json!({ "id": id, "slug": id, "name": id, "description": "", "owner_user_id": "u1", "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:00Z", "version": 1, "role": "owner", "joined_at": "2026-01-01T00:00:00Z", "added_by": null })
    };
    let server = serve(move |r| {
        s.lock().unwrap().push((r.method.clone(), r.url.clone(), r.body.clone()));
        let path = r.url.split('?').next().unwrap();
        match (r.method.as_str(), path) {
            ("GET", "/v1/workspaces") if r.url.contains("cursor=c2") => {
                (200, json!({ "items": [ws("w2")], "page": { "next_cursor": null, "has_more": false } }))
            }
            ("GET", "/v1/workspaces") => (200, json!({ "items": [ws("w1")], "page": { "next_cursor": "c2", "has_more": true } })),
            ("POST", "/v1/workspaces") => {
                let mut w = ws("w3");
                w.as_object_mut().unwrap().remove("role");
                w.as_object_mut().unwrap().remove("joined_at");
                w.as_object_mut().unwrap().remove("added_by");
                (201, json!({ "workspace": w }))
            }
            ("POST", "/v1/sync/clients/register") => (201, json!({ "client_id": "c-1", "protocol_version": 1, "features": [] })),
            ("GET", "/v1/workspaces/w1/sync/pull") => (200, json!({ "changes": [
                { "note_id": "n", "path": "a.md", "version": 2, "deleted": false, "content_hash": "h", "content": "a", "seq": 5 },
                { "note_id": "m", "path": "b.md", "version": 3, "deleted": true, "content_hash": null, "content": null, "seq": 6 }
            ], "checkpoint": 6, "has_more": false })),
            ("POST", "/v1/workspaces/w1/sync/push") => (200, json!({
                "accepted": [{ "op_id": "o1", "note_id": "n", "version": 3, "seq": 7 }],
                "rejected": [{ "op_id": "o2", "note_id": "m", "reason": "path_taken", "message": "taken",
                               "current": { "version": 3, "path": "b.md", "deleted": false, "content": null } },
                             { "op_id": "o3", "note_id": "k", "reason": "some_new_reason", "message": "?", "current": null }],
                "checkpoint": 8 })),
            _ => (404, error("not_found")),
        }
    });
    let remote = HttpRemote::new(&server.url, Arc::new(MemoryTokenStore::new(Some(pair("a", "r", 3600)))));
    let list = remote.list_workspaces().unwrap();
    assert_eq!(list.iter().map(|w| w.id.as_str()).collect::<Vec<_>>(), ["w1", "w2"]);
    assert_eq!(list[0].role, Some(Role::Owner));
    assert_eq!(remote.create_workspace("w3", "").unwrap().role, Some(Role::Owner));
    let info = ClientInfo { device_name: "laptop".into(), platform: "linux".into(), app_version: "0.1.0".into() };
    assert_eq!(remote.register_client(&info).unwrap(), "c-1");
    let page = remote.pull("w1", "c-1", 4, 200).unwrap();
    assert_eq!((page.changes.len(), page.checkpoint, page.changes[1].content.clone()), (2, 6, None));
    let changes = vec![
        PushChange { op_id: "o1".into(), note_id: "n".into(), op: Op::Upsert, base_version: 2, path: None, content: Some("a2".into()) },
        PushChange { op_id: "o2".into(), note_id: "m".into(), op: Op::Delete, base_version: 3, path: None, content: None },
    ];
    let answer = remote.push("w1", "c-1", &changes).unwrap();
    assert_eq!(answer.accepted[0].version, 3);
    assert_eq!(answer.rejected[0].reason, Reason::PathTaken);
    assert_eq!(answer.rejected[1].reason, Reason::Unknown, "a newer reason does not break parsing");

    let seen = seen.lock().unwrap();
    let pull = seen.iter().find(|s| s.1.contains("/sync/pull")).unwrap();
    assert!(pull.1.contains("client_id=c-1") && pull.1.contains("after=4") && pull.1.contains("limit=200"), "{}", pull.1);
    let push = seen.iter().find(|s| s.1.ends_with("/sync/push")).unwrap();
    let body: Value = serde_json::from_str(&push.2).unwrap();
    assert_eq!(
        body,
        json!({ "client_id": "c-1", "changes": [
            { "op_id": "o1", "note_id": "n", "op": "upsert", "base_version": 2, "content": "a2" },
            { "op_id": "o2", "note_id": "m", "op": "delete", "base_version": 3 } ] }),
        "absent fields are left out (absent = unchanged)"
    );
    let register = seen.iter().find(|s| s.1 == "/v1/sync/clients/register").unwrap();
    assert_eq!(serde_json::from_str::<Value>(&register.2).unwrap(), json!({ "device_name": "laptop", "platform": "linux", "app_version": "0.1.0" }));
}

fn device_server(answers: Vec<Value>) -> (TestServer, Arc<AtomicUsize>) {
    let polls = Arc::new(AtomicUsize::new(0));
    let p = polls.clone();
    let answers = Arc::new(answers);
    let server = serve(move |r| match r.url.as_str() {
        "/v1/auth/device/start" => {
            let body: Value = serde_json::from_str(&r.body).unwrap();
            assert_eq!(body, json!({ "client_name": "goo", "device_name": "laptop" }));
            (200, json!({ "device_code": "dc_x", "user_code": "ABCD-EFGH", "verification_uri": "http://s/device",
                         "verification_uri_complete": "http://s/device?user_code=ABCD-EFGH", "expires_in": 600, "interval": 0 }))
        }
        "/v1/auth/device/poll" => {
            let i = p.fetch_add(1, Ordering::SeqCst);
            (if answers[i.min(answers.len() - 1)]["error"].is_null() { 200 } else { 429 }, answers[i.min(answers.len() - 1)].clone())
        }
        _ => (404, error("not_found")),
    });
    (server, polls)
}

#[test]
fn device_sign_in_polls_until_approved() {
    let approved = json!({ "status": "approved", "access_token": "a1", "refresh_token": "r1", "expires_in": 3600, "token_type": "Bearer",
        "user": { "id": "u1", "email": "a@example.com", "display_name": "A", "platform_role": "user", "must_change_password": false } });
    let (server, polls) = device_server(vec![json!({"status": "pending"}), json!({"status": "pending"}), approved]);
    let mut flow = DeviceSignIn::start(&server.url, "goo", "laptop").unwrap();
    assert_eq!(flow.start.user_code, "ABCD-EFGH");
    flow.min_interval = Duration::from_millis(10);
    let store = MemoryTokenStore::default();
    let user = flow.wait(&store, &|| false).unwrap();
    assert_eq!(user.email, "a@example.com");
    assert_eq!(polls.load(Ordering::SeqCst), 3);
    assert_eq!(store.load().unwrap().refresh_token, "r1");
}

#[test]
fn device_sign_in_ends_on_denied_expired_or_cancel() {
    let (server, _) = device_server(vec![json!({"status": "denied", "reason": "password_change_required", "message": "change it"})]);
    let mut flow = DeviceSignIn::start(&server.url, "goo", "laptop").unwrap();
    flow.min_interval = Duration::from_millis(10);
    assert_eq!(flow.wait(&MemoryTokenStore::default(), &|| false).unwrap_err(), SignInError::Denied("change it".into()));

    let (server, _) = device_server(vec![json!({"status": "pending"}), json!({"status": "expired"})]);
    let mut flow = DeviceSignIn::start(&server.url, "goo", "laptop").unwrap();
    flow.min_interval = Duration::from_millis(10);
    assert_eq!(flow.wait(&MemoryTokenStore::default(), &|| false).unwrap_err(), SignInError::Expired);

    let (server, polls) = device_server(vec![json!({"status": "pending"})]);
    let flow = DeviceSignIn::start(&server.url, "goo", "laptop").unwrap();
    assert_eq!(flow.wait(&MemoryTokenStore::default(), &|| true).unwrap_err(), SignInError::Cancelled);
    assert_eq!(polls.load(Ordering::SeqCst), 0);
}
