use std::path::Path;

use crate::common::error::Result;
use crate::runtime::process::cli_link;

/// An Agent launch command, including the data directory it must connect to.
#[derive(Debug, Clone, PartialEq)]
pub struct McpLaunch {
    pub command: String,
    pub args: Vec<String>,
}

impl McpLaunch {
    /// Use this instance's stable CLI entry without resolving its binary target.
    pub fn for_root(root: &Path) -> Result<Self> {
        Self::build(root, true)
    }

    /// Omit the home argument for display only when it resolves to the OS default.
    pub fn for_display(root: &Path, default_root: &Path) -> Result<Self> {
        let is_default = default_root.try_exists()?
            && dunce::canonicalize(root)? == dunce::canonicalize(default_root)?;
        Self::build(root, !is_default)
    }

    fn build(root: &Path, include_home: bool) -> Result<Self> {
        let root = dunce::canonicalize(root)?;
        let link = cli_link(&root).to_string_lossy().into_owned();
        let home = root.to_string_lossy().into_owned();
        #[cfg(not(windows))]
        return Ok(Self {
            command: link,
            args: if include_home {
                vec!["--home".into(), home, "mcp".into()]
            } else {
                vec!["mcp".into()]
            },
        });
        #[cfg(windows)]
        {
            // cmd expands percent expressions even inside quotes.
            if home.contains(['%', '"', '\r', '\n']) {
                return Err(crate::Error::InvalidParams(
                    "the Windows Agent launcher directory cannot contain %, quotes or newlines"
                        .into(),
                ));
            }
            let system = std::env::var_os("SystemRoot")
                .ok_or_else(|| crate::Error::Other("SystemRoot is not set".into()))?;
            Ok(Self {
                command: Path::new(&system)
                    .join("System32")
                    .join("cmd.exe")
                    .to_string_lossy()
                    .into_owned(),
                args: vec![
                    "/d".into(),
                    "/v:off".into(),
                    "/s".into(),
                    "/c".into(),
                    if include_home {
                        format!("\"\"{link}\" --home \"{home}\" mcp\"")
                    } else {
                        format!("\"\"{link}\" mcp\"")
                    },
                ],
            })
        }
    }
}
