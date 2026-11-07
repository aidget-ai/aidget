use std::sync::mpsc::{self, RecvTimeoutError, SyncSender};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::Duration;

use tauri::{Manager, WebviewWindow};
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    GetAncestor, GetClientRect, GetCursorPos, WindowFromPoint, GA_ROOT,
};

use super::{PointerHub, PointerWindow};
use crate::common::error::{Error, Result};

#[derive(Default)]
pub(super) struct Worker {
    sender: OnceLock<SyncSender<()>>,
}

impl Worker {
    fn wake(&self) {
        if let Some(sender) = self.sender.get() {
            let _ = sender.try_send(());
        }
    }
}

pub(super) fn start(app: tauri::AppHandle) {
    let (sender, receiver) = mpsc::sync_channel(1);
    let hub = app.state::<PointerHub>();
    if hub.worker.sender.set(sender).is_err() {
        return;
    }
    thread::spawn(move || run(app, receiver));
}

pub(super) fn wake(app: &tauri::AppHandle) {
    app.state::<PointerHub>().worker.wake();
}

pub(super) fn attach(window: &WebviewWindow, pointer: &Arc<PointerWindow>) -> Result<()> {
    let hwnd = window
        .hwnd()
        .map_err(|error| Error::Other(error.to_string()))?;
    pointer.set_hwnd(hwnd.0 as isize);
    Ok(())
}

fn run(app: tauri::AppHandle, receiver: std::sync::mpsc::Receiver<()>) {
    let mut last_cursor = None;
    loop {
        if !app.state::<PointerHub>().has_enabled() {
            last_cursor = None;
            if receiver.recv().is_err() {
                return;
            }
            continue;
        }

        let mut cursor = POINT { x: 0, y: 0 };
        if unsafe { GetCursorPos(&mut cursor) }.is_ok() {
            let moved = last_cursor.is_some() && last_cursor != Some(cursor);
            poll(&app, cursor, moved);
            last_cursor = Some(cursor);
        }
        match receiver.recv_timeout(Duration::from_millis(16)) {
            Ok(()) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

fn poll(app: &tauri::AppHandle, cursor: POINT, arm: bool) {
    let pointers = app.state::<PointerHub>().enabled();
    let top = unsafe { GetAncestor(WindowFromPoint(cursor), GA_ROOT) };

    for (label, pointer) in pointers {
        if arm {
            pointer.arm();
        }
        if !pointer.is_armed() {
            continue;
        }
        let hwnd = HWND(pointer.hwnd() as *mut _);
        if hwnd.0.is_null() || top.0 != hwnd.0 {
            pointer.record(app, &label, false, 0.0, 0.0);
            continue;
        }

        let mut client = cursor;
        let mut bounds = RECT::default();
        if !unsafe { ScreenToClient(hwnd, &mut client) }.as_bool()
            || unsafe { GetClientRect(hwnd, &mut bounds) }.is_err()
        {
            pointer.record(app, &label, false, 0.0, 0.0);
            continue;
        }

        let scale = (unsafe { GetDpiForWindow(hwnd) }.max(96) as f64) / 96.0;
        let width = bounds.right as f64 / scale;
        let height = bounds.bottom as f64 / scale;
        let x = (client.x as f64 / scale).clamp(0.0, width);
        let y = (client.y as f64 / scale).clamp(0.0, height);
        pointer.record(app, &label, true, x, y);
    }
}
