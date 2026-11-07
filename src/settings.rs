use std::fmt;
use std::fs;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer, Serialize};
use toml_edit::{value, DocumentMut};
use ts_rs::TS;

use crate::common::error::{Error, Result};
use crate::common::utils::atomic_write_private;

pub const SETTINGS_FILE: &str = "settings.toml";
pub const DEFAULT_SCREENSHOT_RETENTION: usize = 30;
pub const DEFAULT_TRASH_RETENTION: usize = 20;
const MIN_GITHUB_TOKEN_LENGTH: usize = 20;
const MAX_GITHUB_TOKEN_LENGTH: usize = 512;

/// A GitHub personal access token that never renders its raw value in debug output.
#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct GithubToken(String);

impl GithubToken {
    /// Validate and normalize a pasted token without retaining surrounding whitespace.
    pub fn parse(raw: impl Into<String>) -> Result<Self> {
        let token = raw.into().trim().to_string();
        if !(MIN_GITHUB_TOKEN_LENGTH..=MAX_GITHUB_TOKEN_LENGTH).contains(&token.len())
            || !token.bytes().all(|byte| byte.is_ascii_graphic())
        {
            return Err(Error::InvalidParams("invalid GitHub token".to_string()));
        }
        Ok(Self(token))
    }

    /// Render only the stable prefix and suffix needed to identify a configured token.
    pub fn masked(&self) -> String {
        format!("ghp_…{}", &self.0[self.0.len() - 4..])
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for GithubToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("GithubToken([REDACTED])")
    }
}

impl<'de> Deserialize<'de> for GithubToken {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::parse(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// Main-window navigation destinations persisted between launches.
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum MainPage {
    /// Installed desktop widgets.
    LocalWidgets,
    /// Community widget directory.
    CommunityWidgets,
    /// Agent MCP and skill integrations.
    #[default]
    Integrations,
    /// General preferences.
    General,
    /// Application information.
    About,
}

impl MainPage {
    fn as_str(self) -> &'static str {
        match self {
            MainPage::LocalWidgets => "local-widgets",
            MainPage::CommunityWidgets => "community-widgets",
            MainPage::Integrations => "integrations",
            MainPage::General => "general",
            MainPage::About => "about",
        }
    }
}

/// Selected UI color theme.
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    /// Follow the operating system color scheme.
    #[default]
    System,
    /// Always use the light color scheme.
    Light,
    /// Always use the dark color scheme.
    Dark,
}

impl Theme {
    fn as_str(self) -> &'static str {
        match self {
            Theme::System => "system",
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }
}

/// Selected UI language.
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq, TS)]
pub enum Language {
    /// US English UI.
    #[default]
    #[serde(rename = "en-US")]
    EnUs,
    /// Simplified Chinese UI.
    #[serde(rename = "zh-CN")]
    ZhCn,
}

impl Language {
    fn as_str(self) -> &'static str {
        match self {
            Language::EnUs => "en-US",
            Language::ZhCn => "zh-CN",
        }
    }
}

/// The action performed by a primary click on the tray icon.
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TrayClickAction {
    /// Open the tray menu, which remains available on right click.
    Menu,
    /// Create, reveal, and focus the main window.
    #[default]
    Window,
}

impl TrayClickAction {
    /// Linux tray backends always own the primary-click menu, so the effective
    /// action there is the menu even if a preference was written elsewhere.
    pub fn effective(self) -> Self {
        if cfg!(target_os = "linux") {
            Self::Menu
        } else {
            self
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            TrayClickAction::Menu => "menu",
            TrayClickAction::Window => "window",
        }
    }
}

/// Preferences edited by the main window and persisted in `settings.toml`.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UiPreferences {
    /// Main-window color theme.
    pub theme: Theme,
    /// Main-window language.
    pub language: Language,
    /// Primary-click behavior for the tray icon.
    pub tray_click_action: TrayClickAction,
    /// Most recently opened main-window destination.
    pub last_page: MainPage,
    /// Whether a newer release is downloaded before the user asks.
    #[serde(default = "auto_download_default")]
    pub auto_download: bool,
    /// Display new widgets try first; `None` keeps automatic placement.
    #[serde(default)]
    pub default_display: Option<String>,
}

