use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::common::error::{ConflictCurrent, Error, ErrorBody, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub id: String,
    pub method: String,
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub id: String,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorBodyValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorBodyValue {
    pub code: String,
    /// Payload only: the receiver rebuilds the error from `code` + `detail`, so
    /// `Display` adds its prefix exactly once.
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<ConflictCurrent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub widget: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_host_version: Option<String>,
}

impl Response {
    pub fn ok(id: &str, result: Value) -> Self {
        Response {
            id: id.to_string(),
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: &str, error: &Error) -> Self {
        let body = ErrorBody::from(error);
        Response {
            id: id.to_string(),
            ok: false,
            result: None,
            error: Some(ErrorBodyValue {
                code: body.code.to_string(),
                detail: body.detail,
                current: body.current,
                widget: body.widget,
                current_version: body.current_version,
                min_host_version: body.min_host_version,
            }),
        }
    }

    pub fn into_result(self) -> Result<Value> {
        if self.ok {
            Ok(self.result.unwrap_or(Value::Null))
        } else {
            let body = self
                .error
                .ok_or_else(|| Error::Other("bridge response has no error body".to_string()))?;
            Err(match body.code.as_str() {
                "revision_conflict" => Error::RevisionConflict {
                    current: Box::new(body.current.ok_or_else(|| {
                        Error::Other("revision conflict without current widget".to_string())
                    })?),
                },
                "not_found" => Error::NotFound(body.detail),
                "invalid_id" => Error::InvalidId(body.detail),
                "invalid_params" => Error::InvalidParams(body.detail),
                "region_not_found" => Error::RegionNotFound(body.detail),
                "region_ambiguous" => Error::RegionAmbiguous(body.detail),
                "host_version_incompatible" => Error::HostVersionIncompatible {
                    widget: body.widget.ok_or_else(|| {
                        Error::Other("incompatible version error without widget".into())
                    })?,
                    current: body.current_version.ok_or_else(|| {
                        Error::Other("incompatible version error without current version".into())
                    })?,
                    minimum: body.min_host_version.ok_or_else(|| {
                        Error::Other("incompatible version error without minimum version".into())
                    })?,
                },
                "host_version_required" => Error::HostVersionRequired {
                    widget: body.widget.ok_or_else(|| {
                        Error::Other("missing version declaration error without widget".into())
                    })?,
                },
                "ui_not_running" => Error::UiNotRunning,
                "timeout" => Error::Timeout,
                _ => Error::Other(body.detail),
            })
        }
    }
}
