use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::agents::config::AgentResourceState;
use crate::common::error::{Error, Result};
use crate::common::utils::{atomic_write, sha256_hex};
use crate::widgets::store::Store;

/// The distributable Agent Skill embedded in this binary.
pub const SKILL_CONTENT: &str = include_str!("../../skills/aidget/SKILL.md");

const MARKER_FILE: &str = ".aidget-skill";

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CopyMarker {
    source: String,
    tree_hash: String,
}

/// Materialize the embedded Skill under the active aidget data directory.
pub fn ensure_skill(store: &Store) -> Result<PathBuf> {
    let directory = store.skills_dir().join("aidget");
    let path = directory.join("SKILL.md");
    if fs::read_to_string(&path).ok().as_deref() != Some(SKILL_CONTENT) {
        atomic_write(&path, SKILL_CONTENT.as_bytes())?;
    }
    Ok(directory)
}

/// Read whether one Agent path is the source Skill link or a managed copy.
pub fn skill_state(path: &Path, source: &Path) -> Result<AgentResourceState> {
    let path_text = path.to_string_lossy().into_owned();
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(AgentResourceState {
                path: path_text,
                installed: false,
                conflict: None,
            });
        }
        Err(error) => return Err(error.into()),
    };
    if is_link(path, &metadata)? {
        return Ok(if link_points_to(path, source)? {
            AgentResourceState {
                path: path_text,
                installed: true,
                conflict: None,
            }
        } else {
            AgentResourceState {
                path: path_text,
                installed: false,
                conflict: Some(format!(
                    "the Skill link does not point to {}",
                    source.display()
                )),
            }
        });
    }
    if metadata.is_dir() && managed_copy_is_current(path, source)? {
        return Ok(AgentResourceState {
            path: path_text,
            installed: true,
            conflict: None,
        });
    }
    Ok(AgentResourceState {
        path: path_text,
        installed: false,
        conflict: Some(format!(
            "the Skill entry is not linked to {}",
            source.display()
        )),
    })
}

/// Install or remove a Skill link, replacing any existing entry without a backup.
pub fn set_skill(path: &Path, source: &Path, enabled: bool) -> Result<()> {
    let exists = match fs::symlink_metadata(path) {
        Ok(_) => true,
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.into()),
    };
    if enabled {
        if exists && entry_is_current(path, source)? {
            return Ok(());
        }
        return replace_entry(path, source);
    }
    if !exists {
        return Ok(());
    }
    if !entry_is_current(path, source)? {
        return Err(Error::InvalidParams(format!(
            "{} is not a Skill entry managed by aidget",
            path.display()
        )));
    }
    remove_entry(path)
}

/// Hash every file and directory in a Skill tree for managed-copy freshness.
pub fn skill_tree_hash(path: &Path) -> Result<String> {
    tree_hash(path, false)
}

fn entry_is_current(path: &Path, source: &Path) -> Result<bool> {
    let metadata = fs::symlink_metadata(path)?;
    if is_link(path, &metadata)? {
        return link_points_to(path, source);
    }
    Ok(metadata.is_dir() && managed_copy_is_current(path, source)?)
}

/// Automatic migration preserves links and copies owned by another aidget home,
/// while allowing this instance to refresh a stale copy that it created.
pub(crate) fn is_automatic_migration_candidate(path: &Path, source: &Path) -> Result<bool> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    if is_link(path, &metadata)? || !metadata.is_dir() {
        return Ok(false);
    }
    let marker = match fs::read_to_string(path.join(MARKER_FILE)) {
        Ok(text) => serde_json::from_str::<CopyMarker>(&text)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(true),
        Err(error) => return Err(error.into()),
    };
    Ok(marker.source == fs::canonicalize(source)?.to_string_lossy())
}

fn managed_copy_is_current(path: &Path, source: &Path) -> Result<bool> {
    let marker = match fs::read_to_string(path.join(MARKER_FILE)) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    let marker = match serde_json::from_str::<CopyMarker>(&marker) {
        Ok(marker) => marker,
        Err(_) => return Ok(false),
    };
    let source = fs::canonicalize(source)?;
    Ok(marker.source == source.to_string_lossy()
        && marker.tree_hash == tree_hash(&source, false)?
        && marker.tree_hash == tree_hash(path, true)?)
}

