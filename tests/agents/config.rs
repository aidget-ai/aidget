use std::fs;

use aidget::agents::config::{mcp_state, set_mcp, McpFormat, McpTarget};
use tempfile::tempdir;

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
    set_mcp(&target, "/a", true, temp.path(), "codex").unwrap();
    assert!(mcp_state(&target, "/a").unwrap().installed);
    assert!(fs::read_to_string(&path).unwrap().contains("// keep"));
    set_mcp(&target, "/a", false, temp.path(), "codex").unwrap();
    assert!(!mcp_state(&target, "/a").unwrap().installed);
    assert!(fs::read_to_string(&path).unwrap().contains("\"other\""));
}

#[test]
fn conflicting_mcp_entry_is_rejected() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("config.json");
    fs::write(
        &path,
        "{\"mcpServers\":{\"aidget\":{\"command\":\"/old\",\"args\":[\"mcp\"]}}}",
    )
    .unwrap();
    let target = McpTarget {
        path,
        format: McpFormat::Claude,
    };
    assert_eq!(
        mcp_state(&target, "/new").unwrap().conflict.as_deref(),
        Some("a different `aidget` entry already exists")
    );
    assert!(set_mcp(&target, "/new", true, temp.path(), "claude").is_err());
}

#[test]
fn opencode_and_cline_shapes_are_read_back() {
    let temp = tempdir().unwrap();
    let opencode = temp.path().join("opencode.json");
    let target = McpTarget {
        path: opencode.clone(),
        format: McpFormat::OpenCode,
    };
    set_mcp(&target, "/a", true, temp.path(), "opencode").unwrap();
    assert!(mcp_state(&target, "/a").unwrap().installed);

    let cline = temp.path().join("cline.json");
    let target = McpTarget {
        path: cline,
        format: McpFormat::Cline,
    };
    set_mcp(&target, "/a", true, temp.path(), "cline").unwrap();
    assert!(mcp_state(&target, "/a").unwrap().installed);
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
    set_mcp(&target, "/a", true, temp.path(), "codex").unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("model = \"gpt-5\""));
    assert!(text.contains("[mcp_servers.other]"));
    assert!(mcp_state(&target, "/a").unwrap().installed);
    set_mcp(&target, "/a", false, temp.path(), "codex").unwrap();
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
    set_mcp(&target, "/a", true, temp.path(), "hermes").unwrap();
    assert!(mcp_state(&target, "/a").unwrap().installed);
    set_mcp(&target, "/a", false, temp.path(), "hermes").unwrap();
    assert!(!mcp_state(&target, "/a").unwrap().installed);

    let dsh = temp.path().join("cordis.patch.yml");
    fs::write(&dsh, "[]\n").unwrap();
    let target = McpTarget {
        path: dsh,
        format: McpFormat::Dsh,
    };
    set_mcp(&target, "/a", true, temp.path(), "dsh").unwrap();
    assert!(mcp_state(&target, "/a").unwrap().installed);
    set_mcp(&target, "/a", false, temp.path(), "dsh").unwrap();
    assert!(!mcp_state(&target, "/a").unwrap().installed);
}

#[test]
fn dsh_enable_deduplicates_matching_home_entries() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("cordis.patch.yml");
    let entry = "- insert:\n  - id: magpie-mcp-aidget\n    config:\n      command: /new\n      args: [mcp]\n";
    fs::write(&path, format!("{entry}{entry}")).unwrap();
    let target = McpTarget {
        path,
        format: McpFormat::Dsh,
    };

    set_mcp(&target, "/new", true, temp.path(), "dsh").unwrap();

    let text = fs::read_to_string(&target.path).unwrap();
    assert_eq!(text.matches("magpie-mcp-aidget").count(), 1);
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

    assert!(mcp_state(&target, "/new").unwrap().conflict.is_some());
    assert!(set_mcp(&target, "/new", true, temp.path(), "dsh").is_err());
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

    let state = mcp_state(&target, "/new").unwrap();
    assert!(!state.installed);
    assert_eq!(state.conflict, None);

    set_mcp(&target, "/new", true, temp.path(), "dsh").unwrap();
    assert!(mcp_state(&target, "/new").unwrap().installed);
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
        "- id: keep\n  name: \"@example/keep\"\n  config:\n    value: 1\n- insert:\n  - id: magpie-mcp-aidget\n    name: \"@deepseek-ai/dsh-mcp-client\"\n    config:\n      serverName: aidget\n      transport: stdio\n      command: /old\n      args:\n        - mcp\n",
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

    set_mcp(&target, "/new", true, temp.path(), "dsh").unwrap();

    assert!(mcp_state(&target, "/new").unwrap().installed);
    let global_text = fs::read_to_string(&target.path).unwrap();
    assert!(global_text.contains("keep-global"));
    assert!(global_text.contains("@example/global"));
    let profile_text = fs::read_to_string(&profile_patch).unwrap();
    assert!(!profile_text.contains("magpie-mcp-aidget"));
    assert!(profile_text.contains("id: keep"));
    assert!(profile_text.contains("@example/keep"));

    // A legacy profile entry next to the new home entry must be removed too.
    fs::write(
        &profile_patch,
        "- id: keep\n  name: \"@example/keep\"\n  config:\n    value: 1\n- insert:\n  - id: magpie-mcp-aidget\n    name: \"@deepseek-ai/dsh-mcp-client\"\n    config:\n      serverName: aidget\n      transport: stdio\n      command: /old\n      args:\n        - mcp\n",
    )
    .unwrap();
    set_mcp(&target, "/new", true, temp.path(), "dsh").unwrap();
    assert!(mcp_state(&target, "/new").unwrap().installed);
    assert!(!fs::read_to_string(&profile_patch)
        .unwrap()
        .contains("magpie-mcp-aidget"));

    set_mcp(&target, "/new", false, temp.path(), "dsh").unwrap();
    assert!(!mcp_state(&target, "/new").unwrap().installed);
    assert!(!fs::read_to_string(&profile_patch)
        .unwrap()
        .contains("magpie-mcp-aidget"));
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
        "insert:\n  - id: magpie-mcp-aidget\n    config:\n      command: /old\n      args: [mcp]\n";
    fs::write(&profile_patch, original).unwrap();
    let target = McpTarget {
        path: home.join("cordis.patch.yml"),
        format: McpFormat::Dsh,
    };
    fs::write(&target.path, "[]\n").unwrap();

    assert!(set_mcp(&target, "/new", true, temp.path(), "dsh").is_err());
    assert_eq!(fs::read_to_string(&target.path).unwrap(), "[]\n");
    assert_eq!(fs::read_to_string(&profile_patch).unwrap(), original);
}

