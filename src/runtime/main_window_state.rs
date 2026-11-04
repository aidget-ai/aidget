//! Persistent widget preview cache, independent of desktop enablement.
use std::fs;
use std::path::{Path, PathBuf};

use crate::common::error::Result;
use crate::common::utils::atomic_write;
use crate::widgets::model::validate_id;

/// Remove the cached image after the widget enters trash.
pub fn forget_widget(root: &Path, id: &str) -> Result<()> {
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
