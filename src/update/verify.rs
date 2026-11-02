//! Checksum handling for a downloaded asset.
//!
//! GitHub publishes `digest` per asset as `sha256:<hex>`. When it is absent the
//! release's `SHA256SUMS` file is the fallback. Without either, nothing is
//! installed.

use std::collections::HashMap;

use crate::common::error::{Error, Result};
use crate::update::github::Asset;

/// The sha-256 a release publishes for `asset`, lowercase hex.
pub fn expected(asset: &Asset, sums: Option<&str>) -> Result<String> {
    if let Some(digest) = asset.digest.as_deref() {
        return digest_hex(digest)
            .ok_or_else(|| Error::Other(format!("{}: malformed digest {digest}", asset.name)));
    }
    let sums = sums.ok_or_else(|| {
        Error::Other(format!(
            "{}: the release publishes no digest and no SHA256SUMS",
            asset.name
        ))
    })?;
    parse_sums(sums)
        .remove(&asset.name)
        .ok_or_else(|| Error::Other(format!("SHA256SUMS has no entry for {}", asset.name)))
}

/// The hex part of a `sha256:<hex>` digest, when it is a well-formed sha-256.
fn digest_hex(digest: &str) -> Option<String> {
    let hex = digest.strip_prefix("sha256:")?;
    let well_formed = hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit());
    well_formed.then(|| hex.to_ascii_lowercase())
}

/// Read a `SHA256SUMS` document: `<hex>  <name>` per line.
///
/// Blank lines are skipped. Lines without a name carry nothing to match on, so
/// they are dropped rather than failing the whole file.
pub fn parse_sums(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let hex = parts.next()?;
            let name = parts.next()?;
            Some((
                name.trim_start_matches('*').to_string(),
                hex.to_ascii_lowercase(),
            ))
        })
        .collect()
}
