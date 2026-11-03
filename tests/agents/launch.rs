use aidget::agents::launch::McpLaunch;
#[cfg(not(windows))]
use aidget::runtime::process::cli_path;
use tempfile::tempdir;

#[test]
fn display_omits_default_home_while_the_installed_command_keeps_it() {
    let home = tempdir().unwrap();
    let actual = McpLaunch::for_root(home.path()).unwrap();
    let display = McpLaunch::for_display(home.path(), home.path()).unwrap();
    assert_eq!(actual.command, display.command);
    assert_ne!(actual.args, display.args);
    assert_eq!(display.args, ["mcp"]);
    assert!(actual.args.join(" ").contains("--home"));
}

#[test]
fn display_keeps_custom_home_even_when_the_default_directory_is_absent() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("development");
    std::fs::create_dir(&home).unwrap();
    let default = temp.path().join("absent-default");
    let display = McpLaunch::for_display(&home, &default).unwrap();
    let home = dunce::canonicalize(home).unwrap();
    assert_eq!(display.command, McpLaunch::for_root(&home).unwrap().command);
    assert_eq!(display.args, ["--home", &home.to_string_lossy(), "mcp"]);
}

#[cfg(unix)]
#[test]
fn default_home_detection_resolves_directory_aliases() {
    let home = tempdir().unwrap();
    let default = home.path().join("default");
    let alias = home.path().join("alias");
    std::fs::create_dir(&default).unwrap();
    std::os::unix::fs::symlink(&default, &alias).unwrap();
    let display = McpLaunch::for_display(&alias, &default).unwrap();
    assert_eq!(display.args, ["mcp"]);
}

#[test]
fn launch_binds_an_absolute_home() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("development space '中文");
    std::fs::create_dir_all(&home).unwrap();
    let launch = McpLaunch::for_root(&home).unwrap();
    let home = dunce::canonicalize(home).unwrap();
    assert_eq!(launch.args, ["--home", &home.to_string_lossy(), "mcp"]);
    #[cfg(not(windows))]
    assert_eq!(launch.command, cli_path(&home).to_string_lossy());
    #[cfg(windows)]
    {
        assert_eq!(launch.command, home.join("aidget.exe").to_string_lossy());
        assert!(!launch.command.starts_with(r"\\?\"));
    }
}

#[cfg(windows)]
#[test]
fn windows_launch_passes_shell_characters_as_literal_home_arguments() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home %literal% ! & 中文");
    std::fs::create_dir(&home).unwrap();
    let launch = McpLaunch::for_root(&home).unwrap();
    let home = dunce::canonicalize(home).unwrap();
    assert_eq!(launch.args, ["--home", &home.to_string_lossy(), "mcp"]);
    assert_eq!(
        McpLaunch::for_display(&home, &temp.path().join("absent-default")).unwrap(),
        launch
    );
}

#[cfg(unix)]
#[test]
fn an_external_launch_uses_its_bound_home() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("development space '中文");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_aidget"))
        .args(["--home", home.to_str().unwrap(), "doctor", "--json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let launch = McpLaunch::for_root(&home).unwrap();
    let output = std::process::Command::new(&launch.command)
        .args(&launch.args[..2])
        .args(["doctor", "--json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        dunce::canonicalize(report["root"].as_str().unwrap()).unwrap(),
        dunce::canonicalize(&home).unwrap()
    );
}
