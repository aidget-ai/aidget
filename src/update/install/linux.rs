//! Replacing a Linux install through the package manager that owns its files.

use std::path::Path;
use std::process::Command;

use crate::common::error::{Error, Result};
use crate::update::github::PackageKind;
use crate::update::target;

/// Capture the package executable before the package manager can replace it.
pub fn prepare(exe: &Path, kind: PackageKind) -> Result<super::Installation> {
    Ok(super::Installation::Linux {
        kind,
        executable: exe.to_path_buf(),
    })
}

/// Install using the destination captured before replacing the running binary.
pub fn install(staged: &Path, kind: PackageKind) -> Result<()> {
    match kind {
        PackageKind::Deb => run_package(PackageKind::Deb, "dpkg", staged),
        PackageKind::Rpm => run_package(PackageKind::Rpm, "rpm", staged),
        _ => Err(Error::Other(format!(
            "a {kind:?} download cannot be installed on Linux"
        ))),
    }
}

/// Start the replaced install again.
///
/// A distribution package puts the new binary at the path the old one had.
pub fn relaunch(exe: &Path) -> Result<()> {
    // Started only once this process is gone: until then the old instance still
    // owns the IPC endpoint, and the new one would find it, ask it to reopen, and
    // exit — leaving nothing running.
    let log = std::env::temp_dir().join("aidget-relaunch.log");
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let script = super::relaunch_script(exe, std::process::id(), &log, &args);
    Command::new("/bin/sh").arg("-c").arg(script).spawn()?;
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
            )));
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
