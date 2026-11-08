//! Main-page payloads, settings, widget mutations, and cached previews.
use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};

use crate::common::error::{Error, Result};
use crate::runtime::main_window_state;
use crate::settings::{
    self, GithubToken, Language, MainPage, Theme, TrayClickAction, UiPreferences,
};
use crate::widgets::model::{validate_id, DeletedWidgetSummary, DisplayInfo, WidgetFrame};
use crate::widgets::store::{Store, WidgetEdit};

use super::capture::{capture_widget, screenshot_mode, MAIN_PREVIEW_SCALE};
use super::content::{content_token, signature};
use super::geometry::displays;
use super::state::UiState;
use super::widget_windows::sync_widgets;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
/// Metadata needed to render one static preview card in the main window.
pub struct WidgetSummary {
    pub id: String,
    pub title: String,
    /// Current npm identity, used only to reconcile the community install state.
    pub package_name: Option<String>,
    pub revision: u64,
    pub token: String,
    pub width: f64,
    pub height: f64,
    pub enabled: bool,
    pub created_at: u64,
}

/// Installed widget metadata and its optional static preview for the local details dialog.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct LocalWidgetDetails {
    pub config: crate::widgets::package::WidgetConfig,
    pub compatibility: crate::widgets::package::HostCompatibility,
    pub description: Option<String>,
    pub preview: Option<String>,
}

/// Read installed widget details, including disabled widgets, without opening a WebView.
pub fn local_widget_details(
    store: &Store,
    id: &str,
    allow_incompatible: bool,
) -> Result<LocalWidgetDetails> {
    let config = store.load_meta(id)?.config;
    let compatibility = crate::widgets::package::HostCompatibility::evaluate(
        &config,
        env!("CARGO_PKG_VERSION"),
        allow_incompatible,
    )?;
    let description = store.load_package(id)?.map(|package| package.description);
    let preview = main_window_state::load_preview(store.root(), id)?.map(preview_data_url);
    Ok(LocalWidgetDetails {
        config,
        compatibility,
        description,
        preview,
    })
}

/// Non-secret state used to identify the configured GitHub token in the main window.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GithubTokenSummary {
    pub masked: String,
}

impl From<&GithubToken> for GithubTokenSummary {
    fn from(token: &GithubToken) -> Self {
        Self {
            masked: token.masked(),
        }
    }
}

/// Current widget inventory, application version, and native preferences shown
/// by the main window.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MainState {
    pub widgets: Vec<WidgetSummary>,
    pub deleted_widgets: Vec<DeletedWidgetSummary>,
    pub broken: Vec<crate::widgets::model::BrokenWidget>,
    pub version: String,
    pub theme: Theme,
    pub language: Option<Language>,
    pub tray_click_action: TrayClickAction,
    pub last_page: MainPage,
    pub github_token: Option<GithubTokenSummary>,
    /// Whether the About page's toggle leaves automatic downloading on.
    pub auto_download: bool,
    /// Display new widgets try first; `None` keeps automatic placement.
    pub default_display: Option<String>,
    /// Connected displays offered by the default display setting.
    pub displays: Vec<DisplayInfo>,
}

/// List every valid widget, including disabled widgets. Display discovery needs
/// the app handle, so `main_payload` fills `displays`.
pub fn main_widgets(store: &Store) -> Result<MainState> {
    let settings = settings::load(store.root())?;
    let github_token = settings.github_token.as_ref().map(GithubTokenSummary::from);
    let board = store.load_board()?;
    let mut widgets = Vec::new();
    for (index, id) in store.widget_ids()?.into_iter().enumerate() {
        let Ok(meta) = store.load_meta(&id) else {
            continue;
        };
        let frame = board
            .widgets
            .get(&id)
            .cloned()
            .unwrap_or_else(|| WidgetFrame::default_for(index));
        let signature = signature(store, &id, meta.revision);
        widgets.push(WidgetSummary {
            package_name: store.load_package(&id)?.map(|package| package.name),
            id: meta.id.clone(),
            title: meta.title.clone(),
            revision: meta.revision,
            token: content_token(&signature),
            width: frame.w,
            height: frame.h,
            enabled: meta.enabled,
            created_at: store.widget_created_at_ms(&id)?,
        });
    }
    Ok(MainState {
        widgets,
        deleted_widgets: store.deleted_widgets()?,
        broken: {
            let mut broken = store.list()?.broken;
            broken.extend(store.trash_errors()?);
            broken
        },
        // The crate version, as `docs/development.md` promises: the same number the
        // CLI, doctor, the MCP server and the bundle report. Whether this build may
        // replace itself is the update row's business, not the version's.
        version: env!("CARGO_PKG_VERSION").to_string(),
        theme: settings.theme,
        language: settings.language,
        tray_click_action: settings.tray_click_action.effective(),
        last_page: settings.last_page,
        github_token,
        auto_download: settings.auto_download,
        default_display: settings.default_display,
        displays: Vec::new(),
    })
}

/// Build the current main-page payload from the widget store and settings.
pub fn main_payload(app: &AppHandle) -> Result<MainState> {
    let store = Store::new(app.state::<UiState>().root.lock().unwrap().clone());
    Ok(MainState {
        displays: displays(app)?,
        ..main_widgets(&store)?
    })
}

