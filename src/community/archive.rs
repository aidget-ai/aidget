//! Validate npm tarballs without invoking npm or executing package scripts.
use super::model::{FileDigest, RegistryEntry};
use crate::{
    common::{
        error::{Error, Result},
        utils::{atomic_write, parse_json},
    },
    widgets::package::{validate_path, PackageManifest, WidgetConfig},
};
use base64::{engine::general_purpose::STANDARD, Engine};
use flate2::read::GzDecoder;
use sha2::{Digest, Sha512};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Cursor, Read},
    path::Path,
};
pub const MAX_ARCHIVE: usize = 20 * 1024 * 1024;
const MAX_EXPANDED: u64 = 100 * 1024 * 1024;
/// Check an exact SHA-512 SRI value before touching the archive contents.
pub fn verify_integrity(bytes: &[u8], integrity: &str) -> Result<()> {
    if bytes.len() > MAX_ARCHIVE
        || integrity != format!("sha512-{}", STANDARD.encode(Sha512::digest(bytes)))
    {
        return Err(Error::InvalidParams(
            "package SHA-512 integrity mismatch or archive too large".into(),
        ));
    }
    Ok(())
}
/// Extract only regular package files; reject traversal, links, duplicates and bombs.
pub fn extract(
    bytes: &[u8],
    directory: &Path,
    expected: Option<&RegistryEntry>,
) -> Result<(WidgetConfig, PackageManifest)> {
    if bytes.len() > MAX_ARCHIVE {
        return Err(Error::InvalidParams("archive exceeds 20 MiB".into()));
    }
    let decoder = GzDecoder::new(Cursor::new(bytes));
    // Bound the uncompressed tar stream too, including headers and non-file entries.
    let mut archive = tar::Archive::new(decoder.take(MAX_EXPANDED + 1));
    let mut total = 0u64;
    let mut seen = BTreeSet::new();
    let mut files = BTreeMap::new();
    for (index, item) in archive.entries()?.enumerate() {
        if index >= 1000 {
            return Err(Error::InvalidParams("archive exceeds 1000 entries".into()));
        }
        let mut item = item?;
        let path = item
            .path()?
            .to_str()
            .ok_or_else(|| Error::InvalidParams("non-UTF8 archive path".into()))?
            .to_owned();
        let path = path
            .strip_prefix("package/")
            .ok_or_else(|| Error::InvalidParams("npm archive must use package/ prefix".into()))?;
        let kind = item.header().entry_type();
        if kind.is_dir() {
            validate_path(path.trim_end_matches('/'))?;
            continue;
        }
        validate_path(path)?;
        if !kind.is_file() || !seen.insert(path.to_lowercase()) {
            return Err(Error::InvalidParams(format!(
                "unsupported or duplicate archive entry: {path}"
            )));
        }
        total = total
            .checked_add(item.size())
            .ok_or_else(|| Error::InvalidParams("archive size overflow".into()))?;
        if total > MAX_EXPANDED {
            return Err(Error::InvalidParams(
                "archive expands beyond 100 MiB".into(),
            ));
        }
        let mut data = Vec::new();
        item.read_to_end(&mut data)?;
        files.insert(
            path.to_string(),
            FileDigest {
                path: path.into(),
                size: data.len() as u64,
                sha256: crate::common::utils::sha256_hex(&data),
            },
        );
        atomic_write(&directory.join(path), &data)?;
    }
    validate_contents(directory, &files, expected)
}

/// Enforce the same runtime and reviewed-content contract for npm and Git snapshots.
pub(super) fn validate_contents(
    directory: &Path,
    files: &BTreeMap<String, FileDigest>,
    expected: Option<&RegistryEntry>,
) -> Result<(WidgetConfig, PackageManifest)> {
    let config: WidgetConfig = parse_json(&fs::read(directory.join("config.json"))?)?;
    let package: PackageManifest = parse_json(&fs::read(directory.join("package.json"))?)?;
    config.validate()?;
    package.validate()?;
    if !directory.join("index.html").is_file() {
        return Err(Error::InvalidParams("package has no index.html".into()));
    }
    for file in config.logo.iter().chain(config.screenshots.iter()) {
        if !files.contains_key(file) {
            return Err(Error::InvalidParams(format!("missing media: {file}")));
        }
    }
    let raw: serde_json::Value = parse_json(&fs::read(directory.join("package.json"))?)?;
    for key in ["dependencies", "optionalDependencies", "peerDependencies"] {
        if raw
            .get(key)
            .and_then(|v| v.as_object())
            .is_some_and(|o| !o.is_empty())
        {
            return Err(Error::InvalidParams("widgets must include their runtime resources; dependency installation is unsupported".into()));
        }
    }
    if let Some(expected) = expected {
        let wanted: BTreeMap<_, _> = expected
            .files
            .iter()
            .map(|file| (file.path.clone(), file.clone()))
            .collect();
        if wanted.len() != expected.files.len()
            || *files != wanted
            || config != expected.config
            || package != expected.package
        {
            return Err(Error::InvalidParams(
                "package differs from the reviewed main snapshot".into(),
            ));
        }
    }
    Ok((config, package))
}
