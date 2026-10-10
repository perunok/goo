//! goo's desktop sync engine for goo cloud (`goo-cloud/docs/DESIGN.md` §4). No Tauri dependency.
//!
//! - [`api`]: the wire contract as serde types, [`api::RemoteError`] and the [`api::Remote`] trait.
//! - [`http`]: [`http::HttpRemote`] (ureq + rustls) with single-flight token refresh over a [`http::TokenStore`] (the OS
//!   keychain in the app), device sign-in ([`http::DeviceSignIn`]), `me`, workspaces, client registration.
//! - [`state`]: per-workspace state outside the notes folder (`state.json` + `base/<note_id>`).
//! - [`scan`]: local changes derived from the folder against the state (creates, edits, renames, deletes).
//! - [`merge`]: 3-way line merge (diffy).
//! - [`engine`]: one sync cycle, [`sync`]: pull + apply, push + apply the answers, up to 4 rounds.

pub mod api;
pub mod engine;
pub mod http;
pub mod merge;
pub mod scan;
pub mod state;

pub use engine::{sync, Conflict, Quarantined, Refused, Report, SyncError, SyncFailure, SyncOptions};

use sha2::{Digest, Sha256};

/// SHA-256 of a note's bytes, lowercase hex (the server's `content_hash`).
pub fn content_hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

pub(crate) fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[cfg(test)]
mod tests;
