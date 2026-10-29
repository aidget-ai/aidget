//! What a download fetches, and where it lands before it is installed.
//!
//! Split from the state machine that drives it: choosing the asset and owning the
//! temporary directory is the download's own concern.

use std::path::{Path, PathBuf};

use crate::common::error::{Error, Result};
use crate::update::github::{Asset, Release};
use crate::update::target::Target;

/// One asset, ready to fetch.
#[derive(Debug)]
pub struct Download {
    pub asset: Asset,
    /// Where `SHA256SUMS` lives, read only when the asset carries no digest.
    pub sums_url: Option<String>,
    pub path: PathBuf,
}

/// Work out what to fetch for `release` on `target`, and where it lands.
///
/// Every download gets its own directory: the previous one is dropped first, so
/// stale archives neither pile up in the temp directory nor keep a `staged` path
/// alive after their file is gone. A per-download name also keeps two downloads
/// from deleting each other's files.
pub fn prepare(release: &Release, target: Target, previous: Option<&Path>) -> Result<Download> {
    let asset = release
        .asset_for(target)
        .ok_or_else(|| {
            Error::Other(format!(
                "release {} has no {} asset",
                release.tag_name,
                target.kind.suffix()
            ))
        })?
        .clone();
    let dir = download_dir();
    if let Some(dir) = previous.and_then(Path::parent) {
        let _ = std::fs::remove_dir_all(dir);
    }
    std::fs::create_dir_all(&dir)?;

    Ok(Download {
        path: dir.join(&asset.name),
        sums_url: release
            .sha256sums()
            .map(|sums| sums.browser_download_url.clone()),
        asset,
    })
}

/// A fresh directory for one download.
fn download_dir() -> PathBuf {
    std::env::temp_dir().join(format!("aidget-update-{}", uuid::Uuid::new_v4()))
}
