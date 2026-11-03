use std::path::Path;

use aidget::update::github::PackageKind;
use aidget::update::install::linux::package_command;
use tempfile::TempDir;

#[test]
fn a_deb_is_installed_through_pkexec_dpkg() {
    let program = Path::new("/usr/bin/dpkg");
    let staged = Path::new("/tmp/aidget-linux-amd64.deb");

    let command = package_command(PackageKind::Deb, program, staged).unwrap();

    assert_eq!(command.get_program(), "pkexec");
    assert_eq!(
        command.get_args().collect::<Vec<_>>(),
        ["/usr/bin/dpkg", "-i", "/tmp/aidget-linux-amd64.deb"]
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

#[cfg(target_os = "linux")]
#[test]
fn native_linux_os_probe_current_exe_after_replacement_has_deleted_suffix() {
    let root = TempDir::new().unwrap();
    let source = root.path().join("probe.rs");
    let binary = root.path().join("probe");
    std::fs::write(
        &source,
        r#"
        fn main() {
            let saved = std::env::current_exe().unwrap();
            let new = saved.with_extension("new");
            std::fs::rename(new, &saved).unwrap();
            let after = std::env::current_exe().unwrap();
            assert!(after.to_string_lossy().ends_with(" (deleted)"), "{after:?}");
            let status = std::process::Command::new(saved).status().unwrap();
            assert!(status.success());
        }
    "#,
    )
    .unwrap();
    assert!(std::process::Command::new("rustc")
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .status()
        .unwrap()
        .success());
    std::fs::write(binary.with_extension("new"), "#!/bin/sh\nexit 0\n").unwrap();
    make_executable(&binary.with_extension("new"));
    assert!(std::process::Command::new(binary)
        .status()
        .unwrap()
        .success());
}

/// Exercises the production captured Installation and relaunch in a process whose
/// executable is actually replaced. On Linux the same test covers deleted-exe
/// semantics; on macOS it verifies saved-path use and owned-process exit ordering.
#[cfg(unix)]
#[test]
fn production_relaunch_uses_prepared_path_after_running_executable_replacement() {
    use aidget::update::install::linux::prepare;
    use std::io::{BufRead, Write};
    use std::process::{Command, Stdio};

    const WORKER: &str = "AIDGET_TEST_REPLACED_EXECUTABLE";
    if std::env::var_os(WORKER).is_some() {
        let executable = std::env::current_exe().unwrap();
        let installation = prepare(&executable, PackageKind::Deb).unwrap();
        // Simulate the package manager unlinking the running executable without
        // invoking elevation or replacing any real installation.
        std::fs::rename(executable.with_extension("new"), &executable).unwrap();
        #[cfg(target_os = "linux")]
        assert!(std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .ends_with(" (deleted)"));
        installation.relaunch().unwrap();
        println!("production restart scheduled");
        std::io::stdout().flush().unwrap();
        let mut release = String::new();
        std::io::stdin().read_line(&mut release).unwrap();
        return;
    }

    let root = TempDir::new().unwrap();
    let executable = root.path().join("it's a running executable");
    let marker = root.path().join("new executable starts");
    std::fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
    let new_program = format!(
        "#!/bin/sh\nprintf 'started\\n' >>{}\n",
        aidget::update::install::sh_quote(&marker)
    );
    std::fs::write(executable.with_extension("new"), new_program).unwrap();
    make_executable(&executable.with_extension("new"));
    let mut worker = Command::new(&executable)
        .args(["--exact", "update::install::linux::production_relaunch_uses_prepared_path_after_running_executable_replacement", "--nocapture"])
        .env(WORKER, "package")
        // The production relaunch log stays inside this owned temporary root.
        .env("TMPDIR", root.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn().unwrap();
    let mut output = std::io::BufReader::new(worker.stdout.take().unwrap());
    let mut line = String::new();
    loop {
        assert_ne!(
            output.read_line(&mut line).unwrap(),
            0,
            "worker exited before scheduling restart"
        );
        if line.contains("production restart scheduled") {
            break;
        }
        line.clear();
    }
    let started_before_exit = marker.exists();
    drop(worker.stdin.take());
    assert!(worker.wait().unwrap().success());
    assert!(
        !started_before_exit,
        "production restart ran before the owned old process exited"
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if let Ok(started) = std::fs::read_to_string(&marker) {
            if started == "started\n" {
                break;
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "prepared executable did not restart; production log: {:?}",
            std::fs::read_to_string(root.path().join("aidget-relaunch.log"))
        );
        std::thread::yield_now();
    }
    assert_eq!(std::fs::read_to_string(marker).unwrap(), "started\n");
}
