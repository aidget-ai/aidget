//! Widget drag state and position persistence.
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager, WebviewWindow};

use crate::common::error::{Error, Result};
use crate::widgets::store::Store;

use super::geometry::{
    connected_displays, moved_frame, named_display, physical_geometry, same_monitor,
};
use super::main_window::emit_main;
use super::relocation::DisplayNames;
use super::state::{DragGesture, UiState};

/// Whether a move changes the stored frame. A finished drag always does, even onto
/// the spot the frame computes to: that spot may only stand in for a display that
/// is gone. Otherwise a window at its frame has not moved, and the system evacuating
/// a missing display must not replace the frame that brings its windows back.
pub fn records_move(
    stored_display: &str,
    connected: &DisplayNames<'_>,
    user_drag: bool,
    at_frame: bool,
) -> bool {
    user_drag || (!at_frame && connected.index(stored_display).is_some())
}

/// Persist the calling widget window's current frame when it has changed. Moves
/// also arrive here from the system, which never count as the user's. A window
/// being dragged waits for `end_drag`, which knows where the drag began.
pub fn persist_position(app: &AppHandle, window: &WebviewWindow) -> Result<()> {
    if gesture_owns(&app.state::<UiState>(), window.label()) {
        return Ok(());
    }
    persist(app, window, false)
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

/// Mark the start of a native drag of `window`.
pub fn begin_drag(app: &AppHandle, window: &WebviewWindow) -> Result<()> {
    let position = window.outer_position()?;
    start_gesture(
        &app.state::<UiState>(),
        window.label(),
        (position.x, position.y),
    );
    Ok(())
}

/// Remember that `window`, labeled as in `DragGesture`, began a drag at `origin`.
pub fn start_gesture(state: &UiState, window: &str, origin: (i32, i32)) {
    *state.dragging.lock().unwrap() = Some(DragGesture {
        window: window.to_string(),
        started: Instant::now(),
        origin,
    });
}

/// Finish a native drag and persist where it landed, if it moved the window.
pub fn end_drag(app: &AppHandle, window: &WebviewWindow) -> Result<()> {
    let position = window.outer_position()?;
    let state = app.state::<UiState>();
    if finish_gesture(&state, window.label(), (position.x, position.y)) {
        persist(app, window, true)
    } else {
        Ok(())
    }
}

/// End `window`'s drag and report whether it left where the drag began. The stored
/// frame cannot answer that: the system may hold the window at a spot no frame
/// describes, where a click is still not a move. Another window's end finishes nothing.
pub fn finish_gesture(state: &UiState, window: &str, position: (i32, i32)) -> bool {
    let gesture = state
        .dragging
        .lock()
        .unwrap()
        .take_if(|gesture| gesture.window == window);
    gesture.is_some_and(|gesture| {
        (position.0 - gesture.origin.0).abs() > 1 || (position.1 - gesture.origin.1).abs() > 1
    })
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
/// `start_dragging` returns before the drop, so this is how a drag learns its end.
pub async fn wait_for_drag_release(app: &AppHandle) -> Result<()> {
    while mouse_pressed(app)? {
        tokio::time::sleep(Duration::from_millis(8)).await;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn mouse_pressed(app: &AppHandle) -> Result<bool> {
    run_on_main_thread(app, || objc2_app_kit::NSEvent::pressedMouseButtons() != 0)
}

#[cfg(windows)]
fn mouse_pressed(_app: &AppHandle) -> Result<bool> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON};

    // The sign bit reports a button held now. Either counts: the user can swap them.
    Ok([VK_LBUTTON, VK_RBUTTON]
        .into_iter()
        .any(|button| unsafe { GetAsyncKeyState(i32::from(button.0)) } < 0))
}

/// The compositor runs a Wayland drag and reports no buttons, so nothing is held
/// there. It also keeps global positions from clients, so a drag there has no
/// position to save however long the gesture is kept open.
#[cfg(all(unix, not(target_os = "macos")))]
fn mouse_pressed(app: &AppHandle) -> Result<bool> {
    use gdk::prelude::*;

    run_on_main_thread(app, || {
        let Some(display) = gdk::Display::default() else {
            return false;
        };
        if display.backend().is_wayland() {
            return false;
        }
        let pointer = display.default_seat().and_then(|seat| seat.pointer());
        let root = display.default_screen().root_window();
        pointer.zip(root).is_some_and(|(pointer, root)| {
            let (_, _, _, mask) = root.device_position(&pointer);
            mask.intersects(gdk::ModifierType::BUTTON1_MASK | gdk::ModifierType::BUTTON3_MASK)
        })
    })
}

#[cfg(not(windows))]
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
    state
        .dragging
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(unexpired)
}

/// Whether a running drag belongs to `window`. A drag says nothing about others.
pub fn gesture_owns(state: &UiState, window: &str) -> bool {
    state
        .dragging
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|gesture| gesture.window == window && unexpired(gesture))
}

fn unexpired(gesture: &DragGesture) -> bool {
    gesture.started.elapsed() < Duration::from_secs(30)
}
