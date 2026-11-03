//! What this machine would install, and how the release workflow names it.
//!
//! The asset name is built from the running platform, and Linux upgrades use the
//! package manager already present on the machine.

use std::path::PathBuf;

use super::github::PackageKind;

/// The release asset a machine installs from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    pub kind: PackageKind,
    /// The architecture token the release workflow writes into asset names.
    pub arch: &'static str,
}

impl Target {
    /// The exact public filename for this platform, architecture and package kind.
    pub fn asset_name(self) -> String {
        let platform = match self.kind {
            PackageKind::MacApp => "darwin",
            PackageKind::WindowsSetup => "windows",
            PackageKind::Deb | PackageKind::Rpm => "linux",
        };
        format!("aidget-{platform}-{}{}", self.arch, self.kind.suffix())
    }
}

/// Which distribution package managers this machine has.
///
/// Only the package manager that owns the installation can upgrade it: the
/// release carries both packages, and picking the wrong one means asking the
/// user for a password to install a package their system cannot use.
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
    target_for(
        std::env::consts::OS,
        std::env::consts::ARCH,
        PackageManagers::detect(),
    )
}

/// [`current`] with the platform facts passed in, so it can be tested.
pub fn target_for(os: &str, arch: &str, managers: PackageManagers) -> Option<Target> {
    match os {
        "macos" => Some(Target {
            kind: PackageKind::MacApp,
            arch: apple_arch(arch)?,
        }),
        "windows" => Some(Target {
            kind: PackageKind::WindowsSetup,
            arch: "amd64",
        }),
        "linux" => linux_target(arch, managers),
        _ => None,
    }
}

/// Public asset architecture tokens for Apple Silicon and Intel.
fn apple_arch(arch: &str) -> Option<&'static str> {
    match arch {
        "aarch64" => Some("arm64"),
        "x86_64" => Some("amd64"),
        _ => None,
    }
}

fn linux_target(arch: &str, managers: PackageManagers) -> Option<Target> {
    if arch != "x86_64" {
        return None;
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
