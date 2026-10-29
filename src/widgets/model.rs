use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::common::error::{Error, Result};
use schemars::JsonSchema;

pub const MAX_WIDGETS: usize = 50;
pub const MAX_PAYLOAD: usize = 256 * 1024;

pub fn validate_id(id: &str) -> Result<()> {
    let bytes = id.as_bytes();
    if bytes.is_empty() || bytes.len() > 64 {
        return Err(Error::InvalidId(id.to_string()));
    }
    let first = bytes[0];
    if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
        return Err(Error::InvalidId(id.to_string()));
    }
    if !bytes
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
    {
        return Err(Error::InvalidId(id.to_string()));
    }
    Ok(())
}

/// Capabilities a widget may ask the host for. A widget only gets the ones it
/// declares in `config.json`; anything else is denied before it leaves Rust.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    JsonSchema,
    ts_rs::TS,
)]
#[serde(rename_all = "lowercase")]
pub enum WidgetPermission {
    Script,
    Location,
    Storage,
}

/// How the host and the widget divide responsibility for the card surface.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WidgetSurface {
    /// Draw aidget's static fallback card.
    #[default]
    Host,
    /// The widget paints every pixel.
    None,
    /// AppKit owns the translucent material and the widget paints content on top.
    Glass,
}

/// Installed portable config plus host-managed revision information.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WidgetMeta {
    #[serde(flatten)]
    pub config: super::package::WidgetConfig,
    pub revision: u64,
    pub updated_at: String,
}
impl std::ops::Deref for WidgetMeta {
    type Target = super::package::WidgetConfig;
    fn deref(&self) -> &Self::Target {
        &self.config
    }
}
impl std::ops::DerefMut for WidgetMeta {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.config
    }
}
impl WidgetMeta {
    /// Construct host state for a new local widget.
    pub fn new(id: &str, title: &str) -> Self {
        Self {
            config: super::package::WidgetConfig::new(id, title),
            revision: 1,
            updated_at: chrono::Local::now().to_rfc3339(),
        }
    }
    /// Whether the current declaration grants this host capability.
    pub fn grants(&self, permission: WidgetPermission) -> bool {
        self.permissions.contains(&permission)
    }
    /// Deduplicate and order declarations so config files stay canonical.
    pub fn set_permissions(&mut self, permissions: &[WidgetPermission]) {
        let mut next = permissions.to_vec();
        next.sort();
        next.dedup();
        self.permissions = next;
    }
}

/// Widget window drag policy; controls remain interactive in either mode.
#[derive(
    Debug,
    Default,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    JsonSchema,
    clap::ValueEnum,
    ts_rs::TS,
)]
#[serde(rename_all = "lowercase")]
pub enum WidgetDragMode {
    /// Drag ordinary widget content, excluding interactive and opted-out regions.
    #[default]
    All,
    /// Drag only the host's top handle.
    Handle,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WidgetFrame {
    pub display: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl WidgetFrame {
    pub fn default_for(index: usize) -> Self {
        let col = index % 3;
        let row = index / 3;
        WidgetFrame {
            display: "primary".to_string(),
            x: 72.0 + col as f64 * 384.0,
            y: 96.0 + row as f64 * 248.0,
            w: 360.0,
            h: 220.0,
        }
    }

    pub fn clamped(&self, area: (f64, f64, f64, f64)) -> Self {
        let fitted = self.fitted(area);
        let (width, height) = (area.2.max(120.0), area.3.max(120.0));
        WidgetFrame {
            x: fitted.x.clamp(0.0, (width - fitted.w).max(0.0)),
            y: fitted.y.clamp(0.0, (height - fitted.h).max(0.0)),
            ..fitted
        }
    }

    /// Clamp the size to the work area while keeping the origin untouched.
    /// A widget may straddle two displays, so negative offsets are valid.
    pub fn fitted(&self, area: (f64, f64, f64, f64)) -> Self {
        let (width, height) = (area.2.max(120.0), area.3.max(120.0));
        let w = self.w.clamp(160.0, width);
        let h = self.h.clamp(120.0, height);
        WidgetFrame {
            display: self.display.clone(),
            x: self.x,
            y: self.y,
            w,
            h,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Board {
    pub revision: u64,
    pub widgets: BTreeMap<String, WidgetFrame>,
}

impl Default for Board {
    fn default() -> Self {
        Board {
            revision: 1,
            widgets: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WidgetSummary {
    pub id: String,
    pub title: String,
    pub revision: u64,
    /// Configured region for initiating native window movement.
    pub drag_mode: WidgetDragMode,
    pub permissions: Vec<WidgetPermission>,
    pub network: Vec<String>,
    pub frame: Option<WidgetFrame>,
}

/// A widget directory currently held in the trash.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedWidgetSummary {
    pub trash_name: String,
    pub id: String,
    pub title: String,
    pub width: f64,
    pub height: f64,
    pub has_preview: bool,
    pub created_at: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BrokenWidget {
    pub id: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WidgetListing {
    pub board_revision: u64,
    pub widgets: Vec<WidgetSummary>,
    pub broken: Vec<BrokenWidget>,
    /// Only the UI can enumerate monitors, so the socket handler fills this.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub displays: Option<Vec<DisplayInfo>>,
}

/// Result of a write, so an agent never has to follow up with a read.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WidgetAck {
    pub widget: String,
    pub revision: u64,
    pub title: String,
    /// Effective window drag policy after this write.
    pub drag_mode: WidgetDragMode,
    pub permissions: Vec<WidgetPermission>,
    pub network: Vec<String>,
    pub updated_at: String,
    /// Present only when this call placed the window (`create`/`move`).
    /// Content edits omit it: the user can drag widgets at any time, so a
    /// frame observed around an edit is not a side effect of that edit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame: Option<WidgetFrame>,
    /// Current board revision. User drags bump it too.
    pub board_revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteResult {
    pub widget: String,
    pub trash_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotResult {
    pub widget: String,
    pub path: String,
    pub bytes: usize,
    pub width: u32,
    pub height: u32,
    /// PNG payload; the MCP layer also returns it as an image block.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base64: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DoctorReport {
    pub version: String,
    pub root: String,
    /// The bridge address on this platform: a socket path, or a named pipe.
    pub bridge: String,
    pub ui_running: bool,
    pub widgets: usize,
    pub displays: Vec<DisplayInfo>,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WidgetDocument {
    pub meta: WidgetMeta,
    pub package: Option<super::package::PackageManifest>,
    pub html: String,
    pub css: String,
    pub agents: Option<String>,
    pub frame: Option<WidgetFrame>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DisplayInfo {
    pub name: String,
    pub primary: bool,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub scale: f64,
}
