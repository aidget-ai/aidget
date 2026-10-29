use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use serde::Serialize;
use serde_json::{json, Value};

use crate::common::error::{Error, Result};
use crate::transport::client::Client;
use crate::widgets::model::{
    validate_id, DeleteResult, DoctorReport, ScreenshotResult, WidgetDragMode, WidgetFrame,
    WidgetListing, WidgetPermission,
};
use crate::widgets::store::{Store, WidgetEdit, CONFIG_FILE, CSS_FILE, HTML_FILE};

#[derive(Parser)]
#[command(name = "aidget", version, about = "Agent-controllable desktop widgets")]
pub struct Cli {
    /// Emit machine-readable JSON for commands and errors.
    #[arg(long, global = true)]
    pub json: bool,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Run the resident desktop UI.
    Ui {
        /// Detach and start in the tray without opening the main window.
        #[arg(long)]
        background: bool,
        /// Start in the tray without opening the main window.
        #[arg(long, hide = true)]
        no_main_window: bool,
    },
    /// Serve MCP over stdio (starts the UI on demand).
    Mcp,
    /// Read and write widgets.
    Widget {
        #[command(subcommand)]
        command: WidgetCommand,
    },
    /// Inspect and move widgets on the board.
    Board {
        #[command(subcommand)]
        command: BoardCommand,
    },
    /// List the displays available to board move.
    Displays,
    /// Browse reviewed widgets or install a complete npm package.
    Community {
        #[command(subcommand)]
        command: CommunityCommand,
    },
    /// Render a widget window to PNG (requires the UI).
    Screenshot {
        widget: String,
        #[arg(long)]
        out: Option<PathBuf>,
        /// Output pixels per layout point; 2 doubles width and height.
        #[arg(long, default_value_t = 1.0)]
        scale: f64,
    },
    /// Check data directory, socket and widgets.
    Doctor,
    /// Print the Markdown widget authoring guide.
    Guide,
}

#[derive(Subcommand)]
pub enum WidgetCommand {
    /// List installed widgets and their revisions.
    List,
    /// Show one widget's files and author guidance.
    Show { widget: String },
    /// Create a widget from inline text or files.
    Create(CreateArgs),
    /// Replace widget files or metadata with revision CAS.
    Update(UpdateArgs),
    /// Replace one data-aidget-region.
    Patch(PatchArgs),
    /// Move a widget directory into the trash.
    Delete { widget: String },
}

#[derive(Args)]
pub struct CreateArgs {
    pub id: String,
    #[arg(long, default_value = "")]
    pub title: String,
    #[arg(long)]
    pub html_file: Option<PathBuf>,
    #[arg(long)]
    pub css_file: Option<PathBuf>,
    #[arg(long)]
    pub html: Option<String>,
    #[arg(long)]
    pub css: Option<String>,
    /// Exact HTTPS origins, comma separated; none means no network access.
    #[arg(long)]
    pub network: Option<String>,
    /// Start dragging from ordinary content or only the top handle.
    #[arg(long, value_enum)]
    pub drag_mode: Option<WidgetDragMode>,
    /// Comma-separated host capabilities.
    #[arg(long)]
    pub permissions: Option<String>,
    #[arg(long)]
    pub agents_file: Option<PathBuf>,
}

#[derive(Args)]
pub struct UpdateArgs {
    pub widget: String,
    #[arg(long)]
    pub html_file: Option<PathBuf>,
    #[arg(long)]
    pub css_file: Option<PathBuf>,
    #[arg(long)]
    pub title: Option<String>,
    /// Replace the network origin list; none clears it.
    #[arg(long)]
    pub network: Option<String>,
    /// Replace the window drag policy; omitted keeps the current mode.
    #[arg(long, value_enum)]
    pub drag_mode: Option<WidgetDragMode>,
    /// Comma-separated host capabilities; `none` clears them.
    #[arg(long)]
    pub permissions: Option<String>,
    #[arg(long)]
    pub agents_file: Option<PathBuf>,
    /// Current widget revision from `widget show`.
    #[arg(long)]
    pub revision: u64,
}

#[derive(Args)]
pub struct PatchArgs {
    pub widget: String,
    pub region: String,
    #[arg(long)]
    pub html: Option<String>,
    #[arg(long)]
    pub html_file: Option<PathBuf>,
    /// Current widget revision from `widget show`.
    #[arg(long)]
    pub revision: u64,
}

