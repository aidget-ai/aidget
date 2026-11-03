use aidget::agents::config::{AgentResourceState, McpFormat, McpTarget};
use aidget::agents::integration::{
    apply_change, select_agents, skill_path, AgentIntegration, IntegrationResource,
    IntegrationState,
};
use aidget::agents::launch::McpLaunch;
use aidget::agents::registry::AgentSpec;
use aidget::agents::skill::{ensure_skill, skill_state, SKILL_CONTENT};
use aidget::widgets::store::Store;
use tempfile::tempdir;

#[test]
fn integration_state_uses_camel_case_and_keeps_agent_capabilities() {
    let state = IntegrationState {
        mcp_command: "/aidget".to_string(),
        mcp_args: vec!["mcp".to_string()],
        mcp_display_command: "/aidget".to_string(),
        mcp_display_args: vec!["mcp".to_string()],
        skill_directory: "/tmp/skills/aidget".to_string(),
        skill_content: "-".to_string(),
        data_directory: "/tmp".to_string(),
        agents: vec![AgentIntegration {
            id: "codex".to_string(),
            name: "Codex".to_string(),
            icon: "codex-color".to_string(),
            mcp: Some(AgentResourceState {
                path: "/tmp/config.toml".to_string(),
                installed: true,
                replaceable: false,
                conflict: None,
            }),
            skills: None,
        }],
    };
    let value = serde_json::to_value(state).unwrap();
    assert_eq!(value["mcpCommand"], "/aidget");
    assert_eq!(value["mcpDisplayCommand"], "/aidget");
    assert_eq!(value["skillDirectory"], "/tmp/skills/aidget");
    assert_eq!(value["agents"][0]["id"], "codex");
    assert_eq!(value["agents"][0]["mcp"]["installed"], true);
    assert!(value["agents"][0].get("skills").is_none());
}

#[test]
fn selected_agents_must_be_detected() {
    let agents = vec![AgentSpec {
        id: "codex",
        name: "Codex",
        icon: "codex-color",
        commands: vec![],
        detect: vec![],
        mcp: Some(McpTarget {
            path: "/tmp/config.toml".into(),
            format: McpFormat::Codex,
        }),
        skills: None,
    }];
    assert!(select_agents(&agents, &["codex".to_string()]).is_ok());
    assert!(select_agents(&agents, &["missing".to_string()]).is_err());
}

#[test]
fn skill_install_path_is_the_aidget_folder_under_the_agent_root() {
    assert_eq!(
        skill_path(std::path::Path::new("/tmp/codex/skills")),
        std::path::PathBuf::from("/tmp/codex/skills/aidget")
    );
}

#[test]
fn batch_change_installs_mcp_and_skill_for_a_detected_agent() {
    let temp = tempdir().unwrap();
    let skill_directory = ensure_skill(&Store::new(temp.path())).unwrap();
    let mcp_path = temp.path().join("config.json");
    let skills = temp.path().join("agent/skills");
    let agent = AgentSpec {
        id: "fixture",
        name: "Fixture",
        icon: "generic",
        commands: vec![],
        detect: vec![],
        mcp: Some(McpTarget {
            path: mcp_path.clone(),
            format: McpFormat::Claude,
        }),
        skills: Some(skills.clone()),
    };
    let agents = vec![&agent];
    assert!(apply_change(
        &agents,
        &McpLaunch {
            command: "/aidget".into(),
            args: vec!["mcp".into()]
        },
        &skill_directory,
        temp.path(),
        IntegrationResource::Mcp,
        true,
    )
    .is_empty());
    assert!(apply_change(
        &agents,
        &McpLaunch {
            command: "/aidget".into(),
            args: vec!["mcp".into()]
        },
        &skill_directory,
        temp.path(),
        IntegrationResource::Skills,
        true,
    )
    .is_empty());
    assert!(mcp_path.exists());
    assert!(skills.join("aidget/SKILL.md").exists());
    assert!(
        skill_state(&skills.join("aidget"), &skill_directory)
            .unwrap()
            .installed
    );
}

#[test]
fn packaged_skill_keeps_its_original_front_matter() {
    let mut front_matter = SKILL_CONTENT.lines();
    assert_eq!(front_matter.next(), Some("---"));
    assert_eq!(front_matter.next(), Some("name: aidget"));
    assert!(SKILL_CONTENT.contains("# aidget"));
}
