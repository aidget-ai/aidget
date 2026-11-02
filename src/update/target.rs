//! What this machine would install, and how the release workflow names it.
//!
//! Magpie decides this the same way: the asset name is built from the running
//! platform, and how the app got onto the machine is read from the executable's
//! path or the `APPIMAGE` variable its runtime exports.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use super::github::PackageKind;

/// The release asset a machine installs from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    pub kind: PackageKind,
    /// The architecture token the release workflow writes into asset names.
    pub arch: &'static str,
}

/// Which distribution package managers this machine has.
///
/// A Linux build outside an AppImage was installed by one of them, and only
/// that one can upgrade it: the release carries both packages, and picking the
/// wrong one means asking the user for a password to install a package their
/// system cannot use.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PackageManagers {
    pub dpkg: bool,
    pub rpm: bool,
}

impl PackageManagers {
    /// What `PATH` finds, which is what an install path would have used.
    pub fn detect() -> Self {
        Self {
            dpkg: on_path("dpkg").is_some(),
            rpm: on_path("rpm").is_some(),
        }
    }
}

/// What this build would install, or `None` when it cannot tell.
pub fn current() -> Option<Target> {
    let exe = std::env::current_exe().ok()?;
    let appimage = std::env::var_os("APPIMAGE");
    target_for(
        std::env::consts::OS,
        std::env::consts::ARCH,
        &exe,
        appimage.as_deref(),
        PackageManagers::detect(),
    )
}

/// [`current`] with the platform facts passed in, so it can be tested.
pub fn target_for(
    os: &str,
    arch: &str,
    exe: &Path,
    appimage: Option<&OsStr>,
    managers: PackageManagers,
) -> Option<Target> {
    match os {
        "macos" => Some(Target {
            kind: PackageKind::MacApp,
            arch: apple_arch(arch)?,
        }),
        "windows" => Some(Target {
            kind: PackageKind::WindowsSetup,
            arch: "x64",
        }),
        "linux" => linux_target(arch, exe, appimage, managers),
        _ => None,
    }
}

/// The file a Linux AppImage runs from, or `None` when this is not one.
///
/// An AppImage mounts itself and runs from `/tmp/.mount_*`, so the executable is
/// not the file to replace or start again: its runtime names the real one in
/// `APPIMAGE`, and only a build run straight from the file is the file itself.
pub fn appimage_path(exe: &Path, appimage: Option<&OsStr>) -> Option<PathBuf> {
    if let Some(path) = appimage {
        return Some(PathBuf::from(path));
    }
    exe.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("AppImage"))
        .then(|| exe.to_path_buf())
}

/// `aarch64` on Apple Silicon, `x64` on Intel.
fn apple_arch(arch: &str) -> Option<&'static str> {
    match arch {
        "aarch64" => Some("aarch64"),
        "x86_64" => Some("x64"),
        _ => None,
    }
}

fn linux_target(
    arch: &str,
    exe: &Path,
    appimage: Option<&OsStr>,
    managers: PackageManagers,
) -> Option<Target> {
    if arch != "x86_64" {
        return None;
    }
    if appimage_path(exe, appimage).is_some() {
        return Some(Target {
            kind: PackageKind::AppImage,
            arch: "amd64",
        });
    }
    // `dpkg` first: it is what a Debian-family system installs with, and where
    // `rpm` is also present it is normally only the tool used to read `.rpm`
    // files. A Fedora or openSUSE machine has no `dpkg` at all.
    let kind = match (managers.dpkg, managers.rpm) {
        (true, _) => PackageKind::Deb,
        (false, true) => PackageKind::Rpm,
        (false, false) => return None,
    };
    Some(Target {
        kind,
        arch: "amd64",
    })
}

/// The first `name` on `PATH`, as an absolute path.
///
/// `pkexec` is given the resolved path: what it runs as root should be the
/// program this build decided on, not whatever the elevated `PATH` resolves to.
pub fn on_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}
