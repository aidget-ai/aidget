//! Canonical Markdown authoring guide shared by the CLI and MCP.

use crate::common::error::Result;

const GUIDE: &str = include_str!("guide.md");

/// Render the guide with the running Aidget version.
pub fn markdown() -> String {
    GUIDE.replace("{{VERSION}}", env!("CARGO_PKG_VERSION"))
}

/// Print the guide to stdout.
pub fn print() -> Result<()> {
    print!("{}", markdown());
    Ok(())
}
