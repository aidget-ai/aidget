use std::fs;

use aidget::agents::config::{mcp_state, set_mcp, McpFormat, McpTarget};
use aidget::agents::launch::McpLaunch;
use tempfile::tempdir;

fn launch(command: &str) -> McpLaunch {
    McpLaunch {
        command: command.into(),
        args: vec!["mcp".into()],
    }
}

#[cfg(windows)]
const PLAIN_EXE: &str = r"C:\Program Files\aidget\aidget.exe";
#[cfg(windows)]
const LEGACY_EXE: &str = r"\\?\C:\Program Files\aidget\aidget.exe";

#[test]
fn json_mcp_entry_round_trips_without_touching_neighbors() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("config.json");
    fs::write(
        &path,
        "{\n  // keep\n  \"theme\": \"dark\",\n  \"mcpServers\": {\"other\": {\"command\": \"x\"}}\n}\n",
    )
    .unwrap();
    let target = McpTarget {
        path: path.clone(),
        format: McpFormat::Claude,
    };
    set_mcp(&target, &launch("/a"), true, temp.path(), "codex").unwrap();
    assert!(mcp_state(&target, &launch("/a")).unwrap().installed);
    assert!(fs::read_to_string(&path).unwrap().contains("// keep"));
    set_mcp(&target, &launch("/a"), false, temp.path(), "codex").unwrap();
    assert!(!mcp_state(&target, &launch("/a")).unwrap().installed);
    assert!(fs::read_to_string(&path).unwrap().contains("\"other\""));
}

#[cfg(windows)]
#[test]
fn legacy_verbatim_mcp_entries_are_recognized_and_removable() {
    for format in [
        McpFormat::Claude,
        McpFormat::Goose,
        McpFormat::OpenCode,
        McpFormat::Cline,
        McpFormat::Codex,
        McpFormat::Hermes,
        McpFormat::Dsh,
    ] {
        let temp = tempdir().unwrap();
        let target = McpTarget {
            path: temp.path().join(format!("{format:?}.config")),
            format,
        };
        set_mcp(&target, &launch(LEGACY_EXE), true, temp.path(), "agent").unwrap();
        let original = fs::read(&target.path).unwrap();

        let state = mcp_state(&target, &launch(PLAIN_EXE)).unwrap();
        assert!(state.installed, "{format:?}: {state:?}");
        assert_eq!(state.conflict, None, "{format:?}");
        set_mcp(&target, &launch(PLAIN_EXE), true, temp.path(), "agent").unwrap();
        assert_eq!(fs::read(&target.path).unwrap(), original, "{format:?}");

        set_mcp(&target, &launch(PLAIN_EXE), false, temp.path(), "agent").unwrap();
        let state = mcp_state(&target, &launch(PLAIN_EXE)).unwrap();
        assert!(!state.installed, "{format:?}");
        assert_eq!(state.conflict, None, "{format:?}");
    }
}

#[cfg(windows)]
#[test]
fn different_executable_is_replaceable_with_verbatim_paths() {
    for format in [
        McpFormat::Claude,
        McpFormat::Goose,
        McpFormat::OpenCode,
        McpFormat::Cline,
        McpFormat::Codex,
        McpFormat::Hermes,
        McpFormat::Dsh,
    ] {
        let temp = tempdir().unwrap();
        let target = McpTarget {
            path: temp.path().join(format!("{format:?}.config")),
            format,
        };
        set_mcp(
            &target,
            &launch(r"\\?\C:\Other\aidget.exe"),
            true,
            temp.path(),
            "agent",
        )
        .unwrap();

        let state = mcp_state(&target, &launch(PLAIN_EXE)).unwrap();
        assert!(!state.installed, "{format:?}");
        assert!(state.replaceable, "{format:?}");
        set_mcp(&target, &launch(PLAIN_EXE), true, temp.path(), "agent").unwrap();
        set_mcp(&target, &launch(PLAIN_EXE), false, temp.path(), "agent").unwrap();
        assert!(!mcp_state(&target, &launch(PLAIN_EXE)).unwrap().installed);
    }
}

