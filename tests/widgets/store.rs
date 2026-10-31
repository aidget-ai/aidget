use crate::common::utils::set_modified;
use aidget::common::error::Error;
use aidget::runtime::main_window_state;
use aidget::widgets::model::{validate_id, WidgetFrame, WidgetPermission, MAX_WIDGETS};
use aidget::widgets::store::{
    patch_html_region, Store, WidgetEdit, AGENTS_FILE, CONFIG_FILE, HTML_FILE, SETTINGS_FILE,
};
use std::fs;

use tempfile::TempDir;

fn store() -> (TempDir, Store) {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store.ensure_layout().unwrap();
    (dir, store)
}

#[test]
fn drag_mode_updates_preserve_position_and_survive_partial_edits() {
    use aidget::widgets::model::WidgetDragMode;
    let (_dir, store) = store();
    let created = store.create("note", "Note", WidgetEdit::default()).unwrap();
    assert_eq!(created.drag_mode, WidgetDragMode::All);
    let board = fs::read(store.board_path()).unwrap();
    let updated = store
        .update(
            "note",
            created.revision,
            WidgetEdit {
                drag_mode: Some(WidgetDragMode::Handle),
                ..WidgetEdit::default()
            },
        )
        .unwrap();
    assert_eq!(updated.revision, created.revision + 1);
    let partial = store
        .update(
            "note",
            updated.revision,
            WidgetEdit {
                title: Some("Renamed"),
                ..WidgetEdit::default()
            },
        )
        .unwrap();
    assert_eq!(partial.drag_mode, WidgetDragMode::Handle);
    assert_eq!(fs::read(store.board_path()).unwrap(), board);
    assert_eq!(
        store.load_document("note").unwrap().meta.drag_mode,
        WidgetDragMode::Handle
    );
    assert_eq!(
        store.list().unwrap().widgets[0].drag_mode,
        WidgetDragMode::Handle
    );
    let ack = store.content_ack(&partial).unwrap();
    assert_eq!(ack.drag_mode, WidgetDragMode::Handle);
    assert!(ack.frame.is_none());
}

#[test]
fn rejects_traversal_and_uppercase_ids() {
    assert!(validate_id("../etc").is_err());
    assert!(validate_id("News").is_err());
    assert!(validate_id("news").is_ok());
    assert!(validate_id("news-2").is_ok());
    assert!(validate_id("-news").is_err());
    assert!(Store::new("/tmp").widget_dir("../etc").is_err());
}

#[test]
fn create_assigns_frame_and_revision() {
    let (_dir, store) = store();
    let meta = store
        .create(
            "note",
            "Note",
            WidgetEdit {
                html: Some("<p>hi</p>"),
                css: Some(""),
                ..WidgetEdit::default()
            },
        )
        .unwrap();
    assert_eq!(meta.revision, 1);
    let board = store.load_board().unwrap();
    assert!(board.widgets.contains_key("note"));

    let duplicate = store.create(
        "note",
        "Note",
        WidgetEdit {
            html: Some(""),
            css: Some(""),
            ..WidgetEdit::default()
        },
    );
    assert!(duplicate.is_err());
}

#[test]
fn initial_placement_does_not_overwrite_a_concurrent_user_move() {
    let (_dir, store) = store();
    store.create("note", "Note", WidgetEdit::default()).unwrap();
    let initial = store.load_board().unwrap().widgets["note"].clone();
    let automatic = WidgetFrame {
        x: 24.0,
        y: 24.0,
        ..initial.clone()
    };
    let moved = WidgetFrame {
        x: 700.0,
        y: 300.0,
        ..initial.clone()
    };
    let revision = store.load_board().unwrap().revision;
    store.move_widget("note", revision, moved.clone()).unwrap();

    store
        .set_initial_frame("note", Some(&initial), automatic)
        .unwrap();

    let frame = store.load_board().unwrap().widgets["note"].clone();
    assert_eq!(frame.x, moved.x);
    assert_eq!(frame.y, moved.y);
}

#[test]
fn fifty_widgets_are_allowed_and_the_fifty_first_is_rejected() {
    let (_dir, store) = store();
    assert_eq!(MAX_WIDGETS, 50);

    for index in 0..MAX_WIDGETS {
        store
            .create(&format!("item-{index:02}"), "Item", WidgetEdit::default())
            .unwrap();
    }

    let error = store
        .create("item-50", "Item", WidgetEdit::default())
        .unwrap_err();
    assert!(matches!(
        error,
        Error::InvalidParams(message) if message == "widget limit reached (50)"
    ));
}

