use aidget::interfaces::cli::{Cli, Command, WidgetCommand};
use clap::Parser;

#[test]
fn create_and_update_accept_drag_mode_and_reject_unknown_modes() {
    let created = Cli::try_parse_from([
        "aidget",
        "widget",
        "create",
        "note",
        "--drag-mode",
        "handle",
    ]);
    assert!(matches!(
        created.unwrap().command,
        Some(Command::Widget {
            command: WidgetCommand::Create(_)
        })
    ));
    assert!(Cli::try_parse_from([
        "aidget",
        "widget",
        "update",
        "note",
        "--revision",
        "1",
        "--drag-mode",
        "all",
    ])
    .is_ok());
    assert!(
        Cli::try_parse_from(["aidget", "widget", "create", "note", "--drag-mode", "wrong"])
            .is_err()
    );
    assert!(Cli::try_parse_from(["aidget", "widget", "update", "note"]).is_err());
    assert!(Cli::try_parse_from(["aidget", "widget", "patch", "note", "value"]).is_err());
    assert!(Cli::try_parse_from(["aidget", "board", "move", "note"]).is_err());
}

#[test]
fn json_is_available_for_agent_write_and_discovery_commands() {
    assert!(Cli::try_parse_from(["aidget", "widget", "create", "note", "--json"]).is_ok());
    assert!(Cli::try_parse_from([
        "aidget",
        "widget",
        "update",
        "note",
        "--revision",
        "1",
        "--json",
    ])
    .is_ok());
    assert!(Cli::try_parse_from(["aidget", "widget", "delete", "note", "--json"]).is_ok());
    assert!(Cli::try_parse_from([
        "aidget",
        "board",
        "move",
        "note",
        "--revision",
        "2",
        "--json",
    ])
    .is_ok());
    assert!(Cli::try_parse_from(["aidget", "screenshot", "note", "--json"]).is_ok());
    assert!(Cli::try_parse_from(["aidget", "displays", "--json"]).is_ok());
}

#[test]
fn template_command_is_not_public() {
    assert!(Cli::try_parse_from(["aidget", "template", "list"]).is_err());
}

#[test]
fn home_is_a_global_option_for_external_mcp_launches() {
    assert!(Cli::try_parse_from(["aidget", "--home", "/tmp/development", "mcp"]).is_ok());
    assert!(Cli::try_parse_from(["aidget", "doctor", "--home", "/tmp/development"]).is_ok());
}

#[test]
fn explicit_home_is_used_in_a_child_process() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("development");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_aidget"))
        .args(["doctor", "--json", "--home", home.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        std::path::Path::new(report["root"].as_str().unwrap()),
        dunce::canonicalize(home).unwrap()
    );
}

#[cfg(unix)]
#[test]
fn aliased_home_is_canonicalized_before_ipc_paths() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("data");
    std::fs::create_dir_all(&root).unwrap();
    let alias = temp.path().join("alias");
    std::os::unix::fs::symlink(&root, &alias).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_aidget"))
        .args(["doctor", "--json", "--home", alias.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let root = dunce::canonicalize(root).unwrap();
    assert_eq!(std::path::Path::new(report["root"].as_str().unwrap()), root);
    assert_eq!(
        report["bridge"].as_str().unwrap(),
        root.join("run/ui.sock").to_string_lossy()
    );
}

#[test]
fn a_launcher_collision_is_reported_without_blocking_existing_cli_commands() {
    let home = tempfile::tempdir().unwrap();
    let entry = aidget::runtime::process::cli_link(home.path());
    std::fs::write(&entry, "keep this file").unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_aidget"))
        .args(["--home", home.path().to_str().unwrap(), "doctor", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8(output.stderr)
        .unwrap()
        .contains("cannot publish the stable CLI entry"));
    assert_eq!(std::fs::read_to_string(entry).unwrap(), "keep this file");
}

#[cfg(unix)]
#[test]
fn the_default_directory_ignores_the_removed_home_environment_variable() {
    let sandbox = tempfile::tempdir().unwrap();
    let other = sandbox.path().join("removed-setting");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_aidget"))
        .args(["doctor", "--json"])
        .env("HOME", sandbox.path())
        .env("AIDGET_HOME", &other)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        std::path::Path::new(report["root"].as_str().unwrap()),
        dunce::canonicalize(sandbox.path().join(".aidget")).unwrap()
    );
    assert!(!other.exists());
}
