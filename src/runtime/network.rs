//! Host-side network and external-link access for script-enabled widgets.
//!
//! Widgets run in an opaque-origin iframe, so browser requests to public APIs
//! are subject to third-party CORS policy. The host performs declared requests
//! instead, but only after checking that the URL's exact origin is present in
//! the widget's `network` list.

use std::collections::BTreeMap;
use std::io::Read;
use std::time::Duration;

use reqwest::{
    blocking::Client,
    header::{HeaderMap, HeaderName, HeaderValue},
    redirect::Policy,
    Method, Url,
};
use serde::Serialize;
use serde_json::Value;
use tauri_plugin_opener::OpenerExt;

use crate::common::error::Error;
use crate::widgets::{
    model::{WidgetMeta, WidgetPermission, MAX_PAYLOAD},
    store::Store,
};

const MAX_JSON_BYTES: usize = 1024 * 1024;
const MAX_HEADER_BYTES: usize = 32 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(12);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_REDIRECTS: usize = 3;

/// Caller-controlled request details for `window.aidget.fetch()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchOptions {
    /// HTTP method; defaults to GET.
    pub method: String,
    /// Application headers supplied by the caller.
    pub headers: BTreeMap<String, String>,
    /// Optional UTF-8 request body.
    pub body: Option<String>,
}

impl Default for FetchOptions {
    fn default() -> Self {
        Self {
            method: "GET".to_string(),
            headers: BTreeMap::new(),
            body: None,
        }
    }
}

/// A serialized failure consumed by the widget bridge as `error.code`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NetworkFailure {
    /// Stable error code exposed as `error.code` to widget scripts.
    pub code: &'static str,
    /// Human-readable failure detail.
    pub message: String,
}

impl NetworkFailure {
    fn permission_denied(message: impl Into<String>) -> Self {
        Self {
            code: "permission_denied",
            message: message.into(),
        }
    }

    fn invalid_params(message: impl Into<String>) -> Self {
        Self {
            code: "invalid_params",
            message: message.into(),
        }
    }

    fn invalid_response(message: impl Into<String>) -> Self {
        Self {
            code: "invalid_response",
            message: message.into(),
        }
    }

    fn unavailable(message: impl Into<String>) -> Self {
        Self {
            code: "unavailable",
            message: message.into(),
        }
    }

    fn timeout(method: &Method, url: &Url) -> Self {
        Self {
            code: "timeout",
            message: format!("{} {url}: timed out", method.as_str()),
        }
    }

    /// Blocking body reads wrap `reqwest::Error` inside `io::Error`, so the
    /// timeout flag has to be read from that inner error.
    fn from_body_read(method: &Method, url: &Url, error: std::io::Error) -> Self {
        let timed_out = error
            .get_ref()
            .and_then(|source| source.downcast_ref::<reqwest::Error>())
            .is_some_and(reqwest::Error::is_timeout);
        if timed_out {
            Self::timeout(method, url)
        } else {
            Self::unavailable(format!("{} {url}: {error}", method.as_str()))
        }
    }
}

impl From<Error> for NetworkFailure {
    fn from(error: Error) -> Self {
        Self {
            code: error.code(),
            message: error.to_string(),
        }
    }
}

/// Validate a widget URL against its script and exact-origin declarations.
pub fn authorize_url(meta: &WidgetMeta, raw: &str) -> std::result::Result<Url, NetworkFailure> {
    if !meta.grants(WidgetPermission::Script) {
        return Err(NetworkFailure::permission_denied(
            "widget does not declare permissions: [\"script\"]",
        ));
    }
    let url = Url::parse(raw)
        .map_err(|error| NetworkFailure::invalid_params(format!("invalid widget URL: {error}")))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(NetworkFailure::invalid_params(
            "widget URL must be an absolute HTTPS URL without credentials",
        ));
    }
    let origin = url.origin().ascii_serialization();
    if !meta.network.iter().any(|allowed| allowed == &origin) {
        return Err(NetworkFailure::permission_denied(format!(
            "widget network list does not declare {origin}"
        )));
    }
    Ok(url)
}

/// Bounded same-origin JSON transport used by widget host commands.
pub struct NetworkClient {
    client: Client,
}

impl NetworkClient {
    /// Build a client with bounded redirects, deadlines and response bodies.
    pub fn new() -> std::result::Result<Self, NetworkFailure> {
        let client = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .redirect(Policy::custom(|attempt| {
                if attempt.previous().len() > MAX_REDIRECTS
                    || attempt.previous().first().is_none_or(|original| {
                        attempt.url().origin().ascii_serialization()
                            != original.origin().ascii_serialization()
                    })
                {
                    attempt.error("widget redirect blocked: cross-origin or too many hops")
                } else {
                    attempt.follow()
                }
            }))
            .build()
            .map_err(|error| NetworkFailure::unavailable(error.to_string()))?;
        Ok(Self { client })
    }

