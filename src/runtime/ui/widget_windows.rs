//! Desktop widget window synchronization and board placement.
use std::collections::HashSet;

use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

use crate::common::error::{Error, Result};
use crate::transport::origin;
use crate::widgets::model::{Board, WidgetFrame, WidgetSurface};
use crate::widgets::store::Store;

use super::content::{content_token, signature, widget_surface, WidgetState};
use super::drag::dragging_active;
use super::geometry::{physical_geometry, place_unplaced_widget};
use super::main_window::emit_main;
#[cfg(target_os = "macos")]
use super::native_surface;
use super::state::{label, retain_widget_revisions, UiState, MAIN_LABEL};

/// Queue a reconciliation of desktop widget windows with the current store.
pub fn sync_widgets(app: &AppHandle) {
    if !app.state::<UiState>().startup.widgets_started() {
        return;
    }
    if let Err(error) = schedule_sync_widgets(app) {
        eprintln!("aidget: sync failed: {error}");
    }
}

fn schedule_sync_widgets(app: &AppHandle) -> Result<()> {
    let handle = app.clone();
    app.run_on_main_thread(move || {
        if let Err(error) = sync_widgets_inner(&handle) {
            eprintln!("aidget: sync failed: {error}");
        }
    })
    .map_err(|error| Error::Other(error.to_string()))
}

/// Start desktop widget creation once, either after the main window paints or
/// immediately for a background launch.
pub fn start_initial_widgets(app: &AppHandle) -> Result<bool> {
    let state = app.state::<UiState>();
    if !state.startup.claim_widgets_start() {
        return Ok(false);
    }
    if let Err(error) = schedule_sync_widgets(app) {
        state.startup.reset_widgets_start();
        return Err(error);
    }
    Ok(true)
}

fn sync_widgets_inner(app: &AppHandle) -> Result<()> {
    let state = app.state::<UiState>();
    let store = Store::new(state.root.lock().unwrap().clone());
    let board = store.load_board()?;
    let ids = enabled_widget_ids(&store, &board);
    let active: HashSet<_> = ids.iter().cloned().collect();
    retain_widget_revisions(
        &mut state.loaded.lock().unwrap(),
        &mut state.ready.lock().unwrap(),
        &active,
    );
    let mut windows = state.windows.lock().unwrap();
    let mut signatures = state.signatures.lock().unwrap();
    let mut board_revision = state.board_revision.lock().unwrap();

    let existing: Vec<String> = windows.keys().cloned().collect();
    for id in existing {
        if !ids.contains(&id) {
            if let Some(window) = app.get_webview_window(&label(&id)) {
                if let Err(error) = window.close() {
                    eprintln!("aidget: cannot close {id}: {error}");
                }
            }
            windows.remove(&id);
            signatures.remove(&id);
        }
    }

    for id in ids {
        let Ok(meta) = store.load_meta(&id) else {
            continue;
        };
        let signature = signature(&store, &id, meta.revision);
        let surface = widget_surface(&store, &id);
        let widget_state = WidgetState {
            id: id.clone(),
            revision: meta.revision,
            token: content_token(&signature),
            drag_mode: meta.drag_mode,
            permissions: meta.permissions.clone(),
            host_surface: surface == WidgetSurface::Host
                || (surface == WidgetSurface::Glass && !cfg!(target_os = "macos")),
            surface,
            title: meta.title.clone(),
        };

        if let Some(window) = app.get_webview_window(&label(&id)) {
            // A previously deleted widget may have left an offscreen window
            // addressable by label; show it again when the widget returns.
            // Keep an already visible widget untouched: `show()` re-orders the
            // window as key, which makes macOS re-evaluate transparency while
            // the user is dragging.
            if !window.is_visible()? {
                #[cfg(windows)]
                if !windows.contains_key(&id) {
                    crate::common::utils::reveal_window_at_bottom(window.hwnd()?)?;
                }
                #[cfg(not(windows))]
                window.show()?;
            }
            let changed = signatures.get(&id) != Some(&signature);
            if changed {
                #[cfg(target_os = "macos")]
                native_surface::apply(&window, widget_state.surface)?;

                let _ = app.emit_to(label(&id), "aidget://widget", &widget_state);
                let _ = window.eval(format!(
                    "window.__aidgetApply && window.__aidgetApply({});",
                    serde_json::to_string(&widget_state)
                        .map_err(|error| Error::Other(error.to_string()))?
                ));
                signatures.insert(id.clone(), signature);
            }
            if let Some(frame) = board.widgets.get(&id) {
                if board.revision > *board_revision && !dragging_active(&state) {
                    apply_frame(app, &window, frame)?;
                }
            }
            continue;
        }

        let Some(mut frame) = board.widgets.get(&id).cloned() else {
            continue;
        };
        if frame.unplaced {
            // Without a display to place on, the next sync tries again.
            if let Err(error) = place_unplaced_widget(app, &store, &id) {
                eprintln!("aidget: cannot place {id}: {error}");
                continue;
            }
            let Some(placed) = store.load_board()?.widgets.remove(&id) else {
                continue;
            };
            frame = placed;
        }
        let window = build_window(app, &id, &frame, &widget_state)?;
        apply_frame(app, &window, &frame)?;
        windows.insert(id.clone(), ());
        signatures.insert(id, signature);
    }
    *board_revision = (*board_revision).max(board.revision);
    emit_main(app);
    if state.startup.claim_widgets_announcement() {
        let _ = app.emit_to(MAIN_LABEL, "aidget://widgets-ready", ());
    }
    Ok(())
}