#[test]
fn content_ack_omits_the_frame_but_keeps_board_revision() {
    let (_dir, store) = store();
    store
        .create(
            "note",
            "Note",
            WidgetEdit {
                html: Some("<p>hi</p>"),
                css: Some(""),
                ..WidgetEdit::default()
            },
        )
        .unwrap();

    let placed = store.ack(&store.load_meta("note").unwrap()).unwrap();
    assert!(placed.frame.is_some());

    let edited = store
        .content_ack(&store.load_meta("note").unwrap())
        .unwrap();
    assert!(edited.frame.is_none());
    assert_eq!(edited.board_revision, placed.board_revision);
}

#[test]
fn update_conflict_returns_current_meta() {
    let (_dir, store) = store();
    store
        .create(
            "note",
            "Note",
            WidgetEdit {
                html: Some("<p>one</p>"),
                css: Some(""),
                ..WidgetEdit::default()
            },
        )
        .unwrap();
    store
        .update(
            "note",
            1,
            WidgetEdit {
                html: Some("<p>two</p>"),
                ..WidgetEdit::default()
            },
        )
        .unwrap();

    let error = store
        .update(
            "note",
            1,
            WidgetEdit {
                html: Some("<p>three</p>"),
                ..WidgetEdit::default()
            },
        )
        .unwrap_err();
    match error {
        Error::RevisionConflict { current } => {
            assert_eq!(current.revision(), 2);
        }
        other => panic!("unexpected error: {other}"),
    }
    assert_eq!(
        fs::read_to_string(store.widget_dir("note").unwrap().join(HTML_FILE)).unwrap(),
        "<p>two</p>"
    );
}

#[test]
fn update_writes_agents_md_without_touching_html() {
    let (_dir, store) = store();
    store
        .create(
            "note",
            "Note",
            WidgetEdit {
                html: Some("<p>hi</p>"),
                css: Some(""),
                ..WidgetEdit::default()
            },
        )
        .unwrap();

    store
        .update(
            "note",
            1,
            WidgetEdit {
                agents: Some("# note\n"),
                ..WidgetEdit::default()
            },
        )
        .unwrap();

    let dir = store.widget_dir("note").unwrap();
    assert_eq!(
        fs::read_to_string(dir.join(AGENTS_FILE)).unwrap(),
        "# note\n"
    );
    assert_eq!(
        fs::read_to_string(dir.join(HTML_FILE)).unwrap(),
        "<p>hi</p>"
    );
}

#[test]
fn patch_region_replaces_once_and_bumps_revision() {
    let (_dir, store) = store();
    let html = r#"<section data-aidget-region="todo"><p>old</p></section>"#;
    store
        .create(
            "note",
            "Note",
            WidgetEdit {
                html: Some(html),
                css: Some(""),
                ..WidgetEdit::default()
            },
        )
        .unwrap();
    let meta = store.patch_region("note", 1, "todo", "<p>new</p>").unwrap();
    assert_eq!(meta.revision, 2);
    let written = fs::read_to_string(store.widget_dir("note").unwrap().join(HTML_FILE)).unwrap();
    assert!(written.contains("<p>new</p>"));
    assert!(!written.contains("<p>old</p>"));
}

#[test]
fn list_reports_board_revision_and_broken_widgets() {
    let (_dir, store) = store();
    store
        .create(
            "note",
            "Note",
            WidgetEdit {
                html: Some("<p>hi</p>"),
                css: Some(""),
                ..WidgetEdit::default()
            },
        )
        .unwrap();
    let broken = store.widget_dir("broken").unwrap();
    fs::create_dir_all(&broken).unwrap();
    fs::write(broken.join(CONFIG_FILE), b"{").unwrap();

    let listing = store.list().unwrap();

    assert_eq!(listing.widgets.len(), 1);
    assert_eq!(listing.widgets[0].id, "note");
    assert_eq!(listing.broken.len(), 1);
    assert_eq!(listing.broken[0].id, "broken");
    assert_eq!(listing.board_revision, 2);
}

