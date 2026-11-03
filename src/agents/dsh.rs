use super::config::{backup_bytes, yaml_command_matches, McpTarget};
use super::launch::McpLaunch;
use crate::common::error::{Error, Result};
use crate::common::utils::atomic_write;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

/// Keep the home patch authoritative by removing profile duplicates first.
/// Restore profile patches if the home write fails.
pub fn set_mcp(
    target: &McpTarget,
    launch: &McpLaunch,
    enabled: bool,
    backup_root: &Path,
    agent: &str,
) -> Result<()> {
    let current = if target.path.exists() {
        Some(fs::read_to_string(&target.path)?)
    } else {
        None
    };
    let state = current
        .as_deref()
        .map(|text| state(text, launch))
        .transpose()?
        .flatten();
    let installed = state == Some(true);
    let duplicate_home_entry = enabled
        && installed
        && current
            .as_deref()
            .map(dsh_has_duplicate_aidget)
            .transpose()?
            .unwrap_or(false);

    let global_next = if enabled {
        (!installed || duplicate_home_entry)
            .then(|| edit_dsh(&target.path, current.as_deref(), launch, true))
            .transpose()?
    } else {
        state
            .is_some()
            .then(|| edit_dsh(&target.path, current.as_deref(), launch, false))
            .transpose()?
    };
    let global_change = global_next
        .as_deref()
        .is_some_and(|next| current.as_deref() != Some(next));

    let profile_changes = collect_dsh_profile_changes(target)?;
    if let Err(error) = apply_dsh_profile_changes(&profile_changes, backup_root) {
        return Err(error);
    }

    if global_change {
        let result = (|| -> Result<()> {
            if let Some(text) = &current {
                backup_bytes(backup_root, agent, &target.path, text.as_bytes())?;
            }
            if let Some(next) = &global_next {
                atomic_write(&target.path, next.as_bytes())?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            if let Err(rollback_error) = rollback_dsh_profile_changes(&profile_changes) {
                return Err(Error::Other(format!(
                    "{error}; profile rollback failed: {rollback_error}"
                )));
            }
            return Err(error);
        }
    }
    Ok(())
}

struct DshProfileChange {
    path: PathBuf,
    original: String,
    next: String,
}

fn collect_dsh_profile_changes(target: &McpTarget) -> Result<Vec<DshProfileChange>> {
    let Some(home) = target.path.parent() else {
        return Ok(Vec::new());
    };
    let profiles = home.join("profiles");
    if !profiles.is_dir() {
        return Ok(Vec::new());
    }
    let mut changes = Vec::new();
    for entry in fs::read_dir(&profiles)? {
        let entry = entry?;
        if !entry.path().is_dir() {
            continue;
        }
        let path = entry.path().join("cordis.patch.yml");
        if !path.is_file() {
            continue;
        }
        let original = fs::read_to_string(&path)?;
        let document: serde_yaml::Value = serde_yaml::from_str(&original)
            .map_err(|error| Error::InvalidParams(format!("{}: {error}", path.display())))?;
        let Some(entries) = document.as_sequence() else {
            if dsh_entry_has_aidget(&document) {
                return Err(Error::InvalidParams(format!(
                    "{}: DSH profile patch root must be a list",
                    path.display()
                )));
            }
            continue;
        };
        if !entries.iter().any(dsh_entry_has_aidget) {
            continue;
        }
        let next = edit_dsh(
            &path,
            Some(&original),
            &McpLaunch {
                command: String::new(),
                args: Vec::new(),
            },
            false,
        )?;
        if next != original {
            changes.push(DshProfileChange {
                path,
                original,
                next,
            });
        }
    }
    Ok(changes)
}

fn apply_dsh_profile_changes(changes: &[DshProfileChange], backup_root: &Path) -> Result<()> {
    let mut applied = 0;
    for change in changes {
        let label = change
            .path
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .map(|name| format!("dsh-{name}"))
            .unwrap_or_else(|| "dsh".to_string());
        let result = backup_bytes(
            backup_root,
            &label,
            &change.path,
            change.original.as_bytes(),
        )
        .and_then(|()| atomic_write(&change.path, change.next.as_bytes()));
        if let Err(error) = result {
            if let Err(rollback_error) = rollback_dsh_profile_changes(&changes[..applied]) {
                return Err(Error::Other(format!(
                    "{error}; profile rollback failed: {rollback_error}"
                )));
            }
            return Err(error);
        }
        applied += 1;
    }
    Ok(())
}

fn rollback_dsh_profile_changes(changes: &[DshProfileChange]) -> Result<()> {
    let mut rollback_error = None;
    for change in changes.iter().rev() {
        if let Err(error) = atomic_write(&change.path, change.original.as_bytes()) {
            rollback_error = Some(error);
        }
    }
    rollback_error.map_or(Ok(()), Err)
}

fn edit_dsh(
    path: &Path,
    current: Option<&str>,
    launch: &McpLaunch,
    enabled: bool,
) -> Result<String> {
    let text = current.unwrap_or("[]\n");
    let mut document: serde_yaml::Value = if text.trim().is_empty() {
        serde_yaml::Value::Sequence(Vec::new())
    } else {
        serde_yaml::from_str(text)
            .map_err(|error| Error::InvalidParams(format!("{}: {error}", path.display())))?
    };
    let list = document
        .as_sequence_mut()
        .ok_or_else(|| Error::InvalidParams("dsh patch root is not a list".to_string()))?;
    list.retain_mut(|entry| {
        if !remove_aidget_insert(entry) {
            return true;
        }
        !is_empty_after_aidget_removal(entry)
    });
    if enabled {
        list.push(aidget_dsh_insert(launch));
    }
    serde_yaml::to_string(&document)
        .map_err(|error| Error::InvalidParams(format!("{}: {error}", path.display())))
}

/// Remove every aidget item from one patch entry's `insert` list.
fn remove_aidget_insert(entry: &mut serde_yaml::Value) -> bool {
    let Some(mapping) = entry.as_mapping_mut() else {
        return false;
    };
    let Some(insert) = mapping
        .get_mut(serde_yaml::Value::String("insert".to_string()))
        .and_then(serde_yaml::Value::as_sequence_mut)
    else {
        return false;
    };
    let before = insert.len();
    insert.retain(|item| !is_aidget_dsh_item(item));
    before != insert.len()
}

fn is_empty_after_aidget_removal(entry: &serde_yaml::Value) -> bool {
    entry.as_mapping().is_some_and(|mapping| {
        mapping.iter().all(|(key, value)| {
            key.as_str() == Some("insert") && value.as_sequence().is_some_and(Vec::is_empty)
        })
    })
}

fn is_aidget_dsh_item(item: &serde_yaml::Value) -> bool {
    item.get("id").and_then(serde_yaml::Value::as_str) == Some("aidget-mcp")
}

fn dsh_entry_has_aidget(entry: &serde_yaml::Value) -> bool {
    entry
        .get("insert")
        .and_then(serde_yaml::Value::as_sequence)
        .is_some_and(|items| items.iter().any(is_aidget_dsh_item))
}

fn dsh_has_duplicate_aidget(text: &str) -> Result<bool> {
    let document: serde_yaml::Value = if text.trim().is_empty() {
        serde_yaml::Value::Sequence(Vec::new())
    } else {
        serde_yaml::from_str(text)
            .map_err(|error| Error::InvalidParams(format!("dsh patch: {error}")))?
    };
    let Some(entries) = document.as_sequence() else {
        return Err(Error::InvalidParams(
            "dsh patch root is not a list".to_string(),
        ));
    };
    Ok(entries
        .iter()
        .filter_map(|entry| entry.get("insert").and_then(serde_yaml::Value::as_sequence))
        .flat_map(|items| items.iter())
        .filter(|item| is_aidget_dsh_item(item))
        .count()
        > 1)
}

/// Read whether all named aidget inserts match this launch command.
pub fn state(text: &str, launch: &McpLaunch) -> Result<Option<bool>> {
    let document: serde_yaml::Value = if text.trim().is_empty() {
        serde_yaml::Value::Sequence(Vec::new())
    } else {
        serde_yaml::from_str(text)
            .map_err(|error| Error::InvalidParams(format!("dsh patch: {error}")))?
    };
    let Some(entries) = document.as_sequence() else {
        return Err(Error::InvalidParams(
            "dsh patch root is not a list".to_string(),
        ));
    };
    let mut found = false;
    for entry in entries {
        let Some(items) = entry.get("insert").and_then(serde_yaml::Value::as_sequence) else {
            continue;
        };
        for item in items {
            if !is_aidget_dsh_item(item) {
                continue;
            }
            found = true;
            if !item
                .get("config")
                .is_some_and(|config| yaml_command_matches(config, launch))
            {
                return Ok(Some(false));
            }
        }
    }
    Ok(found.then_some(true))
}

fn aidget_dsh_insert(launch: &McpLaunch) -> serde_yaml::Value {
    serde_yaml::to_value(json!({
        "insert": [{
            "id": "aidget-mcp",
            "name": "@deepseek-ai/dsh-mcp-client",
            "config": {
                "serverName": "aidget",
                "transport": "stdio",
                "command": launch.command,
                "args": launch.args
            }
        }]
    }))
    .expect("dsh MCP insert is valid YAML data")
}
