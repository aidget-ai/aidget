//! Tauri command endpoints behind the native window authorization boundary.
use std::collections::BTreeMap;

use crate::runtime::{command_access, ui, update};
use crate::settings::UiPreferences;
use crate::widgets::store::Store;
use tauri::Manager;

/// Register the application commands with authorization before argument extraction.
pub fn handler() -> impl Fn(tauri::ipc::Invoke) -> bool + Send + Sync + 'static {
    command_access::guard(tauri::generate_handler![
        widget_state,
        widget_loaded,
        widget_ready,
        persist_window_position,
        begin_widget_drag,
        start_widget_drag,
        end_widget_drag,
        main_state,
        main_painted,
        main_widget_cached_preview,
        main_widget_details,
        set_widget_enabled,
        delete_widget,
        restore_widget,
        deleted_widget_preview,
        purge_deleted_widget,
        main_widget_preview,
        widget_location,
        widget_storage_get,
        widget_storage_set,
        widget_storage_remove,
        widget_storage_clear,
        widget_fetch,
        widget_open_url,
        integration_state,
        set_agent_integration,
        set_preferences,
        set_github_token,
        community_list,
        community_media,
        community_install,
        community_downloads,
        update::update_state,
        update::update_check,
        update::update_download,
        update::update_install
    ])
}

