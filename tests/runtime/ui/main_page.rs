use aidget::runtime::main_window_state;
use aidget::runtime::ui::{
    cached_preview, main_widgets, set_github_token as change_github_token, GithubTokenSummary,
    MainState, WidgetSummary,
};
use aidget::settings::{
    save_github_token, save_preferences, GithubToken, Language, MainPage, Theme, TrayClickAction,
    UiPreferences,
};
use aidget::widgets::model::{DeletedWidgetSummary, WidgetFrame};
use aidget::widgets::store::{Store, WidgetEdit, CONFIG_FILE};
use tempfile::TempDir;

#[test]
fn main_window_lists_all_widgets_and_marks_enabled_state() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store.ensure_layout().unwrap();
    store
        .create(
            "zeta",
            "Zeta",
            WidgetEdit {
                html: Some("<p>z</p>"),
                css: Some(""),
                ..WidgetEdit::default()
            },
        )
        .unwrap();
    store
        .create(
            "alpha",
            "Alpha",
            WidgetEdit {
                html: Some("<p>a</p>"),
                css: Some(""),
                permissions: Some(&[aidget::widgets::model::WidgetPermission::Script]),
                ..WidgetEdit::default()
            },
        )
        .unwrap();
    store
        .create(
            "hidden",
            "Hidden",
            WidgetEdit {
                html: Some("<p>h</p>"),
                css: Some(""),
                ..WidgetEdit::default()
            },
        )
        .unwrap();

    let mut board = store.load_board().unwrap();
    board.widgets.remove("hidden");
    board.widgets.insert(
        "alpha".to_string(),
        WidgetFrame {
            display: "primary".to_string(),
            x: 10.0,
            y: 20.0,
            w: 420.0,
            h: 260.0,
        },
    );
    store.save_board(&board).unwrap();

    let state = main_widgets(&store).unwrap();
    let ids: Vec<_> = state
        .widgets
        .iter()
        .map(|widget| widget.id.as_str())
        .collect();
    assert_eq!(ids, vec!["alpha", "hidden", "zeta"]);
    assert_eq!(state.widgets[0].title, "Alpha");
    assert_eq!(state.widgets[0].width, 420.0);
    assert_eq!(state.widgets[0].height, 260.0);
    assert!(!state.widgets[0].token.is_empty());
    assert!(state.widgets[0].enabled);
    assert!(!state.widgets[1].enabled);
    assert_eq!(state.widgets[1].width, 360.0);
    assert_eq!(state.version, env!("CARGO_PKG_VERSION"));
}

#[test]
fn main_state_restores_last_page_from_settings() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store.ensure_layout().unwrap();

    save_preferences(
        store.root(),
        UiPreferences {
            last_page: MainPage::About,
            ..UiPreferences::default()
        },
    )
    .unwrap();

    assert_eq!(main_widgets(&store).unwrap().last_page, MainPage::About);
}

#[test]
fn archiving_a_widget_keeps_its_board_position_for_restore() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store.ensure_layout().unwrap();
    store
        .create(
            "weather",
            "Weather",
            WidgetEdit {
                html: Some("<p>sunny</p>"),
                css: Some(""),
                ..WidgetEdit::default()
            },
        )
        .unwrap();
    let original = store.load_board().unwrap().widgets["weather"].clone();

    let ids = store.widget_ids().unwrap();
    main_window_state::set_archived(store.root(), &ids, "weather", true).unwrap();
    assert_eq!(
        serde_json::to_value(&store.load_board().unwrap().widgets["weather"]).unwrap(),
        serde_json::to_value(&original).unwrap()
    );
    assert!(!main_widgets(&store).unwrap().widgets[0].enabled);

    main_window_state::set_archived(store.root(), &ids, "weather", false).unwrap();
    assert_eq!(
        serde_json::to_value(&store.load_board().unwrap().widgets["weather"]).unwrap(),
        serde_json::to_value(&original).unwrap()
    );
    assert!(main_widgets(&store).unwrap().widgets[0].enabled);
}

#[test]
fn main_window_skips_widgets_with_broken_config() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store.ensure_layout().unwrap();
    store
        .create(
            "good",
            "Good",
            WidgetEdit {
                html: Some("<p>ok</p>"),
                css: Some(""),
                ..WidgetEdit::default()
            },
        )
        .unwrap();
    store
        .create(
            "broken",
            "Broken",
            WidgetEdit {
                html: Some("<p>bad</p>"),
                css: Some(""),
                ..WidgetEdit::default()
            },
        )
        .unwrap();
    std::fs::remove_file(store.widget_dir("broken").unwrap().join(CONFIG_FILE)).unwrap();

    let state = main_widgets(&store).unwrap();
    assert_eq!(state.widgets.len(), 1);
    assert_eq!(state.widgets[0].id, "good");
}

