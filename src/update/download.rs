//! Fetching a release asset, with the sha-256 computed as it lands.
//!
//! Calls are blocking; the update service runs them inside `spawn_blocking`.

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::common::error::{Error, Result};
use crate::update::fetch;

/// How long a download may take in total. The largest asset is tens of megabytes,
/// so a slow link still fits.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);

/// Download `url` to `dest`, returning the sha-256 of what was written.
///
/// `progress` receives `(written, total)`, where `total` is 0 when the response
/// carries no length. The caller verifies the returned digest before anything
/// treats the file as installable.
pub fn to_file(
    url: &str,
    token: Option<&str>,
    dest: &Path,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<String> {
    fetch::ensure_secure(url)?;
    let client = fetch::client()?;
    let mut response = fetch::github_get(&client, url, token)
        .timeout(DOWNLOAD_TIMEOUT)
        .send()
        .map_err(|error| Error::Other(format!("download: {error}")))?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        let body = error_body(&mut response);
        return Err(Error::Other(format!(
            "download {url}: {}",
            crate::update::github::describe_failure(status, response.headers(), &body)
        )));
    }
    let total = response.content_length().unwrap_or(0);

    let mut file = File::create(dest)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut written = 0u64;
    loop {
        let read = response
            .read(&mut buffer)
            .map_err(|error| Error::Other(format!("download: {error}")))?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])?;
        hasher.update(&buffer[..read]);
        written += read as u64;
        progress(written, total);
    }
    file.flush()?;
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

/// Read a small text asset, such as `SHA256SUMS`.
pub fn text(url: &str, token: Option<&str>) -> Result<String> {
    fetch::ensure_secure(url)?;
    let client = fetch::client()?;
    let mut response = fetch::github_get(&client, url, token)
        .timeout(Duration::from_secs(30))
        .send()
        .map_err(|error| Error::Other(format!("download: {error}")))?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        let body = error_body(&mut response);
        return Err(Error::Other(format!(
            "download {url}: {}",
            crate::update::github::describe_failure(status, response.headers(), &body)
        )));
    }
    let mut body = String::new();
    response
        .take(1024 * 1024)
        .read_to_string(&mut body)
        .map_err(|error| Error::Other(format!("download: {error}")))?;
    Ok(body)
}

/// The first part of a failed answer, so the message can name the reason.
///
/// A body that cannot be read leaves the status, which is the actionable part.
fn error_body(response: &mut reqwest::blocking::Response) -> String {
    let mut body = String::new();
    let _ = response.take(8 * 1024).read_to_string(&mut body);
    body
}
