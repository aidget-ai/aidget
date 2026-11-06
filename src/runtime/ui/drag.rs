//! Widget drag state and position persistence.
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager, WebviewWindow};

use crate::common::error::{Error, Result};
use crate::widgets::store::Store;

use super::geometry::{
    connected_displays, moved_frame, named_display, physical_geometry, same_monitor,
};
use super::main_window::emit_main;
use super::relocation::DisplayNames;
use super::state::UiState;

/// Whether a move changes the stored frame. A window still at its frame has not
/// moved, which also covers a click on the drag handle of a widget whose display
/// is gone. Such a display's windows are evacuated by the system; saving that
/// would lose the frame that brings them back, so only a drag may replace it.
pub fn records_move(
    stored_display: &str,
    connected: &DisplayNames<'_>,
    user_drag: bool,
    at_frame: bool,
) -> bool {
    !at_frame && (user_drag || connected.index(stored_display).is_some())
}

/// Persist the calling widget window's current frame when it has changed. Moves
/// also arrive here from the system, which `records_move` tells apart.
pub fn persist_position(app: &AppHandle, window: &WebviewWindow) -> Result<()> {
    let state = app.state::<UiState>();
    let user_drag = dragging_active(&state) || native_drag_running(&state);
    persist(app, window, user_drag)
}

fn persist(app: &AppHandle, window: &WebviewWindow, user_drag: bool) -> Result<()> {
    let state = app.state::<UiState>();
    if *state.applying.lock().unwrap() {
        return Ok(());
    }
    let id = window
        .label()
        .strip_prefix("widget-")
        .ok_or_else(|| Error::InvalidParams("not a widget window".to_string()))?
        .to_string();
    let store = Store::new(app.state::<UiState>().root.lock().unwrap().clone());
    let _lock = store.write_lock()?;
    let mut board = store.load_board()?;
    let Some(current) = board.widgets.get(&id).cloned() else {
        return Ok(());
    };
    let position = window.outer_position()?;
    let (x, y, _) = physical_geometry(app, &current);
    let at_frame = (position.x - x).abs() <= 1 && (position.y - y).abs() <= 1;
    // With no display at all, there is no position worth recording.
    let Ok((monitors, primary)) = connected_displays(app) else {
        return Ok(());
    };
    let connected = DisplayNames::of(&monitors, primary);
    if !records_move(&current.display, &connected, user_drag, at_frame) {
        return Ok(());
    }
    // Let the toolkit pick the screen that holds most of the window. Computing
    // an overlap ourselves would duplicate `NSWindow.screen` and can disagree
    // while the window is straddling the seam.
    let monitor = window
        .current_monitor()?
        .or_else(|| window.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return Ok(());
    };
    let area = monitor.work_area();
    let scale = monitor.scale_factor();
    let on_primary = window
        .primary_monitor()
        .ok()
        .flatten()
        .is_some_and(|primary| same_monitor(&primary, &monitor));
    let display = if on_primary {
        "primary".to_string()
    } else {
        named_display(app, &monitor)
    };
    let frame = moved_frame(
        &current,
        display,
        (position.x, position.y),
        (area.position.x, area.position.y),
        scale,
    );
    board.widgets.insert(id.clone(), frame);
    board.revision += 1;
    store.save_board(&board)?;
    *app.state::<UiState>().board_revision.lock().unwrap() = board.revision;
    emit_main(app);
    Ok(())
}

/// Mark the start of a native widget drag.
pub fn begin_drag(app: &AppHandle) {
    let state = app.state::<UiState>();
    let now = Some(Instant::now());
    *state.dragging.lock().unwrap() = now;
    *state.drag_started.lock().unwrap() = now;
}

/// Finish a native widget drag and persist its final position.
pub fn end_drag(app: &AppHandle, window: &WebviewWindow) -> Result<()> {
    *app.state::<UiState>().dragging.lock().unwrap() = None;
    persist(app, window, true)
}

/// Push or pop the native grabbing cursor around a macOS window drag.
#[cfg(target_os = "macos")]
pub fn set_dragging_cursor(app: &AppHandle, grabbing: bool) -> Result<()> {
    run_on_main_thread(app, move || {
        if grabbing {
            objc2_app_kit::NSCursor::closedHandCursor().push();
        } else {
            objc2_app_kit::NSCursor::pop_class();
        }
    })
}

/// Wait until the user releases the mouse button that started a native drag.
#[cfg(target_os = "macos")]
pub async fn wait_for_drag_release(app: &AppHandle) -> Result<()> {
    loop {
        let pressed =
            run_on_main_thread(app, || objc2_app_kit::NSEvent::pressedMouseButtons() != 0)?;
        if !pressed {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(8)).await;
    }
}

#[cfg(target_os = "macos")]
fn run_on_main_thread<T: Send + 'static>(
    app: &AppHandle,
    task: impl FnOnce() -> T + Send + 'static,
) -> Result<T> {
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    app.run_on_main_thread(move || {
        let _ = sender.send(task());
    })?;
    receiver
        .recv()
        .map_err(|_| Error::Other("main thread task was dropped".to_string()))
}

/// A drag event can be lost when the drag ends over another app. Expire the
/// freeze so geometry never stays locked.
pub(super) fn dragging_active(state: &UiState) -> bool {
    unexpired(&state.dragging)
}

/// Whether a move may belong to a native drag that outlived `end_drag`. Only macOS
/// waits for the mouse release; elsewhere `start_dragging` returns at once.
///
/// shortcut: every move for 30s after a drag begins counts as the user's, and a stop
/// on the way records that display; read the mouse button as macOS does if that bites.
pub fn native_drag_running(state: &UiState) -> bool {
    cfg!(not(target_os = "macos")) && unexpired(&state.drag_started)
}

fn unexpired(started: &Mutex<Option<Instant>>) -> bool {
    started
        .lock()
        .unwrap()
        .is_some_and(|started| started.elapsed() < Duration::from_secs(30))
}