#[tauri::command]
fn widget_state(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> std::result::Result<ui::WidgetState, String> {
    ui::widget_payload(&app, &window).ok_or_else(|| "unknown widget window".to_string())
}

#[tauri::command]
fn widget_loaded(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    revision: u64,
) -> std::result::Result<(), String> {
    ui::mark_loaded(&app, &window, revision).map_err(|error| error.to_string())
}

#[tauri::command]
fn widget_ready(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    revision: u64,
) -> std::result::Result<(), String> {
    ui::mark_ready(&app, &window, revision).map_err(|error| error.to_string())
}

#[tauri::command]
fn persist_window_position(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> std::result::Result<(), String> {
    ui::persist_position(&app, &window).map_err(|error| error.to_string())
}

#[tauri::command]
fn begin_widget_drag(app: tauri::AppHandle) {
    ui::begin_drag(&app);
}

#[tauri::command]
async fn start_widget_drag(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> std::result::Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        ui::set_dragging_cursor(&app, true).map_err(|error| error.to_string())?;
        if let Err(error) = window.start_dragging() {
            let _ = ui::set_dragging_cursor(&app, false);
            return Err(error.to_string());
        }
        let released = ui::wait_for_drag_release(&app).await;
        let restored = ui::set_dragging_cursor(&app, false);
        released.map_err(|error| error.to_string())?;
        restored.map_err(|error| error.to_string())?;
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        window.start_dragging().map_err(|error| error.to_string())
    }
}

#[tauri::command]
fn end_widget_drag(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> std::result::Result<(), String> {
    ui::end_drag(&app, &window).map_err(|error| error.to_string())
}

#[tauri::command]
fn main_state(app: tauri::AppHandle) -> std::result::Result<ui::MainState, String> {
    ui::main_payload(&app).map_err(|error| error.to_string())
}

/// Start desktop widget windows after the main page has painted its first frame.
///
/// Run outside the WebView IPC callback so scheduling window creation on the
/// main thread cannot reenter WebView2 while processing that callback.
#[tauri::command]
async fn main_painted(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> std::result::Result<bool, String> {
    ui::require_main_window_label(window.label()).map_err(|error| error.to_string())?;
    let started = ui::start_initial_widgets(&app).map_err(|error| error.to_string())?;
    ui::mark_main_painted(&app, &window).map_err(|error| error.to_string())?;
    Ok(started)
}

/// Return a cached widget preview without capturing the widget WebView.
#[tauri::command]
async fn main_widget_cached_preview(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    id: String,
) -> std::result::Result<Option<String>, String> {
    ui::require_main_window_label(window.label()).map_err(|error| error.to_string())?;
    let root = app.state::<ui::UiState>().root.lock().unwrap().clone();
    tauri::async_runtime::spawn_blocking(move || ui::cached_preview(&Store::new(root), &id))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

/// Read installed metadata and the saved preview for the main-window details dialog.
#[tauri::command]
async fn main_widget_details(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    id: String,
) -> std::result::Result<ui::LocalWidgetDetails, String> {
    ui::require_main_window_label(window.label()).map_err(|error| error.to_string())?;
    let root = app.state::<ui::UiState>().root.lock().unwrap().clone();
    tauri::async_runtime::spawn_blocking(move || ui::local_widget_details(&Store::new(root), &id))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn integration_state() -> std::result::Result<crate::agents::integration::IntegrationState, String>
{
    crate::agents::integration::integration_state().map_err(|error| error.to_string())
}

#[tauri::command]
fn set_agent_integration(
    change: crate::agents::integration::IntegrationChange,
) -> std::result::Result<crate::agents::integration::IntegrationMutation, String> {
    crate::agents::integration::set_agent_integration(change).map_err(|error| error.to_string())
}

/// Persist appearance and behavior preferences and apply the primary-click tray
/// action to the running tray icon.
#[tauri::command]
async fn set_preferences(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    preferences: UiPreferences,
) -> std::result::Result<(), String> {
    ui::require_main_window_label(window.label()).map_err(|error| error.to_string())?;
    ui::set_preferences(&app, preferences).map_err(|error| error.to_string())
}

/// Validate, save, or remove the GitHub token used by community API requests.
#[tauri::command]
async fn set_github_token(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    token: Option<String>,
) -> std::result::Result<Option<ui::GithubTokenSummary>, String> {
    ui::require_main_window_label(window.label()).map_err(|error| error.to_string())?;
    let root = app.state::<ui::UiState>().root.lock().unwrap().clone();
    tauri::async_runtime::spawn_blocking(move || ui::set_github_token(&root, token))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

// Widget mutations schedule window creation and must also leave the IPC callback.
#[tauri::command]
async fn set_widget_enabled(
    app: tauri::AppHandle,
    id: String,
    enabled: bool,
) -> std::result::Result<(), String> {
    ui::set_widget_enabled(&app, &id, enabled).map_err(|error| error.to_string())
}

#[tauri::command]
async fn delete_widget(app: tauri::AppHandle, id: String) -> std::result::Result<(), String> {
    ui::delete_widget(&app, &id).map_err(|error| error.to_string())
}

#[tauri::command]
async fn restore_widget(
    app: tauri::AppHandle,
    trash_name: String,
) -> std::result::Result<(), String> {
    ui::restore_widget(&app, &trash_name).map_err(|error| error.to_string())
}

#[tauri::command]
fn deleted_widget_preview(
    app: tauri::AppHandle,
    trash_name: String,
) -> std::result::Result<Option<String>, String> {
    ui::deleted_widget_preview(&app, &trash_name).map_err(|error| error.to_string())
}

#[tauri::command]
async fn purge_deleted_widget(
    app: tauri::AppHandle,
    trash_name: String,
) -> std::result::Result<(), String> {
    ui::purge_deleted_widget(&app, &trash_name).map_err(|error| error.to_string())
}

#[tauri::command]
async fn main_widget_preview(
    app: tauri::AppHandle,
    id: String,
) -> std::result::Result<Option<String>, String> {
    ui::main_widget_preview(&app, &id)
        .await
        .map_err(|error| error.to_string())
}

/// Only the window label decides which widget asks; the page cannot name an id.
#[tauri::command]
async fn widget_location(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> std::result::Result<
    crate::runtime::location::LocationFix,
    crate::runtime::location::LocationFailure,
> {
    let id = ui::widget_id(&window)
        .map_err(|error| crate::runtime::location::LocationFailure::from(&error))?;
    crate::runtime::location::request(&app, &id)
        .await
        .map_err(|error| crate::runtime::location::LocationFailure::from(&error))
}

fn widget_storage_context(
    app: &tauri::AppHandle,
    window: &tauri::WebviewWindow,
) -> std::result::Result<(Store, String), crate::widgets::storage::StorageFailure> {
    let id = ui::widget_id(window)
        .map_err(|error| crate::widgets::storage::StorageFailure::from(&error))?;
    let root = app.state::<ui::UiState>().root.lock().unwrap().clone();
    let store = Store::new(root);
    crate::widgets::storage::authorize(&store, &id)
        .map_err(|error| crate::widgets::storage::StorageFailure::from(&error))?;
    Ok((store, id))
}

#[tauri::command]
fn widget_storage_get(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    key: String,
) -> std::result::Result<Option<String>, crate::widgets::storage::StorageFailure> {
    let state = app.state::<ui::UiState>();
    let _guard = state.storage_lock.lock().unwrap();
    let (store, id) = widget_storage_context(&app, &window)?;
    crate::widgets::storage::get(&store, &id, &key)
        .map_err(|error| crate::widgets::storage::StorageFailure::from(&error))
}

#[tauri::command]
fn widget_storage_set(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    key: String,
    value: String,
) -> std::result::Result<(), crate::widgets::storage::StorageFailure> {
    let state = app.state::<ui::UiState>();
    let _guard = state.storage_lock.lock().unwrap();
    let (store, id) = widget_storage_context(&app, &window)?;
    crate::widgets::storage::set(&store, &id, &key, &value)
        .map_err(|error| crate::widgets::storage::StorageFailure::from(&error))
}

#[tauri::command]
fn widget_storage_remove(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    key: String,
) -> std::result::Result<(), crate::widgets::storage::StorageFailure> {
    let state = app.state::<ui::UiState>();
    let _guard = state.storage_lock.lock().unwrap();
    let (store, id) = widget_storage_context(&app, &window)?;
    crate::widgets::storage::remove(&store, &id, &key)
        .map_err(|error| crate::widgets::storage::StorageFailure::from(&error))
}

#[tauri::command]
fn widget_storage_clear(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> std::result::Result<(), crate::widgets::storage::StorageFailure> {
    let state = app.state::<ui::UiState>();
    let _guard = state.storage_lock.lock().unwrap();
    let (store, id) = widget_storage_context(&app, &window)?;
    crate::widgets::storage::clear(&store, &id)
        .map_err(|error| crate::widgets::storage::StorageFailure::from(&error))
}

#[tauri::command]
async fn widget_fetch(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    url: String,
    method: String,
    headers: BTreeMap<String, String>,
    body: Option<String>,
) -> std::result::Result<serde_json::Value, crate::runtime::network::NetworkFailure> {
    let id = ui::widget_id(&window).map_err(crate::runtime::network::NetworkFailure::from)?;
    let root = app.state::<ui::UiState>().root.lock().unwrap().clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::runtime::network::fetch_json(
            &Store::new(root),
            &id,
            &url,
            &crate::runtime::network::FetchOptions {
                method,
                headers,
                body,
            },
        )
    })
    .await
    .map_err(|error| crate::runtime::network::NetworkFailure {
        code: "unavailable",
        message: error.to_string(),
    })?
}

#[tauri::command]
fn widget_open_url(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    url: String,
) -> std::result::Result<(), crate::runtime::network::NetworkFailure> {
    let id = ui::widget_id(&window).map_err(crate::runtime::network::NetworkFailure::from)?;
    let root = app.state::<ui::UiState>().root.lock().unwrap().clone();
    crate::runtime::network::open_url(&app, &Store::new(root), &id, &url)
}

/// Load a pinned official main snapshot on a blocking HTTP worker.
#[tauri::command]
async fn community_list(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> std::result::Result<crate::community::model::CommunitySnapshot, String> {
    ui::require_main_window_label(window.label()).map_err(|e| e.to_string())?;
    let root = app.state::<ui::UiState>().root.lock().unwrap().clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::community::service::Community::for_store(&root)?.list(&Store::new(root))
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}
/// Download declared screenshot/logo bytes without exposing remote package HTML.
#[tauri::command]
async fn community_media(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    commit: String,
    package: String,
    path: String,
) -> std::result::Result<String, String> {
    ui::require_main_window_label(window.label()).map_err(|e| e.to_string())?;
    let root = app.state::<ui::UiState>().root.lock().unwrap().clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::community::service::Community::for_store(&root)?.media(&commit, &package, &path)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}
/// Install the exact reviewed content whose permissions the main window displayed.
#[tauri::command]
async fn community_install(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    commit: String,
    package: String,
) -> std::result::Result<(), String> {
    ui::require_main_window_label(window.label()).map_err(|e| e.to_string())?;
    let root = app.state::<ui::UiState>().root.lock().unwrap().clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::community::service::Community::for_store(&root)?.install_reviewed(
            &Store::new(root),
            &commit,
            &package,
        )
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;
    ui::sync_widgets(&app);
    Ok(())
}
/// Counts are optional npm telemetry and never determine installation success.
#[tauri::command]
async fn community_downloads(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    packages: Vec<String>,
) -> std::result::Result<Vec<crate::community::model::DownloadCount>, String> {
    ui::require_main_window_label(window.label()).map_err(|e| e.to_string())?;
    let root = app.state::<ui::UiState>().root.lock().unwrap().clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::community::service::Community::for_store(&root)?
            .downloads(&Store::new(root), &packages)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}
