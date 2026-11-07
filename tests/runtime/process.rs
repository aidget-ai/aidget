use aidget::runtime::process::cli_path;
use aidget::runtime::process::publish_cli;

fn root() -> tempfile::TempDir {
    tempfile::TempDir::new().unwrap()
}

#[cfg(unix)]
#[test]
fn publish_points_the_link_at_the_running_binary() {
    let dir = root();
    publish_cli(dir.path()).unwrap();
    let exe = dunce::canonicalize(std::env::current_exe().unwrap()).unwrap();
    assert_eq!(std::fs::read_link(cli_path(dir.path())).unwrap(), exe);
}

#[cfg(unix)]
#[test]
fn publish_is_idempotent() {
    let dir = root();
    publish_cli(dir.path()).unwrap();
    let first = std::fs::read_link(cli_path(dir.path())).unwrap();
    publish_cli(dir.path()).unwrap();
    assert_eq!(std::fs::read_link(cli_path(dir.path())).unwrap(), first);
}

#[cfg(unix)]
#[test]
fn publish_rejects_a_foreign_file_without_modifying_it() {
    let dir = root();
    let link = cli_path(dir.path());
    std::fs::write(&link, b"not ours").unwrap();
    assert!(publish_cli(dir.path()).is_err());
    assert_eq!(std::fs::read(&link).unwrap(), b"not ours");
    assert!(std::fs::read_link(&link).is_err());
}

#[cfg(windows)]
#[test]
fn windows_publish_copies_the_running_binary_to_the_selected_home() {
    let dir = root();
    publish_cli(dir.path()).unwrap();
    let entry = cli_path(dir.path());
    assert_eq!(entry, dir.path().join("aidget.exe"));
    assert_eq!(
        aidget::common::utils::file_sha256(&entry).unwrap(),
        aidget::common::utils::file_sha256(&std::env::current_exe().unwrap()).unwrap()
    );
    let modified = std::fs::metadata(&entry).unwrap().modified().unwrap();
    publish_cli(dir.path()).unwrap();
    assert_eq!(
        std::fs::metadata(entry).unwrap().modified().unwrap(),
        modified
    );
}

#[cfg(windows)]
#[test]
fn windows_publish_preserves_a_foreign_executable() {
    let dir = root();
    let entry = dir.path().join("aidget.exe");
    std::fs::write(&entry, "keep this file").unwrap();
    assert!(publish_cli(dir.path()).is_err());
    assert_eq!(std::fs::read_to_string(entry).unwrap(), "keep this file");
}

#[cfg(windows)]
#[test]
fn windows_publish_refreshes_a_managed_copy_and_preserves_modified_files() {
    use aidget::common::utils::file_sha256;
    let dir = root();
    let entry = cli_path(dir.path());
    let marker = dir.path().join("state").join("cli.sha256");
    std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
    std::fs::write(&entry, "old binary").unwrap();
    std::fs::write(&marker, file_sha256(&entry).unwrap()).unwrap();
    publish_cli(dir.path()).unwrap();
    assert_eq!(
        file_sha256(&entry).unwrap(),
        file_sha256(&std::env::current_exe().unwrap()).unwrap()
    );
    std::fs::write(&entry, "modified binary").unwrap();
    assert!(publish_cli(dir.path()).is_err());
    assert_eq!(std::fs::read_to_string(entry).unwrap(), "modified binary");
}

#[cfg(windows)]
#[test]
fn windows_publish_from_the_published_executable_is_a_noop() {
    if let Some(home) = std::env::var_os("AIDGET_PUBLISH_TEST_HOME") {
        publish_cli(std::path::Path::new(&home)).unwrap();
        return;
    }
    let dir = root();
    publish_cli(dir.path()).unwrap();
    let output = std::process::Command::new(cli_path(dir.path()))
        .args([
            "windows_publish_from_the_published_executable_is_a_noop",
            "--nocapture",
        ])
        .env("AIDGET_PUBLISH_TEST_HOME", dir.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[cfg(windows)]
#[test]
fn windows_cli_path_stays_under_the_selected_home() {
    let dir = root();
    let home = dir.path().join("uncreated");
    let path = cli_path(&home);
    assert_eq!(path, home.join("aidget.exe"));
    assert!(!path.to_string_lossy().starts_with(r"\\?\"));
    assert!(!home.exists());
}

#[cfg(unix)]
#[test]
fn publish_repoints_a_stale_link_without_changing_its_old_target() {
    let dir = root();
    let old = dir.path().join("old-binary");
    std::fs::write(&old, "keep").unwrap();
    std::os::unix::fs::symlink(&old, cli_path(dir.path())).unwrap();
    publish_cli(dir.path()).unwrap();
    assert_eq!(
        std::fs::read_link(cli_path(dir.path())).unwrap(),
        dunce::canonicalize(std::env::current_exe().unwrap()).unwrap()
    );
    assert_eq!(std::fs::read_to_string(old).unwrap(), "keep");
}

#[test]
fn a_gui_child_receives_the_explicit_home_as_an_argument() {
    let temp = root();
    let home = temp.path().join("space '中文");
    let command = aidget::runtime::process::ui_command(&home).unwrap();
    assert_eq!(
        command.get_args().collect::<Vec<_>>(),
        [
            std::ffi::OsStr::new("--home"),
            home.as_os_str(),
            std::ffi::OsStr::new("ui"),
            std::ffi::OsStr::new("--no-main-window"),
        ]
    );
}

#[cfg(unix)]
#[test]
fn a_long_lived_parent_reaps_its_exited_gui_child() {
    const WORKER: &str = "AIDGET_TEST_GUI_REAPER";
    if let Some(home) = std::env::var_os(WORKER) {
        // The test executable rejects the GUI arguments and exits immediately.
        aidget::runtime::process::spawn_ui(std::path::Path::new(&home)).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            let mut info = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
            // Observe exit without reaping; production code must consume the status.
            let result = unsafe {
                libc::waitid(
                    libc::P_ALL,
                    0,
                    info.as_mut_ptr(),
                    libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                )
            };
            if result == -1 {
                assert_eq!(
                    std::io::Error::last_os_error().raw_os_error(),
                    Some(libc::ECHILD)
                );
                break;
            }
            assert_eq!(result, 0);
            assert!(
                std::time::Instant::now() < deadline,
                "GUI child was not reaped"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        return;
    }
    let temp = root();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "runtime::process::a_long_lived_parent_reaps_its_exited_gui_child",
            "--nocapture",
        ])
        .env(WORKER, temp.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
