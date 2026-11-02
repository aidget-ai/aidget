use serde::Deserialize;
use serde_json::{json, Value};
use tauri::AppHandle;

use crate::common::error::{Error, Result};
use crate::runtime::ui;
use crate::widgets::model::{
    validate_id, DeleteResult, DoctorReport, ScreenshotResult, WidgetDragMode, WidgetFrame,
    WidgetPermission, MAX_PAYLOAD,
};
use crate::widgets::store::{Store, WidgetEdit, CONFIG_FILE, CSS_FILE, HTML_FILE};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WidgetRef {
    widget: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateWidget {
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    html: String,
    #[serde(default)]
    css: String,
    #[serde(default)]
    drag_mode: WidgetDragMode,
    #[serde(default)]
    permissions: Vec<WidgetPermission>,
    #[serde(default)]
    network: Vec<String>,
    #[serde(default)]
    agents: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UpdateWidget {
    widget: String,
    base_revision: u64,
    #[serde(default)]
    html: Option<String>,
    #[serde(default)]
    css: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    drag_mode: Option<WidgetDragMode>,
    #[serde(default)]
    permissions: Option<Vec<WidgetPermission>>,
    #[serde(default)]
    network: Option<Vec<String>>,
    #[serde(default)]
    agents: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PatchWidget {
    widget: String,
    base_revision: u64,
    regions: Vec<RegionEdit>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegionEdit {
    region: String,
    html: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MoveWidget {
    widget: String,
    base_revision: u64,
    display: Option<String>,
    x: Option<f64>,
    y: Option<f64>,
    w: Option<f64>,
    h: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Screenshot {
    widget: String,
    #[serde(default)]
    out: Option<String>,
    #[serde(default)]
    scale: Option<f64>,
}

pub async fn dispatch(
    app: AppHandle,
    store: std::sync::Arc<Store>,
    method: &str,
    params: Value,
) -> Result<Value> {
    match method {
        "community_list" => {
            let snapshot = tauri::async_runtime::spawn_blocking(move || {
                crate::community::service::Community::for_store(store.root())?.list(&store)
            })
            .await
            .map_err(|e| Error::Other(e.to_string()))??;
            Ok(serde_json::to_value(snapshot)?)
        }
        "community_install" => {
            let spec = params["package"]
                .as_str()
                .ok_or_else(|| Error::InvalidParams("missing package".into()))?
                .to_owned();
            let cloned = store.clone();
            let meta = tauri::async_runtime::spawn_blocking(move || {
                crate::community::service::Community::for_store(cloned.root())?
                    .install(&cloned, &spec)
            })
            .await
            .map_err(|e| Error::Other(e.to_string()))??;
            ui::place_new_widget(&app, &store, &meta.id)?;
            ui::sync_widgets(&app);
            Ok(serde_json::to_value(store.ack(&meta)?)?)
        }
        "ping" => Ok(json!({ "pong": true, "version": env!("CARGO_PKG_VERSION") })),
        "list" => list(&app, &store),
        "get" => get(&store, params),
        "create" => create(app, &store, params),
        "update" => update(app, &store, params),
        "patch" => patch(app, &store, params),
        "delete" => delete(app, &store, params),
        "move" => r#move(app, &store, params),
        "screenshot" => screenshot(app, &store, params).await,
        "doctor" => doctor(&app, &store),
        "show_main" => {
            ui::show_main(&app)?;
            Ok(json!({ "shown": true }))
        }
        other => Err(Error::InvalidParams(format!("unknown method `{other}`"))),
    }
}

fn list(app: &AppHandle, store: &Store) -> Result<Value> {
    let mut listing = store.list()?;
    listing.displays = Some(ui::displays(app)?);
    Ok(serde_json::to_value(listing)?)
}

fn get(store: &Store, params: Value) -> Result<Value> {
    let request: WidgetRef = serde_json::from_value(params)?;
    let document = store.load_document(&request.widget)?;
    Ok(serde_json::to_value(document)?)
}

fn create(app: AppHandle, store: &Store, params: Value) -> Result<Value> {
    let request: CreateWidget = serde_json::from_value(params)?;
    validate_id(&request.id)?;
    if request.html.len() > MAX_PAYLOAD {
        return Err(Error::InvalidParams("html exceeds 256 KB".to_string()));
    }
    let title = if request.title.is_empty() {
        request.id.clone()
    } else {
        request.title.clone()
    };
    let meta = store.create(
        &request.id,
        &title,
        WidgetEdit {
            html: Some(&request.html),
            css: Some(&request.css),
            drag_mode: Some(request.drag_mode),
            permissions: Some(&request.permissions),
            network: Some(&request.network),
            agents: request.agents.as_deref(),
            ..WidgetEdit::default()
        },
    )?;
    ui::place_new_widget(&app, store, &meta.id)?;
    ui::sync_widgets(&app);
    Ok(serde_json::to_value(store.ack(&meta)?)?)
}

fn update(app: AppHandle, store: &Store, params: Value) -> Result<Value> {
    let request: UpdateWidget = serde_json::from_value(params)?;
    if request
        .html
        .as_ref()
        .is_some_and(|html| html.len() > MAX_PAYLOAD)
    {
        return Err(Error::InvalidParams("html exceeds 256 KB".to_string()));
    }
    let meta = store.update(
        &request.widget,
        request.base_revision,
        WidgetEdit {
            html: request.html.as_deref(),
            css: request.css.as_deref(),
            title: request.title.as_deref(),
            network: request.network.as_deref(),
            drag_mode: request.drag_mode,
            permissions: request.permissions.as_deref(),
            agents: request.agents.as_deref(),
        },
    )?;
    ui::sync_widgets(&app);
    Ok(serde_json::to_value(store.content_ack(&meta)?)?)
}

fn patch(app: AppHandle, store: &Store, params: Value) -> Result<Value> {
    let request: PatchWidget = serde_json::from_value(params)?;
    let edits: Vec<(String, String)> = request
        .regions
        .into_iter()
        .map(|edit| (edit.region, edit.html))
        .collect();
    let meta = store.patch_regions(&request.widget, request.base_revision, &edits)?;
    ui::sync_widgets(&app);
    Ok(serde_json::to_value(store.content_ack(&meta)?)?)
}

fn delete(app: AppHandle, store: &Store, params: Value) -> Result<Value> {
    let request: WidgetRef = serde_json::from_value(params)?;
    let trash_path = store.delete(&request.widget)?;
    ui::sync_widgets(&app);
    Ok(serde_json::to_value(DeleteResult {
        widget: request.widget,
        trash_path: trash_path.to_string_lossy().to_string(),
    })?)
}

fn r#move(app: AppHandle, store: &Store, params: Value) -> Result<Value> {
    let request: MoveWidget = serde_json::from_value(params)?;
    validate_id(&request.widget)?;
    let board = store.load_board()?;
    let mut frame = board
        .widgets
        .get(&request.widget)
        .cloned()
        .unwrap_or_else(|| WidgetFrame::default_for(board.widgets.len()));
    if let Some(display) = request.display {
        frame.display = display;
    }
    if let Some(x) = request.x {
        frame.x = x;
    }
    if let Some(y) = request.y {
        frame.y = y;
    }
    if let Some(w) = request.w {
        frame.w = w;
    }
    if let Some(h) = request.h {
        frame.h = h;
    }
    let board = store.move_widget(&request.widget, request.base_revision, frame)?;
    ui::apply_board(&app, &board);
    let meta = store.load_meta(&request.widget)?;
    Ok(serde_json::to_value(store.ack(&meta)?)?)
}

async fn screenshot(app: AppHandle, store: &Store, params: Value) -> Result<Value> {
    let request: Screenshot = serde_json::from_value(params)?;
    validate_id(&request.widget)?;
    store.load_settings()?;
    if store.load_meta(&request.widget).is_err() {
        return Err(Error::NotFound(request.widget));
    }
    let out = request
        .out
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            store.screenshots_dir().join(format!(
                "{}-{}.png",
                request.widget,
                chrono::Local::now().format("%Y%m%dT%H%M%S")
            ))
        });
    let scale = request.scale.unwrap_or(1.0).clamp(0.25, 2.0);
    let png = ui::screenshot(&app, &request.widget, scale).await?;
    store.write_screenshot(&out, &png)?;
    let (width, height) =
        png_size(&png).ok_or_else(|| Error::Other("snapshot is not a PNG".to_string()))?;
    Ok(serde_json::to_value(ScreenshotResult {
        widget: request.widget,
        path: out.to_string_lossy().to_string(),
        bytes: png.len(),
        width,
        height,
        base64: Some(base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            &png,
        )),
    })?)
}

/// PNG dimensions from the IHDR chunk, so the response can report the real
/// pixel size instead of the requested one.
fn png_size(png: &[u8]) -> Option<(u32, u32)> {
    const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    if png.len() < 24 || png[..8] != SIGNATURE || &png[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes(png[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(png[20..24].try_into().ok()?);
    Some((width, height))
}

fn doctor(app: &AppHandle, store: &Store) -> Result<Value> {
    let ids = store.widget_ids()?;
    Ok(serde_json::to_value(DoctorReport {
        version: env!("CARGO_PKG_VERSION").to_string(),
        root: store.root().to_string_lossy().to_string(),
        bridge: crate::transport::ipc::Endpoint::new(store.socket_path()).address(),
        ui_running: true,
        widgets: ids.len(),
        displays: ui::displays(app)?,
        files: vec![
            CONFIG_FILE.to_string(),
            HTML_FILE.to_string(),
            CSS_FILE.to_string(),
        ],
    })?)
}
