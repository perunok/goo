//! The HTTP client for goo cloud (ureq, rustls): device sign-in, account and workspace calls, and [`HttpRemote`], the
//! [`Remote`] the engine syncs through.
//!
//! Auth (DESIGN §3, §8.10): the token pair lives in a [`TokenStore`] (the app backs it with the OS keychain). The access
//! token is refreshed when it expires within 60 s, or once when a call answers `unauthenticated`. Refreshing is
//! single-flight (one mutex; a caller that waited reuses the pair the other one stored), the new pair is persisted
//! before it is used (refresh tokens are single-use), and a refresh that fails on the network is never retried in the
//! same call (the next sync cycle tries again). Errors are told apart by the server's `error.code`, not the status.

use serde::de::DeserializeOwned;
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::api::*;
use crate::now_secs;

/// Refresh when the access token expires within this many seconds.
pub const REFRESH_MARGIN_SECS: u64 = 60;
/// Largest response body read (a pull page holds about 8 MiB of content, more once JSON-escaped).
const MAX_RESPONSE_BYTES: u64 = 64 * 1024 * 1024;

/// Where the access + refresh token pair is kept (the OS keychain in the app).
pub trait TokenStore: Send + Sync {
    fn load(&self) -> Option<TokenPair>;
    /// Must be durable when it returns Ok: the old refresh token is already spent.
    fn save(&self, pair: &TokenPair) -> Result<(), String>;
}

/// A [`TokenStore`] in memory (tests, and a fallback when no keychain is available).
#[derive(Debug, Default)]
pub struct MemoryTokenStore(Mutex<Option<TokenPair>>);

impl MemoryTokenStore {
    pub fn new(pair: Option<TokenPair>) -> MemoryTokenStore {
        MemoryTokenStore(Mutex::new(pair))
    }
}