#[test]
fn rejects_config_and_board_fields_from_older_layouts() {
    let (_dir, store) = store();
    store
        .create(
            "note",
            "Note",
            WidgetEdit {
                html: Some("<p>hi</p>"),
                css: Some(""),
                ..WidgetEdit::default()
            },
        )
        .unwrap();
    let config = store.widget_dir("note").unwrap().join(CONFIG_FILE);
    let mut meta: serde_json::Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    meta["schema"] = serde_json::json!(1);
    fs::write(&config, serde_json::to_vec_pretty(&meta).unwrap()).unwrap();

    let error = store.load_meta("note").unwrap_err();
    assert!(matches!(error, Error::Json(_)), "got {error}");

    let mut board: serde_json::Value =
        serde_json::from_slice(&fs::read(store.board_path()).unwrap()).unwrap();
    board["schema"] = serde_json::json!(1);
    fs::write(
        store.board_path(),
        serde_json::to_vec_pretty(&board).unwrap(),
    )
    .unwrap();
    assert!(store.load_board().is_err());
}

#[test]
fn patch_region_reports_missing_and_duplicate_regions() {
    let html = r#"<div data-aidget-region="a"></div><div data-aidget-region="a"></div>"#;
    assert!(matches!(
        patch_html_region(html, "b", "<i></i>"),
        Err(Error::RegionNotFound(_))
    ));
    assert!(matches!(
        patch_html_region(html, "a", "<i></i>"),
        Err(Error::RegionAmbiguous(_))
    ));
}

#[test]
fn patch_regions_applies_all_edits_or_none() {
    let (_dir, store) = store();
    let html =
        r#"<section data-aidget-region="a">1</section><section data-aidget-region="b">2</section>"#;
    store
        .create(
            "note",
            "Note",
            WidgetEdit {
                html: Some(html),
                css: Some(""),
                ..WidgetEdit::default()
            },
        )
        .unwrap();
    store
        .patch_regions(
            "note",
            1,
            &[
                ("a".to_string(), "<b>A</b>".to_string()),
                ("b".to_string(), "<b>B</b>".to_string()),
            ],
        )
        .unwrap();
    let written = fs::read_to_string(store.widget_dir("note").unwrap().join(HTML_FILE)).unwrap();
    assert!(written.contains("<b>A</b>"));
    assert!(written.contains("<b>B</b>"));

    let error = store.patch_regions(
        "note",
        2,
        &[
            ("a".to_string(), "x".to_string()),
            ("missing".to_string(), "y".to_string()),
        ],
    );
    assert!(matches!(error, Err(Error::RegionNotFound(_))));
    let untouched = fs::read_to_string(store.widget_dir("note").unwrap().join(HTML_FILE)).unwrap();
    assert!(!untouched.contains(">x<"));
}

