use std::path::Path;

use crate::common::error::Result;
use crate::runtime::process::cli_path;

/// An Agent launch command, including the data directory it must connect to.
#[derive(Debug, Clone, PartialEq)]
pub struct McpLaunch {
    pub command: String,
    pub args: Vec<String>,
}

impl McpLaunch {
    /// Bind MCP to this instance's absolute data directory.
    pub fn for_root(root: &Path) -> Result<Self> {
        Self::build(root, true)
    }

    /// Omit home from the displayed command only for the OS default directory.
    pub fn for_display(root: &Path, default_root: &Path) -> Result<Self> {
        let is_default = default_root.try_exists()?
            && dunce::canonicalize(root)? == dunce::canonicalize(default_root)?;
        Self::build(root, !is_default)
    }

    fn build(root: &Path, include_home: bool) -> Result<Self> {
        let root = dunce::canonicalize(root)?;
        Ok(Self {
            command: cli_path(&root).to_string_lossy().into_owned(),
            args: if include_home {
                vec![
                    "--home".into(),
                    root.to_string_lossy().into_owned(),
                    "mcp".into(),
                ]
            } else {
                vec!["mcp".into()]
            },
        })
    }
}