#[cfg(windows)]
#[test]
fn dsh_mixed_entries_can_be_replaced() {
    let temp = tempdir().unwrap();
    let target = McpTarget {
        path: temp.path().join("cordis.patch.yml"),
        format: McpFormat::Dsh,
    };
    let original = serde_yaml::to_string(&serde_json::json!([
        {"insert": [{"id": "aidget-mcp", "config": {"command": LEGACY_EXE, "args": ["mcp"]}}]},
        {"insert": [{"id": "aidget-mcp", "config": {"command": r"C:\Other\aidget.exe", "args": ["mcp"]}}]}
    ]))
    .unwrap();
    fs::write(&target.path, &original).unwrap();

    let state = mcp_state(&target, &launch(PLAIN_EXE)).unwrap();
    assert!(!state.installed);
    assert!(state.replaceable);
    set_mcp(&target, &launch(PLAIN_EXE), true, temp.path(), "dsh").unwrap();
    set_mcp(&target, &launch(PLAIN_EXE), false, temp.path(), "dsh").unwrap();
    assert!(!mcp_state(&target, &launch(PLAIN_EXE)).unwrap().installed);
}

#[cfg(windows)]
#[test]
fn legacy_verbatim_entries_with_other_args_are_replaceable() {
    let configs = [
        (
            McpFormat::Claude,
            serde_json::json!({"mcpServers": {"aidget": {"command": LEGACY_EXE, "args": ["other"]}}})
                .to_string(),
        ),
        (
            McpFormat::Goose,
            serde_json::json!({"extensions": {"aidget": {"cmd": LEGACY_EXE, "args": ["other"]}}})
                .to_string(),
        ),
        (
            McpFormat::OpenCode,
            serde_json::json!({"mcp": {"aidget": {"command": [LEGACY_EXE, "other"]}}})
                .to_string(),
        ),
        (
            McpFormat::Cline,
            serde_json::json!({"mcpServers": {"aidget": {"transport": {"command": LEGACY_EXE, "args": ["other"]}}}})
                .to_string(),
        ),
        (
            McpFormat::Codex,
            format!("[mcp_servers.aidget]\ncommand = {LEGACY_EXE:?}\nargs = [\"other\"]\n"),
        ),
        (
            McpFormat::Hermes,
            serde_yaml::to_string(&serde_json::json!({"mcp_servers": {"aidget": {"command": LEGACY_EXE, "args": ["other"]}}})).unwrap(),
        ),
        (
            McpFormat::Dsh,
            serde_yaml::to_string(&serde_json::json!([{"insert": [{"id": "aidget-mcp", "config": {"command": LEGACY_EXE, "args": ["other"]}}]}])).unwrap(),
        ),
    ];

    for (format, original) in configs {
        let temp = tempdir().unwrap();
        let target = McpTarget {
            path: temp.path().join(format!("{format:?}.config")),
            format,
        };
        fs::write(&target.path, &original).unwrap();

        let state = mcp_state(&target, &launch(PLAIN_EXE)).unwrap();
        assert!(!state.installed, "{format:?}");
        assert!(state.replaceable, "{format:?}");
        set_mcp(&target, &launch(PLAIN_EXE), true, temp.path(), "agent").unwrap();
        set_mcp(&target, &launch(PLAIN_EXE), false, temp.path(), "agent").unwrap();
        assert!(!mcp_state(&target, &launch(PLAIN_EXE)).unwrap().installed);
    }
}

#[test]
fn opencode_and_cline_shapes_are_read_back() {
    let temp = tempdir().unwrap();
    let opencode = temp.path().join("opencode.json");
    let target = McpTarget {
        path: opencode.clone(),
        format: McpFormat::OpenCode,
    };
    set_mcp(&target, &launch("/a"), true, temp.path(), "opencode").unwrap();
    assert!(mcp_state(&target, &launch("/a")).unwrap().installed);

    let cline = temp.path().join("cline.json");
    let target = McpTarget {
        path: cline,
        format: McpFormat::Cline,
    };
    set_mcp(&target, &launch("/a"), true, temp.path(), "cline").unwrap();
    assert!(mcp_state(&target, &launch("/a")).unwrap().installed);
}

#[test]
fn codex_toml_round_trips_and_preserves_neighbors() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("config.toml");
    fs::write(
        &path,
        "model = \"gpt-5\"\n\n[mcp_servers.other]\ncommand = \"x\"\n",
    )
    .unwrap();
    let target = McpTarget {
        path: path.clone(),
        format: McpFormat::Codex,
    };
    set_mcp(&target, &launch("/a"), true, temp.path(), "codex").unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("model = \"gpt-5\""));
    assert!(text.contains("[mcp_servers.other]"));
    assert!(mcp_state(&target, &launch("/a")).unwrap().installed);
    set_mcp(&target, &launch("/a"), false, temp.path(), "codex").unwrap();
    assert!(fs::read_to_string(&path)
        .unwrap()
        .contains("[mcp_servers.other]"));
}

