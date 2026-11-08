//! UI runtime façade.
//!
//! Public callers keep using `runtime::ui`; implementation details live in
//! focused child modules.

mod capture;
mod content;
mod diagnostics;
mod display_labels;
mod drag;
mod geometry;
mod main_page;
mod main_window;
#[cfg(target_os = "macos")]
mod native_surface;
mod relocation;
mod state;
mod watcher;
mod widget_windows;

use tauri::{AppHandle, Manager};

use crate::common::error::{Error, Result};
use crate::widgets::store::Store;

pub use capture::{mark_loaded, mark_ready, screenshot, screenshot_mode, ScreenshotMode};
pub use content::{content_token, html_paints_own_surface, html_widget_surface, WidgetState};
pub use diagnostics::{DiagnosticInput, Diagnostics};
pub use display_labels::{display_name, distinct_names};
#[cfg(target_os = "macos")]
pub use drag::set_dragging_cursor;
pub use drag::{
    begin_drag, end_drag, finish_gesture, gesture_owns, persist_position, records_move,
    start_gesture, wait_for_drag_release,
};
pub use geometry::{
    best_display_placement, centered_position, displays, moved_frame, physical_geometry,
    place_frame, place_in_work_area, place_new_widget, placement_order,
};
pub use main_page::{
    cached_preview, delete_widget, deleted_widget_preview, initialize_language,
    local_widget_details, main_payload, main_widget_preview, main_widgets, purge_deleted_widget,
    restore_widget, set_github_token, set_preferences, set_widget_enabled, GithubTokenSummary,
    LocalWidgetDetails, MainState, WidgetSummary,
};
pub use main_window::{mark_main_painted, show_main, show_main_now};
pub use relocation::DisplayNames;
pub use state::{
    record_revision, require_main_window_label, retain_widget_revisions, widget_id, DragGesture,
    UiState, MAIN_LABEL,
};

/// Record one diagnostic from a validated widget shell.
pub fn record_diagnostic(
    app: &AppHandle,
    window: &tauri::WebviewWindow,
    event: DiagnosticInput,
) -> Result<()> {
    let id = widget_id(window)?;
    let root = app.state::<UiState>().root.lock().unwrap().clone();
    let meta = Store::new(root).load_meta(&id)?;
    if event.revision != meta.revision {
        return Err(Error::InvalidParams(
            "diagnostic revision does not match the current widget".into(),
        ));
    }
    let allow_incompatible = app.state::<UiState>().allow_incompatible_widgets;
    if !crate::widgets::package::HostCompatibility::evaluate(
        &meta.config,
        env!("CARGO_PKG_VERSION"),
        allow_incompatible,
    )?
    .permits_runtime()
    {
        return Err(Error::PermissionDenied(
            "widget is not running under the current host version".into(),
        ));
    }
    app.state::<UiState>()
        .diagnostics
        .lock()
        .unwrap()
        .push(&id, event);
    Ok(())
}

/// Mark the widget document collector as connected for this revision.
pub fn mark_diagnostics_ready(
    app: &AppHandle,
    window: &tauri::WebviewWindow,
    revision: u64,
) -> Result<()> {
    let id = widget_id(window)?;
    let root = app.state::<UiState>().root.lock().unwrap().clone();
    let meta = Store::new(root).load_meta(&id)?;
    if revision != meta.revision {
        return Err(Error::InvalidParams(
            "diagnostic revision does not match the current widget".into(),
        ));
    }
    app.state::<UiState>()
        .diagnostics
        .lock()
        .unwrap()
        .mark_ready(&id, revision);
    Ok(())
}

/// Account for diagnostics dropped by the widget collector or shell limiter.
pub fn record_diagnostics_dropped(
    app: &AppHandle,
    window: &tauri::WebviewWindow,
    dropped: u64,
) -> Result<()> {
    if dropped == 0 || dropped > 1_000_000 {
        return Err(Error::InvalidParams(
            "diagnostic dropped count must be between 1 and 1000000".into(),
        ));
    }
    let id = widget_id(window)?;
    app.state::<UiState>()
        .diagnostics
        .lock()
        .unwrap()
        .add_dropped(&id, dropped);
    Ok(())
}
pub use watcher::is_relevant;
pub use widget_windows::{apply_board, start_initial_widgets, sync_widgets, widget_payload};

/// Initialize UI state and start the watchers that keep windows synchronized
/// with the widget store and displays.
pub fn start(app: &AppHandle, store: Store, allow_incompatible_widgets: bool) -> Result<()> {
    let state = UiState {
        allow_incompatible_widgets: cfg!(debug_assertions) && allow_incompatible_widgets,
        ..UiState::default()
    };
    *state.root.lock().unwrap() = store.root().to_path_buf();
    app.manage(state);
    crate::runtime::pointer::start(app);
    if let Some(encoded) = geometry::displays(app)
        .ok()
        .and_then(|list| serde_json::to_string(&list).ok())
    {
        *app.state::<UiState>().topology.lock().unwrap() = encoded;
    }
    watcher::watch(app.clone(), store.clone());
    watcher::watch_topology(app.clone(), store);
    Ok(())
}
