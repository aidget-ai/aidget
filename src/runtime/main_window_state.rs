use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::common::error::Result;
use crate::common::utils::{atomic_write, parse_json};
use crate::widgets::model::validate_id;

const WIDGET_STATE_FILE: &str = "widgets.json";

/// The persisted state of one widget. Archived widgets keep their board frame
/// and cached preview but have no desktop window.
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WidgetStateRecord {
    pub archived: bool,
}

/// Path of the file that records widget state, one entry per live widget.
pub fn state_path(root: &Path) -> PathBuf {
    root.join("state").join(WIDGET_STATE_FILE)
}

/// Read every persisted record, rejecting malformed ids and unknown fields.
fn load_state(root: &Path) -> Result<BTreeMap<String, WidgetStateRecord>> {
    let path = state_path(root);
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let state: BTreeMap<String, WidgetStateRecord> = parse_json(&fs::read(path)?)?;
    for id in state.keys() {
        validate_id(id)?;
    }
    Ok(state)
}

/// Archived ids among `ids`, the widget directories on disk. A widget without
/// a record yet is treated as not archived; stale records are ignored.
pub fn archived_widgets(root: &Path, ids: &[String]) -> Result<HashSet<String>> {
    let state = load_state(root)?;
    Ok(ids
        .iter()
        .filter(|id| state.get(*id).is_some_and(|record| record.archived))
        .cloned()
        .collect())
}

/// Change whether a widget window should be present while retaining its board
/// frame. Rewrites one record per widget in `ids`, dropping removed widgets.
pub fn set_archived(root: &Path, ids: &[String], id: &str, archived: bool) -> Result<()> {
    validate_id(id)?;
    for known in ids {
        validate_id(known)?;
    }
    let previous = load_state(root)?;
    let mut state = BTreeMap::new();
    for known in ids {
        let record = if known == id {
            WidgetStateRecord { archived }
        } else {
            previous
                .get(known)
                .copied()
                .unwrap_or(WidgetStateRecord { archived: false })
        };
        state.insert(known.clone(), record);
    }
    let bytes = serde_json::to_vec_pretty(&state)?;
    atomic_write(&state_path(root), &bytes)
}

/// Remove the state record and cached image after the widget enters trash.
pub fn forget_widget(root: &Path, ids: &[String], id: &str) -> Result<()> {
    let remaining: Vec<String> = ids.iter().filter(|known| *known != id).cloned().collect();
    set_archived(root, &remaining, id, false)?;
    let path = preview_path(root, id)?;
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

/// Resolve the persistent static-preview path for one widget.
pub fn preview_path(root: &Path, id: &str) -> Result<PathBuf> {
    validate_id(id)?;
    Ok(root
        .join("state")
        .join("previews")
        .join(format!("{id}.png")))
}

/// Atomically replace the cached static preview.
pub fn save_preview(root: &Path, id: &str, png: &[u8]) -> Result<()> {
    atomic_write(&preview_path(root, id)?, png)
}

/// Read a cached static preview when the widget has not been captured yet.
pub fn load_preview(root: &Path, id: &str) -> Result<Option<Vec<u8>>> {
    let path = preview_path(root, id)?;
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}
