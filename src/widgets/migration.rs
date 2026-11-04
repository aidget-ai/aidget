//! One-time conversion of archived window state into required widget metadata.
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::common::error::{Error, Result};
use crate::common::utils::{atomic_write, parse_json};
use crate::widgets::model::{validate_id, WidgetMeta};
use crate::widgets::store::{Store, CONFIG_FILE};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArchivedState {
    archived: bool,
}

/// Migrate installed and trashed metadata before serving any business reads.
/// Each completed file is resumable; the archive map is retained until all succeed.
pub fn migrate_enabled(store: &Store) -> Result<()> {
    let _lock = store.write_lock()?;
    let legacy_path = store.state_dir().join("widgets.json");
    let migration_dir = store.state_dir().join("migrations/enabled");
    let backup = migration_dir.join("widgets.json");
    let complete = migration_dir.join("complete");
    if complete.try_exists()? {
        if legacy_path.try_exists()? {
            return Err(Error::InvalidParams(
                "legacy widget state reappeared after enabled migration; stop older Aidget processes".into(),
            ));
        }
        // Current corrupt widgets belong to normal diagnostics, not upgrade recovery.
        return Ok(());
    }
    let archived: BTreeMap<String, ArchivedState> = match fs::read(&legacy_path) {
        Ok(bytes) => {
            if backup.try_exists()? {
                return Err(Error::InvalidParams(
                    "legacy widget state reappeared after enabled migration; stop older Aidget processes".into(),
                ));
            }
            parse_json(&bytes)?
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
        Err(error) => return Err(error.into()),
    };
    for id in archived.keys() {
        validate_id(id)?;
    }
    let board = store.load_board()?;
    for id in store.widget_ids()? {
        let enabled = board.widgets.contains_key(&id)
            && !archived.get(&id).is_some_and(|state| state.archived);
        migrate_meta(
            &store.widget_dir(&id)?.join(CONFIG_FILE),
            Some(&id),
            enabled,
        )?;
    }
    for entry in fs::read_dir(store.trash_dir())? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            migrate_meta(&entry.path().join(CONFIG_FILE), None, true)?;
        }
    }
    if legacy_path.try_exists()? {
        fs::create_dir_all(&migration_dir)?;
        fs::rename(legacy_path, backup)?;
    }
    atomic_write(&complete, b"")
}

fn migrate_meta(path: &Path, expected_id: Option<&str>, enabled: bool) -> Result<()> {
    let mut value: serde_json::Value = parse_json(&fs::read(path)?)?;
    let fields = value.as_object_mut().ok_or_else(|| {
        Error::InvalidParams(format!("{} must contain a widget object", path.display()))
    })?;
    let missing = !fields.contains_key("enabled");
    if missing {
        fields.insert("enabled".into(), enabled.into());
    }
    let meta: WidgetMeta = serde_json::from_value(value.clone())?;
    validate_id(&meta.id)?;
    meta.config.validate()?;
    if expected_id.is_some_and(|id| id != meta.id) {
        return Err(Error::InvalidParams(format!(
            "{} id does not match widget directory",
            path.display()
        )));
    }
    if missing {
        atomic_write(path, &serde_json::to_vec_pretty(&value)?)?;
    }
    Ok(())
}
