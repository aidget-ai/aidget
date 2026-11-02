use std::ffi::OsStr;
use std::path::Path;

use aidget::update::github::PackageKind;
use aidget::update::target::{appimage_path, target_for, PackageManagers};

/// A machine with neither package manager, which is what a pure AppImage test
/// needs: the AppImage must be recognised without either.
const NONE: PackageManagers = PackageManagers {
    dpkg: false,
    rpm: false,
};
const DPKG: PackageManagers = PackageManagers {
    dpkg: true,
    rpm: false,
};
const RPM: PackageManagers = PackageManagers {
    dpkg: false,
    rpm: true,
};

#[test]
fn macos_installs_a_zipped_app_for_its_own_architecture() {
    let arm = target_for(
        "macos",
        "aarch64",
        Path::new("/Applications/aidget.app"),
        None,
        NONE,
    )
    .unwrap();
    let intel = target_for(
        "macos",
        "x86_64",
        Path::new("/Applications/aidget.app"),
        None,
        NONE,
    )
    .unwrap();

    assert_eq!(arm.kind, PackageKind::MacApp);
    assert_eq!(arm.arch, "aarch64");
    assert_eq!(intel.kind, PackageKind::MacApp);
    assert_eq!(intel.arch, "x64");
}

#[test]
fn an_apple_architecture_without_a_release_asset_resolves_to_nothing() {
    assert!(target_for(
        "macos",
        "riscv64",
        Path::new("/Applications/aidget.app"),
        None,
        NONE
    )
    .is_none());
}

#[test]
fn windows_installs_the_installer() {
    let target = target_for("windows", "x86_64", Path::new(r"C:\aidget.exe"), None, NONE).unwrap();

    assert_eq!(target.kind, PackageKind::WindowsSetup);
    assert_eq!(target.arch, "x64");
}

#[test]
fn a_linux_appimage_is_told_apart_by_its_runtime_variable() {
    let target = target_for(
        "linux",
        "x86_64",
        Path::new("/tmp/.mount_aidget/aidget"),
        Some(OsStr::new("/home/me/Applications/aidget.AppImage")),
        DPKG,
    )
    .unwrap();

    assert_eq!(target.kind, PackageKind::AppImage);
    assert_eq!(target.arch, "amd64");
}

#[test]
fn a_linux_binary_named_appimage_is_also_an_appimage() {
    let target = target_for(
        "linux",
        "x86_64",
        Path::new("/home/me/Applications/aidget.AppImage"),
        None,
        NONE,
    )
    .unwrap();

    assert_eq!(target.kind, PackageKind::AppImage);
}

#[test]
fn a_linux_install_with_dpkg_takes_the_deb() {
    let target = target_for("linux", "x86_64", Path::new("/usr/bin/aidget"), None, DPKG).unwrap();

    assert_eq!(target.kind, PackageKind::Deb);
    assert_eq!(target.arch, "amd64");
}

#[test]
fn a_linux_install_without_dpkg_takes_the_rpm() {
    let target = target_for("linux", "x86_64", Path::new("/usr/bin/aidget"), None, RPM).unwrap();

    assert_eq!(target.kind, PackageKind::Rpm);
}

#[test]
fn a_machine_with_both_package_managers_takes_the_deb() {
    // `rpm` is present on many Debian machines only to read .rpm files; `dpkg`
    // is what installed the app there.
    let both = PackageManagers {
        dpkg: true,
        rpm: true,
    };

    let target = target_for("linux", "x86_64", Path::new("/usr/bin/aidget"), None, both).unwrap();

    assert_eq!(target.kind, PackageKind::Deb);
}

#[test]
fn a_linux_install_with_neither_package_manager_resolves_to_nothing() {
    assert!(
        target_for("linux", "x86_64", Path::new("/usr/bin/aidget"), None, NONE).is_none(),
        "there is no package to install on a machine with neither tool"
    );
}

#[test]
fn an_unsupported_platform_resolves_to_nothing() {
    assert!(target_for(
        "freebsd",
        "x86_64",
        Path::new("/usr/bin/aidget"),
        None,
        DPKG
    )
    .is_none());
}

#[test]
fn an_appimage_is_replaced_at_the_file_its_runtime_names() {
    let mounted = Path::new("/tmp/.mount_aidget/aidget");
    let image = OsStr::new("/home/me/Applications/aidget.AppImage");

    assert_eq!(
        appimage_path(mounted, Some(image)).unwrap(),
        Path::new("/home/me/Applications/aidget.AppImage")
    );
    assert_eq!(
        appimage_path(Path::new("/home/me/aidget.AppImage"), None).unwrap(),
        Path::new("/home/me/aidget.AppImage")
    );
    assert!(appimage_path(Path::new("/usr/bin/aidget"), None).is_none());
}
