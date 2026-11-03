use aidget::runtime::process::{cli_link, publish_cli, published_target};

fn root() -> tempfile::TempDir {
    tempfile::TempDir::new().unwrap()
}

#[test]
fn publish_points_the_link_at_the_running_binary() {
    let dir = root();
    publish_cli(dir.path()).unwrap();
    let exe = dunce::canonicalize(std::env::current_exe().unwrap()).unwrap();
    assert_eq!(published_target(&cli_link(dir.path())).unwrap(), exe);
}

#[test]
fn publish_is_idempotent() {
    let dir = root();
    publish_cli(dir.path()).unwrap();
    let first = published_target(&cli_link(dir.path())).unwrap();
    publish_cli(dir.path()).unwrap();
    assert_eq!(published_target(&cli_link(dir.path())).unwrap(), first);
}

#[test]
fn publish_rejects_a_foreign_file_without_modifying_it() {
    let dir = root();
    let link = cli_link(dir.path());
    std::fs::write(&link, b"not ours").unwrap();
    assert!(publish_cli(dir.path()).is_err());
    assert_eq!(std::fs::read(&link).unwrap(), b"not ours");
    assert_eq!(published_target(&link), None);
}

/// The published entry is handed to cmd.exe and batch files, which do not
/// understand the `\\?\` verbatim prefix that `canonicalize` emits on
/// Windows.
#[test]
fn publish_does_not_expose_a_verbatim_path() {
    let dir = root();
    publish_cli(dir.path()).unwrap();
    let target = published_target(&cli_link(dir.path())).unwrap();
    assert!(
        !target.as_os_str().to_string_lossy().starts_with(r"\\?\"),
        "published target must be a plain path: {}",
        target.display()
    );
}

#[cfg(unix)]
#[test]
fn publish_repoints_a_stale_link_without_changing_its_old_target() {
    let dir = root();
    let old = dir.path().join("old-binary");
    std::fs::write(&old, "keep").unwrap();
    std::os::unix::fs::symlink(&old, cli_link(dir.path())).unwrap();
    publish_cli(dir.path()).unwrap();
    assert_eq!(
        published_target(&cli_link(dir.path())).unwrap(),
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