#[test]
fn yaml_and_dsh_formats_round_trip() {
    let temp = tempdir().unwrap();
    let hermes = temp.path().join("config.yaml");
    let target = McpTarget {
        path: hermes.clone(),
        format: McpFormat::Hermes,
    };
    set_mcp(&target, &launch("/a"), true, temp.path(), "hermes").unwrap();
    assert!(mcp_state(&target, &launch("/a")).unwrap().installed);
    set_mcp(&target, &launch("/a"), false, temp.path(), "hermes").unwrap();
    assert!(!mcp_state(&target, &launch("/a")).unwrap().installed);

    let dsh = temp.path().join("cordis.patch.yml");
    fs::write(&dsh, "[]\n").unwrap();
    let target = McpTarget {
        path: dsh,
        format: McpFormat::Dsh,
    };
    set_mcp(&target, &launch("/a"), true, temp.path(), "dsh").unwrap();
    assert!(mcp_state(&target, &launch("/a")).unwrap().installed);
    set_mcp(&target, &launch("/a"), false, temp.path(), "dsh").unwrap();
    assert!(!mcp_state(&target, &launch("/a")).unwrap().installed);
}

#[test]
fn dsh_enable_deduplicates_matching_home_entries() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("cordis.patch.yml");
    let entry =
        "- insert:\n  - id: aidget-mcp\n    config:\n      command: /new\n      args: [mcp]\n";
    fs::write(&path, format!("{entry}{entry}")).unwrap();
    let target = McpTarget {
        path,
        format: McpFormat::Dsh,
    };

    set_mcp(&target, &launch("/new"), true, temp.path(), "dsh").unwrap();

    let text = fs::read_to_string(&target.path).unwrap();
    assert_eq!(text.matches("aidget-mcp").count(), 1);
}

#[test]
fn dsh_home_patch_with_non_list_root_is_rejected() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("cordis.patch.yml");
    let original = "mcp: {}\n";
    fs::write(&path, original).unwrap();
    let target = McpTarget {
        path,
        format: McpFormat::Dsh,
    };

    assert!(mcp_state(&target, &launch("/new"))
        .unwrap()
        .conflict
        .is_some());
    assert!(set_mcp(&target, &launch("/new"), true, temp.path(), "dsh").is_err());
    assert_eq!(fs::read_to_string(&target.path).unwrap(), original);
}

#[test]
fn dsh_empty_home_patch_is_treated_as_an_empty_list() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("cordis.patch.yml");
    fs::write(&path, "\n  \n").unwrap();
    let target = McpTarget {
        path,
        format: McpFormat::Dsh,
    };

    let state = mcp_state(&target, &launch("/new")).unwrap();
    assert!(!state.installed);
    assert_eq!(state.conflict, None);

    set_mcp(&target, &launch("/new"), true, temp.path(), "dsh").unwrap();
    assert!(mcp_state(&target, &launch("/new")).unwrap().installed);
}

