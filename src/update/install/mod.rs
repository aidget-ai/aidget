//! Installing a verified update.

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
    /// A Linux distribution package and the executable it owns.
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
        linux::prepare(&exe, target.kind)
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
            Self::Linux { kind, .. } => linux::install(staged, *kind),
            Self::Windows => windows::install(staged, restart),
            #[cfg(not(target_os = "macos"))]
            Self::Mac { .. } => Err(Error::Other("macOS installation requires macOS".into())),
        }
    }

    /// Schedule restart using the saved executable and log to the selected home.
    pub fn relaunch(&self, home: &Path) -> Result<()> {
        match self {
            Self::Mac { executable, .. } | Self::Linux { executable, .. } => {
                linux::relaunch(executable, home)
            }
            Self::Windows => Ok(()),
        }
    }
}

/// Wait for the old process to release IPC, then exec the saved binary with its
/// arguments and append output to the log.
pub fn relaunch_script(exe: &Path, pid: u32, log: &Path, args: &[std::ffi::OsString]) -> String {
    let arguments = args
        .iter()
        .map(|arg| crate::common::utils::shell_quote(&arg.to_string_lossy()))
        .collect::<Vec<_>>()
        .join(" ");
    let arguments = if arguments.is_empty() {
        String::new()
    } else {
        format!(" {arguments}")
    };
    // kill -0 still succeeds for zombies until their parent reaps them.
    format!(
        "while kill -0 {pid} 2>/dev/null; do case \"$(ps -p {pid} -o stat=)\" in *Z*) break;; esac; sleep 0.2; done; exec {}{arguments} >>{} 2>&1",
        sh_quote(exe),
        sh_quote(log)
    )
}

/// Single-quote a path for `/bin/sh`, so spaces and quotes survive.
pub fn sh_quote(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', r"'\''"))
}
