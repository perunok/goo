//! The goo cloud wire contract (`goo-cloud/server/openapi.yaml`) as serde types, the errors a server call can end in,
//! and the [`Remote`] trait the engine talks to (the HTTP client in production, an in-process fake in tests).
//!
//! Field names are the server's `snake_case`. Unknown fields are ignored and new enum values map to an `Other`/`Unknown`
//! variant, so a newer server does not break an older app (the protocol grows additively).

use serde::{Deserialize, Serialize};
use std::fmt;

/// Largest note content in UTF-8 bytes, as on the server. Bigger notes are quarantined locally and never sent.
pub const MAX_NOTE_BYTES: u64 = 4 * 1024 * 1024;
/// Most changes in one push.
pub const MAX_PUSH_CHANGES: usize = 200;
/// Note content per push batch (a single bigger note goes alone).
pub const PUSH_BATCH_BYTES: usize = 8 * 1024 * 1024;
/// Serialized size per push batch, below the server's 26 MiB body cap even when JSON escaping inflates the text.
pub const PUSH_BATCH_JSON_BYTES: usize = 24 * 1024 * 1024;
/// Changes asked for per pull page (the server also stops a page after about 8 MiB of content).
pub const PULL_LIMIT: u32 = 200;
/// The sync protocol version this client speaks.
pub const PROTOCOL_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Auth
// ---------------------------------------------------------------------------

/// What the app keeps in the OS keychain. `expires_at` is the access token's expiry in Unix seconds (computed from the
/// server's `expires_in` when the pair arrives).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: u64,
}

/// `POST /v1/auth/refresh` and the approved device poll.
#[derive(Debug, Clone, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
}

impl TokenResponse {
    pub fn into_pair(self, now: u64) -> TokenPair {
        TokenPair { access_token: self.access_token, refresh_token: self.refresh_token, expires_at: now + self.expires_in }
    }
}

/// `POST /v1/auth/device/start`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceStart {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: String,
    /// Seconds until the code expires.
    pub expires_in: u64,
    /// Seconds between polls.
    pub interval: u64,
}

/// `POST /v1/auth/device/poll`, one of four answers.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum DevicePoll {
    Pending,
    Expired,
    Denied {
        #[serde(default)]
        reason: String,
        #[serde(default)]
        message: String,
    },
    Approved {
        access_token: String,
        refresh_token: String,
        expires_in: u64,
        user: User,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct User {
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub platform_role: String,
    #[serde(default)]
    pub must_change_password: bool,
}

/// `GET /v1/me`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Me {
    pub user: User,
    pub workspace_memberships: Vec<Membership>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Membership {
    pub workspace_id: String,
    pub role: Role,
}

// ---------------------------------------------------------------------------
// Workspaces
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Viewer,
    Editor,
    Admin,
    Owner,
}

