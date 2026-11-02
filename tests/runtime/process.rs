use aidget::runtime::process::{cli_link, publish_cli, published_target};

fn root() -> tempfile::TempDir {
    tempfile::TempDir::new().unwrap()
}

#[test]
fn publish_points_the_link_at_the_running_binary() {
    let dir = root();
    publish_cli(dir.path());
    let exe = dunce::canonicalize(std::env::current_exe().unwrap()).unwrap();
    assert_eq!(published_target(&cli_link(dir.path())).unwrap(), exe);
}

#[test]
fn publish_is_idempotent() {
    let dir = root();
    publish_cli(dir.path());
    let first = published_target(&cli_link(dir.path())).unwrap();
    publish_cli(dir.path());
    assert_eq!(published_target(&cli_link(dir.path())).unwrap(), first);
}

#[test]
fn publish_leaves_a_foreign_file_alone() {
    let dir = root();
    let link = cli_link(dir.path());
    std::fs::write(&link, b"not ours").unwrap();
    publish_cli(dir.path());
    assert_eq!(std::fs::read(&link).unwrap(), b"not ours");
    assert_eq!(published_target(&link), None);
}

/// The published entry is handed to cmd.exe and batch files, which do not
/// understand the `\\?\` verbatim prefix that `canonicalize` emits on
/// Windows.
#[test]
fn publish_does_not_expose_a_verbatim_path() {
    let dir = root();
    publish_cli(dir.path());
    let target = published_target(&cli_link(dir.path())).unwrap();
    assert!(
        !target.as_os_str().to_string_lossy().starts_with(r"\\?\"),
        "published target must be a plain path: {}",
        target.display()
    );
}
