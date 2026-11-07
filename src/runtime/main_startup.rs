use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use crate::common::error::Result;

const REVEAL_PENDING: u8 = 0;
const REVEAL_IN_PROGRESS: u8 = 1;
const REVEAL_COMPLETE: u8 = 2;

/// Window operations needed to stage the main window without exposing its
/// startup frame.
pub trait StartupWindow {
    fn show_for_startup(&self) -> Result<()>;
    fn ignore_cursor_events(&self, ignore: bool) -> Result<()>;
}

/// Stage cursor passthrough before the first paint. GTK cannot receive this
/// operation until its native window has been mapped, so Linux defers it.
pub fn prepare_startup_cursor(window: &impl StartupWindow) -> Result<()> {
    #[cfg(not(target_os = "linux"))]
    window.ignore_cursor_events(true)?;
    Ok(())
}

/// Show the staging frame and complete platform-specific cursor setup.
pub fn prime_startup_window(window: &impl StartupWindow) -> Result<()> {
    window.show_for_startup()?;
    // GTK only realizes its GdkWindow after it is shown. tao 0.37 unwraps the
    // missing GdkWindow when cursor passthrough is enabled before this point.
    #[cfg(target_os = "linux")]
    window.ignore_cursor_events(true)?;
    Ok(())
}

/// One-shot state shared by the main window and the background widget host.
#[derive(Default)]
pub struct MainStartup {
    page_loaded: AtomicBool,
    main_painted: AtomicBool,
    reveal_state: AtomicU8,
    widgets_started: AtomicBool,
    widgets_announced: AtomicBool,
}

impl MainStartup {
    /// Mark a newly created main window as waiting to be revealed.
    pub fn reset_reveal(&self) {
        self.page_loaded.store(false, Ordering::SeqCst);
        self.main_painted.store(false, Ordering::SeqCst);
        self.reveal_state.store(REVEAL_PENDING, Ordering::SeqCst);
    }

    /// Record that the main document has finished loading.
    pub fn mark_page_loaded(&self) {
        self.page_loaded.store(true, Ordering::SeqCst);
    }

    /// Record that the main page has completed its first painted frame.
    pub fn mark_main_painted(&self) {
        self.main_painted.store(true, Ordering::SeqCst);
    }

    /// Attempt the first reveal once both readiness signals arrive; failures can be retried.
    pub fn try_reveal<E>(
        &self,
        reveal: impl FnOnce() -> std::result::Result<(), E>,
    ) -> std::result::Result<bool, E> {
        if !self.page_loaded.load(Ordering::SeqCst)
            || !self.main_painted.load(Ordering::SeqCst)
            || self
                .reveal_state
                .compare_exchange(
                    REVEAL_PENDING,
                    REVEAL_IN_PROGRESS,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                )
                .is_err()
        {
            return Ok(false);
        }
        if let Err(error) = reveal() {
            self.reveal_state.store(REVEAL_PENDING, Ordering::SeqCst);
            return Err(error);
        }
        self.reveal_state.store(REVEAL_COMPLETE, Ordering::SeqCst);
        Ok(true)
    }

    /// Whether the main window has already been shown.
    pub fn is_revealed(&self) -> bool {
        self.reveal_state.load(Ordering::SeqCst) == REVEAL_COMPLETE
    }

    /// Claim the single initial materialization of desktop widget windows.
    pub fn claim_widgets_start(&self) -> bool {
        !self.widgets_started.swap(true, Ordering::SeqCst)
    }

    /// Whether the initial widget materialization has been claimed.
    pub fn widgets_started(&self) -> bool {
        self.widgets_started.load(Ordering::SeqCst)
    }

    /// Release a failed initial start so the caller can surface the error and
    /// allow a later attempt.
    pub fn reset_widgets_start(&self) {
        self.widgets_started.store(false, Ordering::SeqCst);
    }

    /// Claim the one event that tells the main page widgets are available.
    pub fn claim_widgets_announcement(&self) -> bool {
        !self.widgets_announced.swap(true, Ordering::SeqCst)
    }
}