impl Role {
    /// Editors and above push; viewers only pull.
    pub fn can_push(self) -> bool {
        self >= Role::Editor
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Workspace {
    pub id: String,
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub owner_user_id: String,
    pub created_at: String,
    pub updated_at: String,
    pub version: u64,
    /// The caller's role (list items only; `GET /v1/workspaces/{id}` has it in `membership`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkspacePage {
    pub items: Vec<Workspace>,
    pub page: PageInfo,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PageInfo {
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkspaceEnvelope {
    pub workspace: Workspace,
    #[serde(default)]
    pub membership: Option<RoleOnly>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RoleOnly {
    pub role: Role,
}

// ---------------------------------------------------------------------------
// Sync
// ---------------------------------------------------------------------------

/// `POST /v1/sync/clients/register` body: describes this installation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientInfo {
    pub device_name: String,
    pub platform: String,
    pub app_version: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Registered {
    pub client_id: String,
    pub protocol_version: u32,
    #[serde(default)]
    pub features: Vec<String>,
}

/// One note in a pull page: its latest state. `content` and `content_hash` are null for tombstones.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PullChange {
    pub note_id: String,
    pub path: String,
    pub version: u64,
    pub deleted: bool,
    pub content_hash: Option<String>,
    pub content: Option<String>,
    pub seq: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullPage {
    pub changes: Vec<PullChange>,
    pub checkpoint: u64,
    pub has_more: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Op {
    Upsert,
    Delete,
}

/// One change in a push. On upsert `path` and `content` are each optional (absent = unchanged); a new note
/// (`base_version` 0) needs both.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PushChange {
    pub op_id: String,
    pub note_id: String,
    pub op: Op,
    pub base_version: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PushRequest<'a> {
    pub client_id: &'a str,
    pub changes: &'a [PushChange],
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Accepted {
    pub op_id: String,
    pub note_id: String,
    pub version: u64,
    pub seq: u64,
}

/// Why the server refused a change. The engine branches on this, never on the HTTP status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    VersionMismatch,
    PathTaken,
    NotFound,
    TooLarge,
    Invalid,
    Forbidden,
    #[serde(other)]
    Unknown,
}

impl Reason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Reason::VersionMismatch => "version_mismatch",
            Reason::PathTaken => "path_taken",
            Reason::NotFound => "not_found",
            Reason::TooLarge => "too_large",
            Reason::Invalid => "invalid",
            Reason::Forbidden => "forbidden",
            Reason::Unknown => "unknown",
        }
    }
}

/// The server's state of a note, sent with `version_mismatch` / `path_taken`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Current {
    pub version: u64,
    pub path: String,
    pub deleted: bool,
    pub content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Rejected {
    pub op_id: String,
    pub note_id: String,
    pub reason: Reason,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub current: Option<Current>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushResponse {
    pub accepted: Vec<Accepted>,
    pub rejected: Vec<Rejected>,
    /// The workspace checkpoint after the push. Never used to advance the pull checkpoint.
    pub checkpoint: u64,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// `{ "error": { code, message, details, request_id } }`.
#[derive(Debug, Clone, Deserialize)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ErrorBody {
    pub code: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub details: serde_json::Value,
    #[serde(default)]
    pub request_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteError {
    /// The server could not be reached or the connection broke. Try again later (with backoff).
    Network(String),
    /// No tokens, or the server refused the refresh token: the user must sign in again.
    SignInRequired(String),
    /// `rate_limited`: wait `retry_after` seconds.
    RateLimited { retry_after: Option<u64> },
    /// Any other structured error (`code` is the server's error code, e.g. `workspace_access_denied`).
    Api { status: u16, code: String, message: String },
    /// The answer did not match the contract.
    Protocol(String),
    /// The keychain could not store the new token pair (it was not used).
    TokenStore(String),
}

impl RemoteError {
    pub fn code(&self) -> Option<&str> {
        match self {
            RemoteError::Api { code, .. } => Some(code),
            _ => None,
        }
    }
}

impl fmt::Display for RemoteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RemoteError::Network(m) => write!(f, "cannot reach goo cloud: {m}"),
            RemoteError::SignInRequired(m) => write!(f, "sign in to goo cloud again ({m})"),
            RemoteError::RateLimited { retry_after: Some(s) } => write!(f, "goo cloud is busy; try again in {s} s"),
            RemoteError::RateLimited { retry_after: None } => f.write_str("goo cloud is busy; try again later"),
            RemoteError::Api { code, message, .. } => write!(f, "{message} ({code})"),
            RemoteError::Protocol(m) => write!(f, "unexpected answer from goo cloud: {m}"),
            RemoteError::TokenStore(m) => write!(f, "cannot store the sign-in: {m}"),
        }
    }
}

impl std::error::Error for RemoteError {}

// ---------------------------------------------------------------------------
// What the engine needs from a server
// ---------------------------------------------------------------------------

/// The sync calls of one signed-in account. Implemented by [`crate::http::HttpRemote`] and by the tests' fake server.
pub trait Remote {
    /// `POST /v1/sync/clients/register`, returns the `client_id`.
    fn register_client(&self, info: &ClientInfo) -> Result<String, RemoteError>;
    /// `GET /v1/workspaces/{id}/sync/pull`.
    fn pull(&self, workspace_id: &str, client_id: &str, after: u64, limit: u32) -> Result<PullPage, RemoteError>;
    /// `POST /v1/workspaces/{id}/sync/push`.
    fn push(&self, workspace_id: &str, client_id: &str, changes: &[PushChange]) -> Result<PushResponse, RemoteError>;
}
