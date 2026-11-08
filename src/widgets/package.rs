//! Portable widget files, shared by local widgets and reviewed distribution packages.
use super::model::{validate_id, WidgetDragMode, WidgetPermission};
use crate::common::error::{Error, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::{Component, Path};
use ts_rs::TS;

/// A platform on which the widget has been checked by its author.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "lowercase")]
pub enum WidgetPlatform {
    Macos,
    Windows,
    Linux,
}
impl WidgetPlatform {
    /// The platform running this process.
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Macos
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Linux
        }
    }
}
/// Initial logical size; the board remains authoritative after installation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct WidgetSize {
    pub width: f64,
    pub height: f64,
}
impl Default for WidgetSize {
    fn default() -> Self {
        Self {
            width: 360.0,
            height: 220.0,
        }
    }
}
/// Portable aidget configuration. Unused fields are ignored; installed revisions
/// are stored by `WidgetMeta`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct WidgetConfig {
    pub id: String,
    pub title: String,
    /// Minimum Aidget release required by this widget.
    #[serde(default)]
    pub min_host_version: Option<String>,
    pub logo: Option<String>,
    pub screenshots: Vec<String>,
    pub platforms: Vec<WidgetPlatform>,
    pub size: WidgetSize,
    pub drag_mode: WidgetDragMode,
    pub permissions: Vec<WidgetPermission>,
    pub network: Vec<String>,
}
impl WidgetConfig {
    /// A local widget starts with no grants and no published-package identity.
    pub fn new(id: &str, title: &str) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            min_host_version: Some(env!("CARGO_PKG_VERSION").to_string()),
            logo: None,
            screenshots: vec![],
            platforms: vec![WidgetPlatform::current()],
            size: WidgetSize::default(),
            drag_mode: WidgetDragMode::All,
            permissions: vec![],
            network: vec![],
        }
    }
    /// Reject malformed metadata, sizes outside 160×120–8192×8192 and invalid origins.
    pub fn validate(&self) -> Result<()> {
        validate_id(&self.id)?;
        if let Some(version) = &self.min_host_version {
            validate_host_version(version)?;
        }
        if self.title.trim().is_empty()
            || self.platforms.is_empty()
            || !self.size.width.is_finite()
            || !self.size.height.is_finite()
            || self.size.width < 160.0
            || self.size.height < 120.0
            || self.size.width > 8192.0
            || self.size.height > 8192.0
        {
            return Err(Error::InvalidParams(
                "widget requires a title, platforms and size between 160×120 and 8192×8192".into(),
            ));
        }
        for path in self.logo.iter().chain(self.screenshots.iter()) {
            validate_path(path)?;
        }
        for origin in &self.network {
            let url = reqwest::Url::parse(origin)
                .map_err(|e| Error::InvalidParams(format!("invalid network origin: {e}")))?;
            if url.scheme() != "https"
                || url.host_str().is_none()
                || url.host_str().is_some_and(|h| h.contains('*'))
                || !url.username().is_empty()
                || url.password().is_some()
                || url.path() != "/"
                || url.query().is_some()
                || url.fragment().is_some()
                || url.origin().ascii_serialization() != *origin
            {
                return Err(Error::InvalidParams(format!(
                    "network entry must be an exact HTTPS origin: {origin}"
                )));
            }
        }
        Ok(())
    }
}

/// Parse a concrete host version without allowing npm tags, ranges, or a `v` prefix.
pub fn validate_host_version(version: &str) -> Result<semver::Version> {
    if version.starts_with('v') {
        return Err(Error::InvalidParams(
            "minHostVersion must be a concrete SemVer version without a v prefix".into(),
        ));
    }
    semver::Version::parse(version)
        .map_err(|error| Error::InvalidParams(format!("invalid minHostVersion: {error}")))
}

/// Runtime compatibility of a widget with the current Aidget release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum HostCompatibilityState {
    Compatible,
    HostTooOld,
    Undeclared,
}

/// Version requirement details shared by installation, runtime, CLI and MCP.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct HostCompatibility {
    pub state: HostCompatibilityState,
    pub current_version: String,
    pub min_host_version: Option<String>,
    /// True only when a debug build explicitly bypassed an incompatible version.
    pub allowed_by_override: bool,
}

impl HostCompatibility {
    pub fn evaluate(
        config: &WidgetConfig,
        current: &str,
        allow_incompatible: bool,
    ) -> Result<Self> {
        let current_version = validate_host_version(current)?;
        let Some(min) = &config.min_host_version else {
            return Ok(Self {
                state: HostCompatibilityState::Undeclared,
                current_version: current.to_string(),
                min_host_version: None,
                allowed_by_override: false,
            });
        };
        let minimum = validate_host_version(min)?;
        let too_old = current_version.cmp_precedence(&minimum).is_lt();
        Ok(Self {
            state: if too_old {
                HostCompatibilityState::HostTooOld
            } else {
                HostCompatibilityState::Compatible
            },
            current_version: current.to_string(),
            min_host_version: Some(min.clone()),
            allowed_by_override: too_old && allow_incompatible,
        })
    }

    pub fn permits_current_install(&self) -> bool {
        self.state == HostCompatibilityState::Compatible
    }

    pub fn permits_runtime(&self) -> bool {
        self.state != HostCompatibilityState::HostTooOld || self.allowed_by_override
    }
}
/// npm's author value, retaining its standard string and object forms.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(untagged)]
pub enum PackageAuthor {
    Name(String),
    Person {
        name: String,
        email: Option<String>,
        url: Option<String>,
    },
}
/// npm's repository field supports both standard forms.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(untagged)]
pub enum PackageRepository {
    Url(String),
    Git {
        r#type: String,
        url: String,
        directory: Option<String>,
    },
}
/// npm metadata used by aidget. Additional npm fields are retained on disk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct PackageManifest {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    pub author: Option<PackageAuthor>,
    pub repository: Option<PackageRepository>,
    pub license: Option<String>,
    #[serde(default)]
    pub files: Vec<String>,
}
impl PackageManifest {
    /// Validate npm identity; scope classification never grants runtime permissions.
    pub fn validate(&self) -> Result<()> {
        validate_package_name(&self.name)?;
        semver::Version::parse(&self.version)
            .map_err(|e| Error::InvalidParams(format!("invalid package version: {e}")))?;
        Ok(())
    }
    /// Classify by exact scope, without persisting a mutable source label.
    pub fn is_community(&self) -> bool {
        self.name.starts_with("@aidget-community/")
    }
}
/// Accept npm names, not URLs, tags, paths or shell expressions.
pub fn validate_package_name(name: &str) -> Result<()> {
    let parts: Vec<_> = name.split('/').collect();
    let valid = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-_.".contains(&b))
            && !s.starts_with(['.', '_'])
    };
    let ok = if let [scope, package] = parts.as_slice() {
        scope.starts_with('@') && valid(&scope[1..]) && valid(package)
    } else {
        valid(name)
    };
    if !ok || name.len() > 214 {
        return Err(Error::InvalidParams(format!(
            "invalid npm package name: {name}"
        )));
    }
    Ok(())
}
/// Portable paths must stay inside the widget on every supported operating system.
pub fn validate_path(path: &str) -> Result<()> {
    if path.is_empty()
        || path.contains(['\\', ':', '\0'])
        || path.split('/').any(|p| p.is_empty() || p.starts_with('.'))
        || Path::new(path)
            .components()
            .any(|p| !matches!(p, Component::Normal(_)))
    {
        return Err(Error::InvalidParams(format!("unsafe widget path: {path}")));
    }
    Ok(())
}