impl Default for UiPreferences {
    fn default() -> Self {
        UiPreferences {
            theme: Theme::default(),
            language: Language::default(),
            tray_click_action: TrayClickAction::default(),
            last_page: MainPage::default(),
            auto_download: auto_download_default(),
            default_display: None,
        }
    }
}

/// User-editable settings stored at the root of the aidget data directory.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    /// Maximum number of managed PNG files under `state/screenshots/`.
    pub screenshot_retention: NonZeroUsize,
    /// Maximum number of widget directories under `trash/`.
    pub trash_retention: NonZeroUsize,
    /// Main-window color theme.
    #[serde(default)]
    pub theme: Theme,
    /// Main-window language, selected on the first main-window launch.
    #[serde(default)]
    pub language: Option<Language>,
    /// Primary-click behavior for the tray icon.
    #[serde(default)]
    pub tray_click_action: TrayClickAction,
    /// Most recently opened main-window destination.
    #[serde(default)]
    pub last_page: MainPage,
    /// Optional GitHub personal access token used only for GitHub API requests.
    #[serde(default)]
    pub github_token: Option<GithubToken>,
    /// Whether a newer release is downloaded before the user asks.
    #[serde(default = "auto_download_default")]
    pub auto_download: bool,
    /// Display new widgets try first; `None` keeps automatic placement.
    #[serde(default)]
    pub default_display: Option<String>,
}

/// Download updates automatically unless disabled on the About page.
fn auto_download_default() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            screenshot_retention: NonZeroUsize::new(DEFAULT_SCREENSHOT_RETENTION)
                .expect("default screenshot retention is non-zero"),
            trash_retention: NonZeroUsize::new(DEFAULT_TRASH_RETENTION)
                .expect("default trash retention is non-zero"),
            theme: Theme::default(),
            language: None,
            tray_click_action: TrayClickAction::default(),
            last_page: MainPage::default(),
            github_token: None,
            auto_download: auto_download_default(),
            default_display: None,
        }
    }
}

impl Settings {
    /// Return the preference subset exposed to the main window.
    pub fn preferences(&self) -> Result<UiPreferences> {
        Ok(UiPreferences {
            theme: self.theme,
            language: self.language.ok_or_else(|| {
                Error::Other("main-window language is not initialized".to_string())
            })?,
            tray_click_action: self.tray_click_action,
            last_page: self.last_page,
            auto_download: self.auto_download,
            default_display: self.default_display.clone(),
        })
    }
}

/// Create `settings.toml` when absent and add missing UI preference defaults
/// without replacing retention values or comments already in the document.
pub fn ensure(root: &Path) -> Result<()> {
    let path = settings_path(root);
    if !path.exists() {
        atomic_write_private(&path, default_document().as_bytes())?;
        return Ok(());
    }

    load(root)?;
    let text = fs::read_to_string(&path)?;
    let mut document = text
        .parse::<DocumentMut>()
        .map_err(|error| Error::Other(format!("toml: {error}")))?;
    insert_missing(&mut document, "theme", Theme::System.as_str());
    insert_missing(
        &mut document,
        "tray_click_action",
        TrayClickAction::Window.as_str(),
    );
    insert_missing(&mut document, "last_page", MainPage::Integrations.as_str());
    insert_missing(&mut document, "auto_download", auto_download_default());
    let next = document.to_string();
    if next != text {
        atomic_write_private(&path, next.as_bytes())?;
    }
    Ok(())
}