#[test]
fn dsh_home_entry_replaces_profile_entry_and_preserves_neighbors() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("dsh-home");
    let profile = home.join("profiles/desktop");
    fs::create_dir_all(&profile).unwrap();
    let profile_patch = profile.join("cordis.patch.yml");
    fs::write(
        &profile_patch,
        "- id: keep\n  name: \"@example/keep\"\n  config:\n    value: 1\n- insert:\n  - id: aidget-mcp\n    name: \"@deepseek-ai/dsh-mcp-client\"\n    config:\n      serverName: aidget\n      transport: stdio\n      command: /old\n      args:\n        - mcp\n",
    )
    .unwrap();
    let target = McpTarget {
        path: home.join("cordis.patch.yml"),
        format: McpFormat::Dsh,
    };
    fs::write(
        &target.path,
        "- id: keep-global\n  name: \"@example/global\"\n",
    )
    .unwrap();

    set_mcp(&target, &launch("/new"), true, temp.path(), "dsh").unwrap();

    assert!(mcp_state(&target, &launch("/new")).unwrap().installed);
    let global_text = fs::read_to_string(&target.path).unwrap();
    assert!(global_text.contains("keep-global"));
    assert!(global_text.contains("@example/global"));
    let profile_text = fs::read_to_string(&profile_patch).unwrap();
    assert!(!profile_text.contains("aidget-mcp"));
    assert!(profile_text.contains("id: keep"));
    assert!(profile_text.contains("@example/keep"));

    // A legacy profile entry next to the new home entry must be removed too.
    fs::write(
        &profile_patch,
        "- id: keep\n  name: \"@example/keep\"\n  config:\n    value: 1\n- insert:\n  - id: aidget-mcp\n    name: \"@deepseek-ai/dsh-mcp-client\"\n    config:\n      serverName: aidget\n      transport: stdio\n      command: /old\n      args:\n        - mcp\n",
    )
    .unwrap();
    set_mcp(&target, &launch("/new"), true, temp.path(), "dsh").unwrap();
    assert!(mcp_state(&target, &launch("/new")).unwrap().installed);
    assert!(!fs::read_to_string(&profile_patch)
        .unwrap()
        .contains("aidget-mcp"));

    set_mcp(&target, &launch("/new"), false, temp.path(), "dsh").unwrap();
    assert!(!mcp_state(&target, &launch("/new")).unwrap().installed);
    assert!(!fs::read_to_string(&profile_patch)
        .unwrap()
        .contains("aidget-mcp"));
}

fn profile_with_inserts(temp: &std::path::Path, inserts: &str) -> (McpTarget, std::path::PathBuf) {
    let home = temp.join("dsh-home");
    let profile = home.join("profiles/desktop");
    fs::create_dir_all(&profile).unwrap();
    let profile_patch = profile.join("cordis.patch.yml");
    fs::write(&profile_patch, format!("- insert:\n{inserts}")).unwrap();
    (
        McpTarget {
            path: home.join("cordis.patch.yml"),
            format: McpFormat::Dsh,
        },
        profile_patch,
    )
}

#[test]
fn dsh_profile_non_list_root_is_rejected_before_home_write() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("dsh-home");
    let profile = home.join("profiles/desktop");
    fs::create_dir_all(&profile).unwrap();
    let profile_patch = profile.join("cordis.patch.yml");
    let original =
        "insert:\n  - id: aidget-mcp\n    config:\n      command: /old\n      args: [mcp]\n";
    fs::write(&profile_patch, original).unwrap();
    let target = McpTarget {
        path: home.join("cordis.patch.yml"),
        format: McpFormat::Dsh,
    };
    fs::write(&target.path, "[]\n").unwrap();

    assert!(set_mcp(&target, &launch("/new"), true, temp.path(), "dsh").is_err());
    assert_eq!(fs::read_to_string(&target.path).unwrap(), "[]\n");
    assert_eq!(fs::read_to_string(&profile_patch).unwrap(), original);
}

#[test]
fn dsh_cleanup_finds_escaped_aidget_ids() {
    let temp = tempdir().unwrap();
    let (target, profile_patch) = profile_with_inserts(
        temp.path(),
        "  - id: \"\\u0061idget-mcp\"\n    name: \"@deepseek-ai/dsh-mcp-client\"\n    config:\n      serverName: aidget\n      transport: stdio\n      command: /old\n      args:\n        - mcp\n",
    );

    set_mcp(&target, &launch("/new"), true, temp.path(), "dsh").unwrap();

    let text = fs::read_to_string(&profile_patch).unwrap();
    assert!(!text.contains("@deepseek-ai/dsh-mcp-client"));
}

#[test]
fn dsh_cleanup_preserves_sibling_items_when_aidget_is_first() {
    let temp = tempdir().unwrap();
    let (target, profile_patch) = profile_with_inserts(
        temp.path(),
        "  - id: aidget-mcp\n    name: \"@deepseek-ai/dsh-mcp-client\"\n    config:\n      serverName: aidget\n      transport: stdio\n      command: /old\n      args:\n        - mcp\n  - id: other\n    name: \"@example/other\"\n    config:\n      value: 1\n",
    );

    set_mcp(&target, &launch("/new"), true, temp.path(), "dsh").unwrap();

    let text = fs::read_to_string(&profile_patch).unwrap();
    assert!(!text.contains("aidget-mcp"));
    assert!(text.contains("id: other"));
    assert!(text.contains("@example/other"));
    assert!(mcp_state(&target, &launch("/new")).unwrap().installed);
}

