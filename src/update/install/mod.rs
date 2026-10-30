//! Installing a verified update.
//!
//! Each platform replaces its own kind of install; the platform that cannot is
//! left out rather than faked.

use std::path::Path;

use super::target;
use crate::common::error::{Error, Result};

// The Linux and Windows installers only use the standard library, so they are
// compiled everywhere: a cross-target mistake is a compile error here instead of
// a broken release. `macos` shells out to `ditto`, which only tests on macOS.
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
pub mod windows;

/// Replace the running install with the downloaded update at `staged`.
///
/// The process keeps running on the old image until it exits; the next launch is
/// the new one. `restart` says whether the new version should come up by itself:
/// a Windows installer has to be told, while an install that happens on quit must
/// not bring the app back.
pub fn apply(staged: &Path, restart: bool) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let _ = restart;
        let exe = std::env::current_exe()?;
        let bundle = macos::bundle_of(&exe).ok_or_else(|| {
            Error::Other("this copy of aidget is not inside an .app bundle".to_string())
        })?;
        macos::install(staged, &bundle)
    }
    #[cfg(target_os = "windows")]
    {
        windows::install(staged, restart)
    }
    #[cfg(target_os = "linux")]
    {
        let target = target::current()
            .ok_or_else(|| Error::Other("this install cannot update itself".to_string()))?;
        linux::install(staged, target.kind)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = (staged, restart);
        Err(Error::Other(
            "in-place update is not implemented on this platform".to_string(),
        ))
    }
}

/// Start the installed version once this process has exited.
pub fn relaunch() -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        macos::relaunch()
    }
    #[cfg(target_os = "windows")]
    {
        // The installer was told to start it (`/R`); there is nothing to start
        // here, and an app quitting after an update must stay quit.
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        let exe = std::env::current_exe()?;
        let started =
            target::appimage_path(&exe, std::env::var_os("APPIMAGE").as_deref()).unwrap_or(exe);
        linux::relaunch(&started)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Err(Error::Other(
            "in-place update is not implemented on this platform".to_string(),
        ))
    }
}

/// A shell command that waits for `pid` to exit, then replaces itself with `exe`
/// and appends whatever the new process prints to `log`.
///
/// Replacing a running bundle or binary is fine, but starting the new one while the
/// old process is still alive can hand the launch to the dying process — and on
/// Linux the new process can find the old instance over IPC, ask it to reopen, and
/// exit, leaving nothing running. Both platforms share this shape.
pub fn relaunch_script(exe: &Path, pid: u32, log: &Path) -> String {
    format!(
        "while kill -0 {pid} 2>/dev/null; do sleep 0.2; done; exec {} >>{} 2>&1",
        sh_quote(exe),
        sh_quote(log)
    )
}

/// Single-quote a path for `/bin/sh`, so spaces and quotes survive.
pub fn sh_quote(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', r"'\''"))
}
