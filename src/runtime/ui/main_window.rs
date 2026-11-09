//! Main webview window lifecycle and main-page events.
#[cfg(target_os = "macos")]
use objc2_app_kit::NSWindow;
use tauri::{
    webview::PageLoadEvent, AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};

use crate::common::error::{Error, Result};
use crate::runtime::main_startup::{prepare_startup_cursor, prime_startup_window, StartupWindow};
use crate::transport::origin;

use super::main_page::main_payload;
use super::state::{require_main_window_label, UiState, MAIN_LABEL};

#[cfg(not(target_os = "macos"))]
const MAIN_STARTUP_POSITION: f64 = -10_000.0;

impl StartupWindow for WebviewWindow {
    fn show_for_startup(&self) -> Result<()> {
        self.show()
            .map_err(|error| Error::Other(format!("cannot prime main window: {error}")))
    }

    fn ignore_cursor_events(&self, ignore: bool) -> Result<()> {
        self.set_ignore_cursor_events(ignore)
            .map_err(|error| Error::Other(format!("cannot ignore main window cursor: {error}")))
    }
}

/// Queue creation or focus of the main window on the Tauri main thread.
pub fn show_main(app: &AppHandle) -> Result<()> {
    // The bridge calls this from a worker thread; window creation must run on
    // the main thread. Queue it instead of blocking the request.
    let handle = app.clone();
    app.run_on_main_thread(move || {
        if let Err(error) = show_main_now(&handle) {
            eprintln!("aidget: cannot open the main window: {error}");
        }
    })
    .map_err(|error| Error::Other(error.to_string()))?;
    Ok(())
}

/// Show the main window. Must be called on the main thread.
pub fn show_main_now(app: &AppHandle) -> Result<()> {
    if let Some(window) = app.get_webview_window(MAIN_LABEL) {
        if app.state::<UiState>().startup.is_revealed() {
            reveal_main(app, &window)?;
        } else {
            reveal_main_if_ready(app, &window)?;
        }
        return Ok(());
    }

    app.state::<UiState>().startup.reset_reveal();
    let handle_for_load = app.clone();
    let builder = WebviewWindowBuilder::new(
        app,
        MAIN_LABEL,
        WebviewUrl::External(
            origin::main_window_url()
                .parse()
                .map_err(|error| Error::Other(format!("invalid main url: {error}")))?,
        ),
    )
    .title("aidget")
    .inner_size(1080.0, 720.0)
    .min_inner_size(860.0, 560.0)
    .center()
    .visible(false)
    .focused(false)
    .on_page_load(move |window, payload| {
        if matches!(payload.event(), PageLoadEvent::Finished) {
            handle_for_load
                .state::<UiState>()
                .startup
                .mark_page_loaded();
            if let Err(error) = prime_main(&window) {
                eprintln!("aidget: cannot prime the main window: {error}");
            }
            if let Err(error) = reveal_main_if_ready(&handle_for_load, &window) {
                eprintln!("aidget: cannot reveal the main window: {error}");
            }
        }
    });
    #[cfg(target_os = "macos")]
    let builder = builder
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .traffic_light_position(tauri::LogicalPosition::new(24.0, 26.0))
        .hidden_title(true);
    #[cfg(any(target_os = "linux", windows))]
    let builder = builder.decorations(false);
    let window = builder.build()?;
    prepare_main(&window)?;

    let handle = app.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            if let Some(window) = handle.get_webview_window(MAIN_LABEL) {
                hide_main(&handle, &window);
            }
        }
    });
    Ok(())
}

fn reveal_main(app: &AppHandle, window: &WebviewWindow) -> Result<()> {
    // Widget-only starts stay out of the Dock; opening the main window makes
    // aidget behave like a regular app so the window can take focus.
    #[cfg(target_os = "macos")]
    app.set_activation_policy(tauri::ActivationPolicy::Regular)?;
    window.set_ignore_cursor_events(false)?;
    window.show()?;
    window.unminimize()?;
    window.set_focus()?;
    emit_main(app);
    Ok(())
}

#[cfg(target_os = "macos")]
fn set_main_alpha(window: &WebviewWindow, alpha: f64) -> Result<()> {
    window
        .with_webview(move |webview| unsafe {
            let window: &NSWindow = &*webview.ns_window().cast();
            window.setAlphaValue(alpha);
        })
        .map_err(|error| Error::Other(format!("cannot set main window opacity: {error}")))
}

fn prepare_main(window: &WebviewWindow) -> Result<()> {
    // A hidden WebView does not run animation frames. Prime it without visible
    // content: transparent in place on macOS, offscreen where opacity is absent.
    prepare_startup_cursor(window)?;
    #[cfg(target_os = "macos")]
    set_main_alpha(window, 0.0)?;
    #[cfg(not(target_os = "macos"))]
    window
        .set_position(tauri::LogicalPosition::new(
            MAIN_STARTUP_POSITION,
            MAIN_STARTUP_POSITION,
        ))
        .map_err(|error| Error::Other(format!("cannot stage main window: {error}")))?;
    Ok(())
}

fn prime_main(window: &WebviewWindow) -> Result<()> {
    prime_startup_window(window)
}

fn reveal_main_if_ready(app: &AppHandle, window: &WebviewWindow) -> Result<()> {
    app.state::<UiState>()
        .startup
        .try_reveal(|| -> Result<()> {
            #[cfg(target_os = "macos")]
            set_main_alpha(window, 1.0)?;
            #[cfg(target_os = "windows")]
            {
                let monitor = window
                    .primary_monitor()?
                    .ok_or_else(|| Error::Other("cannot find the primary monitor".into()))?;
                // The staged window may have a different DPI than the primary monitor.
                let position = super::geometry::centered_position(
                    monitor.work_area(),
                    window.outer_size()?,
                    window.scale_factor()?,
                    monitor.scale_factor(),
                );
                window.set_position(position)?;
            }
            #[cfg(target_os = "linux")]
            window.center()?;
            reveal_main(app, window)?;
            Ok(())
        })?;
    Ok(())
}

/// Reveal the main window only after the page has loaded and painted its first frame.
pub fn mark_main_painted(app: &AppHandle, window: &WebviewWindow) -> Result<()> {
    require_main_window_label(window.label())?;
    app.state::<UiState>().startup.mark_main_painted();
    reveal_main_if_ready(app, window)
}

fn hide_main(app: &AppHandle, window: &WebviewWindow) {
    let _ = window.hide();
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

pub(super) fn emit_main(app: &AppHandle) {
    if app.get_webview_window(MAIN_LABEL).is_none() {
        return;
    }
    if let Ok(state) = main_payload(app) {
        let _ = app.emit_to(MAIN_LABEL, "aidget://main", &state);
    }
}
