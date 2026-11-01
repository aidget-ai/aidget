use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::common::error::{Error, Result};

/// Marker that identifies a shim this program wrote, so a foreign file of the
/// same name is never overwritten.
#[cfg(windows)]
const SHIM_PREFIX: &str = "@echo off\r\n\"";

/// Where the CLI link lives: short, stable, and independent of the bundle
/// layout, so a script can name it without discovering the app path.
///
/// Windows cannot execute an extension-less file, so it gets a `.cmd` shim.
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

/// Point `<root>/aidget` (or `aidget.cmd`) at the running binary so callers get
/// one predictable absolute path. Best-effort: a read-only or translocated
/// bundle just keeps whatever path the caller already has.
pub fn publish_cli(root: &Path) {
    // cmd.exe cannot invoke the `\\?\` verbatim paths `Path::canonicalize`
    // emits on Windows, so canonicalize to the plain DOS form instead.
    let Ok(exe) = std::env::current_exe().and_then(dunce::canonicalize) else {
        return;
    };
    // A translocated bundle is a throwaway mount; a link into it dies on the
    // next launch, so leave the caller to use the path it was started with.
    if exe.to_string_lossy().contains("/AppTranslocation/") {
        return;
    }
    if std::fs::create_dir_all(root).is_err() {
        return;
    }
    #[cfg(windows)]
    write_cmd_shim(&cli_link(root), &exe);
    #[cfg(unix)]
    write_symlink(&cli_link(root), &exe);
}

/// Windows named symlinks need developer mode or
/// SeCreateSymbolicLinkPrivilege, so a plain batch shim is the portable way to
/// repoint the CLI.
#[cfg(windows)]
fn write_cmd_shim(link: &Path, exe: &Path) {
    let script = format!("{SHIM_PREFIX}{}\" %*\r\n", exe.display());
    match std::fs::read_to_string(link) {
        Ok(text) if text == script => return,
        Ok(text) if !text.starts_with(SHIM_PREFIX) => return,
        Ok(_) => {}
        Err(_) if link.exists() => return,
        Err(_) => {}
    }
    let _ = std::fs::write(link, script);
}

/// The executable a published CLI entry resolves to, or `None` when the entry
/// is missing or not ours. `current_exe` reports the link path when invoked
/// through the link, so resolving it is what makes publishing idempotent.
#[cfg(unix)]
fn write_symlink(link: &Path, exe: &Path) {
    if std::fs::canonicalize(link).is_ok_and(|target| target == exe) {
        return;
    }
    match std::fs::symlink_metadata(link) {
        Ok(metadata) if !metadata.file_type().is_symlink() => return,
        Ok(_) => {
            let _ = std::fs::remove_file(link);
        }
        Err(_) => {}
    }
    let _ = std::os::unix::fs::symlink(exe, link);
}

/// What the published CLI entry currently points at, for assertions and for
/// callers that want to report the resolved binary.
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
pub fn spawn_ui() -> Result<()> {
    let exe = std::env::current_exe()?;
    let mut command = Command::new(exe);
    command
        .arg("ui")
        .arg("--no-main-window")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
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
    command
        .spawn()
        .map_err(|error| Error::Other(format!("cannot start aidget ui: {error}")))?;
    Ok(())
}

/// Start the resident UI when absent and require a real bridge response.
pub fn ensure_ui(client: &crate::transport::client::Client) -> Result<()> {
    if client.is_running() {
        return Ok(());
    }
    if !client.is_connectable() {
        spawn_ui()?;
    }
    if !client.wait_ready(std::time::Duration::from_secs(12)) {
        return Err(crate::Error::Timeout);
    }
    Ok(())
}