#[test]
fn dsh_cleanup_preserves_sibling_items_when_aidget_is_not_first() {
    let temp = tempdir().unwrap();
    let (target, profile_patch) = profile_with_inserts(
        temp.path(),
        "  - id: other\n    name: \"@example/other\"\n    config:\n      value: 1\n  - id: aidget-mcp\n    name: \"@deepseek-ai/dsh-mcp-client\"\n    config:\n      serverName: aidget\n      transport: stdio\n      command: /old\n      args:\n        - mcp\n",
    );

    set_mcp(&target, &launch("/new"), true, temp.path(), "dsh").unwrap();

    let text = fs::read_to_string(&profile_patch).unwrap();
    assert!(!text.contains("aidget-mcp"));
    assert!(text.contains("id: other"));
    assert!(text.contains("@example/other"));
    assert!(mcp_state(&target, &launch("/new")).unwrap().installed);
}

#[test]
fn dsh_cleanup_preserves_outer_patch_fields() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("dsh-home");
    let profile = home.join("profiles/desktop");
    fs::create_dir_all(&profile).unwrap();
    let profile_patch = profile.join("cordis.patch.yml");
    fs::write(
        &profile_patch,
        "- id: wrapper\n  name: \"@example/wrapper\"\n  insert:\n    - id: aidget-mcp\n      name: \"@deepseek-ai/dsh-mcp-client\"\n      config:\n        serverName: aidget\n        transport: stdio\n        command: /old\n        args:\n          - mcp\n    - id: other\n      name: \"@example/other\"\n      config:\n        value: 1\n",
    )
    .unwrap();
    let target = McpTarget {
        path: home.join("cordis.patch.yml"),
        format: McpFormat::Dsh,
    };

    set_mcp(&target, &launch("/new"), true, temp.path(), "dsh").unwrap();

    let text = fs::read_to_string(&profile_patch).unwrap();
    assert!(!text.contains("aidget-mcp"));
    assert!(text.contains("id: wrapper"));
    assert!(text.contains("@example/wrapper"));
    assert!(text.contains("id: other"));
    assert!(text.contains("@example/other"));
}

#[cfg(windows)]
#[test]
fn dsh_cleanup_failure_keeps_the_working_profile_entry() {
    let temp = tempdir().unwrap();
    let (target, profile_patch) = profile_with_inserts(
        temp.path(),
        "  - id: aidget-mcp\n    name: \"@deepseek-ai/dsh-mcp-client\"\n    config:\n      serverName: aidget\n      transport: stdio\n      command: /old\n      args:\n        - mcp\n",
    );
    let original = fs::read_to_string(&profile_patch).unwrap();
    fs::write(&target.path, "[]\n").unwrap();
    let mut permissions = fs::metadata(&target.path).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&target.path, permissions).unwrap();

    let result = set_mcp(&target, &launch("/new"), true, temp.path(), "dsh");

    assert!(result.is_err());
    assert_eq!(fs::read_to_string(&profile_patch).unwrap(), original);

    let mut permissions = fs::metadata(&target.path).unwrap().permissions();
    permissions.set_readonly(false);
    fs::set_permissions(&target.path, permissions).unwrap();
}

#[test]
fn install_overwrites_a_different_named_entry() {
    let temp = tempdir().unwrap();
    let target = McpTarget {
        path: temp.path().join("config.json"),
        format: McpFormat::Claude,
    };
    fs::write(&target.path, r#"{"mcpServers":{"aidget":{"command":"/old","args":["other"]},"keep":{"command":"keep"}}}"#).unwrap();
    set_mcp(&target, &launch("/new"), true, temp.path(), "fixture").unwrap();
    assert!(mcp_state(&target, &launch("/new")).unwrap().installed);
    assert!(fs::read_to_string(&target.path).unwrap().contains("keep"));
}

#[test]
fn uninstall_removes_a_different_named_entry() {
    let temp = tempdir().unwrap();
    let target = McpTarget {
        path: temp.path().join("config.json"),
        format: McpFormat::Claude,
    };
    fs::write(
        &target.path,
        r#"{"mcpServers":{"aidget":{"command":"/old"},"keep":{"command":"keep"}}}"#,
    )
    .unwrap();
    set_mcp(&target, &launch("/new"), false, temp.path(), "fixture").unwrap();
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(&target.path).unwrap()).unwrap();
    assert!(value["mcpServers"].get("aidget").is_none());
    assert_eq!(value["mcpServers"]["keep"]["command"], "keep");
}

