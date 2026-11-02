//! Persistent immutable catalogues and the last successfully checked main revision.
use super::{Community, MAX_JSON};
use crate::{
    common::{
        error::{Error, Result},
        utils::{atomic_write, path_mutex, read_bounded, sha256_hex},
    },
    community::{
        model::*,
        snapshot::{self, validate_commit},
    },
    widgets::store::Store,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

static REGISTRIES: Mutex<VecDeque<(PathBuf, String, Arc<Registry>)>> = Mutex::new(VecDeque::new());

#[derive(Serialize, Deserialize)]
struct Latest {
    commit: String,
    checked_at: u64,
    requested_at: u128,
}

/// Serialize cache filesystem operations across clients and processes.
pub(super) fn cache_lock(store: &Store) -> Result<fs::File> {
    let dir = store.state_dir().join("community");
    fs::create_dir_all(&dir)?;
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join(".cache.lock"))?;
    fs2::FileExt::lock_exclusive(&file)?;
    Ok(file)
}

fn validate(registry: &Registry) -> Result<()> {
    let mut ids = BTreeSet::new();
    let mut names = BTreeSet::new();
    for entry in &registry.widgets {
        entry.package.validate()?;
        entry.config.validate()?;
        if !entry.package.is_community()
            || entry.config.screenshots.is_empty()
            || !ids.insert(&entry.config.id)
            || !names.insert(&entry.package.name)
        {
            return Err(Error::InvalidParams(
                "invalid or duplicate reviewed community entry".into(),
            ));
        }
        snapshot::validate_files(&entry.files)?;
        for path in entry
            .config
            .logo
            .iter()
            .chain(entry.config.screenshots.iter())
        {
            if !entry.files.iter().any(|file| &file.path == path) {
                return Err(Error::InvalidParams(
                    "declared media missing from registry digest list".into(),
                ));
            }
        }
    }
    Ok(())
}

impl Community {
    fn registry_dir(&self, store: &Store) -> PathBuf {
        store
            .state_dir()
            .join("community/registries")
            .join(sha256_hex(self.endpoints.raw.as_bytes()))
    }

    pub(super) fn registry(&self, store: &Store, commit: &str) -> Result<Arc<Registry>> {
        validate_commit(commit)?;
        let directory = self.registry_dir(store);
        let path = directory.join(format!("{commit}.json"));
        let shared = path_mutex(&path)?;
        let _request = shared
            .lock()
            .map_err(|e| Error::Other(format!("registry request lock poisoned: {e}")))?;
        {
            let memory = REGISTRIES
                .lock()
                .map_err(|e| Error::Other(format!("registry cache lock poisoned: {e}")))?;
            if let Some((_, _, value)) = memory
                .iter()
                .find(|(dir, sha, _)| dir == &directory && sha == commit)
            {
                return Ok(Arc::clone(value));
            }
        }
        let cached = {
            let _lock = cache_lock(store)?;
            match read_bounded(&path, MAX_JSON) {
                Ok(bytes) => Some(bytes),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => return Err(e.into()),
            }
        };
        // HTTP never holds the disk lock or blocks readers of a different revision.
        let bytes = match cached {
            Some(bytes) => bytes,
            None => self
                .http
                .get(
                    &format!("{}/{commit}/registry.json", self.endpoints.raw),
                    MAX_JSON,
                )
                .map_err(super::FetchError::into_error)?,
        };
        if bytes.len() > MAX_JSON {
            return Err(Error::InvalidParams(
                "cached registry exceeds size limit".into(),
            ));
        }
        let registry: Registry = serde_json::from_slice(&bytes)?;
        validate(&registry)?;
        {
            let _lock = cache_lock(store)?;
            atomic_write(&path, &bytes)?;
            prune(&directory, commit)?;
        }
        let registry = Arc::new(registry);
        let mut memory = REGISTRIES
            .lock()
            .map_err(|e| Error::Other(format!("registry cache lock poisoned: {e}")))?;
        if memory.len() == 16 {
            memory.pop_front();
        }
        memory.push_back((directory, commit.into(), Arc::clone(&registry)));
        Ok(registry)
    }

    pub(super) fn remember_latest(
        &self,
        store: &Store,
        commit: &str,
        requested_at: u128,
        registry: &Registry,
    ) -> Result<()> {
        let checked_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| Error::Other(e.to_string()))?
            .as_secs();
        let _lock = cache_lock(store)?;
        let directory = self.registry_dir(store);
        let pointer = directory.join("latest.json");
        match read_bounded(&pointer, 1024) {
            Ok(bytes) if serde_json::from_slice::<Latest>(&bytes)?.requested_at > requested_at => {
                return Ok(())
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        let path = directory.join(format!("{commit}.json"));
        if !path.try_exists()? {
            atomic_write(&path, &serde_json::to_vec(registry)?)?;
            prune(&directory, commit)?;
        }
        atomic_write(
            &pointer,
            &serde_json::to_vec(&Latest {
                commit: commit.into(),
                checked_at,
                requested_at,
            })?,
        )
    }

    /// Read the last successful catalogue without HTTP, recomputing local availability.
    pub fn cached(&self, store: &Store) -> Result<Option<CachedCommunitySnapshot>> {
        let latest;
        let registry;
        {
            let _lock = cache_lock(store)?;
            let dir = self.registry_dir(store);
            let bytes = match read_bounded(&dir.join("latest.json"), 1024) {
                Ok(bytes) => bytes,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(e.into()),
            };
            latest = serde_json::from_slice::<Latest>(&bytes)?;
            validate_commit(&latest.commit)?;
            let bytes = read_bounded(&dir.join(format!("{}.json", latest.commit)), MAX_JSON)?;
            if bytes.len() > MAX_JSON {
                return Err(Error::InvalidParams(
                    "cached registry exceeds size limit".into(),
                ));
            }
            registry = serde_json::from_slice::<Registry>(&bytes)?;
            validate(&registry)?;
        }
        Ok(Some(CachedCommunitySnapshot {
            snapshot: Self::snapshot(store, latest.commit, &registry)?,
            checked_at: latest.checked_at,
        }))
    }
}

fn prune(directory: &Path, current: &str) -> Result<()> {
    let latest = match read_bounded(&directory.join("latest.json"), 1024) {
        Ok(bytes) => Some(serde_json::from_slice::<Latest>(&bytes)?.commit),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    let mut entries = vec![];
    for item in fs::read_dir(directory)? {
        let item = item?;
        let path = item.path();
        let Some(stem) = path.file_stem().and_then(|name| name.to_str()) else {
            continue;
        };
        if validate_commit(stem).is_ok() && path.extension().is_some_and(|ext| ext == "json") {
            entries.push((
                stem == current || latest.as_deref() == Some(stem),
                item.metadata()?.modified()?,
                path,
            ));
        }
    }
    entries.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)).then(a.2.cmp(&b.2)));
    for (_, _, path) in entries.into_iter().skip(16) {
        fs::remove_file(path)?;
    }
    Ok(())
}
