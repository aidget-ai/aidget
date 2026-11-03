use aidget::update::github::PackageKind;
use aidget::update::target::{target_for, PackageManagers};

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
    let arm = target_for("macos", "aarch64", NONE).unwrap();
    let intel = target_for("macos", "x86_64", NONE).unwrap();

    assert_eq!(arm.kind, PackageKind::MacApp);
    assert_eq!(arm.arch, "arm64");
    assert_eq!(intel.kind, PackageKind::MacApp);
    assert_eq!(intel.arch, "amd64");
}

#[test]
fn an_apple_architecture_without_a_release_asset_resolves_to_nothing() {
    assert!(target_for("macos", "riscv64", NONE).is_none());
}

#[test]
fn windows_installs_the_installer() {
    let target = target_for("windows", "x86_64", NONE).unwrap();

    assert_eq!(target.kind, PackageKind::WindowsSetup);
    assert_eq!(target.arch, "amd64");
}

#[test]
fn a_linux_install_with_dpkg_takes_the_deb() {
    let target = target_for("linux", "x86_64", DPKG).unwrap();

    assert_eq!(target.kind, PackageKind::Deb);
    assert_eq!(target.arch, "amd64");
}

#[test]
fn a_linux_install_without_dpkg_takes_the_rpm() {
    let target = target_for("linux", "x86_64", RPM).unwrap();

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

    let target = target_for("linux", "x86_64", both).unwrap();

    assert_eq!(target.kind, PackageKind::Deb);
}

#[test]
fn a_linux_install_with_neither_package_manager_resolves_to_nothing() {
    assert!(
        target_for("linux", "x86_64", NONE).is_none(),
        "there is no package to install on a machine with neither tool"
    );
}

#[test]
fn an_unsupported_platform_resolves_to_nothing() {
    assert!(target_for("freebsd", "x86_64", DPKG).is_none());
}
