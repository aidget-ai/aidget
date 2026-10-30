//! Installing a verified update.
//!
//! Each platform replaces its own kind of install; the platform that cannot is
//! left out rather than faked.

use std::path::Path;

#[cfg(target_os = "linux")]
use super::target;
#[cfg(not(target_os = "macos"))]
use crate::common::error::Error;
use crate::common::error::Result;

// The Linux and Windows installers only use the standard library, so they are
// compiled everywhere: a cross-target mistake is a compile error here instead of
// a broken release. `macos` shells out to `ditto`, which only tests on macOS.
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
pub mod windows;

/// Immutable installation and restart paths captured before any replacement.
#[derive(Debug)]
pub enum Installation {
    /// A real macOS bundle and its canonical executable.
    Mac {
        bundle: std::path::PathBuf,
        executable: std::path::PathBuf,
    },
    /// A Linux package or the outer AppImage, never its mounted executable.
    Linux {
        kind: super::github::PackageKind,
        executable: std::path::PathBuf,
    },
    /// NSIS owns replacement and optional restart on Windows.
    Windows,
}

/// Resolve the running installation before a package manager can unlink it.
pub fn prepare() -> Result<Installation> {
    let exe = std::env::current_exe()?;
    #[cfg(target_os = "macos")]
    {
        macos::prepare(&exe)
    }
    #[cfg(target_os = "linux")]
    {
        let target = target::current()
            .ok_or_else(|| Error::Other("this install cannot update itself".into()))?;
        linux::prepare(&exe, std::env::var_os("APPIMAGE").as_deref(), target.kind)
    }
    #[cfg(target_os = "windows")]
    {
        let _ = exe;
        Ok(Installation::Windows)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        let _ = exe;
        Err(Error::Other(
            "in-place update is not implemented on this platform".into(),
        ))
    }
}

impl Installation {
    /// Replace the captured installation. Quit never requests a restart.
    pub fn apply(&self, staged: &Path, restart: bool) -> Result<()> {
        match self {
            #[cfg(target_os = "macos")]
            Self::Mac { bundle, .. } => macos::install(staged, bundle),
            Self::Linux { kind, executable } => linux::install(staged, *kind, executable),
            Self::Windows => windows::install(staged, restart),
            #[cfg(not(target_os = "macos"))]
            Self::Mac { .. } => Err(Error::Other("macOS installation requires macOS".into())),
        }
    }

    /// Schedule restart using only paths saved before replacement.
    pub fn relaunch(&self) -> Result<()> {
        match self {
            Self::Mac { executable, .. } | Self::Linux { executable, .. } => {
                linux::relaunch(executable)
            }
            Self::Windows => Ok(()),
        }
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
