//! UI runtime façade.
//!
//! Public callers keep using `runtime::ui`; implementation details live in
//! focused child modules.

mod capture;
mod content;
mod drag;
mod geometry;
mod main_page;
mod main_window;
#[cfg(target_os = "macos")]
mod native_surface;
mod state;
mod watcher;
mod widget_windows;

use tauri::{AppHandle, Manager};

use crate::common::error::Result;
use crate::widgets::store::Store;

pub use capture::{mark_loaded, mark_ready, screenshot, screenshot_mode, ScreenshotMode};
pub use content::{content_token, html_paints_own_surface, html_widget_surface, WidgetState};
pub use drag::{begin_drag, end_drag, persist_position};
#[cfg(target_os = "macos")]
pub use drag::{set_dragging_cursor, wait_for_drag_release};
pub use geometry::{
    best_display_placement, centered_position, display_name, displays, physical_geometry,
    place_frame, place_in_work_area, place_new_widget,
};
pub use main_page::{
    cached_preview, delete_widget, deleted_widget_preview, local_widget_details, main_payload,
    main_widget_preview, main_widgets, purge_deleted_widget, restore_widget, set_github_token,
    set_preferences, set_widget_enabled, GithubTokenSummary, LocalWidgetDetails, MainState,
    WidgetSummary,
};
pub use main_window::{mark_main_painted, show_main, show_main_now};
pub use state::{
    record_revision, require_main_window_label, retain_widget_revisions, widget_id, UiState,
    MAIN_LABEL,
};
pub use watcher::is_relevant;
pub use widget_windows::{apply_board, start_initial_widgets, sync_widgets, widget_payload};

/// Initialize UI state and start the watchers that keep windows synchronized
/// with the widget store and displays.
pub fn start(app: &AppHandle, store: Store) -> Result<()> {
    let state = UiState::default();
    *state.root.lock().unwrap() = store.root().to_path_buf();
    app.manage(state);
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