/// Read and strictly validate `settings.toml`.
pub fn load(root: &Path) -> Result<Settings> {
    let bytes = fs::read(settings_path(root))?;
    let text =
        std::str::from_utf8(&bytes).map_err(|error| Error::Other(format!("toml: {error}")))?;
    let mut document = text
        .parse::<DocumentMut>()
        .map_err(|error| Error::Other(format!("toml: {error}")))?;
    let mut replaced_invalid_language = false;
    if let Some(item) = document.get("language") {
        match item.as_str() {
            Some("zh-CN" | "en-US") => {}
            Some("en") => {
                document["language"] = value(Language::EnUs.as_str());
            }
            Some("system") => {
                document.remove("language");
            }
            _ => {
                document["language"] = value(Language::EnUs.as_str());
                replaced_invalid_language = true;
            }
        }
    }

    let normalized = document.to_string();
    let settings =
        toml::from_str(&normalized).map_err(|error| Error::Other(format!("toml: {error}")))?;
    if normalized != text {
        atomic_write_private(&settings_path(root), normalized.as_bytes())?;
        if replaced_invalid_language {
            eprintln!("aidget: invalid language setting; using en-US");
        }
    }
    Ok(settings)
}

/// Update only the UI preferences while preserving retention settings
/// and the rest of the TOML document.
pub fn save_preferences(root: &Path, preferences: UiPreferences) -> Result<()> {
    load(root)?;
    let path = settings_path(root);
    let text = fs::read_to_string(&path)?;
    let mut document = text
        .parse::<DocumentMut>()
        .map_err(|error| Error::Other(format!("toml: {error}")))?;
    document["theme"] = value(preferences.theme.as_str());
    document["language"] = value(preferences.language.as_str());
    document["tray_click_action"] = value(preferences.tray_click_action.as_str());
    document["last_page"] = value(preferences.last_page.as_str());
    document["auto_download"] = value(preferences.auto_download);
    match preferences.default_display {
        Some(display) => document["default_display"] = value(display),
        None => {
            document.remove("default_display");
        }
    }
    atomic_write_private(&path, document.to_string().as_bytes())
}

/// Save the system-derived language only when no explicit choice exists.
pub fn initialize_language(root: &Path, language: Language) -> Result<Language> {
    let settings = load(root)?;
    if let Some(current) = settings.language {
        return Ok(current);
    }

    let path = settings_path(root);
    let text = fs::read_to_string(&path)?;
    let mut document = text
        .parse::<DocumentMut>()
        .map_err(|error| Error::Other(format!("toml: {error}")))?;
    document["language"] = value(language.as_str());
    atomic_write_private(&path, document.to_string().as_bytes())?;
    Ok(language)
}

/// Save or remove the optional GitHub token without changing other settings.
pub fn save_github_token(root: &Path, token: Option<&GithubToken>) -> Result<()> {
    let path = settings_path(root);
    let text = fs::read_to_string(&path)?;
    let mut document = text
        .parse::<DocumentMut>()
        .map_err(|error| Error::Other(format!("toml: {error}")))?;
    match token {
        Some(token) => document["github_token"] = value(token.as_str()),
        None => {
            document.remove("github_token");
        }
    }
    atomic_write_private(&path, document.to_string().as_bytes())
}

fn settings_path(root: &Path) -> PathBuf {
    root.join(SETTINGS_FILE)
}

fn insert_missing(document: &mut DocumentMut, key: &str, value: impl Into<toml_edit::Value>) {
    if !document.contains_key(key) {
        document[key] = toml_edit::value(value);
    }
}

fn default_document() -> String {
    format!(
        "# Maximum number of screenshots kept in state/screenshots.\n\
screenshot_retention = {DEFAULT_SCREENSHOT_RETENTION}\n\
\n\
# Maximum number of deleted widgets kept in trash.\n\
trash_retention = {DEFAULT_TRASH_RETENTION}\n\
\n\
# Main-window color theme: system, light, or dark.\n\
theme = \"system\"\n\
\n\
# Primary tray click: window or menu.\n\
tray_click_action = \"window\"\n\
\n\
# Main-window destination restored on launch.\n\
last_page = \"integrations\"\n\
\n\
# Download a newer release as soon as it is found: true or false.\n\
auto_download = true\n\
\n\
# Optional GitHub personal access token for authenticated API requests.\n\
# This value is stored in plain text if configured.\n"
    )
}
