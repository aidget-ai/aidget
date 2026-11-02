//! Wire formats shared by the repository, native installer and community panel.
use crate::widgets::package::{PackageManifest, WidgetConfig};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One immutable file in the npm package, hashed before packing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct FileDigest {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}
/// A reviewed package and the exact content displayed before installation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct RegistryEntry {
    /// First inclusion in the reviewed community catalogue, in UTC RFC3339.
    #[serde(with = "published_at")]
    #[schemars(with = "String", extend("format" = "date-time"))]
    #[ts(as = "String")]
    pub published_at: String,
    pub package: PackageManifest,
    pub config: WidgetConfig,
    pub files: Vec<FileDigest>,
}
/// Deterministically generated discovery document on main.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
pub struct Registry {
    pub widgets: Vec<RegistryEntry>,
}
/// User-facing installation availability, separate from package metadata.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CommunityEntry {
    pub entry: RegistryEntry,
    pub installed: bool,
    pub id_conflict: bool,
    pub supported: bool,
}
/// Catalogue metadata, media and installation share one reviewed main revision.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CommunitySnapshot {
    pub commit: String,
    pub widgets: Vec<CommunityEntry>,
}
/// Last successfully checked catalogue, with current local installation availability.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CachedCommunitySnapshot {
    pub snapshot: CommunitySnapshot,
    /// Unix seconds of the last successful main-branch check.
    pub checked_at: u64,
}
/// Optional npm telemetry. An unavailable count is distinct from zero.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DownloadCount {
    pub package: String,
    pub weekly: Option<u64>,
    pub error: Option<String>,
}

mod published_at {
    use serde::{Deserialize, Serialize};

    pub(super) fn serialize<S: serde::Serializer>(
        value: &str,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value.serialize(serializer)
    }

    pub(super) fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<String, D::Error> {
        let value = String::deserialize(deserializer)?;
        chrono::DateTime::parse_from_rfc3339(&value)
            .map_err(|error| serde::de::Error::custom(format!("invalid publishedAt: {error}")))?;
        if !value.ends_with('Z') {
            return Err(serde::de::Error::custom("publishedAt must use UTC (Z)"));
        }
        Ok(value)
    }
}