#[test]
fn dsh_cleanup_finds_escaped_aidget_ids() {
    let temp = tempdir().unwrap();
    let (target, profile_patch) = profile_with_inserts(
        temp.path(),
        "  - id: \\\"\\u006dagpie-mcp-aidget\\\"\n    name: \\\"@deepseek-ai/dsh-mcp-client\\\"\n    config:\n      serverName: aidget\n      transport: stdio\n      command: /old\n      args:\n        - mcp\n",
    );

    set_mcp(&target, "/new", true, temp.path(), "dsh").unwrap();

    let text = fs::read_to_string(&profile_patch).unwrap();
    assert!(!text.contains("magpie-mcp-aidget"));
}

#[test]
fn dsh_cleanup_preserves_sibling_items_when_aidget_is_first() {
    let temp = tempdir().unwrap();
    let (target, profile_patch) = profile_with_inserts(
        temp.path(),
        "  - id: magpie-mcp-aidget\n    name: \"@deepseek-ai/dsh-mcp-client\"\n    config:\n      serverName: aidget\n      transport: stdio\n      command: /old\n      args:\n        - mcp\n  - id: other\n    name: \"@example/other\"\n    config:\n      value: 1\n",
    );

    set_mcp(&target, "/new", true, temp.path(), "dsh").unwrap();

    let text = fs::read_to_string(&profile_patch).unwrap();
    assert!(!text.contains("magpie-mcp-aidget"));
    assert!(text.contains("id: other"));
    assert!(text.contains("@example/other"));
    assert!(mcp_state(&target, "/new").unwrap().installed);
}

#[test]
fn dsh_cleanup_preserves_sibling_items_when_aidget_is_not_first() {
    let temp = tempdir().unwrap();
    let (target, profile_patch) = profile_with_inserts(
        temp.path(),
        "  - id: other\n    name: \"@example/other\"\n    config:\n      value: 1\n  - id: magpie-mcp-aidget\n    name: \"@deepseek-ai/dsh-mcp-client\"\n    config:\n      serverName: aidget\n      transport: stdio\n      command: /old\n      args:\n        - mcp\n",
    );

    set_mcp(&target, "/new", true, temp.path(), "dsh").unwrap();

    let text = fs::read_to_string(&profile_patch).unwrap();
    assert!(!text.contains("magpie-mcp-aidget"));
    assert!(text.contains("id: other"));
    assert!(text.contains("@example/other"));
    assert!(mcp_state(&target, "/new").unwrap().installed);
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
        "- id: wrapper\n  name: \"@example/wrapper\"\n  insert:\n    - id: magpie-mcp-aidget\n      name: \"@deepseek-ai/dsh-mcp-client\"\n      config:\n        serverName: aidget\n        transport: stdio\n        command: /old\n        args:\n          - mcp\n    - id: other\n      name: \"@example/other\"\n      config:\n        value: 1\n",
    )
    .unwrap();
    let target = McpTarget {
        path: home.join("cordis.patch.yml"),
        format: McpFormat::Dsh,
    };

    set_mcp(&target, "/new", true, temp.path(), "dsh").unwrap();

    let text = fs::read_to_string(&profile_patch).unwrap();
    assert!(!text.contains("magpie-mcp-aidget"));
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
        "  - id: magpie-mcp-aidget\n    name: \"@deepseek-ai/dsh-mcp-client\"\n    config:\n      serverName: aidget\n      transport: stdio\n      command: /old\n      args:\n        - mcp\n",
    );
    let original = fs::read_to_string(&profile_patch).unwrap();
    fs::write(&target.path, "[]\n").unwrap();
    let mut permissions = fs::metadata(&target.path).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&target.path, permissions).unwrap();

    let result = set_mcp(&target, "/new", true, temp.path(), "dsh");

    assert!(result.is_err());
    assert_eq!(fs::read_to_string(&profile_patch).unwrap(), original);

    let mut permissions = fs::metadata(&target.path).unwrap().permissions();
    permissions.set_readonly(false);
    fs::set_permissions(&target.path, permissions).unwrap();
}
