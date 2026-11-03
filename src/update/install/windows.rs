//! Replace a Windows installation through Tauri's NSIS installer.

use std::path::Path;
use std::process::Command;

use crate::common::error::{Error, Result};

/// Silent update flags, with `/R /ARGS` preserving app arguments on restart.
pub fn installer_args(restart: bool, current_args: &[String]) -> String {
    let mut args = String::from("/S /UPDATE");
    if restart {
        args.push_str(" /R");
        if !current_args.is_empty() {
            args.push_str(" /ARGS ");
            args.push_str(
                &current_args
                    .iter()
                    .map(|arg| crate::common::utils::windows_quote(arg))
                    .collect::<Vec<_>>()
                    .join(" "),
            );
        }
    }
    args
}

/// Start the installer without waiting: it stops the running app before copying.
pub fn install(staged: &Path, restart: bool) -> Result<()> {
    let current_args = std::env::args_os()
        .skip(1)
        .map(|arg| {
            arg.into_string()
                .map_err(|_| Error::InvalidParams("update restart arguments are not UTF-8".into()))
        })
        .collect::<Result<Vec<_>>>()?;
    let arguments = installer_args(restart, &current_args);
    let mut command = Command::new(staged);
    #[cfg(windows)]
    {
        // NSIS reads /ARGS from the raw command line before launching the app.
        use std::os::windows::process::CommandExt;
        command.raw_arg(arguments);
    }
    #[cfg(not(windows))]
    command.arg(arguments);
    command.spawn().map_err(|error| {
        Error::Other(format!(
            "cannot start the installer {}: {error}",
            staged.display()
        ))
    })?;
    Ok(())
}
