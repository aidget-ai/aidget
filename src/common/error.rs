use serde::Serialize;

use crate::widgets::model::{Board, WidgetMeta};

/// What a stale write collided with. Widget edits carry their widget; board
/// edits carry the board, so the retry uses the revision that was compared.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum ConflictCurrent {
    Widget(WidgetMeta),
    Board(Board),
}

impl ConflictCurrent {
    pub fn revision(&self) -> u64 {
        match self {
            ConflictCurrent::Widget(meta) => meta.revision,
            ConflictCurrent::Board(board) => board.revision,
        }
    }

    fn label(&self) -> &str {
        match self {
            ConflictCurrent::Widget(meta) => &meta.id,
            ConflictCurrent::Board(_) => "board",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid widget id: {0}")]
    InvalidId(String),
    #[error("widget not found: {0}")]
    NotFound(String),
    #[error("revision conflict: {} is now at revision {}", .current.label(), .current.revision())]
    RevisionConflict { current: Box<ConflictCurrent> },
    #[error("region not found: {0}")]
    RegionNotFound(String),
    #[error("region appears more than once: {0}")]
    RegionAmbiguous(String),
    #[error("invalid params: {0}")]
    InvalidParams(String),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("widget requires Aidget {minimum} or newer; current version is {current}")]
    HostVersionIncompatible {
        widget: String,
        current: String,
        minimum: String,
    },
    #[error("widget `{widget}` does not declare a minimum Aidget version")]
    HostVersionRequired { widget: String },
    #[error("location: {}", .0.code())]
    Location(crate::runtime::location::LocationError),
    #[error("ui is not running")]
    UiNotRunning,
    #[error("timeout")]
    Timeout,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Other(String),
}

impl Error {
    pub fn code(&self) -> &'static str {
        match self {
            Error::InvalidId(_) => "invalid_id",
            Error::NotFound(_) => "not_found",
            Error::RevisionConflict { .. } => "revision_conflict",
            Error::RegionNotFound(_) => "region_not_found",
            Error::RegionAmbiguous(_) => "region_ambiguous",
            Error::InvalidParams(_) => "invalid_params",
            Error::PermissionDenied(_) => "permission_denied",
            Error::HostVersionIncompatible { .. } => "host_version_incompatible",
            Error::HostVersionRequired { .. } => "host_version_required",
            Error::Location(error) => error.code(),
            Error::UiNotRunning => "ui_not_running",
            Error::Timeout => "timeout",
            Error::Io(_) | Error::Json(_) | Error::Other(_) => "io_error",
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorBody {
    pub code: &'static str,
    /// Payload only, never the rendered message: the receiver rebuilds the
    /// variant from `code + detail` so `Display` adds the prefix exactly once.
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<ConflictCurrent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub widget: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_host_version: Option<String>,
}

impl From<&Error> for ErrorBody {
    fn from(error: &Error) -> Self {
        let (detail, current) = match error {
            Error::InvalidId(detail) => (detail.clone(), None),
            Error::NotFound(detail) => (detail.clone(), None),
            Error::RegionNotFound(detail) => (detail.clone(), None),
            Error::RegionAmbiguous(detail) => (detail.clone(), None),
            Error::InvalidParams(detail) => (detail.clone(), None),
            Error::PermissionDenied(detail) => (detail.clone(), None),
            Error::HostVersionIncompatible {
                widget,
                current,
                minimum,
            } => (
                format!("widget `{widget}` requires Aidget {minimum} or newer; current version is {current}"),
                None,
            ),
            Error::HostVersionRequired { widget } => (
                format!("widget `{widget}` does not declare a minimum Aidget version"),
                None,
            ),
            Error::Location(_) => (String::new(), None),
            Error::RevisionConflict { current } => {
                let current = (**current).clone();
                (current.label().to_string(), Some(current))
            }
            Error::Other(message) => (message.clone(), None),
            Error::UiNotRunning | Error::Timeout => (String::new(), None),
            Error::Io(error) => (error.to_string(), None),
            Error::Json(error) => (error.to_string(), None),
        };
        ErrorBody {
            code: error.code(),
            detail,
            current,
            widget: match error {
                Error::HostVersionIncompatible { widget, .. }
                | Error::HostVersionRequired { widget } => Some(widget.clone()),
                _ => None,
            },
            current_version: match error {
                Error::HostVersionIncompatible { current, .. } => Some(current.clone()),
                _ => None,
            },
            min_host_version: match error {
                Error::HostVersionIncompatible { minimum, .. } => Some(minimum.clone()),
                _ => None,
            },
        }
    }
}

impl From<tauri::Error> for Error {
    fn from(error: tauri::Error) -> Self {
        Error::Other(error.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
