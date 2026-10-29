//! Bounded GitHub reads for the update feed and its assets.
//!
//! `community::http::Http` is the two-channel client: it hides the status and the
//! headers because its callers only ask "missing or unavailable?". The updater
//! needs both — a 404 on `releases/latest` means the repository has published
//! nothing, and the rate-limit headers say how long to wait — so it builds its own
//! client from the same settings instead of widening that one.

use std::time::Duration;

use reqwest::blocking::{Client, RequestBuilder};
use reqwest::header::{ACCEPT, AUTHORIZATION};

/// The media type GitHub's API answers with.
const GITHUB_ACCEPT: &str = "application/vnd.github+json";
/// Pins the response shape, as GitHub asks of API clients.
const GITHUB_API_VERSION: &str = "2022-11-28";

/// A client with fixed deadlines and HTTPS-only redirects.
///
/// GitHub answers an asset request with a redirect to its object storage, so
/// redirects are followed — but only while they stay on HTTPS.
pub fn client() -> crate::common::error::Result<Client> {
    Client::builder()
        .user_agent(concat!("aidget/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 || attempt.url().scheme() != "https" {
                attempt.error("invalid download redirect")
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|error| crate::common::error::Error::Other(error.to_string()))
}

/// A GitHub API GET, carrying the user's token when one is configured.
///
/// An unauthenticated caller gets 60 requests an hour for the whole address; with
/// a token it is 5000, which is why the check uses the stored one. An asset
/// request redirects to object storage on another host, where `reqwest` drops the
/// `Authorization` header itself, so the token reaches `api.github.com` only.
pub fn github_get(client: &Client, url: &str, token: Option<&str>) -> RequestBuilder {
    let request = client
        .get(url)
        .header(ACCEPT, GITHUB_ACCEPT)
        .header("X-GitHub-Api-Version", GITHUB_API_VERSION);
    match token {
        Some(token) => request.header(AUTHORIZATION, format!("Bearer {token}")),
        None => request,
    }
}

/// Refuse a URL that would cross the network in clear text.
///
/// The feed and its assets are HTTPS, so a plaintext URL means the feed was
/// tampered with or is not GitHub. Loopback is the exception: that is the local
/// server the tests use, and what it serves still has to match a published
/// checksum before anything installs it.
pub fn ensure_secure(url: &str) -> crate::common::error::Result<()> {
    use crate::common::error::Error;

    let parsed =
        reqwest::Url::parse(url).map_err(|error| Error::Other(format!("download: {error}")))?;
    // An IPv6 literal keeps its brackets in `host_str`, so `[::1]` has to lose
    // them before it can be parsed as an address.
    let loopback = parsed.host_str().is_some_and(|host| {
        let bare = host
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
            .unwrap_or(host);
        bare == "localhost"
            || bare
                .parse::<std::net::IpAddr>()
                .is_ok_and(|address| address.is_loopback())
    });
    match parsed.scheme() {
        "https" => Ok(()),
        "http" if loopback => Ok(()),
        scheme => Err(Error::Other(format!(
            "refusing to download a release asset over {scheme}"
        ))),
    }
}