fn replace_entry(path: &Path, source: &Path) -> Result<()> {
    let source = fs::canonicalize(source)?;
    if fs::canonicalize(path).is_ok_and(|path| path == source) {
        return Err(Error::InvalidParams(
            "the Skill source and destination are the same directory".to_string(),
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| Error::Other("Skill path has no parent directory".to_string()))?;
    fs::create_dir_all(parent)?;
    let staged = sibling_path(path, "new");
    let previous = sibling_path(path, "old");
    // Build the replacement beside the destination so a failed swap can put
    // the previous Skill entry back without leaving the Agent without one.
    create_entry(&staged, &source)?;

    let had_previous = match fs::symlink_metadata(path) {
        Ok(_) => true,
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(error) => {
            let _ = remove_entry(&staged);
            return Err(error.into());
        }
    };
    if had_previous {
        if let Err(error) = fs::rename(path, &previous) {
            let _ = remove_entry(&staged);
            return Err(error.into());
        }
    }
    if let Err(error) = fs::rename(&staged, path) {
        let _ = remove_entry(&staged);
        if had_previous {
            let _ = fs::rename(&previous, path);
        }
        return Err(error.into());
    }
    if had_previous {
        remove_entry(&previous)?;
    }
    Ok(())
}

fn sibling_path(path: &Path, purpose: &str) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("aidget");
    path.with_file_name(format!(".{name}.aidget-{purpose}-{}", uuid::Uuid::new_v4()))
}

#[cfg(unix)]
fn create_entry(path: &Path, source: &Path) -> Result<()> {
    std::os::unix::fs::symlink(source, path)?;
    Ok(())
}

#[cfg(windows)]
fn create_entry(path: &Path, source: &Path) -> Result<()> {
    // Match Magpie: a privileged symlink first, a junction when Windows
    // refuses the privilege, then a marked copy when neither link is possible.
    let link_error = match std::os::windows::fs::symlink_dir(source, path) {
        Ok(()) => match verify_created_link(path) {
            Ok(()) => return Ok(()),
            Err(error) if error.raw_os_error() == Some(1314) => error,
            Err(error) => {
                let _ = remove_entry(path);
                return Err(error.into());
            }
        },
        Err(error) => error,
    };
    if link_error.raw_os_error() == Some(1314) {
        if junction::create(source, path).is_ok() {
            return Ok(());
        }
    }
    let _ = remove_entry(path);
    let source_hash = tree_hash(source, false)?;
    if let Err(error) = copy_tree(source, path) {
        let _ = remove_entry(path);
        return Err(Error::Other(format!(
            "cannot link or copy the aidget Skill: {link_error}; {error}"
        )));
    }
    let marker = CopyMarker {
        source: source.to_string_lossy().into_owned(),
        tree_hash: source_hash,
    };
    let written = atomic_write(&path.join(MARKER_FILE), &serde_json::to_vec(&marker)?);
    if written.is_err() {
        let _ = remove_entry(path);
    }
    written
}

#[cfg(windows)]
fn verify_created_link(path: &Path) -> std::io::Result<()> {
    // Some security software reports success for symlink creation without
    // materializing the reparse point, which would fail the later rename with
    // NotFound. A missing link counts as a refused privilege so the junction
    // and marked-copy fallbacks apply; any other metadata error keeps its
    // original cause instead of being masked by a fallback.
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            Err(io::Error::from_raw_os_error(1314))
        }
        Err(error) => Err(error),
    }
}

#[cfg(windows)]
fn is_link(path: &Path, metadata: &fs::Metadata) -> Result<bool> {
    if metadata.file_type().is_symlink() {
        return Ok(true);
    }
    is_junction(path)
}

#[cfg(not(windows))]
fn is_link(_path: &Path, metadata: &fs::Metadata) -> Result<bool> {
    Ok(metadata.file_type().is_symlink())
}

fn link_points_to(path: &Path, source: &Path) -> Result<bool> {
    let target = link_target(path)?;
    let target = if target.is_absolute() {
        target
    } else {
        path.parent().unwrap_or_else(|| Path::new("")).join(target)
    };
    let Ok(target) = fs::canonicalize(target) else {
        return Ok(false);
    };
    Ok(target == fs::canonicalize(source)?)
}

fn link_target(path: &Path) -> Result<PathBuf> {
    #[cfg(windows)]
    if is_junction(path)? {
        return Ok(junction::get_target(path)?);
    }
    Ok(fs::read_link(path)?)
}

fn remove_entry(path: &Path) -> Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if is_link(path, &metadata)? {
        return remove_link(path);
    }
    if metadata.is_dir() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(unix)]
fn remove_link(path: &Path) -> Result<()> {
    fs::remove_file(path)?;
    Ok(())
}

#[cfg(windows)]
fn remove_link(path: &Path) -> Result<()> {
    if is_junction(path)? {
        junction::delete(path)?;
    }
    fs::remove_dir(path)?;
    Ok(())
}

#[cfg(windows)]
fn is_junction(path: &Path) -> Result<bool> {
    const ERROR_NOT_A_REPARSE_POINT: i32 = 4390;
    match junction::exists(path) {
        Ok(is_junction) => Ok(is_junction),
        Err(error) if error.raw_os_error() == Some(ERROR_NOT_A_REPARSE_POINT) => Ok(false),
        Err(error) => Err(error.into()),
    }
}

#[cfg(windows)]
fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for entry in sorted_entries(source)? {
        let from = entry.path();
        let to = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

fn tree_hash(root: &Path, skip_root_marker: bool) -> Result<String> {
    let mut bytes = Vec::new();
    append_tree(root, Path::new(""), &mut bytes, skip_root_marker)?;
    Ok(sha256_hex(&bytes))
}

fn append_tree(root: &Path, relative: &Path, bytes: &mut Vec<u8>, root_level: bool) -> Result<()> {
    for entry in sorted_entries(root)? {
        if root_level && skip_marker_file(&entry) {
            continue;
        }
        let child = relative.join(entry.file_name());
        bytes.extend(child.as_os_str().as_encoded_bytes());
        bytes.push(0);
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            bytes.push(b'd');
            append_tree(&entry.path(), &child, bytes, false)?;
        } else if file_type.is_symlink() {
            bytes.push(b'l');
            bytes.extend(fs::read_link(entry.path())?.as_os_str().as_encoded_bytes());
            bytes.push(0);
        } else {
            bytes.push(b'f');
            bytes.extend(fs::read(entry.path())?);
            bytes.push(0);
        }
    }
    Ok(())
}

fn skip_marker_file(entry: &fs::DirEntry) -> bool {
    entry.file_name() == MARKER_FILE
}

fn sorted_entries(path: &Path) -> Result<Vec<fs::DirEntry>> {
    let mut entries = fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    Ok(entries)
}

#[cfg(all(test, windows))]
#[path = "../../tests/agents/skill_created_link.rs"]
mod created_link_tests;
