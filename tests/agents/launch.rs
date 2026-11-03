use aidget::agents::launch::McpLaunch;
use aidget::runtime::process::cli_link;
use tempfile::tempdir;

#[test]
fn display_omits_default_home_while_the_installed_command_keeps_it() {
    let home = tempdir().unwrap();
    let actual = McpLaunch::for_root(home.path()).unwrap();
    let display = McpLaunch::for_display(home.path(), home.path()).unwrap();
    assert_eq!(actual.command, display.command);
    assert_ne!(actual.args, display.args);
    #[cfg(not(windows))]
    assert_eq!(display.args, ["mcp"]);
    #[cfg(windows)]
    assert_eq!(
        display.args[4],
        format!(
            "\"\"{}\" mcp\"",
            cli_link(&dunce::canonicalize(home.path()).unwrap()).display()
        )
    );
    assert!(actual.args.join(" ").contains("--home"));
}

#[test]
fn display_keeps_custom_home_even_when_the_default_directory_is_absent() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("development");
    std::fs::create_dir(&home).unwrap();
    let default = temp.path().join("absent-default");
    assert_eq!(
        McpLaunch::for_display(&home, &default).unwrap(),
        McpLaunch::for_root(&home).unwrap(),
    );
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
fn launch_binds_an_absolute_home_without_resolving_the_stable_entry() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("development space '中文");
    std::fs::create_dir_all(&home).unwrap();
    let launch = McpLaunch::for_root(&home).unwrap();
    let home = dunce::canonicalize(home).unwrap();
    #[cfg(not(windows))]
    {
        assert_eq!(launch.command, cli_link(&home).to_string_lossy());
        assert_eq!(launch.args, ["--home", &home.to_string_lossy(), "mcp"]);
    }
    #[cfg(windows)]
    {
        assert!(launch.command.ends_with("System32\\cmd.exe"));
        assert_eq!(&launch.args[..4], ["/d", "/v:off", "/s", "/c"]);
        assert_eq!(
            launch.args[4],
            format!(
                "\"\"{}\" --home \"{}\" mcp\"",
                cli_link(&home).display(),
                home.display()
            )
        );
    }
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
