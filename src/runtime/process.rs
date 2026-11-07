use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::common::error::{Error, Result};

/// Fixed CLI path in the selected data directory.
pub fn cli_path(root: &Path) -> PathBuf {
    root.join(if cfg!(windows) {
        "aidget.exe"
    } else {
        "aidget"
    })
}

/// Publish a Windows executable copy or Unix CLI link; foreign files are errors.
pub fn publish_cli(root: &Path) -> Result<()> {
    let exe = dunce::canonicalize(std::env::current_exe()?)?;
    if exe.to_string_lossy().contains("/AppTranslocation/") {
        return Err(Error::Other(
            "move aidget out of AppTranslocation before publishing its CLI".into(),
        ));
    }
    std::fs::create_dir_all(root)?;
    let _lock = crate::widgets::store::Store::new(root).write_lock()?;
    let entry = cli_path(root);
    // A published executable must never try to overwrite itself.
    if entry.try_exists()? && dunce::canonicalize(&entry)? == exe {
        return Ok(());
    }
    #[cfg(windows)]
    {
        use crate::common::utils::{atomic_write, file_sha256};
        let marker = root.join("state").join("cli.sha256");
        let source_hash = file_sha256(&exe)?;
        if entry.try_exists()? {
            let metadata = std::fs::symlink_metadata(&entry)?;
            let entry_hash = file_sha256(&entry)?;
            if metadata.is_file() && !metadata.file_type().is_symlink() {
                // Identical copies may already be running; leave their file intact.
                if entry_hash == source_hash {
                    atomic_write(&marker, source_hash.as_bytes())?;
                    return Ok(());
                }
                match std::fs::read_to_string(&marker) {
                    Ok(recorded) if recorded == entry_hash => {}
                    Ok(_) => {
                        return Err(Error::Other(format!(
                            "{} is not a managed aidget executable",
                            entry.display()
                        )))
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        return Err(Error::Other(format!(
                            "{} is not a managed aidget executable",
                            entry.display()
                        )));
                    }
                    Err(error) => return Err(error.into()),
                }
            } else {
                return Err(Error::Other(format!(
                    "{} is not a managed aidget executable",
                    entry.display()
                )));
            }
        }
        let staged = root.join(format!(".aidget-cli-{}.exe", uuid::Uuid::new_v4()));
        let copied = (|| -> Result<()> {
            std::fs::copy(&exe, &staged)?;
            if file_sha256(&staged)? != source_hash {
                return Err(Error::Other(
                    "aidget executable changed while publishing its CLI".into(),
                ));
            }
            // Stage beside the destination so a failed replacement keeps the old copy.
            std::fs::rename(&staged, &entry)?;
            Ok(())
        })();
        if copied.is_err() {
            let _ = std::fs::remove_file(&staged);
        }
        copied?;
        atomic_write(&marker, source_hash.as_bytes())?;
    }
    #[cfg(not(windows))]
    {
        match std::fs::symlink_metadata(&entry) {
            Ok(metadata) if !metadata.file_type().is_symlink() => {
                return Err(Error::Other(format!(
                    "{} is not an aidget launcher",
                    entry.display()
                )))
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let staged = entry.with_file_name(format!(".aidget-cli-{}", uuid::Uuid::new_v4()));
        std::os::unix::fs::symlink(exe, &staged)?;
        if let Err(error) = std::fs::rename(&staged, &entry) {
            let _ = std::fs::remove_file(staged);
            return Err(error.into());
        }
    }
    Ok(())
}

/// Spawn `aidget ui` in its own session so it survives the calling
/// CLI/MCP process and any terminal that started it.
pub fn spawn_ui(root: &Path) -> Result<()> {
    let mut command = ui_command(root)?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = command
        .spawn()
        .map_err(|error| Error::Other(format!("cannot start aidget ui: {error}")))?;
    // Reap asynchronously so a long-lived MCP parent never leaves GUI zombies.
    std::thread::Builder::new()
        .name("aidget-ui-reaper".into())
        .spawn(move || {
            if let Err(error) = child.wait() {
                eprintln!("aidget: cannot reap the GUI process: {error}");
            }
        })?;
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