#[test]
fn patch_preserves_nested_regions_outside_target() {
    let html =
        r#"<div data-aidget-region="outer"><span data-aidget-region="inner">keep</span></div>"#;
    let patched = patch_html_region(html, "inner", "next").unwrap();
    assert!(patched.contains("next"));
    assert!(patched.contains(r#"data-aidget-region="outer""#));
}

#[test]
fn delete_moves_widget_to_trash_and_clears_board() {
    let (_dir, store) = store();
    store
        .create(
            "note",
            "Note",
            WidgetEdit {
                html: Some("<p>hi</p>"),
                css: Some(""),
                ..WidgetEdit::default()
            },
        )
        .unwrap();
    store.delete("note").unwrap();
    assert!(!store.widget_dir("note").unwrap().exists());
    assert!(!store.load_board().unwrap().widgets.contains_key("note"));
    let trashed = fs::read_dir(store.trash_dir()).unwrap().count();
    assert_eq!(trashed, 1);
}

#[test]
fn delete_preserves_the_cached_preview_with_the_trashed_widget() {
    let (_dir, store) = store();
    store
        .create(
            "note",
            "Note",
            WidgetEdit {
                html: Some("<p>hi</p>"),
                css: Some(""),
                ..WidgetEdit::default()
            },
        )
        .unwrap();
    main_window_state::save_preview(store.root(), "note", b"preview").unwrap();

    let target = store.delete("note").unwrap();

    assert_eq!(
        fs::read(target.join("deletion-preview.png")).unwrap(),
        b"preview"
    );
    let deleted = store.deleted_widgets().unwrap();
    assert!(deleted[0].has_preview);
    assert_eq!(
        store.deleted_preview(&deleted[0].trash_name).unwrap(),
        Some(b"preview".to_vec())
    );

    store.restore(&deleted[0].trash_name).unwrap();

    assert_eq!(
        fs::read(main_window_state::preview_path(store.root(), "note").unwrap()).unwrap(),
        b"preview"
    );
    assert!(!store
        .widget_dir("note")
        .unwrap()
        .join("deletion-preview.png")
        .exists());
}

#[test]
fn deleted_widget_can_be_restored_with_its_saved_frame() {
    let (_dir, store) = store();
    store
        .create(
            "note",
            "Note",
            WidgetEdit {
                html: Some("<p>hi</p>"),
                css: Some(""),
                ..WidgetEdit::default()
            },
        )
        .unwrap();
    let frame = WidgetFrame {
        display: "primary".to_string(),
        x: 140.0,
        y: 220.0,
        w: 420.0,
        h: 260.0,
    };
    let revision = store.load_board().unwrap().revision;
    store.move_widget("note", revision, frame.clone()).unwrap();
    store.delete("note").unwrap();

    let deleted = store.deleted_widgets().unwrap();
    assert_eq!(deleted.len(), 1);
    assert_eq!(deleted[0].id, "note");
    assert_eq!(deleted[0].title, "Note");
    assert_eq!(deleted[0].width, 420.0);
    assert_eq!(deleted[0].height, 260.0);
    assert!(!deleted[0].has_preview);

    store.restore(&deleted[0].trash_name).unwrap();

    assert!(store.widget_dir("note").unwrap().is_dir());
    assert_eq!(
        serde_json::to_value(&store.load_board().unwrap().widgets["note"]).unwrap(),
        serde_json::to_value(&frame).unwrap()
    );
    assert!(store.deleted_widgets().unwrap().is_empty());
}

#[test]
fn move_rejects_unknown_widgets_without_creating_a_board_entry() {
    let (_dir, store) = store();
    let revision = store.load_board().unwrap().revision;
    let error = store
        .move_widget("ghost", revision, WidgetFrame::default_for(0))
        .unwrap_err();

    assert!(matches!(error, Error::NotFound(id) if id == "ghost"));
    assert!(!store.load_board().unwrap().widgets.contains_key("ghost"));
}

#[test]
fn move_rejects_a_stale_board_revision() {
    let (_dir, store) = store();
    store.create("note", "Note", WidgetEdit::default()).unwrap();
    let board = store.load_board().unwrap();
    let current = board.widgets["note"].clone();
    let error = store
        .move_widget(
            "note",
            board.revision - 1,
            WidgetFrame {
                x: current.x + 30.0,
                ..current.clone()
            },
        )
        .unwrap_err();

    assert!(matches!(error, Error::RevisionConflict { .. }));
    assert_eq!(
        serde_json::to_value(&store.load_board().unwrap().widgets["note"]).unwrap(),
        serde_json::to_value(&current).unwrap()
    );
}

#[test]
fn existing_trash_directory_without_metadata_can_be_restored() {
    let (_dir, store) = store();
    store
        .create(
            "note",
            "Note",
            WidgetEdit {
                html: Some("<p>hi</p>"),
                css: Some(""),
                ..WidgetEdit::default()
            },
        )
        .unwrap();
    let source = store.widget_dir("note").unwrap();
    let target = store.trash_dir().join("note-20261022T000000");
    fs::rename(&source, &target).unwrap();
    let mut board = store.load_board().unwrap();
    board.widgets.remove("note");
    board.revision += 1;
    store.save_board(&board).unwrap();

    let deleted = store.deleted_widgets().unwrap();
    assert_eq!(deleted.len(), 1);
    assert_eq!(deleted[0].width, 360.0);
    assert_eq!(deleted[0].height, 220.0);
    assert!(!deleted[0].has_preview);

    store.restore(&deleted[0].trash_name).unwrap();
    assert!(store.widget_dir("note").unwrap().is_dir());
}

#[test]
fn trash_name_cannot_escape_the_trash_directory() {
    let (_dir, store) = store();
    assert!(store.restore("../widgets/note").is_err());
    assert!(store.purge("../widgets/note").is_err());
}

#[test]
fn purge_permanently_removes_only_the_selected_trash_directory() {
    let (_dir, store) = store();
    let first = store.trash_dir().join("first-20261022T000000");
    let second = store.trash_dir().join("second-20261022T000000");
    fs::create_dir(&first).unwrap();
    fs::create_dir(&second).unwrap();
    fs::write(first.join("keep.txt"), b"first").unwrap();
    fs::write(second.join("keep.txt"), b"second").unwrap();

    store.purge("first-20261022T000000").unwrap();

    assert!(!first.exists());
    assert!(second.exists());
}

#[test]
fn delete_applies_trash_retention_and_returns_a_retained_path() {
    let (_dir, store) = store();
    fs::write(
        store.root().join(SETTINGS_FILE),
        "screenshot_retention = 30\ntrash_retention = 1\n",
    )
    .unwrap();
    store
        .create(
            "note",
            "Note",
            WidgetEdit {
                html: Some("<p>hi</p>"),
                css: Some(""),
                ..WidgetEdit::default()
            },
        )
        .unwrap();
    let old = store.trash_dir().join("old-20261022T000000");
    fs::create_dir(&old).unwrap();
    set_modified(&old, 1_000);

    let target = store.delete("note").unwrap();

    assert!(target.exists());
    assert!(!old.exists());
    assert_eq!(
        fs::read_dir(store.trash_dir())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .count(),
        1
    );
}

#[test]
fn delete_rejects_invalid_settings_before_moving_the_widget() {
    let (_dir, store) = store();
    store
        .create(
            "note",
            "Note",
            WidgetEdit {
                html: Some("<p>hi</p>"),
                css: Some(""),
                ..WidgetEdit::default()
            },
        )
        .unwrap();
    fs::write(store.root().join(SETTINGS_FILE), "not = [valid").unwrap();

    assert!(store.delete("note").is_err());
    assert!(store.widget_dir("note").unwrap().exists());
}

#[test]
fn permissions_default_to_empty_and_survive_a_round_trip() {
    let (_dir, store) = store();
    store
        .create(
            "note",
            "Note",
            WidgetEdit {
                html: Some("<p>hi</p>"),
                css: Some(""),
                ..WidgetEdit::default()
            },
        )
        .unwrap();
    assert!(store.load_meta("note").unwrap().permissions.is_empty());

    store
        .update(
            "note",
            1,
            WidgetEdit {
                permissions: Some(&[WidgetPermission::Location]),
                ..WidgetEdit::default()
            },
        )
        .unwrap();

    let meta = store.load_meta("note").unwrap();
    assert_eq!(meta.permissions, vec![WidgetPermission::Location]);
    assert!(meta.grants(WidgetPermission::Location));
    assert_eq!(meta.revision, 2);
    assert_eq!(
        store.list().unwrap().widgets[0].permissions,
        vec![WidgetPermission::Location]
    );
    assert_eq!(
        store.ack(&meta).unwrap().permissions,
        vec![WidgetPermission::Location]
    );

    let cleared = store
        .update(
            "note",
            2,
            WidgetEdit {
                permissions: Some(&[]),
                ..WidgetEdit::default()
            },
        )
        .unwrap();
    assert!(cleared.permissions.is_empty());
    assert!(!cleared.grants(WidgetPermission::Location));
}

#[test]
fn duplicate_permissions_are_stored_once() {
    let (_dir, store) = store();
    let meta = store
        .create(
            "note",
            "Note",
            WidgetEdit {
                html: Some("<p>hi</p>"),
                permissions: Some(&[WidgetPermission::Location, WidgetPermission::Location]),
                ..WidgetEdit::default()
            },
        )
        .unwrap();
    assert_eq!(meta.permissions, vec![WidgetPermission::Location]);
}

#[test]
fn unknown_permission_names_are_rejected() {
    let (_dir, store) = store();
    store
        .create(
            "note",
            "Note",
            WidgetEdit {
                html: Some("<p>hi</p>"),
                css: Some(""),
                ..WidgetEdit::default()
            },
        )
        .unwrap();
    let config = store.widget_dir("note").unwrap().join(CONFIG_FILE);
    let mut meta: serde_json::Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    meta["permissions"] = serde_json::json!(["camera"]);
    fs::write(&config, serde_json::to_vec_pretty(&meta).unwrap()).unwrap();

    let error = store.load_meta("note").unwrap_err();
    assert!(matches!(error, Error::Json(_)), "got {error}");
}

#[test]
fn config_without_permissions_is_rejected() {
    let (_dir, store) = store();
    store
        .create(
            "note",
            "Note",
            WidgetEdit {
                html: Some("<p>hi</p>"),
                css: Some(""),
                ..WidgetEdit::default()
            },
        )
        .unwrap();
    let config = store.widget_dir("note").unwrap().join(CONFIG_FILE);
    let mut meta: serde_json::Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    meta.as_object_mut().unwrap().remove("permissions");
    fs::write(&config, serde_json::to_vec_pretty(&meta).unwrap()).unwrap();

    assert!(store.load_meta("note").is_err());
}

#[test]
fn invalid_network_declarations_fail_before_creating_a_widget_directory() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::new(root.path());
    store.ensure_layout().unwrap();
    let network = vec!["https://*.example.com".to_string()];
    assert!(store
        .create(
            "bad",
            "Bad",
            WidgetEdit {
                network: Some(&network),
                ..Default::default()
            }
        )
        .is_err());
    assert!(!store.widget_dir("bad").unwrap().exists());
}