#[test]
fn every_format_switches_homes_and_uninstalls_by_name() {
    for format in [
        McpFormat::Claude,
        McpFormat::Codex,
        McpFormat::Gemini,
        McpFormat::OpenCode,
        McpFormat::Omp,
        McpFormat::Antigravity,
        McpFormat::Goose,
        McpFormat::Copilot,
        McpFormat::Crush,
        McpFormat::ZCode,
        McpFormat::Zed,
        McpFormat::Kimi,
        McpFormat::Devin,
        McpFormat::Cline,
        McpFormat::CommandCode,
        McpFormat::Simple,
        McpFormat::Hermes,
        McpFormat::Dsh,
    ] {
        let temp = tempdir().unwrap();
        let target = McpTarget {
            path: temp.path().join("config"),
            format,
        };
        let production = McpLaunch {
            command: "/production/aidget".into(),
            args: vec!["--home".into(), "/production".into(), "mcp".into()],
        };
        let development = McpLaunch {
            command: "/development/aidget".into(),
            args: vec!["--home".into(), "/development space".into(), "mcp".into()],
        };
        for current in [&production, &development, &production] {
            set_mcp(&target, current, true, temp.path(), "fixture").unwrap();
            assert!(mcp_state(&target, current).unwrap().installed, "{format:?}");
            let other = if current == &production {
                &development
            } else {
                &production
            };
            let state = mcp_state(&target, other).unwrap();
            assert!(!state.installed, "{format:?}");
            assert!(state.replaceable && state.conflict.is_none(), "{format:?}");
            let original = fs::read(&target.path).unwrap();
            set_mcp(&target, current, true, temp.path(), "fixture").unwrap();
            assert_eq!(fs::read(&target.path).unwrap(), original, "{format:?}");
        }
        set_mcp(&target, &development, false, temp.path(), "fixture").unwrap();
        let state = mcp_state(&target, &production).unwrap();
        assert!(
            !state.installed && !state.replaceable && state.conflict.is_none(),
            "{format:?}"
        );
        let original = fs::read(&target.path).unwrap();
        set_mcp(&target, &development, false, temp.path(), "fixture").unwrap();
        assert_eq!(fs::read(&target.path).unwrap(), original);
        assert_eq!(
            fs::read_dir(temp.path().join("state/agent-backups"))
                .unwrap()
                .count(),
            3
        );
    }
}

#[test]
fn malformed_configs_are_reported_and_never_overwritten() {
    for (format, original) in [
        (McpFormat::Claude, "{"),
        (McpFormat::Codex, "[mcp_servers"),
        (McpFormat::Hermes, "mcp_servers: ["),
        (McpFormat::Dsh, "insert: ["),
        (McpFormat::Claude, "[]"),
        (McpFormat::Codex, "mcp_servers = 42"),
        (McpFormat::Hermes, "mcp_servers: 42"),
    ] {
        let temp = tempdir().unwrap();
        let target = McpTarget {
            path: temp.path().join("config"),
            format,
        };
        fs::write(&target.path, original).unwrap();
        let state = mcp_state(&target, &launch("/aidget")).unwrap();
        assert!(
            state.conflict.is_some() && !state.replaceable,
            "{format:?}: {original}"
        );
        for enabled in [true, false] {
            assert!(set_mcp(&target, &launch("/aidget"), enabled, temp.path(), "fixture").is_err());
            assert_eq!(fs::read_to_string(&target.path).unwrap(), original);
        }
    }
}

#[test]
fn codex_scalar_aidget_entry_is_replaceable_and_removable() {
    let temp = tempdir().unwrap();
    let target = McpTarget {
        path: temp.path().join("config.toml"),
        format: McpFormat::Codex,
    };
    fs::write(&target.path, "[mcp_servers]\naidget = 42\nother = 7\n").unwrap();
    assert!(mcp_state(&target, &launch("/aidget")).unwrap().replaceable);
    set_mcp(&target, &launch("/aidget"), false, temp.path(), "fixture").unwrap();
    assert_eq!(
        fs::read_to_string(&target.path).unwrap(),
        "[mcp_servers]\nother = 7\n"
    );
}
