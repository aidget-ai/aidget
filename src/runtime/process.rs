use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::common::error::{Error, Result};

/// Marker that identifies a shim this program wrote, so a foreign file of the
/// same name is never overwritten.
#[cfg(windows)]
const SHIM_PREFIX: &str = "@echo off\r\n\"";

/// Stable CLI path; Windows uses a `.cmd` shim.
pub fn cli_link(root: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        root.join("aidget.cmd")
    }
    #[cfg(not(windows))]
    {
        root.join("aidget")
    }
}

/// Publish this binary at the instance's stable CLI path; foreign files are errors.
pub fn publish_cli(root: &Path) -> Result<()> {
    let exe = dunce::canonicalize(std::env::current_exe()?)?;
    if exe.to_string_lossy().contains("/AppTranslocation/") {
        return Err(Error::Other(
            "move aidget out of AppTranslocation before publishing its CLI".into(),
        ));
    }
    std::fs::create_dir_all(root)?;
    let link = cli_link(root);
    #[cfg(windows)]
    {
        let script = format!("{SHIM_PREFIX}{}\" %*\r\n", exe.display());
        match std::fs::read_to_string(&link) {
            Ok(text) if text == script => return Ok(()),
            Ok(text) if !text.starts_with(SHIM_PREFIX) => {
                return Err(Error::Other(format!(
                    "{} is not an aidget launcher",
                    link.display()
                )))
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        crate::common::utils::atomic_write(&link, script.as_bytes())?;
    }
    #[cfg(unix)]
    {
        if std::fs::canonicalize(&link).is_ok_and(|target| target == exe) {
            return Ok(());
        }
        match std::fs::symlink_metadata(&link) {
            Ok(metadata) if !metadata.file_type().is_symlink() => {
                return Err(Error::Other(format!(
                    "{} is not an aidget launcher",
                    link.display()
                )))
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let staged = link.with_file_name(format!(".aidget-cli-{}", uuid::Uuid::new_v4()));
        std::os::unix::fs::symlink(exe, &staged)?;
        if let Err(error) = std::fs::rename(&staged, &link) {
            let _ = std::fs::remove_file(staged);
            return Err(error.into());
        }
    }
    Ok(())
}

/// Read the binary targeted by a published CLI entry.
pub fn published_target(link: &Path) -> Option<PathBuf> {
    #[cfg(unix)]
    {
        std::fs::read_link(link).ok()
    }
    #[cfg(windows)]
    {
        let text = std::fs::read_to_string(link).ok()?;
        let rest = text.strip_prefix(SHIM_PREFIX)?;
        Some(PathBuf::from(rest.split('"').next()?))
    }
}

/// Spawn `aidget ui` in its own session so it survives the calling
/// CLI/MCP process and any terminal that started it.
pub fn spawn_ui(root: &Path) -> Result<()> {
    let mut command = ui_command(root)?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
        .spawn()
        .map_err(|error| Error::Other(format!("cannot start aidget ui: {error}")))?;
    Ok(())
}

/// Build a detached GUI command bound to an absolute data directory.
pub fn ui_command(root: &Path) -> Result<Command> {
    let exe = std::env::current_exe()?;
    let mut command = Command::new(exe);
    command
        .arg("--home")
        .arg(std::path::absolute(root)?)
        .arg("ui")
        .arg("--no-main-window");
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    // Detach from the parent console and its process group so the UI outlives
    // the terminal or MCP host that started it.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    Ok(command)
}

/// Start the resident UI when absent and require a real bridge response.
pub fn ensure_ui(client: &crate::transport::client::Client, root: &Path) -> Result<()> {
    if client.is_running() {
        return Ok(());
    }
    if !client.is_connectable() {
        spawn_ui(root)?;
    }
    if !client.wait_ready(std::time::Duration::from_secs(12)) {
        return Err(crate::Error::Timeout);
    }
    Ok(())
}
