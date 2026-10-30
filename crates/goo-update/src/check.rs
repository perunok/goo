//! "Is there a newer goo?" One GET to GitHub's latest-release endpoint (drafts and pre-releases are never returned
//! there). It sends nothing about the user or their notes and downloads no installer.

use crate::release::{self, ReleaseInfo, LATEST_RELEASE_API, MAX_RELEASE_JSON};
use serde::Serialize;
use serde_json::Value;
use std::fmt;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    pub current_version: String,
    /// None: no release has been published yet.
    pub latest: Option<ReleaseInfo>,
    pub update_available: bool,
    /// Unix seconds.
    pub checked_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckError {
    /// GitHub could not be reached, did not answer in time, or refused (its rate limit is 60 requests an hour).
    Network(String),
    /// The answer was not release information goo understands.
    Unreadable(String),
}

impl fmt::Display for CheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CheckError::Network(m) | CheckError::Unreadable(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for CheckError {}

pub fn now_secs() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// The HTTP client for GitHub: rustls, redirects followed (downloads are redirected to GitHub's CDN), HTTPS only unless
/// `allow_http` (a local test feed), status codes handled by the caller.
pub(crate) fn agent(allow_http: bool, global_timeout: Option<Duration>) -> ureq::Agent {
    ureq::Agent::config_builder()
        .https_only(!allow_http)
        .http_status_as_error(false)
        .timeout_global(global_timeout)
        .timeout_connect(Some(Duration::from_secs(15)))
        .build()
        .into()
}

/// Fetches the release JSON from `feed_url` (GitHub's latest-release endpoint, or a test feed).
pub(crate) fn fetch_release_json(feed_url: &str, current: &str) -> Result<Option<Value>, CheckError> {
    let allow_http = feed_url != LATEST_RELEASE_API;
    let mut res = agent(allow_http, Some(TIMEOUT))
        .get(feed_url)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", &format!("goo/{current}"))
        .header("X-GitHub-Api-Version", "2022-11-28")
        .call()
        .map_err(|e| match e {
            ureq::Error::Timeout(_) => CheckError::Network("GitHub did not answer in time".into()),
            _ => CheckError::Network("Could not reach GitHub to check for updates".into()),
        })?;
    match res.status().as_u16() {
        404 => return Ok(None), // no release published yet
        200..=299 => {}
        code => return Err(CheckError::Network(format!("GitHub answered HTTP {code}"))),
    }
    let text = res
        .body_mut()
        .with_config()
        .limit(MAX_RELEASE_JSON + 1)
        .read_to_string()
        .map_err(|e| match e {
            ureq::Error::BodyExceedsLimit(_) => CheckError::Unreadable("The release information from GitHub is too large".into()),
            _ => CheckError::Network("Could not read GitHub's answer".into()),
        })?;
    if text.len() as u64 > MAX_RELEASE_JSON {
        return Err(CheckError::Unreadable("The release information from GitHub is too large".into()));
    }
    serde_json::from_str(&text).map(Some).map_err(|_| CheckError::Unreadable("GitHub sent release information that is not JSON".into()))
}

pub fn check(current_version: &str, feed_url: &str) -> Result<CheckResult, CheckError> {
    let checked_at = now_secs();
    let Some(body) = fetch_release_json(feed_url, current_version)? else {
        return Ok(CheckResult { current_version: current_version.into(), latest: None, update_available: false, checked_at });
    };
    let latest = release::parse_release(&body).ok_or_else(|| CheckError::Unreadable("GitHub sent release information goo does not understand".into()))?;
    let update_available = release::is_newer(&latest.version, current_version);
    Ok(CheckResult { current_version: current_version.into(), latest: Some(latest), update_available, checked_at })
}

/// Where to look: `GOO_UPDATE_FEED_URL` (http/https) if set, GitHub otherwise; None when update checks are turned off
/// (`GOO_UPDATES=off`, for automated runs and packagers that update goo themselves).
pub fn feed_url(override_url: Option<&str>, updates_off: bool) -> Result<Option<String>, String> {
    if let Some(url) = override_url.map(str::trim).filter(|u| !u.is_empty()) {
        let lower = url.to_ascii_lowercase();
        return if lower.starts_with("http://") || lower.starts_with("https://") {
            Ok(Some(url.to_string()))
        } else {
            Err("GOO_UPDATE_FEED_URL must be an http(s) URL".into())
        };
    }
    Ok((!updates_off).then(|| LATEST_RELEASE_API.to_string()))
}
