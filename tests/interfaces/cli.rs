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