/// Persist UI preferences and apply the primary-click tray behavior without
/// restarting. Linux rejects attempts to choose a non-menu tray action.
pub fn set_preferences<R: Runtime>(app: &AppHandle<R>, preferences: UiPreferences) -> Result<()> {
    if cfg!(target_os = "linux") && preferences.tray_click_action != TrayClickAction::Menu {
        return Err(Error::InvalidParams(
            "changing the tray click action is unsupported on Linux".to_string(),
        ));
    }
    let state = app.state::<UiState>();
    let _preferences_lock = state.preferences.lock().unwrap();
    let store = Store::new(app.state::<UiState>().root.lock().unwrap().clone());
    let previous = settings::load(store.root())?.preferences()?;
    let tray = app
        .tray_by_id("aidget")
        .ok_or_else(|| Error::Other("tray icon is not available".to_string()))?;
    let previous_tray = previous.tray_click_action.effective();
    let next_tray = preferences.tray_click_action.effective();
    if previous_tray != next_tray {
        tray.set_show_menu_on_left_click(next_tray == TrayClickAction::Menu)
            .map_err(|error| Error::Other(format!("cannot update tray click action: {error}")))?;
    }
    if let Err(error) = settings::save_preferences(store.root(), preferences) {
        if previous_tray != next_tray {
            let _ = tray.set_show_menu_on_left_click(previous_tray == TrayClickAction::Menu);
        }
        return Err(error);
    }
    Ok(())
}

/// Save the first system-derived language, preserving an explicit choice.
pub fn initialize_language<R: Runtime>(app: &AppHandle<R>, language: Language) -> Result<Language> {
    let state = app.state::<UiState>();
    let _preferences_lock = state.preferences.lock().unwrap();
    let root = state.root.lock().unwrap().clone();
    settings::initialize_language(&root, language)
}

/// Verify and persist a newly entered GitHub token, or remove the existing one.
pub fn set_github_token(
    root: &std::path::Path,
    token: Option<String>,
) -> Result<Option<GithubTokenSummary>> {
    let token = token.map(GithubToken::parse).transpose()?;
    if let Some(token) = &token {
        crate::community::service::Community::with_endpoints_and_token(
            crate::community::service::Endpoints::default(),
            Some(token.clone()),
        )?
        .verify_github_access()?;
    }
    settings::save_github_token(root, token.as_ref())?;
    Ok(token.as_ref().map(GithubTokenSummary::from))
}

/// Enable or disable a desktop widget without changing its saved position.
pub fn set_widget_enabled(
    app: &AppHandle,
    id: &str,
    enabled: bool,
    base_revision: u64,
) -> Result<()> {
    validate_id(id)?;
    let state = app.state::<UiState>();
    let store = Store::new(state.root.lock().unwrap().clone());
    store.update(
        id,
        base_revision,
        WidgetEdit {
            enabled: Some(enabled),
            ..WidgetEdit::default()
        },
    )?;
    sync_widgets(app);
    Ok(())
}

/// Move a widget into trash and remove its main-window preview state.
pub fn delete_widget(app: &AppHandle, id: &str) -> Result<()> {
    validate_id(id)?;
    let state = app.state::<UiState>();
    let store = Store::new(state.root.lock().unwrap().clone());
    store.delete(id)?;
    main_window_state::forget_widget(store.root(), id)?;
    sync_widgets(app);
    Ok(())
}

/// Restore one trashed widget and recreate its desktop window.
pub fn restore_widget(app: &AppHandle, trash_name: &str) -> Result<()> {
    let state = app.state::<UiState>();
    let store = Store::new(state.root.lock().unwrap().clone());
    store.restore(trash_name)?;
    sync_widgets(app);
    Ok(())
}

/// Return the preview saved with a trashed widget.
pub fn deleted_widget_preview(app: &AppHandle, trash_name: &str) -> Result<Option<String>> {
    let state = app.state::<UiState>();
    let store = Store::new(state.root.lock().unwrap().clone());
    let Some(png) = store.deleted_preview(trash_name)? else {
        return Ok(None);
    };
    Ok(Some(preview_data_url(png)))
}

/// Permanently remove a trashed widget.
pub fn purge_deleted_widget(app: &AppHandle, trash_name: &str) -> Result<()> {
    let state = app.state::<UiState>();
    let store = Store::new(state.root.lock().unwrap().clone());
    store.purge(trash_name)?;
    sync_widgets(app);
    Ok(())
}

/// Return a PNG data URL, refreshing enabled widgets and reusing disabled cache.
pub async fn main_widget_preview(app: &AppHandle, id: &str) -> Result<Option<String>> {
    validate_id(id)?;
    let state = app.state::<UiState>();
    let store = Store::new(state.root.lock().unwrap().clone());
    let meta = store.load_meta(id)?;
    let board = store.load_board()?;
    let compatibility = crate::widgets::package::HostCompatibility::evaluate(
        &meta.config,
        env!("CARGO_PKG_VERSION"),
        app.state::<UiState>().allow_incompatible_widgets,
    )?;
    let png = if board.widgets.contains_key(id) && meta.enabled && compatibility.permits_runtime() {
        let mode = screenshot_mode(true, MAIN_PREVIEW_SCALE);
        let png = capture_widget(app, id, mode).await?;
        main_window_state::save_preview(store.root(), id, &png)?;
        png
    } else {
        let Some(png) = main_window_state::load_preview(store.root(), id)? else {
            return Ok(None);
        };
        png
    };
    Ok(Some(preview_data_url(png)))
}

/// Return the last persisted preview without capturing the widget WebView.
pub fn cached_preview(store: &Store, id: &str) -> Result<Option<String>> {
    store.load_meta(id)?;
    Ok(main_window_state::load_preview(store.root(), id)?.map(preview_data_url))
}

fn preview_data_url(png: Vec<u8>) -> String {
    let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, png);
    format!("data:image/png;base64,{encoded}")
}
