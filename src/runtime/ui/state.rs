//! Shared UI runtime state and window identity helpers.
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::Instant;

use tauri::WebviewWindow;

use crate::common::error::{Error, Result};
use crate::runtime::main_startup::MainStartup;

/// Window label reserved for the main application page.
pub const MAIN_LABEL: &str = "main";

/// Runtime state shared by every Tauri window and UI command.
#[derive(Default)]
pub struct UiState {
    pub windows: Mutex<HashMap<String, ()>>,
    pub signatures: Mutex<HashMap<String, String>>,
    /// Last revision whose iframe reported `load`, so a screenshot can wait
    /// for the document the caller just wrote instead of the previous one.
    pub loaded: Mutex<HashMap<String, u64>>,
    /// Last revision whose iframe called `window.aidget.ready()`.
    pub ready: Mutex<HashMap<String, u64>>,
    pub applying: Mutex<bool>,
    pub dragging: Mutex<Option<Instant>>,
    pub topology: Mutex<String>,
    pub board_revision: Mutex<u64>,
    pub root: Mutex<std::path::PathBuf>,
    /// Serialize language initialization with all other preference writes.
    pub preferences: Mutex<()>,
    pub storage_lock: Mutex<()>,
    pub startup: MainStartup,
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
