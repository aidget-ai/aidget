//! Shared UI runtime state and window identity helpers.
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::Instant;

use tauri::WebviewWindow;

use crate::common::error::{Error, Result};
use crate::runtime::main_startup::MainStartup;

/// Window label reserved for the main application page.
pub const MAIN_LABEL: &str = "main";

/// A native drag of one widget window, from the press until the release.
#[derive(Debug, Clone, PartialEq)]
pub struct DragGesture {
    /// Label of the dragged window; moves of other windows are not part of it.
    pub window: String,
    pub started: Instant,
    /// Where the window really was at the press.
    pub origin: (i32, i32),
}

/// Runtime state shared by every Tauri window and UI command.
#[derive(Default)]
pub struct UiState {
    pub allow_incompatible_widgets: bool,
    pub windows: Mutex<HashMap<String, ()>>,
    pub signatures: Mutex<HashMap<String, String>>,
    /// Last revision whose iframe reported `load`, so a screenshot can wait
    /// for the document the caller just wrote instead of the previous one.
    pub loaded: Mutex<HashMap<String, u64>>,
    /// Last revision whose iframe called `window.aidget.ready()`.
    pub ready: Mutex<HashMap<String, u64>>,
    pub applying: Mutex<bool>,
    pub dragging: Mutex<Option<DragGesture>>,
    pub topology: Mutex<String>,
    pub board_revision: Mutex<u64>,
    pub root: Mutex<std::path::PathBuf>,
    /// Serialize language initialization with all other preference writes.
    pub preferences: Mutex<()>,
    pub storage_lock: Mutex<()>,
    pub startup: MainStartup,
    pub(crate) diagnostics: Mutex<super::diagnostics::Diagnostics>,
}

pub(super) fn label(id: &str) -> String {
    format!("widget-{id}")
}

/// The window label is the only authority on which widget is asking, so a page
/// cannot borrow another widget's permissions.
pub fn widget_id(window: &WebviewWindow) -> Result<String> {
    window
        .label()
        .strip_prefix("widget-")
        .map(str::to_string)
        .ok_or_else(|| Error::InvalidParams("not a widget window".to_string()))
}

/// Reject commands that are only valid for the main page.
pub fn require_main_window_label(label: &str) -> Result<()> {
    if label == MAIN_LABEL {
        Ok(())
    } else {
        Err(Error::InvalidParams("main window only".to_string()))
    }
}

/// Drop load and ready receipts for closed widgets so a recreated window cannot
/// satisfy the next screenshot wait with state from its predecessor.
pub fn retain_widget_revisions(
    loaded: &mut HashMap<String, u64>,
    ready: &mut HashMap<String, u64>,
    active: &HashSet<String>,
) {
    loaded.retain(|id, _| active.contains(id));
    ready.retain(|id, _| active.contains(id));
}

/// Raise the receipt for `id` to `revision`, never letting it move backwards.
pub fn record_revision(revisions: &mut HashMap<String, u64>, id: &str, revision: u64) {
    let current = revisions.entry(id.to_string()).or_default();
    *current = (*current).max(revision);
}
