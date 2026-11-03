//! Stable-release notes selected for the About dialog and retained across runs.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::common::error::{Error, Result};
use crate::common::utils::{atomic_write, parse_json, read_bounded};
use crate::update::github::Release;
use crate::update::version;

const CACHE_SCHEMA: u32 = 1;
const CACHE_LIMIT: usize = 8 * 1024 * 1024;
const RECENT_LIMIT: usize = 5;

/// One stable release body included in an aggregated notes range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseNote {
    /// Normalized version without the leading `v`.
    pub version: String,
    /// Complete bilingual Markdown body published with the release.
    pub body: String,
}

/// Bilingual release bodies covering an installed version to a target version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateNotes {
    /// Installed version from which the retained range begins.
    pub from: String,
    /// Stable target version shown in the dialog title.
    pub to: String,
    /// Releases ordered newest-first.
    pub releases: Vec<ReleaseNote>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NotesCache {
    schema_version: u32,
    #[serde(default)]
    last_range: Option<UpdateNotes>,
    #[serde(default)]
    recent: Option<UpdateNotes>,
}

impl NotesCache {
    fn validate(self) -> Result<Self> {
        if self.schema_version != CACHE_SCHEMA {
            return Err(Error::InvalidParams(format!(
                "unsupported update-notes cache schema {}",
                self.schema_version
            )));
        }
        Ok(self)
    }
}

/// Read the retained note bundles, treating a missing file as an empty cache.
pub(crate) fn load(path: &Path) -> Result<NotesCache> {
    let bytes = match read_bounded(path, CACHE_LIMIT) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(NotesCache {
                schema_version: CACHE_SCHEMA,
                ..NotesCache::default()
            });
        }
        Err(error) => return Err(error.into()),
    };
    parse_json::<NotesCache>(&bytes)?.validate()
}

/// Atomically persist the retained note bundles.
pub(crate) fn save(path: &Path, cache: &NotesCache) -> Result<()> {
    let bytes = serde_json::to_vec(cache)?;
    if bytes.len() > CACHE_LIMIT {
        return Err(Error::InvalidParams(format!(
            "update-notes cache exceeds {CACHE_LIMIT} bytes"
        )));
    }
    atomic_write(path, &bytes)
}

/// Notes to show before the first network check completes.
pub(crate) fn initial(cache: &NotesCache) -> Option<UpdateNotes> {
    cache.last_range.clone().or_else(|| cache.recent.clone())
}

/// Refresh retained bundles and choose the one matching the running build.
///
/// A newer target always replaces `last_range`. Once installed, that bundle is
/// reused because its target equals the running version. Without a matching
/// retained range, the newest five stable releases are shown.
pub(crate) fn select(
    cache: &mut NotesCache,
    current: Option<&str>,
    releases: &[Release],
) -> Result<Option<UpdateNotes>> {
    let Some(latest) = releases.first() else {
        return Ok(None);
    };
    let latest = version::normalised(&latest.tag_name);
    let recent_versions = releases.iter().take(RECENT_LIMIT).collect::<Vec<_>>();
    let recent_from = recent_versions
        .last()
        .map(|release| version::normalised(&release.tag_name))
        .ok_or_else(|| Error::Other("update notes have no releases".into()))?;
    cache.recent = non_empty(bundle(
        recent_from,
        latest.clone(),
        recent_versions.into_iter(),
    ));

    let current = current.map(version::normalised);
    let current_version = current
        .as_deref()
        .and_then(|current| semver::Version::parse(current).ok());
    let latest_version = semver::Version::parse(&latest).map_err(|error| {
        Error::Other(format!(
            "update feed: invalid release tag {latest:?}: {error}"
        ))
    })?;
    if let (Some(current_text), Some(current_version)) =
        (current.as_ref(), current_version.as_ref())
    {
        if current_version.cmp_precedence(&latest_version).is_lt() {
            let in_range = releases.iter().filter(|release| {
                let release = version::normalised(&release.tag_name);
                semver::Version::parse(&release).is_ok_and(|release| {
                    release.cmp_precedence(&latest_version).is_le()
                        && release.cmp_precedence(current_version).is_gt()
                })
            });
            cache.last_range = non_empty(bundle(current_text.clone(), latest.clone(), in_range));
            return Ok(cache.last_range.clone());
        }
    }

    if cache
        .last_range
        .as_ref()
        .zip(current.as_deref())
        .is_some_and(|(notes, current)| notes.to == current)
    {
        return Ok(cache.last_range.clone());
    }
    Ok(cache.recent.clone())
}

/// Notes for one exact installed-to-target range, regardless of newer releases
/// already staged or present in the feed.
pub(crate) fn range(current: &str, target: &str, releases: &[Release]) -> Option<UpdateNotes> {
    let current = version::normalised(current);
    let target = semver::Version::parse(&version::normalised(target)).ok()?;
    let current = semver::Version::parse(&current).ok()?;
    let releases = releases.iter().filter(|release| {
        semver::Version::parse(&version::normalised(&release.tag_name)).is_ok_and(|release| {
            release.cmp_precedence(&target).is_le() && release.cmp_precedence(&current).is_gt()
        })
    });
    non_empty(bundle(
        semver::Version::to_string(&current),
        semver::Version::to_string(&target),
        releases,
    ))
}

fn bundle<'a>(
    from: String,
    to: String,
    releases: impl IntoIterator<Item = &'a Release>,
) -> UpdateNotes {
    UpdateNotes {
        from,
        to,
        releases: releases
            .into_iter()
            .filter(|release| !release.body.trim().is_empty())
            .map(|release| ReleaseNote {
                version: version::normalised(&release.tag_name),
                body: release.body.trim().to_string(),
            })
            .collect(),
    }
}

fn non_empty(notes: UpdateNotes) -> Option<UpdateNotes> {
    (!notes.releases.is_empty()).then_some(notes)
}
