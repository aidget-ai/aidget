use aidget::agents::registry::{builtin_agents, dsh_mcp_path, AgentSpec};
use std::path::Path;
use tempfile::tempdir;

#[test]
fn builtin_registry_covers_both_resources_and_unique_ids() {
    let agents = builtin_agents();
    assert!(agents.len() >= 30);
    assert!(agents.iter().any(|agent| agent.mcp.is_some()));
    assert!(agents.iter().any(|agent| agent.skills.is_some()));
    let mut ids = agents.iter().map(|agent| agent.id).collect::<Vec<_>>();
    let count = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), count);
    for expected in [
        "claude",
        "codex",
        "gemini",
        "agy",
        "opencode",
        "deveco",
        "mimocode",
        "pi",
        "dsh",
        "omp",
        "omo",
        "goose",
        "cursor",
        "copilot",
        "crush",
        "zcode",
        "kimi",
        "alma",
        "cindy",
        "hermes",
        "minimax-code",
        "devin",
        "grok",
        "grok-app",
        "droid",
        "qoder",
        "qoder-cn",
        "cline",
        "commandcode",
        "workbuddy",
        "workbuddy-ai",
        "codebuddy",
        "hanako",
        "fx",
        "atomcode",
        "zed",
        "claude-desktop",
    ] {
        assert!(ids.contains(&expected), "missing Agent {expected}");
    }
}

#[test]
fn detection_accepts_a_config_path_without_a_cli() {
    let temp = tempdir().unwrap();
    let config = temp.path().join("config.json");
    let agent = AgentSpec {
        id: "fixture",
        name: "Fixture",
        icon: "generic",
        commands: vec![],
        detect: vec![config.clone()],
        mcp: None,
        skills: None,
    };
    assert!(!agent.detected());
    std::fs::write(config, "{}").unwrap();
    assert!(agent.detected());
}

#[test]
fn dsh_mcp_path_is_the_home_level_patch_file() {
    let home = Path::new("custom-home");
    assert_eq!(dsh_mcp_path(home), home.join("cordis.patch.yml"));
}
