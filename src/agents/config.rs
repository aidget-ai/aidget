use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use toml_edit::{value, DocumentMut, Item, Table};

use crate::agents::{dsh, jsonc, launch::McpLaunch};
use crate::common::error::{Error, Result};
use crate::common::utils::{atomic_write, decode_text, executable_paths_match, read_text};
use ts_rs::TS;

/// MCP configuration shape used by one Agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpFormat {
    Claude,
    Codex,
    Gemini,
    OpenCode,
    Omp,
    Antigravity,
    Goose,
    Copilot,
    Crush,
    ZCode,
    Zed,
    Kimi,
    Devin,
    Cline,
    CommandCode,
    Simple,
    Hermes,
    Dsh,
}

/// One Agent's MCP destination.
#[derive(Debug, Clone)]
pub struct McpTarget {
    pub path: PathBuf,
    pub format: McpFormat,
}

/// Current state of one resource for one Agent.
#[derive(Debug, Clone, PartialEq, serde::Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentResourceState {
    pub path: String,
    pub installed: bool,
    /// A readable aidget entry exists and can be replaced by this instance.
    pub replaceable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub conflict: Option<String>,
}

/// Read whether the named MCP entry matches this complete launch command.
pub fn mcp_state(target: &McpTarget, launch: &McpLaunch) -> Result<AgentResourceState> {
    let path = target.path.to_string_lossy().into_owned();
    if !target.path.exists() {
        return Ok(AgentResourceState {
            path,
            installed: false,
            replaceable: false,
            conflict: None,
        });
    }
    match read_mcp(target, launch) {
        Ok(Some(true)) => Ok(AgentResourceState {
            path,
            installed: true,
            replaceable: false,
            conflict: None,
        }),
        Ok(Some(false)) => Ok(AgentResourceState {
            path,
            installed: false,
            replaceable: true,
            conflict: None,
        }),
        Ok(None) => Ok(AgentResourceState {
            path,
            installed: false,
            replaceable: false,
            conflict: None,
        }),
        Err(error) => Ok(AgentResourceState {
            path,
            installed: false,
            replaceable: false,
            conflict: Some(error.to_string()),
        }),
    }
}

/// Replace or remove only the named aidget entry, backing up existing configuration.
pub fn set_mcp(
    target: &McpTarget,
    launch: &McpLaunch,
    enabled: bool,
    backup_root: &Path,
    agent: &str,
) -> Result<()> {
    if target.format == McpFormat::Dsh {
        return dsh::set_mcp(target, launch, enabled, backup_root, agent);
    }
    let current_bytes = if target.path.exists() {
        Some(fs::read(&target.path)?)
    } else {
        None
    };
    let current = current_bytes
        .as_deref()
        .map(decode_text)
        .transpose()?
        .map(|text| text.into_owned());
    if let Some(text) = &current {
        match read_mcp_text(target, text, launch)? {
            Some(true) if enabled => return Ok(()),
            None if !enabled => return Ok(()),
            _ => {}
        }
    } else if !enabled {
        return Ok(());
    }

    let next = match target.format {
        McpFormat::Codex => edit_toml(&target.path, current.as_deref(), launch, enabled)?,
        McpFormat::Hermes => edit_yaml(&target.path, current.as_deref(), launch, enabled)?,
        McpFormat::Dsh => unreachable!("DSH writes are handled separately"),
        _ => edit_json(target.format, current.as_deref(), launch, enabled)?,
    };
    if current.as_deref() == Some(next.as_str()) {
        return Ok(());
    }
    if let Some(bytes) = current_bytes {
        backup_bytes(backup_root, agent, &target.path, &bytes)?;
    }
    atomic_write(&target.path, next.as_bytes())
}

fn read_mcp(target: &McpTarget, launch: &McpLaunch) -> Result<Option<bool>> {
    let text = read_text(&target.path)?;
    read_mcp_text(target, &text, launch)
}

