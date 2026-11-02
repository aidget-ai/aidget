//! Native distribution operations. The UI never supplies trusted manifests or URLs.
use serde::Deserialize;
use serde_json::Value;

use super::{
    archive,
    http::{FetchError, Http},
    model::*,
    snapshot,
};
use crate::{
    common::error::{Error, Result},
    settings::{self, GithubToken},
    widgets::{
        model::WidgetMeta,
        package::{validate_package_name, WidgetPlatform},
        store::Store,
    },
};
mod downloads;
mod media;
mod registry;

const MAX_JSON: usize = 8 * 1024 * 1024;
/// Trusted endpoint roots. Production always uses the official public sources.
#[derive(Clone)]
pub struct Endpoints {
    pub github: String,
    pub raw: String,
    pub npm: String,
    pub downloads: String,
}
impl Default for Endpoints {
    fn default() -> Self {
        Self {
            github: "https://api.github.com/repos/aidget-ai/widgets".into(),
            raw: "https://raw.githubusercontent.com/aidget-ai/widgets".into(),
            npm: "https://registry.npmjs.org".into(),
            downloads: "https://api.npmjs.org".into(),
        }
    }
}
/// A short-lived native client; construct and use on a blocking worker thread.
pub struct Community {
    http: Http,
    endpoints: Endpoints,
    github_token: Option<GithubToken>,
}
#[derive(Deserialize)]
struct Branch {
    commit: Commit,
}
#[derive(Deserialize)]
struct Commit {
    sha: String,
}
impl Community {
    /// Official community and npm endpoints.
    pub fn new() -> Result<Self> {
        Self::with_endpoints(Endpoints::default())
    }
    /// Official endpoints with the token configured in `settings.toml`.
    pub fn for_store(root: &std::path::Path) -> Result<Self> {
        let token = settings::load(root)?.github_token;
        Self::with_endpoints_and_token(Endpoints::default(), token)
    }
    /// Explicit endpoint injection for bounded local HTTP integration tests.
    pub fn with_endpoints(endpoints: Endpoints) -> Result<Self> {
        Self::with_endpoints_and_token(endpoints, None)
    }
    /// Explicit endpoint and credential injection for bounded HTTP integration tests.
    pub fn with_endpoints_and_token(
        endpoints: Endpoints,
        github_token: Option<GithubToken>,
    ) -> Result<Self> {
        Ok(Self {
            http: Http::new()?,
            endpoints,
            github_token,
        })
    }
    fn json<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T> {
        Ok(serde_json::from_slice(
            &self.http.get(url, MAX_JSON).map_err(|e| {
                if e.missing {
                    Error::NotFound(e.message)
                } else {
                    e.into_error()
                }
            })?,
        )?)
    }
    fn branch(&self, branch: &str) -> Result<String> {
        let value: Branch =
            self.github_json(&format!("{}/branches/{branch}", self.endpoints.github))?;
        snapshot::validate_commit(&value.commit.sha)?;
        Ok(value.commit.sha)
    }
    fn github_json<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T> {
        Ok(serde_json::from_slice(
            &self
                .http
                .get_github(
                    url,
                    MAX_JSON,
                    self.github_token.as_ref().map(GithubToken::as_str),
                )
                .map_err(|e| {
                    if e.missing {
                        Error::NotFound(e.message)
                    } else {
                        e.into_error()
                    }
                })?,
        )?)
    }
    /// Verify that the configured GitHub credential can read the reviewed branch.
    pub fn verify_github_access(&self) -> Result<()> {
        self.branch("main")?;
        Ok(())
    }
    fn npm_metadata(&self, name: &str, version: &str) -> std::result::Result<Value, FetchError> {
        let bytes = self.http.get(
            &format!("{}/{}/{version}", self.endpoints.npm, encoded_package(name)),
            MAX_JSON,
        )?;
        serde_json::from_slice(&bytes).map_err(|e| FetchError {
            unavailable: false,
            missing: false,
            message: format!("invalid npm metadata: {e}"),
        })
    }
    /// Discover a reviewed main snapshot that is immediately installable from Git.
    pub fn list(&self, store: &Store) -> Result<CommunitySnapshot> {
        let requested_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| Error::Other(e.to_string()))?
            .as_nanos();
        let commit = self.branch("main")?;
        let registry = self.registry(store, &commit)?;
        let snapshot = Self::snapshot(store, commit.clone(), &registry)?;
        self.remember_latest(store, &commit, requested_at, &registry)?;
        Ok(snapshot)
    }
    fn snapshot(store: &Store, commit: String, registry: &Registry) -> Result<CommunitySnapshot> {
        let ids = store.widget_ids()?;
        let mut names = std::collections::BTreeSet::new();
        for valid in store.list()?.widgets {
            if let Some(package) = store.load_package(&valid.id)? {
                names.insert(package.name);
            }
        }
        let widgets = registry
            .widgets
            .iter()
            .cloned()
            .map(|entry| CommunityEntry {
                installed: names.contains(&entry.package.name),
                id_conflict: ids.contains(&entry.config.id),
                supported: entry.config.platforms.contains(&WidgetPlatform::current()),
                entry,
            })
            .collect();
        Ok(CommunitySnapshot { commit, widgets })
    }
    /// Install the snapshot selected by the main window; never trust caller metadata.
    pub fn install_reviewed(&self, store: &Store, commit: &str, name: &str) -> Result<WidgetMeta> {
        let registry = self.registry(store, commit)?;
        let entry = registry
            .widgets
            .iter()
            .find(|e| e.package.name == name)
            .ok_or_else(|| Error::NotFound(name.into()))?;
        self.install_version(store, name, &entry.package.version, Some((commit, entry)))
    }
    /// Install a full npm name with an optional exact version. Community defaults use main.
    pub fn install(&self, store: &Store, spec: &str) -> Result<WidgetMeta> {
        let (name, version) = split_spec(spec)?;
        if name.starts_with("@aidget-community/") {
            let main = self.branch("main")?;
            let registry = self.registry(store, &main)?;
            if let Some(entry) = registry.widgets.iter().find(|e| {
                e.package.name == name && version.as_ref().is_none_or(|v| v == &e.package.version)
            }) {
                return self.install_version(
                    store,
                    &name,
                    &entry.package.version,
                    Some((&main, entry)),
                );
            }
            return Err(Error::NotFound(format!(
                "{spec} does not match the current reviewed community snapshot"
            )));
        }
        let version = match version {
            Some(version) => version,
            None => {
                let value = self
                    .npm_metadata(&name, "latest")
                    .map_err(FetchError::into_error)?;
                value["version"]
                    .as_str()
                    .ok_or_else(|| Error::InvalidParams("npm latest has no version".into()))?
                    .to_owned()
            }
        };
        semver::Version::parse(&version).map_err(|e| Error::InvalidParams(e.to_string()))?;
        self.install_version(store, &name, &version, None)
    }
    fn npm_tarball(
        &self,
        name: &str,
        version: &str,
    ) -> std::result::Result<(Vec<u8>, String), FetchError> {
        let value = self.npm_metadata(name, version)?;
        validate_npm_identity(&value, name, version).map_err(|e| FetchError {
            unavailable: false,
            missing: false,
            message: e.to_string(),
        })?;
        let Some(url) = value["dist"]["tarball"].as_str() else {
            return Err(FetchError {
                unavailable: false,
                missing: false,
                message: "npm metadata has no tarball".into(),
            });
        };
        let parsed = reqwest::Url::parse(url).map_err(|e| FetchError {
            unavailable: false,
            missing: false,
            message: e.to_string(),
        })?;
        if parsed.scheme() != "https"
            && !(self.endpoints.npm.starts_with("http://127.0.0.1:")
                && url.starts_with(&self.endpoints.npm))
        {
            return Err(FetchError {
                unavailable: false,
                missing: false,
                message: "npm tarball must use HTTPS".into(),
            });
        }
        let integrity = value["dist"]["integrity"]
            .as_str()
            .ok_or_else(|| FetchError {
                unavailable: false,
                missing: false,
                message: "npm metadata has no integrity".into(),
            })?
            .to_owned();
        Ok((self.http.get(url, archive::MAX_ARCHIVE)?, integrity))
    }
    fn install_version(
        &self,
        store: &Store,
        name: &str,
        version: &str,
        expected: Option<(&str, &RegistryEntry)>,
    ) -> Result<WidgetMeta> {
        let staged = tempfile::tempdir_in(store.state_dir())?;
        let (config, package) = match self.npm_tarball(name, version) {
            Ok((bytes, integrity)) => {
                archive::verify_integrity(&bytes, &integrity)?;
                archive::extract(&bytes, staged.path(), expected.map(|(_, entry)| entry))?
            }
            // Only transport failures permit switching channels, and only when a
            // reviewed manifest pins the same name/version to an exact Git commit.
            Err(error) if error.unavailable && expected.is_some() => {
                let (commit, entry) = expected.unwrap();
                snapshot::download(
                    &self.http,
                    &self.endpoints.raw,
                    commit,
                    entry,
                    staged.path(),
                )?
            }
            Err(error) => return Err(error.into_error()),
        };
        if package.name != name || package.version != version {
            return Err(Error::InvalidParams(
                "tarball identity differs from requested package".into(),
            ));
        }
        if !config.platforms.contains(&WidgetPlatform::current()) {
            return Err(Error::InvalidParams(
                "widget does not support this operating system".into(),
            ));
        }
        let id = config.id.clone();
        crate::widgets::install::commit(store, staged.path(), config, &package, &id)
    }
}
/// Encode an entire npm name, retaining the scope/name distinction in one path segment.
pub fn encoded_package(name: &str) -> String {
    percent_encoding::utf8_percent_encode(name, percent_encoding::NON_ALPHANUMERIC).to_string()
}
fn validate_npm_identity(value: &Value, name: &str, version: &str) -> Result<()> {
    if value["name"] != name || value["version"] != version {
        return Err(Error::InvalidParams(
            "npm metadata identity mismatch".into(),
        ));
    }
    Ok(())
}
/// Parse only an exact version; ranges and tags must not change a confirmed installation.
pub fn split_spec(spec: &str) -> Result<(String, Option<String>)> {
    let (name, version) = match spec.rsplit_once('@') {
        Some((name, version)) if !name.is_empty() => (name, Some(version)),
        _ => (spec, None),
    };
    validate_package_name(name)?;
    if let Some(version) = version {
        semver::Version::parse(version)
            .map_err(|e| Error::InvalidParams(format!("expected exact package version: {e}")))?;
    }
    Ok((name.into(), version.map(str::to_string)))
}
