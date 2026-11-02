use std::sync::{Arc, Mutex};

use aidget::runtime::{
    command_access::guard,
    command_names::{MAIN_COMMANDS, WIDGET_COMMANDS},
};
use tauri::{
    ipc::{CallbackFn, InvokeBody},
    test::{get_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY},
    webview::InvokeRequest,
    WebviewWindowBuilder,
};

#[test]
fn github_token_changes_are_reserved_for_the_main_window() {
    assert!(MAIN_COMMANDS.contains(&"set_github_token"));
    assert!(!WIDGET_COMMANDS.contains(&"set_github_token"));
}

#[test]
fn cached_community_catalogues_are_reserved_for_the_main_window() {
    assert!(MAIN_COMMANDS.contains(&"community_cached"));
    assert!(!WIDGET_COMMANDS.contains(&"community_cached"));
}

#[test]
fn ipc_rejects_other_window_roles_before_host_reads_or_writes() {
    let effects = Arc::new(Mutex::new(Vec::new()));
    let seen = effects.clone();
    let app = mock_builder()
        .invoke_handler(guard(move |invoke| {
            seen.lock()
                .unwrap()
                .push(invoke.message.command().to_owned());
            invoke.resolver.resolve("executed");
            true
        }))
        .build(mock_context(noop_assets()))
        .unwrap();
    for (label, denied, allowed) in [
        ("widget-note", MAIN_COMMANDS, WIDGET_COMMANDS),
        ("main", WIDGET_COMMANDS, MAIN_COMMANDS),
        ("widget-", MAIN_COMMANDS, &[][..]),
        ("other", MAIN_COMMANDS, &[][..]),
    ] {
        let window = WebviewWindowBuilder::new(&app, label, Default::default())
            .build()
            .unwrap();
        for command in denied.iter().chain(["unknown_command"].iter()) {
            let before = effects.lock().unwrap().clone();
            let result = get_ipc_response(&window, request(command));
            assert!(result.is_err(), "{label} executed {command}");
            assert_eq!(
                *effects.lock().unwrap(),
                before,
                "denied calls reached host operations"
            );
        }
        for command in allowed {
            assert_eq!(
                get_ipc_response(&window, request(command))
                    .unwrap()
                    .deserialize::<String>()
                    .unwrap(),
                "executed"
            );
        }
    }
}

fn request(command: &str) -> InvokeRequest {
    InvokeRequest {
        cmd: command.into(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        url: if cfg!(windows) {
            "http://tauri.localhost"
        } else {
            "tauri://localhost"
        }
        .parse()
        .unwrap(),
        body: InvokeBody::Json(
            serde_json::json!({"id":"victim", "trashName":"victim", "change":{}}),
        ),
        headers: Default::default(),
        invoke_key: INVOKE_KEY.into(),
    }
}

#[test]
fn built_capabilities_deny_cross_role_commands_even_without_the_native_guard() {
    let effects = Arc::new(Mutex::new(Vec::new()));
    let seen = effects.clone();
    let app = mock_builder()
        .invoke_handler(move |invoke| {
            seen.lock()
                .unwrap()
                .push(invoke.message.command().to_owned());
            invoke.resolver.resolve("executed");
            true
        })
        .build(tauri::generate_context!())
        .unwrap();
    for (label, allowed, denied) in [
        ("main", MAIN_COMMANDS, WIDGET_COMMANDS),
        ("widget-note", WIDGET_COMMANDS, MAIN_COMMANDS),
        ("other", &[][..], MAIN_COMMANDS),
    ] {
        let window = WebviewWindowBuilder::new(&app, label, Default::default())
            .build()
            .unwrap();
        for command in denied.iter().chain(["unknown_command"].iter()) {
            let before = effects.lock().unwrap().clone();
            assert!(
                get_ipc_response(&window, request(command)).is_err(),
                "ACL allowed {command} from {label}"
            );
            assert_eq!(*effects.lock().unwrap(), before);
        }
        for command in allowed {
            assert!(
                get_ipc_response(&window, request(command)).is_ok(),
                "ACL blocked {command} from {label}"
            );
        }
    }
}
