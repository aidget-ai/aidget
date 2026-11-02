use std::env;
use std::path::{Path, PathBuf};

use crate::agents::config::{McpFormat, McpTarget};

/// One built-in local Agent aidget can configure.
#[derive(Debug, Clone)]
pub struct AgentSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub icon: &'static str,
    pub commands: Vec<&'static str>,
    pub detect: Vec<PathBuf>,
    pub mcp: Option<McpTarget>,
    pub skills: Option<PathBuf>,
}

impl AgentSpec {
    pub fn detected(&self) -> bool {
        self.detect.iter().any(|path| path.exists())
            || self.commands.iter().any(|command| command_exists(command))
    }
}

/// Every built-in local target in Magpie that can receive MCP servers or skills.
pub fn builtin_agents() -> Vec<AgentSpec> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let config = env_path("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config"));
    let mut agents = Vec::new();

    let claude_dir = env_path("CLAUDE_CONFIG_DIR").unwrap_or_else(|| home.join(".claude"));
    agents.push(agent(
        "claude",
        "Claude Code",
        "claudecode-color",
        &["claude"],
        vec![claude_dir.clone(), home.join(".claude.json")],
        Some(McpTarget {
            path: if env::var_os("CLAUDE_CONFIG_DIR").is_some() {
                claude_dir.join(".claude.json")
            } else {
                home.join(".claude.json")
            },
            format: McpFormat::Claude,
        }),
        Some(claude_dir.join("skills")),
    ));

    let codex_dir = env_path("CODEX_HOME").unwrap_or_else(|| home.join(".codex"));
    agents.push(agent(
        "codex",
        "Codex",
        "codex-color",
        &["codex"],
        vec![codex_dir.clone()],
        Some(mcp(codex_dir.join("config.toml"), McpFormat::Codex)),
        Some(codex_dir.join("skills")),
    ));

    agents.push(agent(
        "gemini",
        "Gemini CLI",
        "geminicli-color",
        &["gemini"],
        vec![home.join(".gemini/settings.json")],
        Some(mcp(home.join(".gemini/settings.json"), McpFormat::Gemini)),
        Some(home.join(".gemini/skills")),
    ));

    agents.push(agent(
        "agy",
        "Antigravity CLI",
        "antigravity-color",
        &["agy"],
        vec![home.join(".gemini/config")],
        Some(mcp(
            home.join(".gemini/config/mcp_config.json"),
            McpFormat::Antigravity,
        )),
        Some(home.join(".gemini/config/skills")),
    ));

    let opencode_dir = env_path("OPENCODE_CONFIG_DIR").unwrap_or_else(|| config.join("opencode"));
    agents.push(agent(
        "opencode",
        "OpenCode",
        "opencode",
        &["opencode"],
        vec![opencode_dir.clone()],
        Some(mcp(
            first_existing(&opencode_dir, &["opencode.json", "opencode.jsonc"]),
            McpFormat::OpenCode,
        )),
        Some(opencode_dir.join("skills")),
    ));

    let deveco_dir = env_path("DEVECO_CONFIG_DIR").unwrap_or_else(|| config.join("deveco"));
    agents.push(agent(
        "deveco",
        "DevEco Code",
        "deveco-color",
        &["deveco"],
        vec![deveco_dir.clone()],
        Some(mcp(
            first_existing(&deveco_dir, &["deveco.json", "deveco.jsonc", "config.json"]),
            McpFormat::OpenCode,
        )),
        Some(deveco_dir.join("skills")),
    ));

    let mimocode_dir = env_path("MIMOCODE_HOME")
        .map(|path| path.join("config"))
        .unwrap_or_else(|| config.join("mimocode"));
    agents.push(agent(
        "mimocode",
        "MiMo Code",
        "mimocode",
        &["mimocode"],
        vec![mimocode_dir.clone()],
        Some(mcp(
            first_existing(&mimocode_dir, &["mimocode.json", "mimocode.jsonc"]),
            McpFormat::OpenCode,
        )),
        Some(mimocode_dir.join("skills")),
    ));

    let pi_dir = env_path("PI_CODING_AGENT_DIR").unwrap_or_else(|| home.join(".pi/agent"));
    agents.push(agent(
        "pi",
        "Pi",
        "pi",
        &["pi"],
        vec![pi_dir.clone()],
        Some(mcp(pi_dir.join("mcp.json"), McpFormat::Simple)),
        Some(pi_dir.join("skills")),
    ));

    let dsh_dir = env_path("DSH_HOME").unwrap_or_else(|| home.join(".dsh"));
    agents.push(agent(
        "dsh",
        "DeepSeek Harness",
        "deepseek-color",
        &["dsh"],
        vec![dsh_dir.clone()],
        Some(mcp(dsh_mcp_path(&dsh_dir), McpFormat::Dsh)),
        Some(dsh_dir.join("skills")),
    ));

    let omp_dir = env_path("PI_CONFIG_DIR")
        .map(|path| home.join(path))
        .unwrap_or_else(|| home.join(".omp"))
        .join("agent");
    agents.push(agent(
        "omp",
        "omp",
        "omp",
        &["omp"],
        vec![omp_dir.clone()],
        Some(mcp(omp_dir.join("mcp.json"), McpFormat::Omp)),
        Some(omp_dir.join("skills")),
    ));

    let omo_dir = env_path("OMO_CODING_AGENT_DIR")
        .or_else(|| env_path("SENPI_CODING_AGENT_DIR"))
        .unwrap_or_else(|| home.join(".omo/agent"));
    agents.push(agent(
        "omo",
        "OmO",
        "omo",
        &["omo"],
        vec![omo_dir.clone()],
        Some(mcp(omo_dir.join("mcp.json"), McpFormat::Simple)),
        Some(omo_dir.join("skills")),
    ));

    let goose_config = if cfg!(windows) {
        env_path("APPDATA")
            .map(|path| path.join("Block/goose/config/config.yaml"))
            .unwrap_or_else(|| config.join("goose/config.yaml"))
    } else {
        config.join("goose/config.yaml")
    };
    agents.push(agent(
        "goose",
        "Goose",
        "goose",
        &["goose"],
        vec![goose_config.clone()],
        Some(mcp(goose_config, McpFormat::Goose)),
        Some(home.join(".agents/skills")),
    ));

    agents.push(agent(
        "cursor",
        "Cursor",
        "cursor",
        &["cursor-agent"],
        vec![home.join(".cursor")],
        Some(mcp(home.join(".cursor/mcp.json"), McpFormat::Simple)),
        Some(home.join(".cursor/skills")),
    ));

    let copilot_dir = env_path("COPILOT_HOME").unwrap_or_else(|| home.join(".copilot"));
    agents.push(agent(
        "copilot",
        "Copilot CLI",
        "githubcopilot",
        &["copilot"],
        vec![copilot_dir.clone()],
        Some(mcp(copilot_dir.join("mcp-config.json"), McpFormat::Copilot)),
        Some(copilot_dir.join("skills")),
    ));

    let crush_dir = if cfg!(windows) {
        env_path("LOCALAPPDATA")
            .map(|path| path.join("crush"))
            .unwrap_or_else(|| config.join("crush"))
    } else {
        config.join("crush")
    };
    agents.push(agent(
        "crush",
        "Crush",
        "crush",
        &["crush"],
        vec![crush_dir.clone()],
        Some(mcp(crush_dir.join("crush.json"), McpFormat::Crush)),
        Some(crush_dir.join("skills")),
    ));

    let zcode_dir = home.join(".zcode");
    agents.push(agent(
        "zcode",
        "ZCode",
        "zcode",
        &["zcode"],
        vec![zcode_dir.clone()],
        Some(mcp(zcode_dir.join("cli/config.json"), McpFormat::ZCode)),
        Some(zcode_dir.join("skills")),
    ));

    let kimi_dir = env_path("KIMI_CODE_HOME")
        .or_else(|| env_path("KIMI_SHARE_DIR"))
        .unwrap_or_else(|| home.join(".kimi-code"));
    agents.push(agent(
        "kimi",
        "Kimi Code",
        "kimi",
        &["kimi"],
        vec![kimi_dir.clone()],
        Some(mcp(kimi_dir.join("mcp.json"), McpFormat::Kimi)),
        Some(home.join(".agents/skills")),
    ));

    let alma_dir = home.join(".config/alma");
    agents.push(agent(
        "alma",
        "Alma",
        "alma",
        &[],
        vec![alma_dir.clone()],
        Some(mcp(alma_dir.join("mcp.json"), McpFormat::Simple)),
        Some(alma_dir.join("skills")),
    ));

    agents.push(agent(
        "cindy",
        "Cindy",
        "cindy",
        &[],
        vec![home.join(".cindy")],
        None,
        Some(home.join(".agents/skills")),
    ));

    let hermes_dir = env_path("HERMES_HOME").unwrap_or_else(|| {
        if cfg!(windows) {
            env_path("LOCALAPPDATA")
                .map(|path| path.join("hermes"))
                .unwrap_or_else(|| home.join(".hermes"))
        } else {
            home.join(".hermes")
        }
    });
    agents.push(agent(
        "hermes",
        "Hermes Agent",
        "hermes",
        &["hermes"],
        vec![hermes_dir.clone()],
        Some(mcp(hermes_dir.join("config.yaml"), McpFormat::Hermes)),
        Some(hermes_dir.join("skills")),
    ));

    let minimax_dir = env_path("MINIMAX_DATA_DIR").unwrap_or_else(|| home.join(".minimax"));
    agents.push(agent(
        "minimax-code",
        "MiniMax Code",
        "minimax-color",
        &["mcode"],
        vec![minimax_dir.clone()],
        None,
        Some(minimax_dir.join("skills")),
    ));

    let devin_dir = config.join("devin");
    agents.push(agent(
        "devin",
        "Devin",
        "devin",
        &["devin"],
        vec![devin_dir.clone()],
        Some(mcp(devin_dir.join("mcp_config.json"), McpFormat::Devin)),
        Some(devin_dir.join("skills")),
    ));

    let grok_dir = env_path("GROK_HOME").unwrap_or_else(|| home.join(".grok"));
    agents.push(agent(
        "grok",
        "Grok Build",
        "xai",
        &["grok"],
        vec![grok_dir.clone()],
        Some(mcp(grok_dir.join("config.toml"), McpFormat::Codex)),
        Some(grok_dir.join("skills")),
    ));

    let grok_app_dir = grok_app_dir(&home);
    let grok_home = grok_app_dir.join("agent-home");
    agents.push(agent(
        "grok-app",
        "Grok App",
        "grok",
        &[],
        vec![grok_app_dir],
        Some(mcp(grok_home.join("config.toml"), McpFormat::Codex)),
        Some(grok_home.join("skills")),
    ));

    let factory_home = env_path("FACTORY_HOME_OVERRIDE").unwrap_or(home.clone());
    let droid_dir = factory_home.join(".factory");
    agents.push(agent(
        "droid",
        "Droid",
        "factory",
        &["droid"],
        vec![droid_dir.clone()],
        Some(mcp(droid_dir.join("mcp.json"), McpFormat::Claude)),
        Some(droid_dir.join("skills")),
    ));

    for (id, name, env_name, default, command, icon) in [
        (
            "qoder",
            "Qoder",
            "QODER_CONFIG_DIR",
            ".qoder",
            "qodercli",
            "qoder",
        ),
        (
            "qoder-cn",
            "Qoder CN",
            "QODERCN_CONFIG_DIR",
            ".qoder-cn",
            "qoderclicn",
            "qoder",
        ),
    ] {
        let dir = env_path(env_name).unwrap_or_else(|| home.join(default));
        agents.push(agent(
            id,
            name,
            icon,
            &[command],
            vec![dir.clone()],
            Some(mcp(dir.join("settings.json"), McpFormat::Omp)),
            Some(dir.join("skills")),
        ));
    }

    let cline_dir = env_path("CLINE_DIR").unwrap_or_else(|| home.join(".cline"));
    let cline_data = env_path("CLINE_DATA_DIR").unwrap_or_else(|| cline_dir.join("data"));
    agents.push(agent(
        "cline",
        "Cline",
        "cline",
        &["cline"],
        vec![cline_dir],
        Some(mcp(
            env_path("CLINE_MCP_SETTINGS_PATH")
                .unwrap_or_else(|| cline_data.join("settings/cline_mcp_settings.json")),
            McpFormat::Cline,
        )),
        Some(home.join(".cline/skills")),
    ));

    let commandcode_dir = home.join(".commandcode");
    agents.push(agent(
        "commandcode",
        "Command Code",
        "commandcode",
        &["command-code"],
        vec![commandcode_dir.clone()],
        Some(mcp(
            commandcode_dir.join("mcp.json"),
            McpFormat::CommandCode,
        )),
        Some(commandcode_dir.join("skills")),
    ));

    let workbuddy_dir = env_path("WORKBUDDY_CONFIG_DIR").unwrap_or_else(|| home.join(".workbuddy"));
    agents.push(agent(
        "workbuddy",
        "WorkBuddy",
        "workbuddy-color",
        &["work-buddy"],
        vec![workbuddy_dir.clone()],
        Some(mcp(workbuddy_dir.join("mcp.json"), McpFormat::Claude)),
        Some(workbuddy_dir.join("skills")),
    ));
    let workbuddy_ai_dir = home.join(".workbuddy-ai");
    agents.push(agent(
        "workbuddy-ai",
        "WorkBuddy AI",
        "workbuddy-color",
        &["workbuddyai"],
        vec![workbuddy_ai_dir.clone()],
        Some(mcp(workbuddy_ai_dir.join("mcp.json"), McpFormat::Claude)),
        Some(workbuddy_ai_dir.join("skills")),
    ));

    let codebuddy_dir = env_path("CODEBUDDY_CONFIG_DIR").unwrap_or_else(|| home.join(".codebuddy"));
    agents.push(agent(
        "codebuddy",
        "CodeBuddy Code",
        "codebuddy-color",
        &["codebuddy"],
        vec![codebuddy_dir.clone()],
        Some(mcp(
            first_existing(
                &codebuddy_dir,
                &[".mcp.json", "mcp.json", "../.codebuddy.json"],
            ),
            McpFormat::Claude,
        )),
        Some(codebuddy_dir.join("skills")),
    ));

    let hanako_dir = env_path("HANA_HOME").unwrap_or_else(|| home.join(".hanako"));
    agents.push(agent(
        "hanako",
        "OpenHanako",
        "hanako",
        &[],
        vec![hanako_dir.clone()],
        None,
        Some(hanako_dir.join("skills")),
    ));

    let fx_dir = home.join(".fx");
    agents.push(agent(
        "fx",
        "fx",
        "fx",
        &[],
        vec![fx_dir.clone()],
        None,
        Some(fx_dir.join("skills")),
    ));

    let atomcode_dir = home.join(".atomcode");
    agents.push(agent(
        "atomcode",
        "AtomCode",
        "atomcode",
        &[],
        vec![atomcode_dir.clone()],
        Some(mcp(atomcode_dir.join("mcp.json"), McpFormat::Omp)),
        Some(atomcode_dir.join("skills")),
    ));

    let zed_dir = zed_dir(&home, &config);
    agents.push(agent(
        "zed",
        "Zed",
        "zed",
        &["zed"],
        vec![zed_dir.clone()],
        Some(mcp(zed_dir.join("settings.json"), McpFormat::Zed)),
        None,
    ));

    let desktop = claude_desktop_dir(&home);
    agents.push(agent(
        "claude-desktop",
        "Claude Desktop",
        "claude-color",
        &[],
        vec![desktop.clone()],
        Some(mcp(
            desktop.join("claude_desktop_config.json"),
            McpFormat::Simple,
        )),
        None,
    ));

    agents
}