fn read_mcp_text(target: &McpTarget, text: &str, launch: &McpLaunch) -> Result<Option<bool>> {
    match target.format {
        McpFormat::Codex => {
            let document = text.parse::<DocumentMut>().map_err(|error| {
                Error::InvalidParams(format!("{}: {error}", target.path.display()))
            })?;
            let Some(servers) = document.get("mcp_servers") else {
                return Ok(None);
            };
            let servers = servers
                .as_table()
                .ok_or_else(|| Error::InvalidParams("mcp_servers is not a table".into()))?;
            Ok(servers.get("aidget").map(|entry| {
                entry
                    .as_table()
                    .is_some_and(|entry| toml_entry_matches(entry, launch))
            }))
        }
        McpFormat::Hermes => {
            let document: serde_yaml::Value = serde_yaml::from_str(text).map_err(|error| {
                Error::InvalidParams(format!("{}: {error}", target.path.display()))
            })?;
            if !document.is_mapping() && !document.is_null() {
                return Err(Error::InvalidParams("YAML root is not a mapping".into()));
            }
            if document
                .get("mcp_servers")
                .is_some_and(|servers| !servers.is_mapping())
            {
                return Err(Error::InvalidParams("mcp_servers is not a mapping".into()));
            }
            let entry = document
                .get("mcp_servers")
                .and_then(|value| value.get("aidget"));
            Ok(entry.map(|entry| yaml_command_matches(entry, launch)))
        }
        McpFormat::Dsh => dsh::state(text, launch),
        _ => {
            let root = jsonc::read_path(text, &[])?;
            let mut parent = root.as_ref();
            for key in key_path(target.format) {
                if parent.is_some_and(|parent| !parent.is_object()) {
                    return Err(Error::InvalidParams(
                        "MCP configuration parent is not an object".into(),
                    ));
                }
                parent = parent.and_then(|parent| parent.get(key));
            }
            let entry = jsonc::read_path(text, key_path(target.format))?;
            Ok(entry.map(|entry| json_entry_matches(target.format, &entry, launch)))
        }
    }
}

fn edit_json(
    format: McpFormat,
    current: Option<&str>,
    launch: &McpLaunch,
    enabled: bool,
) -> Result<String> {
    let text = current.unwrap_or("{\n}\n");
    let path = key_path(format);
    if enabled {
        jsonc::set_path(text, path, json_entry(format, launch))
    } else {
        jsonc::remove_path(text, path)
    }
}

fn edit_toml(
    path: &Path,
    current: Option<&str>,
    launch: &McpLaunch,
    enabled: bool,
) -> Result<String> {
    let text = current.unwrap_or("");
    let mut document = text
        .parse::<DocumentMut>()
        .map_err(|error| Error::InvalidParams(format!("{}: {error}", path.display())))?;
    if enabled {
        let servers = document
            .entry("mcp_servers")
            .or_insert(Item::Table(Table::new()))
            .as_table_mut()
            .ok_or_else(|| Error::InvalidParams("mcp_servers is not a table".to_string()))?;
        let mut table = Table::new();
        table["command"] = value(&launch.command);
        let mut args = toml_edit::Array::new();
        for argument in &launch.args {
            args.push(argument.as_str());
        }
        table["args"] = value(args);
        servers["aidget"] = Item::Table(table);
    } else if let Some(servers) = document.get_mut("mcp_servers").and_then(Item::as_table_mut) {
        servers.remove("aidget");
    }
    Ok(document.to_string())
}

fn edit_yaml(
    path: &Path,
    current: Option<&str>,
    launch: &McpLaunch,
    enabled: bool,
) -> Result<String> {
    let text = current.unwrap_or("");
    let mut document: serde_yaml::Value = if text.trim().is_empty() {
        serde_yaml::Value::Mapping(Default::default())
    } else {
        serde_yaml::from_str(text)
            .map_err(|error| Error::InvalidParams(format!("{}: {error}", path.display())))?
    };
    let root = document
        .as_mapping_mut()
        .ok_or_else(|| Error::InvalidParams("YAML root is not a mapping".to_string()))?;
    let key = serde_yaml::Value::String("mcp_servers".to_string());
    if enabled {
        let servers = root
            .entry(key)
            .or_insert_with(|| serde_yaml::Value::Mapping(Default::default()))
            .as_mapping_mut()
            .ok_or_else(|| Error::InvalidParams("mcp_servers is not a mapping".to_string()))?;
        servers.insert(
            serde_yaml::Value::String("aidget".to_string()),
            yaml_entry(launch),
        );
    } else if let Some(servers) = root
        .get_mut(serde_yaml::Value::String("mcp_servers".to_string()))
        .and_then(serde_yaml::Value::as_mapping_mut)
    {
        servers.remove(serde_yaml::Value::String("aidget".to_string()));
    }
    serde_yaml::to_string(&document)
        .map_err(|error| Error::InvalidParams(format!("{}: {error}", path.display())))
}

fn json_entry(format: McpFormat, launch: &McpLaunch) -> Value {
    match format {
        McpFormat::Claude | McpFormat::Crush | McpFormat::ZCode => json!({
            "type": "stdio",
            "command": launch.command,
            "args": launch.args,
            "env": {}
        }),
        McpFormat::Gemini | McpFormat::Antigravity | McpFormat::Simple | McpFormat::Zed => json!({
            "command": launch.command,
            "args": launch.args
        }),
        McpFormat::OpenCode => json!({
            "type": "local",
            "command": std::iter::once(launch.command.clone()).chain(launch.args.clone()).collect::<Vec<_>>(),
            "enabled": true
        }),
        McpFormat::Goose => json!({
            "enabled": true,
            "name": "aidget",
            "type": "stdio",
            "cmd": launch.command,
            "args": launch.args,
            "timeout": 300
        }),
        McpFormat::Copilot => json!({
            "type": "local",
            "command": launch.command,
            "args": launch.args,
            "tools": ["*"]
        }),
        McpFormat::Kimi | McpFormat::Omp => json!({
            "command": launch.command,
            "args": launch.args
        }),
        McpFormat::Devin => json!({
            "command": launch.command,
            "args": launch.args,
            "transport": "stdio"
        }),
        McpFormat::Cline => json!({
            "transport": {
                "type": "stdio",
                "command": launch.command,
                "args": launch.args
            }
        }),
        McpFormat::CommandCode => json!({
            "transport": "stdio",
            "enabled": true,
            "command": launch.command,
            "args": launch.args
        }),
        _ => unreachable!("non-JSON MCP format"),
    }
}