#[test]
fn main_state_exposes_widget_metadata_in_camel_case() {
    let value = serde_json::to_value(MainState {
        widgets: vec![WidgetSummary {
            package_name: None,
            id: "weather".to_string(),
            title: "Weather".to_string(),
            revision: 2,
            token: "token".to_string(),
            width: 360.0,
            height: 220.0,
            enabled: true,
            created_at: 1_761_158_300_000,
        }],
        broken: vec![],
        deleted_widgets: vec![DeletedWidgetSummary {
            trash_name: "note-20261022T000000".to_string(),
            id: "note".to_string(),
            title: "Note".to_string(),
            width: 360.0,
            height: 220.0,
            has_preview: true,
            created_at: 1_761_158_400_000,
        }],
        version: "0.1.0".to_string(),
        theme: Theme::Dark,
        language: Language::ZhCn,
        tray_click_action: TrayClickAction::Window,
        last_page: MainPage::Integrations,
        github_token: Some(GithubTokenSummary {
            masked: "ghp_…WXYZ".to_string(),
        }),
    })
    .unwrap();

    let deleted = value.get("deletedWidgets").unwrap();
    assert_eq!(value["widgets"][0]["createdAt"], 1_761_158_300_000_u64);
    assert_eq!(deleted[0]["trashName"], "note-20261022T000000");
    assert_eq!(deleted[0]["hasPreview"], true);
    assert_eq!(deleted[0]["createdAt"], 1_761_158_400_000_u64);
    assert!(deleted[0].get("trash_name").is_none());
    assert!(value.get("deleted_widgets").is_none());
    assert_eq!(value["theme"], "dark");
    assert_eq!(value["language"], "zh-CN");
    assert_eq!(value["trayClickAction"], "window");
    assert_eq!(value["lastPage"], "integrations");
    assert_eq!(value["githubToken"]["masked"], "ghp_…WXYZ");
    assert!(!value
        .to_string()
        .contains("samplevalueABCDEFGHIJKLMNOPQRSTUVWXYZ"));
}

#[test]
fn main_state_exposes_only_the_configured_token_prefix_and_suffix() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store.ensure_layout().unwrap();
    let token = GithubToken::parse("samplevalueABCDEFGHIJKLMNOPQRSTUVWXYZ").unwrap();
    save_github_token(store.root(), Some(&token)).unwrap();

    let value = serde_json::to_value(main_widgets(&store).unwrap()).unwrap();

    assert_eq!(value["githubToken"]["masked"], "ghp_…WXYZ");
    assert!(!value
        .to_string()
        .contains("samplevalueABCDEFGHIJKLMNOPQRSTUVWXYZ"));
}

#[test]
fn failed_github_token_changes_keep_the_existing_token() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store.ensure_layout().unwrap();
    let token = GithubToken::parse("samplevalueABCDEFGHIJKLMNOPQRSTUVWXYZ").unwrap();
    save_github_token(store.root(), Some(&token)).unwrap();

    assert!(change_github_token(store.root(), Some("short".into())).is_err());
    assert_eq!(
        aidget::settings::load(store.root()).unwrap().github_token,
        Some(token)
    );

    assert_eq!(change_github_token(store.root(), None).unwrap(), None);
    assert_eq!(
        aidget::settings::load(store.root()).unwrap().github_token,
        None
    );
}

#[test]
fn cached_preview_reads_the_saved_png_without_a_webview() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store.ensure_layout().unwrap();
    store.create("note", "Note", WidgetEdit::default()).unwrap();

    assert_eq!(cached_preview(&store, "note").unwrap(), None);

    main_window_state::save_preview(store.root(), "note", b"png bytes").unwrap();
    assert_eq!(
        cached_preview(&store, "note").unwrap().unwrap(),
        "data:image/png;base64,cG5nIGJ5dGVz"
    );
}

#[test]
fn cached_preview_rejects_unknown_and_invalid_widgets() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store.ensure_layout().unwrap();

    assert!(cached_preview(&store, "missing").is_err());
    assert!(cached_preview(&store, "../outside").is_err());
}

#[test]
fn main_state_tracks_current_package_identity_without_a_source_badge() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::new(directory.path());
    store.ensure_layout().unwrap();
    store.create("note", "Note", WidgetEdit::default()).unwrap();
    assert_eq!(main_widgets(&store).unwrap().widgets[0].package_name, None);
    std::fs::write(
        store.widget_dir("note").unwrap().join("package.json"),
        br#"{"name":"@someone/note","version":"1.0.0"}"#,
    )
    .unwrap();
    let state = main_widgets(&store).unwrap();
    assert_eq!(
        state.widgets[0].package_name.as_deref(),
        Some("@someone/note")
    );
    let value = serde_json::to_value(state).unwrap();
    assert!(value["widgets"][0].get("source").is_none());
}

#[test]
fn local_details_reads_installed_config_description_and_cached_preview() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store.ensure_layout().unwrap();
    store
        .create("note", "Local note", WidgetEdit::default())
        .unwrap();
    let details = aidget::runtime::ui::local_widget_details(&store, "note").unwrap();
    assert_eq!(details.config.title, "Local note");
    assert_eq!(details.description, None);
    assert_eq!(details.preview, None);
    std::fs::write(
        store.widget_dir("note").unwrap().join("package.json"),
        br#"{"name":"@someone/note","version":"1.0.0","description":"My local widget"}"#,
    )
    .unwrap();
    main_window_state::save_preview(store.root(), "note", b"png bytes").unwrap();
    main_window_state::set_archived(store.root(), &store.widget_ids().unwrap(), "note", true)
        .unwrap();
    let details = aidget::runtime::ui::local_widget_details(&store, "note").unwrap();
    assert_eq!(details.description.as_deref(), Some("My local widget"));
    assert_eq!(
        details.preview.as_deref(),
        Some("data:image/png;base64,cG5nIGJ5dGVz")
    );
    assert_eq!(details.config, store.load_meta("note").unwrap().config);
    assert!(aidget::runtime::ui::local_widget_details(&store, "missing").is_err());
    assert!(aidget::runtime::ui::local_widget_details(&store, "../outside").is_err());
}