#[derive(Subcommand)]
pub enum BoardCommand {
    /// Show enabled widget frames.
    Show,
    /// Move or resize a widget with board revision CAS.
    Move {
        widget: String,
        #[arg(long)]
        display: Option<String>,
        #[arg(long)]
        x: Option<f64>,
        #[arg(long)]
        y: Option<f64>,
        #[arg(long)]
        w: Option<f64>,
        #[arg(long)]
        h: Option<f64>,
        #[arg(long)]
        revision: u64,
    },
}

/// Native package distribution without requiring npm or Git on the client.
#[derive(Subcommand)]
pub enum CommunityCommand {
    /// List reviewed packages from official main.
    List,
    /// Install a complete npm name and an optional exact version.
    Install { package: String },
}

pub fn run(cli: Cli) -> Result<()> {
    let store = Store::open_default();
    store.ensure_layout()?;
    crate::runtime::process::publish_cli(store.root());
    let json = cli.json;
    match cli.command.unwrap_or(Command::Guide) {
        Command::Ui { .. } | Command::Mcp => Err(Error::Other(
            "internal: ui and mcp are handled by main".to_string(),
        )),
        Command::Widget { command } => widget_command(&store, command, json),
        Command::Board { command } => board_command(&store, command, json),
        Command::Displays => displays_command(&store, json),
        Command::Community { command } => {
            let client = Client::new(store.socket_path());
            crate::runtime::process::ensure_ui(&client)?;
            let result = match command {
                CommunityCommand::List => client.call_with_timeout(
                    "community_list",
                    json!({}),
                    std::time::Duration::from_secs(180),
                )?,
                CommunityCommand::Install { package } => client.call_with_timeout(
                    "community_install",
                    json!({"package":package}),
                    std::time::Duration::from_secs(180),
                )?,
            };
            if json {
                print_json(&result)?;
            } else {
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
            Ok(())
        }
        Command::Screenshot { widget, out, scale } => {
            screenshot_command(&store, &widget, out, scale, json)
        }
        Command::Doctor => doctor_command(&store, json),
        Command::Guide if json => Err(Error::InvalidParams(
            "guide always prints Markdown; omit --json".to_string(),
        )),
        Command::Guide => crate::interfaces::guide::print(),
    }
}

fn widget_command(store: &Store, command: WidgetCommand, json: bool) -> Result<()> {
    match command {
        WidgetCommand::List => {
            let listing = store.list()?;
            if json {
                print_json(&listing)?;
            } else {
                for broken in &listing.broken {
                    eprintln!("broken {}: {}", broken.id, broken.error);
                }
                if listing.widgets.is_empty() {
                    println!("no widgets yet");
                }
                for widget in listing.widgets {
                    let frame = widget
                        .frame
                        .map(|f| format!("{:.0}x{:.0} @ {:.0},{:.0}", f.w, f.h, f.x, f.y))
                        .unwrap_or_else(|| "-".to_string());
                    println!(
                        "{:<24} rev {:<4} {:<20} {}",
                        widget.id, widget.revision, frame, widget.title
                    );
                }
            }
            Ok(())
        }
        WidgetCommand::Show { widget } => {
            let document = store.load_document(&widget)?;
            if json {
                print_json(&document)?;
            } else {
                println!("# {}", document.meta.title);
                println!(
                    "id={} revision={} scripts={} drag={} permissions={} frame={}",
                    document.meta.id,
                    document.meta.revision,
                    document.meta.grants(WidgetPermission::Script),
                    serde_json::to_string(&document.meta.drag_mode)?,
                    serde_json::to_string(&document.meta.permissions)?,
                    document
                        .frame
                        .as_ref()
                        .map(|f| format!(
                            "{} {:.0}x{:.0} @ {:.0},{:.0}",
                            f.display, f.w, f.h, f.x, f.y
                        ))
                        .unwrap_or_else(|| "-".to_string())
                );
                println!("\n--- index.html ---\n{}", document.html);
                if !document.css.is_empty() {
                    println!("\n--- style.css ---\n{}", document.css);
                }
                if let Some(agents) = document.agents {
                    println!("\n--- agents.md ---\n{agents}");
                }
            }
            Ok(())
        }
        WidgetCommand::Create(args) => {
            validate_id(&args.id)?;
            let html = read_input(args.html, args.html_file)?;
            let css = read_input(args.css, args.css_file)?.unwrap_or_default();
            let agents = args.agents_file.map(std::fs::read_to_string).transpose()?;
            let title = if args.title.is_empty() {
                args.id.clone()
            } else {
                args.title
            };
            let permissions = parse_permissions(args.permissions.as_deref())?;
            let meta = store.create(
                &args.id,
                &title,
                WidgetEdit {
                    html: Some(&html.unwrap_or_default()),
                    css: Some(&css),
                    network: Some(&parse_network(args.network.as_deref())),
                    drag_mode: args.drag_mode,
                    permissions: Some(&permissions),
                    agents: agents.as_deref(),
                    ..WidgetEdit::default()
                },
            )?;
            if json {
                print_json(&store.ack(&meta)?)?;
            } else {
                println!("created {} revision={}", meta.id, meta.revision);
            }
            Ok(())
        }
        WidgetCommand::Update(args) => {
            let html = args.html_file.map(std::fs::read_to_string).transpose()?;
            let css = args.css_file.map(std::fs::read_to_string).transpose()?;
            let agents = args.agents_file.map(std::fs::read_to_string).transpose()?;
            let permissions = args
                .permissions
                .as_deref()
                .map(|raw| parse_permissions(Some(raw)))
                .transpose()?;
            let meta = store.update(
                &args.widget,
                args.revision,
                WidgetEdit {
                    html: html.as_deref(),
                    css: css.as_deref(),
                    title: args.title.as_deref(),
                    network: args.network.as_deref().map(parse_network_value).as_deref(),
                    drag_mode: args.drag_mode,
                    permissions: permissions.as_deref(),
                    agents: agents.as_deref(),
                },
            )?;
            if json {
                print_json(&store.content_ack(&meta)?)?;
            } else {
                println!("updated {} revision={}", meta.id, meta.revision);
            }
            Ok(())
        }
        WidgetCommand::Patch(args) => {
            let html = read_input(args.html, args.html_file)?
                .ok_or_else(|| Error::InvalidParams("pass --html or --html-file".to_string()))?;
            let meta = store.patch_region(&args.widget, args.revision, &args.region, &html)?;
            if json {
                print_json(&store.content_ack(&meta)?)?;
            } else {
                println!(
                    "patched {} region={} revision={}",
                    meta.id, args.region, meta.revision
                );
            }
            Ok(())
        }
        WidgetCommand::Delete { widget } => {
            let trash = store.delete(&widget)?;
            if json {
                print_json(&DeleteResult {
                    widget,
                    trash_path: trash.to_string_lossy().to_string(),
                })?;
            } else {
                println!("moved {widget} to {}", trash.display());
            }
            Ok(())
        }
    }
}

fn board_command(store: &Store, command: BoardCommand, json: bool) -> Result<()> {
    match command {
        BoardCommand::Show => {
            let board = store.load_board()?;
            if json {
                print_json(&board)?;
            } else {
                println!("revision {}", board.revision);
                for (id, frame) in board.widgets {
                    println!(
                        "{:<24} {:<16} {:.0}x{:.0} @ {:.0},{:.0}",
                        id, frame.display, frame.w, frame.h, frame.x, frame.y
                    );
                }
            }
            Ok(())
        }
        BoardCommand::Move {
            widget,
            display,
            x,
            y,
            w,
            h,
            revision,
        } => {
            validate_id(&widget)?;
            let board = store.load_board()?;
            let meta = store.load_meta(&widget)?;
            let mut frame = board
                .widgets
                .get(&widget)
                .cloned()
                .unwrap_or_else(|| WidgetFrame::default_for(board.widgets.len()));
            if let Some(display) = display {
                frame.display = display;
            }
            if let Some(x) = x {
                frame.x = x;
            }
            if let Some(y) = y {
                frame.y = y;
            }
            if let Some(w) = w {
                frame.w = w;
            }
            if let Some(h) = h {
                frame.h = h;
            }
            store.move_widget(&widget, revision, frame)?;
            let ack = store.ack(&meta)?;
            if json {
                print_json(&ack)?;
            } else {
                let frame = ack
                    .frame
                    .as_ref()
                    .ok_or_else(|| Error::Other("move ack omitted frame".to_string()))?;
                println!(
                    "moved {} to {} {:.0}x{:.0} @ {:.0},{:.0}; board revision {}",
                    widget, frame.display, frame.w, frame.h, frame.x, frame.y, ack.board_revision
                );
            }
            Ok(())
        }
    }
}

fn displays_command(store: &Store, json: bool) -> Result<()> {
    let client = Client::new(store.socket_path());
    let listing: WidgetListing = serde_json::from_value(client.call("list", json!({}))?)?;
    let displays = listing
        .displays
        .ok_or_else(|| Error::Other("ui returned no display list".to_string()))?;
    if json {
        print_json(&displays)?;
    } else {
        for display in displays {
            println!(
                "{:<24} primary={:<5} {:.0}x{:.0} @ {:.0},{:.0} scale={}",
                display.name,
                display.primary,
                display.width,
                display.height,
                display.x,
                display.y,
                display.scale
            );
        }
    }
    Ok(())
}

fn screenshot_command(
    store: &Store,
    widget: &str,
    out: Option<PathBuf>,
    scale: f64,
    json: bool,
) -> Result<()> {
    let client = Client::new(store.socket_path());
    let mut params = json!({ "widget": widget, "scale": scale });
    if let Some(out) = out {
        params["out"] = Value::String(out.to_string_lossy().to_string());
    }
    let mut result: ScreenshotResult = serde_json::from_value(client.call("screenshot", params)?)?;
    if json {
        result.base64 = None;
        print_json(&result)?;
    } else {
        println!("{}", result.path);
    }
    Ok(())
}

fn doctor_command(store: &Store, json: bool) -> Result<()> {
    let client = Client::new(store.socket_path());
    let responsive = client.ping(std::time::Duration::from_millis(800));
    let ids = store.widget_ids()?;
    if json {
        let report = if responsive {
            serde_json::from_value::<DoctorReport>(client.call("doctor", json!({}))?)?
        } else {
            DoctorReport {
                version: env!("CARGO_PKG_VERSION").to_string(),
                root: store.root().to_string_lossy().to_string(),
                bridge: client.address(),
                ui_running: false,
                widgets: ids.len(),
                displays: Vec::new(),
                files: vec![
                    CONFIG_FILE.to_string(),
                    HTML_FILE.to_string(),
                    CSS_FILE.to_string(),
                ],
            }
        };
        print_json(&report)?;
        return Ok(());
    }
    println!("aidget {}", env!("CARGO_PKG_VERSION"));
    println!("root       {}", store.root().display());
    println!(
        "bridge     {} ({})",
        client.address(),
        if responsive {
            "ui running and responding"
        } else if client.is_connectable() {
            "bridge open but not responding"
        } else {
            "ui not running"
        }
    );
    println!("widgets    {}", ids.len());
    for id in ids {
        match store.load_meta(&id) {
            Ok(meta) => println!(
                "  ok       {:<24} rev {} scripts={}",
                id,
                meta.revision,
                meta.grants(WidgetPermission::Script)
            ),
            Err(error) => println!("  broken   {:<24} {error}", id),
        }
    }
    let board = store.load_board()?;
    println!("board      revision {}", board.revision);
    if board.revision > 0 {
        for (id, frame) in board.widgets {
            println!(
                "  {:<24} {} {:.0}x{:.0} @ {:.0},{:.0}",
                id, frame.display, frame.w, frame.h, frame.x, frame.y
            );
        }
    }
    if !store
        .root()
        .join(crate::widgets::store::BOOTSTRAP_MARKER)
        .exists()
    {
        println!("note       next launch will install the welcome widget when widgets/ is empty");
    }
    Ok(())
}

fn print_json(value: &impl Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn read_input(inline: Option<String>, file: Option<PathBuf>) -> Result<Option<String>> {
    match (inline, file) {
        (Some(_), Some(_)) => Err(Error::InvalidParams(
            "pass either inline text or a file, not both".to_string(),
        )),
        (Some(value), None) => Ok(Some(value)),
        (None, Some(path)) => Ok(Some(std::fs::read_to_string(path)?)),
        (None, None) => Ok(None),
    }
}

/// Replace the full permission list; `none` expresses an explicitly empty list.
fn parse_permissions(raw: Option<&str>) -> Result<Vec<WidgetPermission>> {
    let Some(raw) = raw else {
        return Ok(Vec::new());
    };
    let mut permissions = Vec::new();
    for part in raw
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        match part {
            "none" | "clear" => return Ok(Vec::new()),
            "script" => permissions.push(WidgetPermission::Script),
            "location" => permissions.push(WidgetPermission::Location),
            "storage" => permissions.push(WidgetPermission::Storage),
            other => {
                return Err(Error::InvalidParams(format!(
                    "unknown permission `{other}`; supported: script, location, storage, none"
                )))
            }
        }
    }
    Ok(permissions)
}

/// Parse the complete network declaration; metadata validation checks origins.
fn parse_network(raw: Option<&str>) -> Vec<String> {
    raw.map(parse_network_value).unwrap_or_default()
}
fn parse_network_value(raw: &str) -> Vec<String> {
    if raw == "none" {
        vec![]
    } else {
        raw.split(',').map(str::trim).map(str::to_string).collect()
    }
}
