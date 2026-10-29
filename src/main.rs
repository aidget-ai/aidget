#![cfg_attr(all(not(debug_assertions), windows), windows_subsystem = "windows")]

use std::sync::Arc;

use aidget::common::error::{Error, ErrorBody, Result};
use aidget::interfaces::cli::{Cli, Command};
use aidget::runtime::ui;
use aidget::settings::{self, TrayClickAction};
use aidget::widgets::store::Store;
use clap::error::ErrorKind;
use clap::Parser;
use tauri::Manager;

fn main() {
    let json = std::env::args_os().any(|argument| argument == "--json");
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error)
            if json
                && !matches!(
                    error.kind(),
                    ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
                ) =>
        {
            let body = serde_json::to_string(&serde_json::json!({
                "code": "invalid_params",
                "detail": error.to_string(),
            }))
            .expect("clap errors are serializable");
            eprintln!("{body}");
            std::process::exit(error.exit_code());
        }
        Err(error) => error.exit(),
    };
    let json = cli.json;
    let result = match cli.command {
        // Launching the bundled app (or `tauri dev`) passes no arguments.
        None => run_ui(true, false),
        Some(Command::Ui {
            background,
            no_main_window,
        }) => run_ui(!background && !no_main_window, background),
        Some(Command::Mcp) => run_mcp(),
        other => aidget::interfaces::cli::run(Cli {
            command: other,
            json,
        }),
    };
    if let Err(error) = result {
        if json {
            match serde_json::to_string(&ErrorBody::from(&error)) {
                Ok(body) => eprintln!("{body}"),
                Err(serialize_error) => eprintln!("aidget: {serialize_error}"),
            }
        } else {
            eprintln!("aidget: {error}");
        }
        std::process::exit(1);
    }
}

fn run_ui(show_main_window: bool, detach: bool) -> Result<()> {
    let store = Store::open_default();
    store.ensure_layout()?;
    aidget::runtime::process::publish_cli(store.root());
    let client = aidget::transport::client::Client::new(store.socket_path());
    if client.is_running() {
        if show_main_window {
            reopen_running_app(&client)?;
            return Ok(());
        }
        println!("aidget ui is already running");
        return Ok(());
    }
    if client.is_connectable() {
        // A socket that accepts but does not answer may belong to an instance
        // that is still starting. Never treat it as stale and bind over it.
        if client.wait_ready(std::time::Duration::from_secs(3)) {
            if show_main_window {
                reopen_running_app(&client)?;
            } else {
                println!("aidget ui is already running");
            }
            return Ok(());
        }
        return Err(Error::Other(
            "aidget ui socket is open but not responding; stop that process first".to_string(),
        ));
    }
    if detach {
        return aidget::runtime::process::spawn_ui();
    }

    let skill_directory = aidget::agents::skill::ensure_skill(&store)?;
    for problem in aidget::agents::integration::migrate_skill_links(&skill_directory) {
        eprintln!(
            "agent skill migration {}: {}",
            problem.agent_id, problem.message
        );
    }
    let endpoint = aidget::transport::ipc::Endpoint::new(store.socket_path());
    let listener = endpoint
        .bind()
        .map_err(|error| Error::Other(format!("cannot bind socket: {error}")))?;
    let migration_errors = aidget::widgets::migration::run(&store)?;
    for broken in &migration_errors {
        eprintln!("widget migration {}: {}", broken.id, broken.error);
    }
    if migration_errors.is_empty() {
        store.prune_retained()?;
    } else {
        store.prune_screenshots()?;
    }
    aidget::widgets::template::bootstrap_welcome(&store)?;
    let store_for_ui = store.clone();
    let store_for_protocol = Arc::new(store.clone());
    let store_for_server = Arc::new(store.clone());

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["ui", "--no-main-window"]),
        ))
        .plugin(tauri_plugin_opener::init())
        .register_uri_scheme_protocol("aidget", move |ctx, request| {
            aidget::transport::server::protocol::handle(&ctx, request, &store_for_protocol)
        })
        .manage(aidget::update::UpdateService::new())
        .invoke_handler(aidget::runtime::commands::handler())
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(if show_main_window {
                tauri::ActivationPolicy::Regular
            } else {
                tauri::ActivationPolicy::Accessory
            });
            ui::start(app.handle(), store_for_ui.clone())?;
            if show_main_window {
                ui::show_main_now(app.handle())?;
            } else {
                ui::start_initial_widgets(app.handle())?;
            }
            aidget::transport::server::serve(
                app.handle().clone(),
                store_for_server,
                listener,
                endpoint,
            )?;
            tray(app.handle())?;
            aidget::runtime::update::start_check(app.handle());
            if show_main_window {
                println!(
                    "aidget ui ready — data at {}",
                    store_for_ui.root().display()
                );
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .map_err(|error| Error::Other(error.to_string()))?;

    app.run(|handle, event| {
        match event {
            // A tray app must survive closing its last widget window.
            tauri::RunEvent::ExitRequested {
                code: None, api, ..
            } => api.prevent_exit(),
            // A downloaded update also goes in as the app quits, so the next
            // launch is the new version even when the user never clicked install.
            tauri::RunEvent::Exit => aidget::runtime::update::install_on_quit(handle),
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => {
                if let Err(error) = ui::show_main_now(handle) {
                    eprintln!("aidget: cannot open the main window: {error}");
                }
            }
            _ => {}
        }
    });
    Ok(())
}

