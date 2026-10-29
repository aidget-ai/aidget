//! Replacing a Linux install: an `AppImage` in place, or the package that owns
//! the files.
//!
//! The two are told apart by [`crate::update::target`], which also decides which
//! distribution package applies.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::target;
use crate::common::error::{Error, Result};
use crate::update::github::PackageKind;

/// Install the downloaded `staged` file for `kind`.
pub fn install(staged: &Path, kind: PackageKind) -> Result<()> {
    match kind {
        PackageKind::AppImage => {
            let exe = std::env::current_exe()?;
            let appimage = target::appimage_path(&exe, std::env::var_os("APPIMAGE").as_deref())
                .ok_or_else(|| {
                    Error::Other("this build does not run from an AppImage".to_string())
                })?;
            replace_appimage(staged, &appimage)
        }
        PackageKind::Deb => run_package(PackageKind::Deb, "dpkg", staged),
        PackageKind::Rpm => run_package(PackageKind::Rpm, "rpm", staged),
        _ => Err(Error::Other(format!(
            "a {kind:?} download cannot be installed on Linux"
        ))),
    }
}

/// Start the replaced install again.
///
/// An AppImage runs from the file that was replaced; a distribution package puts
/// the new binary at the path the old one had.
pub fn relaunch(exe: &Path) -> Result<()> {
    Command::new(exe)
        .spawn()
        .map(|_| ())
        .map_err(|error| Error::Other(format!("cannot start {}: {error}", exe.display())))
}

/// Replace `appimage` with `staged`, keeping its permission bits.
///
/// The new file is written beside the old one, so both renames stay on the same
/// filesystem, and the old one is only dropped once the new one is in place.
pub fn replace_appimage(staged: &Path, appimage: &Path) -> Result<()> {
    let permissions = std::fs::metadata(appimage)?.permissions();
    let incoming = sibling(appimage, "new")?;
    let aside = sibling(appimage, "old")?;

    std::fs::copy(staged, &incoming)?;
    std::fs::set_permissions(&incoming, permissions)?;
    std::fs::rename(appimage, &aside)?;
    if let Err(error) = std::fs::rename(&incoming, appimage) {
        let _ = std::fs::rename(&aside, appimage);
        let _ = std::fs::remove_file(&incoming);
        return Err(error.into());
    }
    let _ = std::fs::remove_file(&aside);
    Ok(())
}

/// The command that installs a distribution package, with the administrator's
/// password asked for by the system.
///
/// `program` is the resolved path of `dpkg` or `rpm`: what runs as root should
/// be the tool this build chose, not whatever the elevated `PATH` finds.
pub fn package_command(kind: PackageKind, program: &Path, staged: &Path) -> Result<Command> {
    let flag = match kind {
        PackageKind::Deb => "-i",
        PackageKind::Rpm => "-U",
        _ => {
            return Err(Error::Other(format!(
                "a {kind:?} download is not a distribution package"
            )))
        }
    };
    let mut command = Command::new("pkexec");
    command.arg(program).arg(flag).arg(staged);
    Ok(command)
}

fn run_package(kind: PackageKind, tool: &str, staged: &Path) -> Result<()> {
    let program = target::on_path(tool)
        .ok_or_else(|| Error::Other(format!("no {tool} on this machine to install this update")))?;
    let status = package_command(kind, &program, staged)?.status()?;
    if !status.success() {
        return Err(Error::Other(format!(
            "{tool} exited with {status}; the previous version is still installed"
        )));
    }
    Ok(())
}

fn sibling(path: &Path, tag: &str) -> Result<PathBuf> {
    let name = path
        .file_name()
        .ok_or_else(|| Error::Other(format!("{} has no file name", path.display())))?;
    Ok(path.with_file_name(format!("{}.{tag}", name.to_string_lossy())))
}
