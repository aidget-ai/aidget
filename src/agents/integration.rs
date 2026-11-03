use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use crate::agents::config::{self, AgentResourceState};
use crate::agents::launch::McpLaunch;
use crate::agents::registry::{builtin_agents, AgentSpec};
use crate::agents::skill;
use crate::common::error::{Error, Result};
use crate::widgets::store::Store;
use ts_rs::TS;

/// Values and detected Agents the main window needs for integrations.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationState {
    pub mcp_command: String,
    pub mcp_args: Vec<String>,
    /// Stable CLI shown on the page; installation uses mcp_command.
    pub mcp_display_command: String,
    /// Omit the default home on the page; installation always uses mcp_args.
    pub mcp_display_args: Vec<String>,
    pub skill_directory: String,
    pub skill_content: String,
    pub data_directory: String,
    pub agents: Vec<AgentIntegration>,
}

/// One detected Agent and the resources aidget can configure for it.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentIntegration {
    pub id: String,
    pub name: String,
    pub icon: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mcp: Option<AgentResourceState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub skills: Option<AgentResourceState>,
}

/// The resource a page installs or removes.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum IntegrationResource {
    Mcp,
    Skills,
}

/// One batch request from an MCP or skill page.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationChange {
    pub agent_ids: Vec<String>,
    pub resource: IntegrationResource,
    pub enabled: bool,
}

/// An Agent whose write failed while the rest of the batch continued.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentProblem {
    pub agent_id: String,
    pub resource: IntegrationResource,
    pub message: String,
}

/// Result of one or more writes, with the state read back from disk.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationMutation {
    pub state: IntegrationState,
    pub problems: Vec<AgentProblem>,
}

static WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Read the current binary, packaged skill and every detected Agent's live state.
pub fn integration_state(store: &Store) -> Result<IntegrationState> {
    let skill_directory = skill::ensure_skill(store)?;
    let launch = McpLaunch::for_root(store.root())?;
    state_for(&launch, &skill_directory, store.root())
}

/// Install or remove one resource for selected detected Agents.
pub fn set_agent_integration(
    store: &Store,
    change: IntegrationChange,
) -> Result<IntegrationMutation> {
    let skill_directory = skill::ensure_skill(store)?;
    let launch = McpLaunch::for_root(store.root())?;
    if change.enabled {
        crate::runtime::process::publish_cli(store.root())?;
    }
    let agents = builtin_agents()
        .into_iter()
        .filter(AgentSpec::detected)
        .collect::<Vec<_>>();
    let selected = select_agents(&agents, &change.agent_ids)?;
    let guard = WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| Error::Other("integration write lock is poisoned".to_string()))?;
    let problems = apply_change(
        &selected,
        &launch,
        &skill_directory,
        store.root(),
        change.resource,
        change.enabled,
    );
    drop(guard);
    Ok(IntegrationMutation {
        state: state_for(&launch, &skill_directory, store.root())?,
        problems,
    })
}

/// Apply one resource change to every selected Agent and collect per-Agent failures.
pub fn apply_change(
    selected: &[&AgentSpec],
    launch: &McpLaunch,
    skill_directory: &Path,
    backup_root: &Path,
    resource: IntegrationResource,
    enabled: bool,
) -> Vec<AgentProblem> {
    let mut problems = Vec::new();
    match resource {
        IntegrationResource::Mcp => {
            for agent in selected {
                let Some(target) = &agent.mcp else {
                    problems.push(problem(agent, resource, "this Agent has no MCP config"));
                    continue;
                };
                if let Err(error) = config::set_mcp(target, launch, enabled, backup_root, agent.id)
                {
                    problems.push(problem(agent, resource, &error.to_string()));
                }
            }
        }
        IntegrationResource::Skills => {
            let mut handled = HashMap::<std::path::PathBuf, std::result::Result<(), String>>::new();
            for agent in selected {
                let Some(root) = &agent.skills else {
                    problems.push(problem(agent, resource, "this Agent has no skills folder"));
                    continue;
                };
                let path = skill_path(root);
                let result = handled.entry(path.clone()).or_insert_with(|| {
                    skill::set_skill(&path, skill_directory, enabled)
                        .map_err(|error| error.to_string())
                });
                if let Err(message) = result {
                    problems.push(problem(agent, resource, message));
                }
            }
        }
    }
    problems
}

fn state_for(launch: &McpLaunch, skill_directory: &Path, root: &Path) -> Result<IntegrationState> {
    let default_root = dirs::home_dir()
        .ok_or_else(|| Error::Other("cannot determine the default user data directory".into()))?
        .join(".aidget");
    let agents = builtin_agents()
        .into_iter()
        .filter(AgentSpec::detected)
        .filter(|agent| agent.mcp.is_some() || agent.skills.is_some())
        .map(|agent| {
            Ok(AgentIntegration {
                id: agent.id.to_string(),
                name: agent.name.to_string(),
                icon: agent.icon.to_string(),
                mcp: agent
                    .mcp
                    .as_ref()
                    .map(|target| config::mcp_state(target, launch))
                    .transpose()?,
                skills: agent.skills.as_ref().map(|root| {
                    let path = skill_path(root);
                    skill::skill_state(&path, skill_directory).unwrap_or_else(|error| {
                        AgentResourceState {
                            path: path.to_string_lossy().into_owned(),
                            installed: false,
                            replaceable: false,
                            conflict: Some(error.to_string()),
                        }
                    })
                }),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let display = McpLaunch::for_display(root, &default_root)?;
    Ok(IntegrationState {
        mcp_command: launch.command.clone(),
        mcp_args: launch.args.clone(),
        mcp_display_command: display.command,
        mcp_display_args: display.args,
        skill_directory: skill_directory.to_string_lossy().into_owned(),
        skill_content: std::fs::read_to_string(skill_directory.join("SKILL.md"))?,
        data_directory: dunce::canonicalize(root)?.to_string_lossy().into_owned(),
        agents,
    })
}

/// Resolve the aidget Skill directory below one Agent's skills root.
pub fn skill_path(root: &Path) -> std::path::PathBuf {
    root.join("aidget")
}

/// Select requested Agent definitions that were detected for installation.
pub fn select_agents<'a>(agents: &'a [AgentSpec], ids: &[String]) -> Result<Vec<&'a AgentSpec>> {
    if ids.is_empty() {
        return Err(Error::InvalidParams(
            "at least one detected Agent is required".to_string(),
        ));
    }
    ids.iter()
        .map(|id| {
            agents
                .iter()
                .find(|agent| agent.id == id)
                .ok_or_else(|| Error::InvalidParams(format!("unknown Agent: {id}")))
        })
        .collect()
}

fn problem(agent: &AgentSpec, resource: IntegrationResource, message: &str) -> AgentProblem {
    AgentProblem {
        agent_id: agent.id.to_string(),
        resource,
        message: message.to_string(),
    }
}
