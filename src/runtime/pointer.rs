//! Host-tracked pointer hover for widget windows.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

use crate::common::error::{Error, Result};
use crate::widgets::model::WidgetPermission;
use crate::widgets::store::Store;

#[cfg(all(unix, not(target_os = "macos")))]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

const EVENT_NAME: &str = "aidget://pointer";

/// Mouse movement phase delivered to widget hover listeners.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PointerPhase {
    Enter,
    Move,
    Leave,
}

/// Local pointer event forwarded through the widget shell.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PointerEvent {
    #[serde(rename = "type")]
    pub phase: PointerPhase,
    pub x: f64,
    pub y: f64,
}

/// Platform-independent hover state for one widget window.
#[derive(Debug, Default)]
pub struct PointerState {
    inside: bool,
    x: f64,
    y: f64,
}

impl PointerState {
    /// Record a pointer sample and return the event it produces, if any.
    pub fn sample(&mut self, inside: bool, x: f64, y: f64) -> Option<PointerEvent> {
        if !inside {
            if !self.inside {
                return None;
            }
            self.inside = false;
            return Some(PointerEvent {
                phase: PointerPhase::Leave,
                x: self.x,
                y: self.y,
            });
        }

        if !self.inside {
            self.inside = true;
            self.x = x;
            self.y = y;
            return Some(PointerEvent {
                phase: PointerPhase::Enter,
                x,
                y,
            });
        }

        if self.x == x && self.y == y {
            return None;
        }
        self.x = x;
        self.y = y;
        Some(PointerEvent {
            phase: PointerPhase::Move,
            x,
            y,
        })
    }

    /// Forget the previous hover state after tracking is paused or resumed.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

pub(super) struct PointerWindow {
    enabled: AtomicBool,
    state: Mutex<PointerState>,
    #[cfg(any(windows, test))]
    armed: AtomicBool,
    #[cfg(windows)]
    hwnd: std::sync::atomic::AtomicIsize,
}

impl PointerWindow {
    fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            state: Mutex::new(PointerState::default()),
            #[cfg(any(windows, test))]
            armed: AtomicBool::new(false),
            #[cfg(windows)]
            hwnd: std::sync::atomic::AtomicIsize::new(0),
        }
    }

    pub(super) fn set_enabled(&self, enabled: bool) {
        if self.enabled.swap(enabled, Ordering::SeqCst) != enabled {
            self.state.lock().unwrap().reset();
            #[cfg(any(windows, test))]
            self.armed.store(false, Ordering::SeqCst);
        }
    }

    pub(super) fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    #[cfg(any(windows, test))]
    pub(super) fn arm(&self) {
        if self.is_enabled() {
            self.armed.store(true, Ordering::SeqCst);
        }
    }

    #[cfg(any(windows, test))]
    pub(super) fn is_armed(&self) -> bool {
        self.armed.load(Ordering::SeqCst)
    }

    pub(super) fn record(&self, app: &AppHandle, label: &str, inside: bool, x: f64, y: f64) {
        if !self.is_enabled() {
            return;
        }
        let event = self.state.lock().unwrap().sample(inside, x, y);
        if let Some(event) = event {
            let _ = app.emit_to(label, EVENT_NAME, event);
        }
    }

    #[cfg(windows)]
    pub(super) fn set_hwnd(&self, hwnd: isize) {
        self.hwnd.store(hwnd, Ordering::SeqCst);
    }

    #[cfg(windows)]
    pub(super) fn hwnd(&self) -> isize {
        self.hwnd.load(Ordering::SeqCst)
    }
}

/// Per-application pointer tracker registry.
#[derive(Default)]
pub struct PointerHub {
    windows: Mutex<HashMap<String, Arc<PointerWindow>>>,
    #[cfg(windows)]
    worker: windows::Worker,
}

impl PointerHub {
    fn register(&self, label: &str) -> Arc<PointerWindow> {
        let pointer = Arc::new(PointerWindow::new());
        self.windows
            .lock()
            .unwrap()
            .insert(label.to_string(), pointer.clone());
        pointer
    }

    fn set_enabled(&self, label: &str, enabled: bool) -> bool {
        let pointer = self.windows.lock().unwrap().get(label).cloned();
        let Some(pointer) = pointer else {
            return false;
        };
        pointer.set_enabled(enabled);
        true
    }

    fn remove(&self, label: &str) {
        self.windows.lock().unwrap().remove(label);
    }

    #[cfg(windows)]
    fn enabled(&self) -> Vec<(String, Arc<PointerWindow>)> {
        self.windows
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, pointer)| pointer.is_enabled())
            .map(|(label, pointer)| (label.clone(), pointer.clone()))
            .collect()
    }

    #[cfg(windows)]
    fn has_enabled(&self) -> bool {
        self.windows
            .lock()
            .unwrap()
            .values()
            .any(|pointer| pointer.is_enabled())
    }
}

/// Register the pointer hub with the Tauri application.
pub fn start(app: &AppHandle) {
    if app.try_state::<PointerHub>().is_some() {
        return;
    }
    app.manage(PointerHub::default());
    #[cfg(windows)]
    windows::start(app.clone());
}

/// Install the platform tracker for a newly created widget window.
pub fn attach(app: &AppHandle, window: &WebviewWindow) -> Result<()> {
    let label = window.label().to_string();
    let pointer = app.state::<PointerHub>().register(&label);

    #[cfg(target_os = "macos")]
    macos::attach(app.clone(), window.clone(), label, pointer)?;
    #[cfg(all(unix, not(target_os = "macos")))]
    linux::attach(app.clone(), window.clone(), label, pointer)?;
    #[cfg(windows)]
    windows::attach(window, &pointer)?;

    Ok(())
}

/// Enable or pause delivery for one widget window.
pub fn set_enabled(app: &AppHandle, window: &WebviewWindow, enabled: bool) -> Result<()> {
    let label = window.label();
    let found = app.state::<PointerHub>().set_enabled(label, enabled);
    if enabled && !found {
        return Err(Error::Other(format!(
            "pointer tracker is unavailable for {label}"
        )));
    }
    #[cfg(windows)]
    windows::wake(app);
    Ok(())
}

/// Enforce the script permission before a widget enables pointer tracking.
pub fn require_script(store: &Store, id: &str) -> Result<()> {
    let meta = store.load_meta(id)?;
    if meta.grants(WidgetPermission::Script) {
        Ok(())
    } else {
        Err(Error::PermissionDenied(
            "widget does not declare permissions: [\"script\"]".to_string(),
        ))
    }
}

/// Remove all native and registry state for a closed widget window.
pub fn detach(app: &AppHandle, window: &WebviewWindow) {
    #[cfg(target_os = "macos")]
    macos::detach(window);
    app.state::<PointerHub>().remove(window.label());
    #[cfg(windows)]
    windows::wake(app);
}

#[cfg(test)]
mod tests {
    use super::PointerWindow;

    #[test]
    fn first_move_arming_is_per_widget_and_resets_with_subscription() {
        let first = PointerWindow::new();
        let second = PointerWindow::new();

        first.set_enabled(true);
        first.arm();
        second.set_enabled(true);

        assert!(first.is_armed());
        assert!(!second.is_armed());

        first.set_enabled(false);
        first.set_enabled(true);
        assert!(!first.is_armed());
    }
}
