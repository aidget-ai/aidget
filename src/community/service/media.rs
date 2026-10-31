//! Verified, content-addressed raster media with bounded persistent storage.
use super::{registry::cache_lock, Community, FetchError};
use crate::{
    common::{
        error::{Error, Result},
        utils::{atomic_write, path_mutex, read_bounded, sha256_hex},
    },
    community::{model::FileDigest, snapshot},
    widgets::{package::validate_path, store::Store},
};
use base64::{engine::general_purpose::STANDARD, Engine};
use std::{fs, path::Path, time::SystemTime};

const MAX_MEDIA: usize = 5 * 1024 * 1024;
const CACHE_LIMIT: u64 = 128 * 1024 * 1024;

impl Community {
    /// Return declared raster bytes, verifying both disk hits and downloads against the pinned snapshot.
    /// An explicit refresh bypasses the disk entry without changing the selected revision.
    pub fn media(
        &self,
        store: &Store,
        commit: &str,
        name: &str,
        path: &str,
        refresh: bool,
    ) -> Result<String> {
        let registry = self.registry(store, commit)?;
        let entry = registry
            .widgets
            .iter()
            .find(|entry| entry.package.name == name)
            .ok_or_else(|| Error::NotFound(name.into()))?;
        if !entry
            .config
            .logo
            .iter()
            .chain(entry.config.screenshots.iter())
            .any(|p| p == path)
        {
            return Err(Error::InvalidParams("not a declared media file".into()));
        }
        validate_path(path)?;
        let file = entry
            .files
            .iter()
            .find(|file| file.path == path)
            .ok_or_else(|| {
                Error::InvalidParams("media missing from registry digest list".into())
            })?;
        if file.size > MAX_MEDIA as u64 {
            return Err(Error::InvalidParams("media exceeds size limit".into()));
        }
        let directory = store.state_dir().join("community/media");
        let destination = directory.join(file.sha256.to_ascii_lowercase());
        let shared = path_mutex(&destination)?;
        let _request = shared
            .lock()
            .map_err(|e| Error::Other(format!("media request lock poisoned: {e}")))?;
        {
            let _lock = cache_lock(store)?;
            // Windows can report a child of a regular file as missing; reject the
            // invalid cache directory before treating that error as a cache miss.
            match fs::metadata(&directory) {
                Ok(metadata) if !metadata.is_dir() => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::NotADirectory,
                        format!("media cache is not a directory: {}", directory.display()),
                    )
                    .into());
                }
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
            if !refresh {
                match read_bounded(&destination, MAX_MEDIA) {
                    Ok(bytes) => {
                        let mime = validate(&bytes, file)?;
                        fs::OpenOptions::new()
                            .write(true)
                            .open(&destination)?
                            .set_times(fs::FileTimes::new().set_modified(SystemTime::now()))?;
                        return Ok(format!("data:{mime};base64,{}", STANDARD.encode(bytes)));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e.into()),
                }
            }
        }
        let bytes = self
            .http
            .get(
                &snapshot::file_url(&self.endpoints.raw, commit, &entry.config.id, path)?,
                MAX_MEDIA,
            )
            .map_err(FetchError::into_error)?;
        let mime = validate(&bytes, file)?;
        {
            let _lock = cache_lock(store)?;
            atomic_write(&destination, &bytes)?;
            prune(&directory, CACHE_LIMIT)?;
        }
        Ok(format!("data:{mime};base64,{}", STANDARD.encode(bytes)))
    }
}

fn validate(bytes: &[u8], file: &FileDigest) -> Result<&'static str> {
    if bytes.len() > MAX_MEDIA
        || bytes.len() as u64 != file.size
        || sha256_hex(bytes) != file.sha256.to_ascii_lowercase()
    {
        return Err(Error::InvalidParams(
            "media differs from reviewed snapshot".into(),
        ));
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Ok("image/png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Ok("image/jpeg")
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Ok("image/webp")
    } else {
        Err(Error::InvalidParams(
            "media must be PNG, JPEG or WebP".into(),
        ))
    }
}

fn prune(directory: &Path, limit: u64) -> Result<()> {
    let mut entries = vec![];
    let mut size = 0;
    for item in fs::read_dir(directory)? {
        let item = item?;
        let metadata = item.metadata()?;
        if metadata.is_file() {
            size += metadata.len();
            entries.push((metadata.modified()?, item.path(), metadata.len()));
        }
    }
    entries.sort();
    for (_, path, length) in entries {
        if size <= limit {
            break;
        }
        fs::remove_file(path)?;
        size -= length;
    }
    Ok(())
}
