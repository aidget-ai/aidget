//! Replacing the running `.app` with a downloaded one, then starting it again.
//!
//! The swap follows Magpie: unpack beside the bundle so installing is a rename
//! on one volume, keep the previous app aside until the new one is in place, and
//! only then drop it.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::common::error::{Error, Result};

/// The `.app` directory `exe` lives in, or `None` when it runs outside one.
pub fn bundle_of(exe: &Path) -> Option<PathBuf> {
    let exe = std::fs::canonicalize(exe).ok()?;
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    let inside = macos.file_name()? == "MacOS"
        && contents.file_name()? == "Contents"
        && bundle.extension()? == "app";
    inside.then(|| bundle.to_path_buf())
}

/// Replace `bundle` with the app inside the `staged` zip.
///
/// The running process keeps using the old image until it exits; the next launch
/// is the new one. On failure the previous bundle is put back where it was.
pub fn install(staged: &Path, bundle: &Path) -> Result<()> {
    let work = staging_dir(bundle)?;
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work)?;

    let unpacked = work.join("unpacked");
    let output = Command::new("ditto")
        .args(["-x", "-k"])
        .arg(staged)
        .arg(&unpacked)
        .output()?;
    if !output.status.success() {
        return Err(Error::Other(format!(
            "cannot unpack the update: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    let new_app = find_app(&unpacked)
        .ok_or_else(|| Error::Other("the downloaded archive contains no .app".to_string()))?;

    let previous = work.join("previous.app");
    std::fs::rename(bundle, &previous)?;
    if let Err(error) = std::fs::rename(&new_app, bundle) {
        // Put the old bundle back before reporting, so a failed install never
        // leaves the user without an app. If even that fails, name where the
        // previous app is: a bare rename error would leave them hunting.
        if std::fs::rename(&previous, bundle).is_err() {
            return Err(Error::Other(format!(
                "cannot put the update in place ({error}), and the previous app could not be \
                 moved back: it is at {}",
                previous.display()
            )));
        }
        return Err(error.into());
    }
    let _ = std::fs::remove_dir_all(&work);
    Ok(())
}

/// Resolve symlinks and reject non-bundle launches before touching the install.
pub fn prepare(exe: &Path) -> Result<super::Installation> {
    let executable = std::fs::canonicalize(exe)?;
    let bundle = bundle_of(&executable)
        .ok_or_else(|| Error::Other("this copy of aidget is not inside an .app bundle".into()))?;
    Ok(super::Installation::Mac { bundle, executable })
}

/// Where an update is unpacked: beside the bundle, so the swap is a rename on
/// the same volume.
fn staging_dir(bundle: &Path) -> Result<PathBuf> {
    let parent = bundle
        .parent()
        .ok_or_else(|| Error::Other("the app has no parent directory".to_string()))?;
    ensure_writable(parent)?;
    Ok(parent.join(".aidget-update"))
}

/// Fail early when the bundle's folder is not ours to change; the caller turns
/// this into advice rather than a puzzling rename error.
fn ensure_writable(dir: &Path) -> Result<()> {
    let probe = dir.join(".aidget-update-probe");
    std::fs::write(&probe, b"").map_err(|error| {
        Error::Other(format!(
            "{} is not writable ({error}); move aidget to a folder you can write to",
            dir.display()
        ))
    })?;
    let _ = std::fs::remove_file(&probe);
    Ok(())
}

/// The first `.app` directly inside `dir`.
fn find_app(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|extension| extension == "app"))
}
