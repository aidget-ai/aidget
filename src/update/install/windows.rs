//! Running the NSIS installer that replaces this install.
//!
//! Windows cannot swap a running `.exe` the way macOS swaps a bundle, and Tauri
//! ships an NSIS installer rather than a bare executable, so the install is the
//! installer's job. Its flags are the ones Tauri's own updater passes.

use std::path::Path;
use std::process::Command;

use crate::common::error::{Error, Result};

/// The flags Tauri's NSIS installer understands for an update.
///
/// `/S` silent and `/UPDATE` because this is an update; `/R` has it start the
/// new version once installed, as the signed-in user. A silent installer kills
/// the running app before copying, which is why nothing here waits for it.
pub fn installer_args(restart: bool) -> Vec<&'static str> {
    let mut args = vec!["/S", "/UPDATE"];
    if restart {
        args.push("/R");
    }
    args
}

/// Hand the downloaded `-setup.exe` to the system and return.
///
/// Nothing waits: the installer is a separate program that outlives this one and
/// stops it, so a failure after this point cannot be reported in the window.
pub fn install(staged: &Path, restart: bool) -> Result<()> {
    Command::new(staged)
        .args(installer_args(restart))
        .spawn()
        .map_err(|error| {
            Error::Other(format!(
                "cannot start the installer {}: {error}",
                staged.display()
            ))
        })?;
    Ok(())
}
