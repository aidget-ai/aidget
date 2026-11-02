//! Cached npm download statistics with bounded concurrent HTTP requests.
use super::{encoded_package, Community};
use crate::{
    common::{
        error::{Error, Result},
        utils::{atomic_write, sha256_hex},
    },
    community::model::DownloadCount,
    widgets::{package::validate_package_name, store::Store},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

const DOWNLOAD_WORKERS: usize = 6;
// atomic_write uses a process-wide temporary name for each destination path.
static CACHE_WRITES: Mutex<()> = Mutex::new(());

#[derive(Serialize, Deserialize)]
struct CountCache {
    at: u64,
    count: u64,
}

impl Community {
    /// Fetch weekly counts with up to six workers and a one-hour cache, preserving input order.
    /// Report cached and completed results immediately; `on_count` may run concurrently.
    /// Unavailable counts never become zero; cache and callback errors propagate.
    pub fn downloads(
        &self,
        store: &Store,
        names: &[String],
        on_count: impl Fn(&DownloadCount) -> Result<()> + Sync,
    ) -> Result<Vec<DownloadCount>> {
        if names.len() > 100 {
            return Err(Error::InvalidParams(
                "too many download-count requests".into(),
            ));
        }
        for name in names {
            validate_package_name(name)?;
        }
        let cache_dir = store.state_dir().join("community/downloads");
        fs::create_dir_all(&cache_dir)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| Error::Other(e.to_string()))?
            .as_secs();
        let mut counts = vec![];
        let mut pending = vec![];
        // Read caches before sending requests so invalid local data fails immediately.
        for (index, name) in names.iter().enumerate() {
            let path = cache_dir.join(format!("{}.json", sha256_hex(name.as_bytes())));
            let cached = match fs::read(&path) {
                Ok(bytes) => Some(serde_json::from_slice::<CountCache>(&bytes)?),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => return Err(e.into()),
            };
            if let Some(cache) = cached.filter(|c| now.saturating_sub(c.at) < 3600) {
                counts.push((
                    index,
                    DownloadCount {
                        package: name.clone(),
                        weekly: Some(cache.count),
                        error: None,
                    },
                ));
            } else {
                pending.push((index, name, path));
            }
        }
        for (_, count) in &counts {
            on_count(count)?;
        }
        let next = AtomicUsize::new(0);
        thread::scope(|scope| -> Result<()> {
            let workers: Vec<_> = (0..pending.len().min(DOWNLOAD_WORKERS))
                .map(|_| {
                    scope.spawn(|| -> Result<Vec<_>> {
                        let mut values = vec![];
                        while let Some((index, name, path)) =
                            pending.get(next.fetch_add(1, Ordering::Relaxed))
                        {
                            let result = self.json::<Value>(&format!(
                                "{}/downloads/point/last-week/{}",
                                self.endpoints.downloads,
                                encoded_package(name)
                            ));
                            let result = result.and_then(|v| {
                                v["downloads"].as_u64().ok_or_else(|| {
                                    Error::InvalidParams("invalid npm downloads response".into())
                                })
                            });
                            let count = match result {
                                Ok(count) => {
                                    let _write = CACHE_WRITES.lock().map_err(|error| {
                                        Error::Other(format!(
                                            "download cache lock poisoned: {error}"
                                        ))
                                    })?;
                                    atomic_write(
                                        path,
                                        &serde_json::to_vec(&CountCache { at: now, count })?,
                                    )?;
                                    DownloadCount {
                                        package: (*name).clone(),
                                        weekly: Some(count),
                                        error: None,
                                    }
                                }
                                // A failed statistic remains optional; failures in cache I/O above propagate.
                                Err(error) => DownloadCount {
                                    package: (*name).clone(),
                                    weekly: None,
                                    error: Some(error.to_string()),
                                },
                            };
                            on_count(&count)?;
                            values.push((*index, count));
                        }
                        Ok(values)
                    })
                })
                .collect();
            for worker in workers {
                counts.extend(
                    worker
                        .join()
                        .map_err(|_| Error::Other("download-count worker panicked".into()))??,
                );
            }
            Ok(())
        })?;
        counts.sort_by_key(|(index, _)| *index);
        Ok(counts.into_iter().map(|(_, count)| count).collect())
    }
}