fn key_path(format: McpFormat) -> &'static [&'static str] {
    match format {
        McpFormat::OpenCode => &["mcp", "aidget"],
        McpFormat::Crush => &["mcp", "aidget"],
        McpFormat::Goose => &["extensions", "aidget"],
        McpFormat::ZCode => &["mcp", "servers", "aidget"],
        McpFormat::Zed => &["context_servers", "aidget"],
        _ => &["mcpServers", "aidget"],
    }
}

fn json_entry_matches(format: McpFormat, entry: &Value, launch: &McpLaunch) -> bool {
    if entry.get("enabled") == Some(&json!(false)) || entry.get("disabled") == Some(&json!(true)) {
        return false;
    }
    let expected = json_entry(format, launch);
    for key in ["type", "transport"] {
        if let Some(value) = expected.get(key).filter(|value| value.is_string()) {
            if entry.get(key) != Some(value) {
                return false;
            }
        }
    }
    if format == McpFormat::Cline {
        let transport = entry.get("transport");
        return transport
            .and_then(|value| value.get("command"))
            .and_then(Value::as_str)
            .is_some_and(|path| executable_paths_match(path, &launch.command))
            && transport.and_then(|value| value.get("type")) == Some(&json!("stdio"))
            && transport.and_then(|value| value.get("args")) == Some(&json!(launch.args));
    }
    if format == McpFormat::OpenCode {
        return entry
            .get("command")
            .and_then(Value::as_array)
            .is_some_and(|parts| {
                parts.len() == launch.args.len() + 1
                    && parts[0]
                        .as_str()
                        .is_some_and(|path| executable_paths_match(path, &launch.command))
                    && parts[1..] == json!(launch.args).as_array().unwrap()[..]
            });
    }
    let command_match = entry
        .get(if format == McpFormat::Goose {
            "cmd"
        } else {
            "command"
        })
        .and_then(Value::as_str)
        .is_some_and(|path| executable_paths_match(path, &launch.command));
    let args_match = entry.get("args") == Some(&json!(launch.args));
    command_match && args_match
}

fn toml_entry_matches(entry: &Table, launch: &McpLaunch) -> bool {
    if entry.get("enabled").and_then(Item::as_bool) == Some(false) {
        return false;
    }
    let command_match = entry
        .get("command")
        .and_then(Item::as_str)
        .is_some_and(|path| executable_paths_match(path, &launch.command));
    let args_match = entry
        .get("args")
        .and_then(Item::as_array)
        .is_some_and(|args| {
            args.iter()
                .map(|value| value.as_str())
                .eq(launch.args.iter().map(|value| Some(value.as_str())))
        });
    command_match && args_match
}

fn yaml_entry(launch: &McpLaunch) -> serde_yaml::Value {
    serde_yaml::Value::Mapping(
        [
            (
                serde_yaml::Value::String("command".to_string()),
                serde_yaml::Value::String(launch.command.clone()),
            ),
            (
                serde_yaml::Value::String("args".to_string()),
                serde_yaml::Value::Sequence(
                    launch
                        .args
                        .iter()
                        .cloned()
                        .map(serde_yaml::Value::String)
                        .collect(),
                ),
            ),
        ]
        .into_iter()
        .collect(),
    )
}

pub(super) fn yaml_command_matches(entry: &serde_yaml::Value, launch: &McpLaunch) -> bool {
    entry
        .get("command")
        .and_then(serde_yaml::Value::as_str)
        .is_some_and(|path| executable_paths_match(path, &launch.command))
        && entry
            .get("args")
            .and_then(serde_yaml::Value::as_sequence)
            .is_some_and(|args| {
                args.iter()
                    .filter_map(serde_yaml::Value::as_str)
                    .eq(launch.args.iter().map(String::as_str))
                    && args.len() == launch.args.len()
            })
}

pub(super) fn backup_bytes(root: &Path, agent: &str, path: &Path, bytes: &[u8]) -> Result<()> {
    let stamp = chrono::Local::now().format("%Y%m%dT%H%M%S%.3f");
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    let backup = root
        .join("state")
        .join("agent-backups")
        .join(format!("{stamp}-{}", uuid::Uuid::new_v4()))
        .join(agent)
        .join(name);
    atomic_write(&backup, bytes)
}