fn enabled_widget_ids(store: &Store, board: &Board) -> Vec<String> {
    board
        .widgets
        .keys()
        .filter(|id| store.load_meta(id).is_ok_and(|meta| meta.enabled))
        .cloned()
        .collect()
}

fn build_window(
    app: &AppHandle,
    id: &str,
    frame: &WidgetFrame,
    state: &WidgetState,
) -> Result<WebviewWindow> {
    let (x, y, scale) = physical_geometry(app, frame);
    // The page reads its state from `widget_state`, never from the URL.
    let url = origin::widget_window_url();
    let builder = WebviewWindowBuilder::new(
        app,
        label(id),
        WebviewUrl::External(
            url.parse()
                .map_err(|error| Error::Other(format!("invalid widget url: {error}")))?,
        ),
    )
    .title(&state.title)
    .inner_size(frame.w, frame.h)
    .position(x as f64 / scale, y as f64 / scale)
    // Installed in every frame: the widget document itself runs inside the
    // shell's iframe. The script ignores everything outside `/content/`.
    .initialization_script_for_all_frames(crate::runtime::bridge_script::WIDGET_API)
    .initialization_script_for_all_frames(crate::runtime::bridge_script::WIDGET_DRAG)
    .transparent(true)
    .decorations(false)
    .shadow(false)
    .resizable(false)
    // Open without taking key focus; clicking a widget still works and its
    // window stays behind normal windows.
    .focused(false)
    .always_on_bottom(true)
    .skip_taskbar(true)
    .visible_on_all_workspaces(true)
    .accept_first_mouse(true);

    // Avoid the redirection bitmap's stale transparent-window surface on
    // Windows, and wait for the document before revealing the window.
    #[cfg(windows)]
    let builder = builder.visible(false).no_redirection_bitmap(true);
    let window = builder.build()?;

    // A hidden WebView2 window keeps the native title bar from creation.
    // Remove the frame before the first reveal while Tauri still considers the
    // window undecorated.
    #[cfg(windows)]
    crate::common::utils::remove_window_decorations(window.hwnd()?)?;

    #[cfg(target_os = "macos")]
    native_surface::apply(&window, state.surface)?;

    Ok(window)
}

/// Apply a persisted board to every matching widget window.
pub fn apply_board(app: &AppHandle, board: &crate::widgets::model::Board) {
    apply_board_inner(app, board, false);
}

/// Re-apply every frame even when the board revision did not change.
pub(super) fn apply_board_forced(app: &AppHandle, board: &crate::widgets::model::Board) {
    apply_board_inner(app, board, true);
}

fn apply_board_inner(app: &AppHandle, board: &crate::widgets::model::Board, force: bool) {
    let handle = app.clone();
    let widgets = board.widgets.clone();
    let revision = board.revision;
    let _ = app.run_on_main_thread(move || {
        let state = handle.state::<UiState>();
        // Never re-place a window while the user is dragging it. Board writes
        // triggered by that same drag would otherwise snap it back to an older
        // frame for one or two frames.
        if dragging_active(&state) {
            return;
        }
        let mut board_revision = state.board_revision.lock().unwrap();
        // A watcher may read an older board while a drag is saving. Never move
        // a window back to a snapshot we have already superseded.
        if !force && revision <= *board_revision {
            return;
        }
        let mut applying = state.applying.lock().unwrap();
        *applying = true;
        drop(applying);
        for (id, frame) in widgets {
            if let Some(window) = handle.get_webview_window(&label(&id)) {
                if let Err(error) = apply_frame(&handle, &window, &frame) {
                    eprintln!("aidget: cannot place {id}: {error}");
                }
            }
        }
        *state.applying.lock().unwrap() = false;
        *board_revision = revision;
        emit_main(&handle);
    });
}

fn apply_frame(app: &AppHandle, window: &WebviewWindow, frame: &WidgetFrame) -> Result<()> {
    let (x, y, scale) = physical_geometry(app, frame);
    let position = window.outer_position().ok();
    let size = window.inner_size().ok();
    let moved = position
        .map(|current| (current.x - x).abs() > 1 || (current.y - y).abs() > 1)
        .unwrap_or(true);
    let resized = size
        .map(|current| {
            (current.width as f64 - frame.w * scale).abs() > 1.0
                || (current.height as f64 - frame.h * scale).abs() > 1.0
        })
        .unwrap_or(true);
    if moved {
        window.set_position(PhysicalPosition::new(x, y))?;
    }
    if scale > 0.0 && resized {
        window.set_size(PhysicalSize::new(
            (frame.w * scale).round() as u32,
            (frame.h * scale).round() as u32,
        ))?;
    }
    Ok(())
}

/// Return the serializable state owned by the calling widget window.
pub fn widget_payload(app: &AppHandle, window: &WebviewWindow) -> Option<WidgetState> {
    let label = window.label();
    let id = label.strip_prefix("widget-")?;
    let store = Store::new(app.state::<UiState>().root.lock().unwrap().clone());
    let meta = store.load_meta(id).ok()?;
    let _ = app;
    let surface = widget_surface(&store, id);
    Some(WidgetState {
        id: id.to_string(),
        revision: meta.revision,
        token: content_token(&signature(&store, id, meta.revision)),
        drag_mode: meta.drag_mode,
        permissions: meta.permissions.clone(),
        host_surface: surface == WidgetSurface::Host
            || (surface == WidgetSurface::Glass && !cfg!(target_os = "macos")),
        surface,
        title: meta.title.clone(),
    })
}
