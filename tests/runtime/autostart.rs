#[cfg(target_os = "macos")]
#[test]
fn refresh_updates_an_enabled_launch_agent_without_enabling_a_disabled_one() {
    use aidget::runtime::autostart::refresh;
    use tauri::test::{mock_builder, mock_context, noop_assets};
    use tauri_plugin_autostart::ManagerExt;

    const WORKER: &str = "AIDGET_TEST_AUTOSTART_HOME";
    let Some(home) = std::env::var_os(WORKER) else {
        let home = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "runtime::autostart::refresh_updates_an_enabled_launch_agent_without_enabling_a_disabled_one",
                "--nocapture",
            ])
            .env(WORKER, home.path())
            .env("HOME", home.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    };
    let home = std::path::PathBuf::from(home);
    assert_eq!(dirs::home_dir().unwrap(), home);
    let data = home.join("custom data");
    let app = mock_builder()
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .app_name("aidget-autostart-test")
                .args(["--home", data.to_str().unwrap(), "ui", "--no-main-window"])
                .build(),
        )
        .build(mock_context(noop_assets()))
        .unwrap();
    let entry = home.join("Library/LaunchAgents/aidget-autostart-test.plist");
    std::fs::create_dir_all(entry.parent().unwrap()).unwrap();

    refresh(app.handle(), false).unwrap();
    assert!(!entry.exists(), "disabled autostart must stay disabled");

    std::fs::write(&entry, "stale registration for a removed executable").unwrap();
    assert!(app.autolaunch().is_enabled().unwrap());
    refresh(app.handle(), false).unwrap();
    let output = std::process::Command::new("/usr/bin/plutil")
        .args(["-convert", "json", "-o", "-"])
        .arg(&entry)
        .output()
        .unwrap();
    assert!(output.status.success(), "invalid refreshed LaunchAgent");
    let plist: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        plist["ProgramArguments"],
        serde_json::json!([
            std::env::current_exe().unwrap().canonicalize().unwrap(),
            "--home",
            data,
            "ui",
            "--no-main-window"
        ])
    );
    assert_eq!(plist["RunAtLoad"], true);

    let release_entry = std::fs::read(&entry).unwrap();
    let dev = mock_builder()
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .app_name("aidget-dev-autostart-test")
                .build(),
        )
        .build(mock_context(noop_assets()))
        .unwrap();
    dev.autolaunch().enable().unwrap();
    refresh(dev.handle(), true).unwrap();
    assert!(!dev.autolaunch().is_enabled().unwrap());
    refresh(dev.handle(), true).unwrap();
    assert_eq!(std::fs::read(&entry).unwrap(), release_entry);

    app.autolaunch().disable().unwrap();
    refresh(app.handle(), false).unwrap();
    assert!(!entry.exists());

    std::fs::create_dir(&entry).unwrap();
    assert!(
        refresh(app.handle(), false).is_err(),
        "refresh errors must propagate"
    );
}
