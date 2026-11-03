//! Per-widget persistent string storage exposed through `window.aidget.storage`.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde::Serialize;

use crate::common::error::{Error, Result};
use crate::common::utils::{atomic_write, parse_json};
use crate::widgets::model::{validate_id, WidgetPermission, MAX_PAYLOAD};
use crate::widgets::store::Store;

const STORAGE_DIR: &str = "storage";

/// Serializable failure returned to a widget without losing the error code.
#[derive(Debug, Serialize)]
pub struct StorageFailure {
    pub code: &'static str,
    pub message: String,
}

impl From<&Error> for StorageFailure {
    fn from(error: &Error) -> Self {
        StorageFailure {
            code: error.code(),
            message: error.to_string(),
        }
    }
}

/// Enforce the capability declaration before every storage request.
pub fn authorize(store: &Store, id: &str) -> Result<()> {
    let meta = store.load_meta(id)?;
    if meta.grants(WidgetPermission::Storage) {
        return Ok(());
    }
    Err(Error::PermissionDenied(
        "widget does not declare permissions: [\"storage\"]".to_string(),
    ))
}

/// Read one key from a widget's namespace.
pub fn get(store: &Store, id: &str, key: &str) -> Result<Option<String>> {
    validate_key(key)?;
    Ok(load(store, id)?.remove(key))
}

/// Set one string value and atomically persist the namespace.
pub fn set(store: &Store, id: &str, key: &str, value: &str) -> Result<()> {
    validate_key(key)?;
    let mut values = load(store, id)?;
    values.insert(key.to_string(), value.to_string());
    save(store, id, &values)
}

/// Remove one key, leaving an empty namespace file when no values remain.
pub fn remove(store: &Store, id: &str, key: &str) -> Result<()> {
    validate_key(key)?;
    let mut values = load(store, id)?;
    if values.remove(key).is_some() {
        save(store, id, &values)
    } else {
        Ok(())
    }
}

/// Delete every value owned by a widget.
pub fn clear(store: &Store, id: &str) -> Result<()> {
    match fs::remove_file(storage_path(store, id)?) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn load(store: &Store, id: &str) -> Result<BTreeMap<String, String>> {
    let path = storage_path(store, id)?;
    match fs::read(path) {
        Ok(bytes) => Ok(parse_json(&bytes)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(error) => Err(error.into()),
    }
}

fn save(store: &Store, id: &str, values: &BTreeMap<String, String>) -> Result<()> {
    let bytes = serde_json::to_vec(values)?;
    if bytes.len() > MAX_PAYLOAD {
        return Err(Error::InvalidParams(
            "widget storage exceeds 256 KB".to_string(),
        ));
    }
    atomic_write(&storage_path(store, id)?, &bytes)
}

fn storage_path(store: &Store, id: &str) -> Result<PathBuf> {
    validate_id(id)?;
    Ok(store
        .state_dir()
        .join(STORAGE_DIR)
        .join(format!("{id}.json")))
}

fn validate_key(key: &str) -> Result<()> {
    if key.is_empty() {
        return Err(Error::InvalidParams(
            "storage key must not be empty".to_string(),
        ));
    }
    Ok(())
}