/// Ask the running app to reopen its main window over the bridge.
fn reopen_running_app(client: &aidget::transport::client::Client) -> Result<()> {
    // Talk to the running instance directly. Falling back to `open` could
    // launch a second process with the same bundle id.
    client
        .call_with_timeout(
            "show_main",
            serde_json::json!({}),
            std::time::Duration::from_secs(3),
        )
        .map(|_| ())
}

fn run_mcp() -> Result<()> {
    let store = Store::open_default();
    store.ensure_layout()?;
    store.prune_retained()?;
    aidget::runtime::process::publish_cli(store.root());
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| Error::Other(error.to_string()))?
        .block_on(aidget::interfaces::mcp::serve(store))
}

fn tray(app: &tauri::AppHandle) -> Result<()> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let root = app.state::<ui::UiState>().root.lock().unwrap().clone();
    let tray_click_action = settings::load(&root)?.tray_click_action.effective();
    let version = format!("版本 {}", app.package_info().version);
    let open = MenuItem::with_id(app, "open", "打开 aidget", true, None::<&str>)?;
    #[cfg(debug_assertions)]
    let restart = MenuItem::with_id(app, "restart", "重启 aidget (dev)", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let version = MenuItem::with_id(app, "version", version, false, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出 aidget", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &open,
            #[cfg(debug_assertions)]
            &restart,
            &separator,
            &version,
            &quit,
        ],
    )?;

    let icon = tray_icon();
    let builder = TrayIconBuilder::with_id("aidget")
        .icon(icon)
        .menu(&menu)
        .show_menu_on_left_click(tray_click_action == TrayClickAction::Menu)
        .on_tray_icon_event(|tray, event| {
            let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            else {
                return;
            };
            let app = tray.app_handle();
            let root = app.state::<ui::UiState>().root.lock().unwrap().clone();
            match settings::load(&root) {
                Ok(settings)
                    if settings.tray_click_action.effective() == TrayClickAction::Window =>
                {
                    if let Err(error) = ui::show_main_now(app) {
                        eprintln!("aidget: cannot open the main window: {error}");
                    }
                }
                Ok(_) => {}
                Err(error) => eprintln!("aidget: cannot read preferences: {error}"),
            }
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => {
                if let Err(error) = ui::show_main_now(app) {
                    eprintln!("aidget: cannot open the main window: {error}");
                }
            }
            #[cfg(debug_assertions)]
            "restart" => app.request_restart(),
            "quit" => app.exit(0),
            _ => {}
        });
    // Template icons are macOS-only; other platforms draw the icon as-is.
    #[cfg(target_os = "macos")]
    let builder = builder.icon_as_template(true);
    builder.build(app)?;
    Ok(())
}

fn tray_icon() -> tauri::image::Image<'static> {
    const SIZE: u32 = 32;
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let in_widget = (5..=26).contains(&x) && (7..=24).contains(&y);
            let in_chrome = in_widget && (7..=12).contains(&y);
            let border = in_widget && (x <= 6 || x >= 25 || y <= 8 || y >= 23);
            if in_chrome || border {
                let index = ((y * SIZE + x) * 4) as usize;
                rgba[index] = 0;
                rgba[index + 1] = 0;
                rgba[index + 2] = 0;
                rgba[index + 3] = 255;
            }
        }
    }
    tauri::image::Image::new_owned(rgba, SIZE, SIZE)
}
