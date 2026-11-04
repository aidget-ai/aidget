use std::fs;
use std::path::Path;

use aidget::widgets::migration::migrate_enabled;
use aidget::widgets::model::WidgetMeta;
use aidget::widgets::store::{Store, WidgetEdit, CONFIG_FILE};
use serde_json::{json, Value};
use tempfile::TempDir;

fn old_meta(path: &Path) {
    let mut value: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    value.as_object_mut().unwrap().remove("enabled");
    fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
}

fn fixture() -> (TempDir, Store) {
    let temp = TempDir::new().unwrap();
    let store = Store::new(temp.path());
    store.ensure_layout().unwrap();
    (temp, store)
}

#[test]
fn enabled_migration_preserves_visibility_metadata_frames_and_previews() {
    let (_temp, store) = fixture();
    for id in ["active", "disabled", "unplaced", "trashed"] {
        store.create(id, id, WidgetEdit::default()).unwrap();
    }
    let trashed = store.delete("trashed").unwrap();
    old_meta(&trashed.join(CONFIG_FILE));
    for id in ["active", "disabled", "unplaced"] {
        old_meta(&store.widget_dir(id).unwrap().join(CONFIG_FILE));
    }
    let mut board = store.load_board().unwrap();
    board.widgets.remove("unplaced");
    store.save_board(&board).unwrap();
    let layout = fs::read(store.board_path()).unwrap();
    let path = store.widget_dir("disabled").unwrap().join(CONFIG_FILE);
    let before: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let legacy = store.state_dir().join("widgets.json");
    let legacy_bytes =
        br#"{"disabled":{"archived":true},"active":{"archived":false},"stale":{"archived":true}}"#;
    fs::write(&legacy, legacy_bytes).unwrap();
    aidget::runtime::main_window_state::save_preview(store.root(), "disabled", b"preview").unwrap();

    migrate_enabled(&store).unwrap();
    assert!(store.load_meta("active").unwrap().enabled);
    assert!(!store.load_meta("disabled").unwrap().enabled);
    assert!(!store.load_meta("unplaced").unwrap().enabled);
    let after: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(after["revision"], before["revision"]);
    assert_eq!(after["updatedAt"], before["updatedAt"]);
    assert_eq!(fs::read(store.board_path()).unwrap(), layout);
    assert_eq!(
        aidget::runtime::main_window_state::load_preview(store.root(), "disabled").unwrap(),
        Some(b"preview".to_vec())
    );
    let meta: WidgetMeta =
        serde_json::from_slice(&fs::read(trashed.join(CONFIG_FILE)).unwrap()).unwrap();
    assert!(meta.enabled);
    assert!(!legacy.exists());
    assert_eq!(
        fs::read(store.state_dir().join("migrations/enabled/widgets.json")).unwrap(),
        legacy_bytes
    );
    let committed = fs::read(&path).unwrap();
    migrate_enabled(&store).unwrap();
    assert_eq!(fs::read(path).unwrap(), committed);
}

