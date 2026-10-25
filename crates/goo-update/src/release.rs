//! What GitHub says about a release, trusted as little as possible: only a stable semantic version, the publish time and
//! the assets (name, URL, size, SHA-256 digest) are read, and the release page link is built from the version, never
//! taken from the answer.

use semver::Version;
use serde::Serialize;
use serde_json::Value;

pub const RELEASES_REPO: &str = "perunok/goo";
pub const LATEST_RELEASE_API: &str = "https://api.github.com/repos/perunok/goo/releases/latest";
pub const DOWNLOAD_PREFIX: &str = "https://github.com/perunok/goo/releases/download/";

/// Larger answers are refused (a release JSON is a few KB, mostly the notes).
pub const MAX_RELEASE_JSON: u64 = 1024 * 1024;

pub fn release_page_url(version: &str) -> String {
    format!("https://github.com/{RELEASES_REPO}/releases/tag/v{version}")
}

/// A stable version: "1.2.3" or "v1.2.3", no pre-release part, no build metadata.
pub fn parse_stable(tag: &str) -> Option<Version> {
    let v = Version::parse(tag.trim().strip_prefix('v').unwrap_or(tag.trim())).ok()?;
    (v.pre.is_empty() && v.build.is_empty()).then_some(v)
}

/// Whether `latest` is newer than `current`. An unparseable current version (a development build) never offers anything.
pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_stable(latest), Version::parse(current).ok()) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseInfo {
    pub version: String,
    /// The release page on GitHub (built from the version).
    pub url: String,
    /// Unix seconds.
    pub published_at: Option<i64>,
}

/// `tag_name` and `published_at` of GitHub's release JSON; drafts, pre-releases and unreadable tags give None.
pub fn parse_release(body: &Value) -> Option<ReleaseInfo> {
    let o = body.as_object()?;
    if o.get("draft").and_then(Value::as_bool) == Some(true) || o.get("prerelease").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let version = parse_stable(o.get("tag_name")?.as_str()?)?.to_string();
    let published_at = o.get("published_at").and_then(Value::as_str).and_then(parse_iso8601);
    Some(ReleaseInfo { url: release_page_url(&version), version, published_at })
}

/// `2026-10-12T13:16:19Z` as Unix seconds (GitHub always sends UTC with a Z).
pub fn parse_iso8601(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() != 20 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' || b[16] != b':' || b[19] != b'Z' {
        return None;
    }
    let n = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, mo, d, h, mi, se) = (n(0..4)?, n(5..7)?, n(8..10)?, n(11..13)?, n(14..16)?, n(17..19)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || se > 60 {
        return None;
    }
    // days from civil (Howard Hinnant)
    let y = if mo <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (mo + if mo > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(days * 86400 + h * 3600 + mi * 60 + se)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseAsset {
    pub name: String,
    pub url: String,
    pub size: u64,
    /// Lower-case hex SHA-256, or None when GitHub gave no digest.
    pub sha256: Option<String>,
}

/// `assets[]` of the release JSON; entries that are not well-formed are skipped.
pub fn parse_assets(body: &Value) -> Vec<ReleaseAsset> {
    let Some(list) = body.get("assets").and_then(Value::as_array) else { return Vec::new() };
    list.iter()
        .filter_map(|a| {
            let name = a.get("name")?.as_str()?.to_string();
            let url = a.get("browser_download_url")?.as_str()?.to_string();
            let size = a.get("size")?.as_u64()?;
            let sha256 = a
                .get("digest")
                .and_then(Value::as_str)
                .and_then(|d| d.strip_prefix("sha256:"))
                .filter(|h| h.len() == 64 && h.bytes().all(|c| c.is_ascii_hexdigit()))
                .map(str::to_ascii_lowercase);
            Some(ReleaseAsset { name, url, size, sha256 })
        })
        .collect()
}