    /// Send a caller-defined request and parse the response as JSON.
    pub fn fetch_json(
        &self,
        url: Url,
        options: &FetchOptions,
    ) -> std::result::Result<Value, NetworkFailure> {
        let method = Method::from_bytes(options.method.trim().to_ascii_uppercase().as_bytes())
            .map_err(|_| NetworkFailure::invalid_params("invalid HTTP method"))?;
        if matches!(method.as_str(), "CONNECT" | "TRACE" | "TRACK" | "HEAD") {
            return Err(NetworkFailure::invalid_params(
                "CONNECT, TRACE, TRACK and HEAD are not allowed",
            ));
        }
        if method == Method::GET && options.body.is_some() {
            return Err(NetworkFailure::invalid_params(
                "GET requests cannot have a body",
            ));
        }
        if options
            .body
            .as_ref()
            .is_some_and(|body| body.len() > MAX_PAYLOAD)
        {
            return Err(NetworkFailure::invalid_params(format!(
                "request body exceeds {MAX_PAYLOAD} bytes"
            )));
        }
        let headers = header_map(&options.headers)?;
        let mut request = self
            .client
            .request(method.clone(), url.clone())
            .headers(headers);
        if let Some(body) = &options.body {
            request = request.body(body.clone());
        }
        let response = request.send().map_err(|error| {
            if error.is_timeout() {
                NetworkFailure::timeout(&method, &url)
            } else {
                NetworkFailure::unavailable(format!("{} {url}: {error}", method.as_str()))
            }
        })?;
        let status = response.status();
        if !status.is_success() {
            return Err(NetworkFailure {
                code: "http_error",
                message: format!("{} {url}: HTTP {status}", method.as_str()),
            });
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_JSON_BYTES as u64)
        {
            return Err(NetworkFailure::invalid_response(format!(
                "{} {url}: response exceeds {MAX_JSON_BYTES} bytes",
                method.as_str()
            )));
        }
        let mut bytes = Vec::new();
        response
            .take(MAX_JSON_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| NetworkFailure::from_body_read(&method, &url, error))?;
        if bytes.len() > MAX_JSON_BYTES {
            return Err(NetworkFailure::invalid_response(format!(
                "{} {url}: response exceeds {MAX_JSON_BYTES} bytes",
                method.as_str()
            )));
        }
        serde_json::from_slice(&bytes).map_err(|error| {
            NetworkFailure::invalid_response(format!("{} {url}: {error}", method.as_str()))
        })
    }
}

/// Fetch JSON for one widget after validating its current declaration.
pub fn fetch_json(
    store: &Store,
    id: &str,
    raw: &str,
    options: &FetchOptions,
) -> std::result::Result<Value, NetworkFailure> {
    let meta = store.load_meta(id).map_err(NetworkFailure::from)?;
    let url = authorize_url(&meta, raw)?;
    NetworkClient::new()?.fetch_json(url, options)
}

fn header_map(
    headers: &BTreeMap<String, String>,
) -> std::result::Result<HeaderMap, NetworkFailure> {
    let mut result = HeaderMap::new();
    let mut bytes = 0;
    for (name, value) in headers {
        let header_name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| NetworkFailure::invalid_params(format!("invalid header name: {name}")))?;
        if matches!(
            header_name.as_str(),
            "host"
                | "content-length"
                | "connection"
                | "transfer-encoding"
                | "upgrade"
                | "proxy-connection"
                | "expect"
                | "te"
                | "trailer"
        ) {
            return Err(NetworkFailure::invalid_params(format!(
                "header is managed by the host: {name}"
            )));
        }
        let header_value = HeaderValue::from_str(value).map_err(|_| {
            NetworkFailure::invalid_params(format!("invalid value for header: {name}"))
        })?;
        bytes += name.len() + value.len();
        if bytes > MAX_HEADER_BYTES {
            return Err(NetworkFailure::invalid_params(format!(
                "request headers exceed {MAX_HEADER_BYTES} bytes"
            )));
        }
        result.insert(header_name, header_value);
    }
    Ok(result)
}

/// Open an allowlisted HTTPS URL in the system browser.
pub fn open_url(
    app: &tauri::AppHandle,
    store: &Store,
    id: &str,
    raw: &str,
) -> std::result::Result<(), NetworkFailure> {
    let meta = store.load_meta(id).map_err(NetworkFailure::from)?;
    let url = authorize_url(&meta, raw)?;
    app.opener()
        .open_url(url.as_str(), None::<&str>)
        .map_err(|error| NetworkFailure::unavailable(error.to_string()))
}