#[test]
fn enabled_migration_resumes_after_a_bad_later_record_without_overwriting_completed_state() {
    let (_temp, store) = fixture();
    for id in ["alpha", "zeta"] {
        store.create(id, id, WidgetEdit::default()).unwrap();
        old_meta(&store.widget_dir(id).unwrap().join(CONFIG_FILE));
    }
    let zeta = store.widget_dir("zeta").unwrap().join(CONFIG_FILE);
    let original = fs::read(&zeta).unwrap();
    fs::write(&zeta, b"{").unwrap();
    let legacy = store.state_dir().join("widgets.json");
    fs::write(&legacy, br#"{"alpha":{"archived":true}}"#).unwrap();
    assert!(migrate_enabled(&store).is_err());
    assert!(legacy.exists());
    let alpha = store.widget_dir("alpha").unwrap().join(CONFIG_FILE);
    let completed = fs::read(&alpha).unwrap();
    fs::write(&zeta, original).unwrap();
    migrate_enabled(&store).unwrap();
    assert_eq!(fs::read(alpha).unwrap(), completed);
    assert!(!store.load_meta("alpha").unwrap().enabled);
    assert!(store.load_meta("zeta").unwrap().enabled);
}

#[test]
fn enabled_migration_rejects_bad_archive_state_and_current_values_without_backup() {
    for bytes in [
        br#"{"../escape":{"archived":true}}"#.as_slice(),
        br#"{"note":{"archived":true,"extra":1}}"#.as_slice(),
        br#"{"note":{}}"#.as_slice(),
        b"{".as_slice(),
    ] {
        let (_temp, store) = fixture();
        store.create("note", "Note", WidgetEdit::default()).unwrap();
        let legacy = store.state_dir().join("widgets.json");
        fs::write(&legacy, bytes).unwrap();
        assert!(migrate_enabled(&store).is_err());
        assert_eq!(fs::read(legacy).unwrap(), bytes);
        assert!(!store
            .state_dir()
            .join("migrations/enabled/widgets.json")
            .exists());
    }
    let (_temp, store) = fixture();
    store.create("note", "Note", WidgetEdit::default()).unwrap();
    let path = store.widget_dir("note").unwrap().join(CONFIG_FILE);
    for value in [json!(null), json!("false"), json!(1)] {
        let mut meta = serde_json::to_value(WidgetMeta::new("note", "Note")).unwrap();
        meta["enabled"] = value;
        let bytes = serde_json::to_vec(&meta).unwrap();
        fs::write(&path, &bytes).unwrap();
        assert!(migrate_enabled(&store).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn enabled_migration_preserves_existing_state_and_rejects_reintroduced_legacy_file() {
    let (_temp, store) = fixture();
    store.create("note", "Note", WidgetEdit::default()).unwrap();
    store
        .update(
            "note",
            1,
            WidgetEdit {
                enabled: Some(false),
                ..WidgetEdit::default()
            },
        )
        .unwrap();
    let path = store.widget_dir("note").unwrap().join(CONFIG_FILE);
    let original = fs::read(&path).unwrap();
    let legacy = store.state_dir().join("widgets.json");
    fs::write(&legacy, br#"{"note":{"archived":false}}"#).unwrap();
    migrate_enabled(&store).unwrap();
    assert_eq!(fs::read(path).unwrap(), original);
    fs::write(&legacy, b"{}").unwrap();
    assert!(migrate_enabled(&store).is_err());
    assert!(legacy.exists());
}

#[test]
fn enabled_migration_without_archive_map_uses_board_membership() {
    let (_temp, store) = fixture();
    for id in ["active", "unplaced"] {
        store.create(id, id, WidgetEdit::default()).unwrap();
        old_meta(&store.widget_dir(id).unwrap().join(CONFIG_FILE));
    }
    let mut board = store.load_board().unwrap();
    board.widgets.remove("unplaced");
    store.save_board(&board).unwrap();
    migrate_enabled(&store).unwrap();
    assert!(store.load_meta("active").unwrap().enabled);
    assert!(!store.load_meta("unplaced").unwrap().enabled);
    assert!(!store
        .state_dir()
        .join("migrations/enabled/widgets.json")
        .exists());
}

#[test]
fn enabled_migration_preserves_legacy_map_when_metadata_write_fails() {
    let (_temp, store) = fixture();
    store.create("note", "Note", WidgetEdit::default()).unwrap();
    let dir = store.widget_dir("note").unwrap();
    let path = dir.join(CONFIG_FILE);
    old_meta(&path);
    let original = fs::read(&path).unwrap();
    let legacy = store.state_dir().join("widgets.json");
    fs::write(&legacy, br#"{"note":{"archived":true}}"#).unwrap();
    let collision = dir.join(format!(".tmp-config.json-{}", std::process::id()));
    fs::create_dir(&collision).unwrap();
    assert!(migrate_enabled(&store).is_err());
    assert_eq!(fs::read(&path).unwrap(), original);
    assert!(legacy.exists());
    fs::remove_dir(collision).unwrap();
    migrate_enabled(&store).unwrap();
    assert!(!store.load_meta("note").unwrap().enabled);
}

#[test]
fn completed_enabled_migration_leaves_broken_current_widgets_to_normal_diagnostics() {
    let (_temp, store) = fixture();
    store.create("note", "Note", WidgetEdit::default()).unwrap();
    migrate_enabled(&store).unwrap();
    fs::write(store.widget_dir("note").unwrap().join(CONFIG_FILE), b"{").unwrap();
    migrate_enabled(&store).unwrap();
    assert_eq!(store.list().unwrap().broken[0].id, "note");
}
