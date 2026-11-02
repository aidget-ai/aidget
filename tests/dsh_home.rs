use aidget::agents::registry::builtin_agents;
use std::env;
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

fn dsh_mcp_path_from_registry() -> PathBuf {
    builtin_agents()
        .into_iter()
        .find(|agent| agent.id == "dsh")
        .and_then(|agent| agent.mcp)
        .expect("dsh MCP target")
        .path
}

#[cfg(windows)]
fn user_home() -> PathBuf {
    PathBuf::from(env::var_os("USERPROFILE").expect("USERPROFILE"))
}

#[cfg(not(windows))]
fn user_home() -> PathBuf {
    PathBuf::from(env::var_os("HOME").expect("HOME"))
}

#[test]
fn dsh_mcp_target_uses_dsh_home_and_ignores_profile_variables() {
    let temp = tempdir().unwrap();
    let custom_home = temp.path().join("dsh-home");
    let profile_dir = temp.path().join("profile-dir");
    fs::create_dir_all(&custom_home).unwrap();
    fs::create_dir_all(&profile_dir).unwrap();

    // SAFETY: this integration test runs in its own process and has one test.
    env::set_var("DSH_HOME", &custom_home);
    env::set_var("DSH_PROFILE", "desktop");
    env::set_var("DSH_PROFILE_DIR", &profile_dir);

    assert_eq!(
        dsh_mcp_path_from_registry(),
        custom_home.join("cordis.patch.yml")
    );

    env::remove_var("DSH_HOME");
    env::remove_var("DSH_PROFILE");
    env::remove_var("DSH_PROFILE_DIR");

    assert_eq!(
        dsh_mcp_path_from_registry(),
        user_home().join(".dsh").join("cordis.patch.yml")
    );
}
