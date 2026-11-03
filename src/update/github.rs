//! The GitHub release the update check reads.
//!
//! `GET /repos/{owner}/{repo}/releases/latest` returns the newest published
//! release; drafts and pre-releases are excluded by GitHub itself, so the
//! client needs no channel logic.

use std::io::Read;
use std::time::Duration;

use serde::Deserialize;

use crate::common::error::{Error, Result};
use crate::update::fetch;
use crate::update::target::Target;

/// The repository every build reads for releases.
pub const REPOSITORY: &str = "aidget-ai/aidget";

/// Where the release feed lives. Tests point [`latest`] elsewhere instead.
pub const API_BASE: &str = "https://api.github.com";

/// What a download would do on this machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageKind {
    /// macOS: a zipped `.app` the updater can swap in.
    MacApp,
    /// Windows: the NSIS installer.
    WindowsSetup,
    /// Linux: a distribution package upgraded through the package manager.
    Deb,
    /// Linux: a distribution package upgraded through the package manager.
    Rpm,
}

impl PackageKind {
    /// The public filename extension for this distribution format.
    pub fn suffix(self) -> &'static str {
        match self {
            PackageKind::MacApp => ".zip",
            PackageKind::WindowsSetup => ".exe",
            PackageKind::Deb => ".deb",
            PackageKind::Rpm => ".rpm",
        }
    }
}

/// Read the newest published release of `repository` from `base`.
///
/// `None` means the repository has published nothing: GitHub answers 404 on
/// `releases/latest` for a project before its first release, which is a state,
/// not a failure. GitHub's endpoint already excludes drafts and pre-releases.
/// `token` is the user's, when configured: an unauthenticated caller shares 60
/// requests an hour per address, a token raises that to 5000.
pub fn latest(base: &str, repository: &str, token: Option<&str>) -> Result<Option<Release>> {
    let url = format!("{base}/repos/{repository}/releases/latest");
    fetch::ensure_secure(&url)?;
    let client = fetch::client()?;
    let mut response = fetch::github_api_get(&client, &url, token)?
        .timeout(Duration::from_secs(30))
        .send()
        .map_err(|error| Error::Other(format!("update feed: {error}")))?;
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let body = read_body(&mut response)?;

    if status == 404 && nothing_published(&body) {
        return Ok(None);
    }
    if !(200..300).contains(&status) {
        return Err(Error::Other(format!(
            "update feed: {}",
            describe_failure(status, &headers, &body)
        )));
    }
    let release = serde_json::from_str::<Release>(&body)
        .map_err(|error| Error::Other(format!("update feed: {error}")))?;
    Ok(Some(release))
}

/// Whether a 404 body is GitHub saying the repository has no release.
///
/// The same status answers a mistyped repository, but that name is a constant
/// chosen at compile time, so a release build cannot be pointed elsewhere and a
/// silent "up to date" is the worse error of the two.
fn nothing_published(body: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("message")
                .and_then(|message| message.as_str())
                .map(str::to_string)
        })
        .as_deref()
        == Some("Not Found")
}

/// Read a feed answer, refusing an oversized one.
///
/// Release notes are large, but not this large, and the limit is enforced whether
/// or not the answer declares a length.
fn read_body(response: &mut reqwest::blocking::Response) -> Result<String> {
    if response
        .content_length()
        .is_some_and(|length| length > FEED_LIMIT)
    {
        return Err(Error::Other(format!(
            "update feed: body exceeds {FEED_LIMIT} bytes"
        )));
    }
    let mut bytes = Vec::new();
    response
        .take(FEED_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| Error::Other(format!("update feed: {error}")))?;
    if bytes.len() as u64 > FEED_LIMIT {
        return Err(Error::Other(format!(
            "update feed: body exceeds {FEED_LIMIT} bytes"
        )));
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Largest feed answer we are willing to read: release notes are large, but not
/// this large.
const FEED_LIMIT: u64 = 1024 * 1024;

/// What to tell the user about a failed answer.
///
/// Unauthenticated requests share a quota of 60 per hour per address, and a proxy
/// exit node is shared with everyone else behind it, so a bare "HTTP 403" hides
/// the one thing the user can act on.
pub fn describe_failure(status: u16, headers: &reqwest::header::HeaderMap, body: &str) -> String {
    let message = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("message")
                .and_then(|message| message.as_str())
                .map(str::to_string)
        });
    let remaining = headers
        .get("x-ratelimit-remaining")
        .and_then(|value| value.to_str().ok());
    let reset = headers
        .get("x-ratelimit-reset")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());

    let limit_reached = matches!(status, 403 | 429) && remaining == Some("0");
    if limit_reached {
        let wait = reset.map_or_else(
            || "later".to_string(),
            |reset| {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|since| since.as_secs())
                    .unwrap_or(0);
                let minutes = reset.saturating_sub(now).div_ceil(60);
                format!("in about {minutes} minute(s)")
            },
        );
        return format!("GitHub rate limit reached; try again {wait}");
    }
    match message {
        Some(message) => format!("HTTP {status}: {message}"),
        None => format!("HTTP {status}"),
    }
}

/// One published release, as much of it as the updater reads.
#[derive(Debug, Clone, Deserialize)]
pub struct Release {
    pub tag_name: String,
    #[serde(default)]
    pub body: String,
    pub html_url: String,
    #[serde(default)]
    pub assets: Vec<Asset>,
}

/// One downloadable file of a release.
#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
    /// `sha256:<hex>`, or null when GitHub did not compute one.
    #[serde(default)]
    pub digest: Option<String>,
}

impl Release {
    /// The `SHA256SUMS` asset, when the release publishes one.
    pub fn sha256sums(&self) -> Option<&Asset> {
        self.assets.iter().find(|asset| asset.name == "SHA256SUMS")
    }

    /// The asset this machine would install, if the release has one.
    ///
    /// Match the complete public filename, including the product and platform.
    pub fn asset_for(&self, target: Target) -> Option<&Asset> {
        let wanted = target.asset_name();
        self.assets.iter().find(|asset| asset.name == wanted)
    }
}
