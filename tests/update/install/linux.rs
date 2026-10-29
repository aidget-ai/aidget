use std::path::Path;

use aidget::update::github::PackageKind;
use aidget::update::install::linux::{package_command, replace_appimage};
use tempfile::TempDir;

/// The mode bits are a unix property, so these two only run where they mean
/// something.
#[cfg(unix)]
#[test]
fn installing_an_appimage_replaces_the_file_and_keeps_it_executable() {
    let root = TempDir::new().unwrap();
    let appimage = root.path().join("aidget.AppImage");
    let staged = root.path().join("downloaded.AppImage");
    std::fs::write(&appimage, "old").unwrap();
    std::fs::write(&staged, "new").unwrap();
    make_executable(&appimage);

    replace_appimage(&staged, &appimage).unwrap();

    assert_eq!(std::fs::read_to_string(&appimage).unwrap(), "new");
    assert!(
        is_executable(&appimage),
        "an AppImage that lost its mode bit would not start"
    );
    assert!(!root.path().join("aidget.AppImage.new").exists());
    assert!(!root.path().join("aidget.AppImage.old").exists());
}

#[cfg(unix)]
#[test]
fn a_failed_swap_puts_the_previous_appimage_back() {
    let root = TempDir::new().unwrap();
    let appimage = root.path().join("aidget.AppImage");
    std::fs::write(&appimage, "old").unwrap();
    make_executable(&appimage);
    // Nothing to copy: the download went missing between the check and here.
    let staged = root.path().join("gone.AppImage");

    assert!(replace_appimage(&staged, &appimage).is_err());
    assert_eq!(std::fs::read_to_string(&appimage).unwrap(), "old");
    assert!(is_executable(&appimage));
}

#[test]
fn a_deb_is_installed_through_pkexec_dpkg() {
    let program = Path::new("/usr/bin/dpkg");
    let staged = Path::new("/tmp/aidget_0.1.1_amd64.deb");

    let command = package_command(PackageKind::Deb, program, staged).unwrap();

    assert_eq!(command.get_program(), "pkexec");
    assert_eq!(
        command.get_args().collect::<Vec<_>>(),
        ["/usr/bin/dpkg", "-i", "/tmp/aidget_0.1.1_amd64.deb"]
    );
}

#[test]
fn an_rpm_is_upgraded_through_pkexec_rpm() {
    let program = Path::new("/usr/bin/rpm");
    let staged = Path::new("/tmp/aidget-0.1.1-1.x86_64.rpm");

    let command = package_command(PackageKind::Rpm, program, staged).unwrap();

    assert_eq!(command.get_program(), "pkexec");
    assert_eq!(
        command.get_args().collect::<Vec<_>>(),
        ["/usr/bin/rpm", "-U", "/tmp/aidget-0.1.1-1.x86_64.rpm"]
    );
}

#[test]
fn a_download_that_is_not_a_package_has_no_install_command() {
    assert!(package_command(
        PackageKind::AppImage,
        Path::new("/usr/bin/dpkg"),
        Path::new("/tmp/x")
    )
    .is_err());
    assert!(package_command(
        PackageKind::MacApp,
        Path::new("/usr/bin/dpkg"),
        Path::new("/tmp/x")
    )
    .is_err());
}

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).unwrap().permissions().mode() & 0o111 != 0
}