impl TokenStore for MemoryTokenStore {
    fn load(&self) -> Option<TokenPair> {
        self.0.lock().unwrap().clone()
    }
    fn save(&self, pair: &TokenPair) -> Result<(), String> {
        *self.0.lock().unwrap() = Some(pair.clone());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Plain requests
// ---------------------------------------------------------------------------

struct Answer {
    status: u16,
    body: String,
    retry_after: Option<u64>,
}

/// An agent for one server. Statuses are not errors at this level: the body carries the structured error.
#[derive(Clone)]
struct Http {
    base: String,
    agent: ureq::Agent,
}

impl Http {
    fn new(base_url: &str) -> Http {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_global(Some(Duration::from_secs(300)))
            .build()
            .into();
        Http { base: base_url.trim_end_matches('/').to_string(), agent }
    }

    fn send(&self, method: &str, path: &str, query: &[(&str, String)], token: Option<&str>, body: Option<&str>) -> Result<Answer, RemoteError> {
        let url = format!("{}{}", self.base, path);
        let auth = token.map(|t| format!("Bearer {t}"));
        let result = if method == "GET" {
            let mut req = self.agent.get(&url).header("Accept", "application/json");
            for (k, v) in query {
                req = req.query(*k, v);
            }
            if let Some(a) = &auth {
                req = req.header("Authorization", a);
            }
            req.call()
        } else {
            let mut req = self.agent.post(&url).header("Accept", "application/json").header("Content-Type", "application/json");
            if let Some(a) = &auth {
                req = req.header("Authorization", a);
            }
            req.send(body.unwrap_or("{}"))
        };
        let resp = result.map_err(|e| RemoteError::Network(e.to_string()))?;
        let status = resp.status().as_u16();
        let retry_after = resp.headers().get("retry-after").and_then(|v| v.to_str().ok()).and_then(|s| s.trim().parse().ok());
        let mut body = resp.into_body();
        let body = body.with_config().limit(MAX_RESPONSE_BYTES).read_to_string().map_err(|e| RemoteError::Network(e.to_string()))?;
        Ok(Answer { status, body, retry_after })
    }
}

fn is_success(a: &Answer) -> bool {
    (200..300).contains(&a.status)
}

/// The structured error of an answer, if it has one.
fn error_body(a: &Answer) -> Option<ErrorBody> {
    serde_json::from_str::<ErrorEnvelope>(&a.body).ok().map(|e| e.error)
}

fn is_unauthenticated(a: &Answer) -> bool {
    !is_success(a) && error_body(a).is_some_and(|e| e.code == "unauthenticated")
}

fn to_error(a: &Answer) -> RemoteError {
    match error_body(a) {
        Some(e) if e.code == "rate_limited" => {
            let from_details = e.details.get("retry_after_seconds").and_then(|v| v.as_u64());
            RemoteError::RateLimited { retry_after: a.retry_after.or(from_details) }
        }
        Some(e) if e.code == "unauthenticated" => RemoteError::SignInRequired(e.message),
        Some(e) => RemoteError::Api { status: a.status, code: e.code, message: e.message },
        // No envelope: a proxy or load balancer answered (502/503/504), the server is not reachable right now.
        None if a.status >= 500 => RemoteError::Network(format!("HTTP {}", a.status)),
        None => RemoteError::Protocol(format!("HTTP {} without an error body", a.status)),
    }
}

fn decode<T: DeserializeOwned>(a: Answer) -> Result<T, RemoteError> {
    if !is_success(&a) {
        return Err(to_error(&a));
    }
    serde_json::from_str(&a.body).map_err(|e| RemoteError::Protocol(e.to_string()))
}

/// Percent-encodes an id used as a URL path segment (ids are UUIDs; anything else must not change the path).
fn segment(id: &str) -> String {
    id.bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

// ---------------------------------------------------------------------------
// Device sign-in
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignInError {
    /// The code expired before it was approved (start again).
    Expired,
    /// The server refused it (e.g. the account must change its password first).
    Denied(String),
    /// The caller cancelled the wait.
    Cancelled,
    Remote(RemoteError),
}

impl std::fmt::Display for SignInError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SignInError::Expired => f.write_str("the sign-in code expired; start again"),
            SignInError::Denied(m) => write!(f, "sign-in refused: {m}"),
            SignInError::Cancelled => f.write_str("sign-in cancelled"),
            SignInError::Remote(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for SignInError {}

/// A device sign-in in progress: show `start.user_code` and open `start.verification_uri_complete`, then [`wait`].
///
/// [`wait`]: DeviceSignIn::wait
pub struct DeviceSignIn {
    http: Http,
    pub start: DeviceStart,
    /// Lower bound for the poll interval (the server asks for 5 s).
    pub min_interval: Duration,
}

impl DeviceSignIn {
    /// `POST /v1/auth/device/start`. `client_name` is the app ("goo"), `device_name` the computer.
    pub fn start(base_url: &str, client_name: &str, device_name: &str) -> Result<DeviceSignIn, RemoteError> {
        let http = Http::new(base_url);
        let body = json!({ "client_name": client_name, "device_name": device_name }).to_string();
        let start: DeviceStart = decode(http.send("POST", "/v1/auth/device/start", &[], None, Some(&body))?)?;
        Ok(DeviceSignIn { http, start, min_interval: Duration::from_secs(1) })
    }

    /// One `POST /v1/auth/device/poll`.
    pub fn poll(&self) -> Result<DevicePoll, RemoteError> {
        let body = json!({ "device_code": self.start.device_code }).to_string();
        decode(self.http.send("POST", "/v1/auth/device/poll", &[], None, Some(&body))?)
    }

    /// Polls at the server's interval until the sign-in is approved (the token pair is saved to `store` before this
    /// returns the user), denied, expired or `cancelled()` turns true. Network hiccups and `rate_limited` answers slow
    /// the polling down instead of failing it.
    pub fn wait(&self, store: &dyn TokenStore, cancelled: &dyn Fn() -> bool) -> Result<User, SignInError> {
        let deadline = Instant::now() + Duration::from_secs(self.start.expires_in);
        let mut interval = Duration::from_secs(self.start.interval).max(self.min_interval);
        loop {
            let until = Instant::now() + interval;
            while Instant::now() < until {
                if cancelled() {
                    return Err(SignInError::Cancelled);
                }
                thread::sleep(Duration::from_millis(50).min(until.saturating_duration_since(Instant::now())));
            }
            if Instant::now() >= deadline {
                return Err(SignInError::Expired);
            }
            match self.poll() {
                Ok(DevicePoll::Pending) => {}
                Ok(DevicePoll::Expired) => return Err(SignInError::Expired),
                Ok(DevicePoll::Denied { message, .. }) => return Err(SignInError::Denied(message)),
                Ok(DevicePoll::Approved { access_token, refresh_token, expires_in, user }) => {
                    let pair = TokenResponse { access_token, refresh_token, expires_in }.into_pair(now_secs());
                    store.save(&pair).map_err(|e| SignInError::Remote(RemoteError::TokenStore(e)))?;
                    return Ok(user);
                }
                Err(RemoteError::RateLimited { retry_after }) => {
                    interval = interval.max(Duration::from_secs(retry_after.unwrap_or(0))) + Duration::from_secs(5);
                }
                Err(RemoteError::Network(_)) => interval += Duration::from_secs(1).min(interval),
                Err(e) => return Err(SignInError::Remote(e)),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Signed-in calls
// ---------------------------------------------------------------------------

/// A signed-in connection to one goo cloud server. Cheap to share (`Arc`); calls from several threads share one refresh.
pub struct HttpRemote {
    http: Http,
    store: Arc<dyn TokenStore>,
    refresh_lock: Mutex<()>,
}

impl HttpRemote {
    pub fn new(base_url: &str, store: Arc<dyn TokenStore>) -> HttpRemote {
        HttpRemote { http: Http::new(base_url), store, refresh_lock: Mutex::new(()) }
    }

    /// An access token that is valid for at least another minute.
    fn access_token(&self) -> Result<String, RemoteError> {
        let pair = self.store.load().ok_or_else(|| RemoteError::SignInRequired("not signed in".into()))?;
        if pair.expires_at > now_secs() + REFRESH_MARGIN_SECS {
            return Ok(pair.access_token);
        }
        self.refresh(&pair.access_token)
    }

    /// Replaces the access token `stale` (single-flight): whoever gets the lock first refreshes; the others find a newer
    /// pair in the store and use it.
    fn refresh(&self, stale: &str) -> Result<String, RemoteError> {
        let _guard = self.refresh_lock.lock().unwrap_or_else(|p| p.into_inner());
        let pair = self.store.load().ok_or_else(|| RemoteError::SignInRequired("not signed in".into()))?;
        if pair.access_token != stale && pair.expires_at > now_secs() + REFRESH_MARGIN_SECS {
            return Ok(pair.access_token);
        }
        let body = json!({ "refresh_token": pair.refresh_token }).to_string();
        // A network error is returned as is: the refresh token may already be spent on the server, and retrying it right
        // away could trip reuse detection (which signs the user out everywhere).
        let answer = self.http.send("POST", "/v1/auth/refresh", &[], None, Some(&body))?;
        let fresh: TokenResponse = decode(answer)?;
        let fresh = fresh.into_pair(now_secs());
        self.store.save(&fresh).map_err(RemoteError::TokenStore)?;
        Ok(fresh.access_token)
    }

    /// A Bearer request; refreshes once and repeats it when the server answers `unauthenticated`.
    fn call<T: DeserializeOwned>(&self, method: &str, path: &str, query: &[(&str, String)], body: Option<&str>) -> Result<T, RemoteError> {
        let token = self.access_token()?;
        let answer = self.http.send(method, path, query, Some(&token), body)?;
        if !is_unauthenticated(&answer) {
            return decode(answer);
        }
        let token = self.refresh(&token)?;
        let answer = self.http.send(method, path, query, Some(&token), body)?;
        if is_unauthenticated(&answer) {
            return Err(RemoteError::SignInRequired(error_body(&answer).map(|e| e.message).unwrap_or_default()));
        }
        decode(answer)
    }

    /// `GET /v1/me`.
    pub fn me(&self) -> Result<Me, RemoteError> {
        self.call("GET", "/v1/me", &[], None)
    }

    /// `GET /v1/workspaces`, every page.
    pub fn list_workspaces(&self) -> Result<Vec<Workspace>, RemoteError> {
        let mut out = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let mut query = vec![("limit", "100".to_string())];
            if let Some(c) = &cursor {
                query.push(("cursor", c.clone()));
            }
            let page: WorkspacePage = self.call("GET", "/v1/workspaces", &query, None)?;
            out.extend(page.items);
            match page.page.next_cursor {
                Some(c) if page.page.has_more => cursor = Some(c),
                _ => return Ok(out),
            }
        }
    }

    /// `POST /v1/workspaces`; the caller becomes the owner.
    pub fn create_workspace(&self, name: &str, description: &str) -> Result<Workspace, RemoteError> {
        let body = json!({ "name": name, "description": description }).to_string();
        let env: WorkspaceEnvelope = self.call("POST", "/v1/workspaces", &[], Some(&body))?;
        Ok(Workspace { role: Some(Role::Owner), ..env.workspace })
    }

    /// `GET /v1/workspaces/{id}` with the caller's role in `role` (None for a platform admin who is not a member).
    pub fn get_workspace(&self, workspace_id: &str) -> Result<Workspace, RemoteError> {
        let env: WorkspaceEnvelope = self.call("GET", &format!("/v1/workspaces/{}", segment(workspace_id)), &[], None)?;
        Ok(Workspace { role: env.membership.map(|m| m.role), ..env.workspace })
    }

    /// `POST /v1/auth/logout`: revokes the refresh token on the server. The caller clears the keychain either way.
    pub fn sign_out(&self) -> Result<(), RemoteError> {
        let Some(pair) = self.store.load() else { return Ok(()) };
        let body = json!({ "refresh_token": pair.refresh_token }).to_string();
        let _: serde_json::Value = decode(self.http.send("POST", "/v1/auth/logout", &[], None, Some(&body))?)?;
        Ok(())
    }
}

impl Remote for HttpRemote {
    fn register_client(&self, info: &ClientInfo) -> Result<String, RemoteError> {
        let body = serde_json::to_string(info).map_err(|e| RemoteError::Protocol(e.to_string()))?;
        let r: Registered = self.call("POST", "/v1/sync/clients/register", &[], Some(&body))?;
        Ok(r.client_id)
    }

    fn pull(&self, workspace_id: &str, client_id: &str, after: u64, limit: u32) -> Result<PullPage, RemoteError> {
        let query = [("client_id", client_id.to_string()), ("after", after.to_string()), ("limit", limit.to_string())];
        self.call("GET", &format!("/v1/workspaces/{}/sync/pull", segment(workspace_id)), &query, None)
    }

    fn push(&self, workspace_id: &str, client_id: &str, changes: &[PushChange]) -> Result<PushResponse, RemoteError> {
        let body = serde_json::to_string(&PushRequest { client_id, changes }).map_err(|e| RemoteError::Protocol(e.to_string()))?;
        self.call("POST", &format!("/v1/workspaces/{}/sync/push", segment(workspace_id)), &[], Some(&body))
    }
}
