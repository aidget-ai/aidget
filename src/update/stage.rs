//! What a download fetches, and where it lands before it is installed.
//!
//! Split from the state machine that drives it: choosing the asset and owning the
//! temporary directory is the download's own concern.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::common::error::{Error, Result};
use crate::update::github::{Asset, Release};
use crate::update::snapshot::UpdateSnapshot;
use crate::update::target::Target;
use crate::update::{download, verify};

/// One asset, ready to fetch.
#[derive(Debug)]
pub struct Download {
    pub asset: Asset,
    /// Release identity frozen before network work begins.
    pub version: String,
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
        version: crate::update::version::normalised(&release.tag_name),
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

impl Download {
    /// Fetch the asset, verify it, and return where it landed.
    ///
    /// A file that fails verification is removed: nothing unverified is left
    /// looking installable, and a failed attempt takes its download directory
    /// with it instead of leaving an empty folder in the temp directory.
    pub fn run(
        self,
        progress: &Arc<Mutex<UpdateSnapshot>>,
        token: Option<&str>,
    ) -> Result<PathBuf> {
        let path = self.path.clone();
        let fetched = self.fetch(progress, token);
        if fetched.is_err() {
            if let Some(dir) = path.parent() {
                let _ = std::fs::remove_dir_all(dir);
            }
        }
        fetched
    }

    fn fetch(self, progress: &Arc<Mutex<UpdateSnapshot>>, token: Option<&str>) -> Result<PathBuf> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let sums = match self.asset.digest {
            Some(_) => None,
            None => {
                let url = self.sums_url.as_deref().ok_or_else(|| {
                    Error::Other("the release publishes no SHA256SUMS".to_string())
                })?;
                Some(download::text(url, token)?)
            }
        };
        let expected = verify::expected(&self.asset, sums.as_deref())?;

        let progress = progress.clone();
        let actual = download::to_file(
            &self.asset.browser_download_url,
            token,
            &self.path,
            &mut |done, total| {
                let mut snapshot = progress.lock().unwrap();
                snapshot.done = Some(done);
                if total > 0 {
                    snapshot.total = Some(total);
                }
            },
        )?;

        if actual != expected {
            let _ = std::fs::remove_file(&self.path);
            return Err(Error::Other(format!(
                "{}: checksum mismatch, expected {expected}, got {actual}",
                self.asset.name
            )));
        }
        Ok(self.path)
    }
}