/// Resolve the DeepSeek Harness home-level `cordis.patch.yml`.
///
/// DSH applies this layer after the selected profile, so one file is loaded by
/// every profile, including the desktop profile. `home` is `DSH_HOME` when set
/// and `~/.dsh` otherwise.
pub fn dsh_mcp_path(home: &Path) -> PathBuf {
    home.join("cordis.patch.yml")
}

fn agent(
    id: &'static str,
    name: &'static str,
    icon: &'static str,
    commands: &[&'static str],
    detect: Vec<PathBuf>,
    mcp: Option<McpTarget>,
    skills: Option<PathBuf>,
) -> AgentSpec {
    AgentSpec {
        id,
        name,
        icon,
        commands: commands.to_vec(),
        detect,
        mcp,
        skills,
    }
}

fn mcp(path: PathBuf, format: McpFormat) -> McpTarget {
    McpTarget { path, format }
}

fn env_path(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn first_existing(dir: &Path, names: &[&str]) -> PathBuf {
    names
        .iter()
        .map(|name| dir.join(name))
        .find(|path| path.exists())
        .unwrap_or_else(|| dir.join(names[0]))
}

fn command_exists(command: &str) -> bool {
    let mut dirs = env::var_os("PATH")
        .map(|path| env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default();
    if let Some(home) = dirs::home_dir() {
        dirs.extend([
            home.join(".local/bin"),
            home.join(".opencode/bin"),
            home.join(".bun/bin"),
            home.join(".cargo/bin"),
        ]);
    }
    dirs.extend([
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ]);
    dirs.into_iter().any(|dir| {
        let candidate = dir.join(command);
        if candidate.is_file() {
            return true;
        }
        if cfg!(windows) {
            ["exe", "cmd", "bat", "ps1"]
                .iter()
                .any(|extension| candidate.with_extension(extension).is_file())
        } else {
            false
        }
    })
}

fn grok_app_dir(home: &Path) -> PathBuf {
    if let Some(path) = env_path("GROK_APP_HOME") {
        return path;
    }
    if cfg!(target_os = "macos") {
        let app = home.join("Library/Application Support/com.grokapp.grok-app");
        return if app.exists() {
            app
        } else {
            home.join(".grok-app")
        };
    }
    if cfg!(windows) {
        return env_path("APPDATA")
            .map(|path| path.join("grokapp/grok-app/data"))
            .unwrap_or_else(|| home.join("AppData/Roaming/grokapp/grok-app/data"));
    }
    env_path("XDG_DATA_HOME")
        .map(|path| path.join("grok-app"))
        .unwrap_or_else(|| home.join(".local/share/grok-app"))
}

fn zed_dir(home: &Path, config: &Path) -> PathBuf {
    if let Some(path) = env_path("MAGPIE_ZED_CONFIG_DIR") {
        return path;
    }
    if cfg!(windows) {
        return env_path("APPDATA")
            .map(|path| path.join("Zed"))
            .unwrap_or_else(|| home.join("AppData/Roaming/Zed"));
    }
    config.join("zed")
}

fn claude_desktop_dir(home: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/Claude")
    } else if cfg!(windows) {
        env_path("APPDATA")
            .map(|path| path.join("Claude"))
            .unwrap_or_else(|| home.join("AppData/Roaming/Claude"))
    } else {
        env_path("XDG_CONFIG_HOME")
            .map(|path| path.join("Claude"))
            .unwrap_or_else(|| home.join(".config/Claude"))
    }
}
