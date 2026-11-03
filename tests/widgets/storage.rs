use aidget::common::error::Error;
use aidget::widgets::model::{WidgetPermission, MAX_PAYLOAD};
use aidget::widgets::storage;
use aidget::widgets::store::{Store, WidgetEdit};
use std::fs;
use tempfile::TempDir;

fn root() -> TempDir {
    tempfile::tempdir().unwrap()
}

fn widget(store: &Store, allow_scripts: bool, permissions: &[WidgetPermission]) {
    let mut permissions = permissions.to_vec();
    if allow_scripts {
        permissions.push(WidgetPermission::Script);
    }
    store
        .create(
            "note",
            "Note",
            WidgetEdit {
                html: Some("<p>note</p>"),

                permissions: Some(&permissions),
                ..WidgetEdit::default()
            },
        )
        .unwrap();
}

#[test]
fn storage_requires_an_explicit_permission() {
    let dir = root();
    let store = Store::new(dir.path());
    store.ensure_layout().unwrap();
    widget(&store, true, &[]);

    let error = storage::authorize(&store, "note").unwrap_err();
    assert!(matches!(error, Error::PermissionDenied(_)), "got {error}");

    store
        .update(
            "note",
            1,
            WidgetEdit {
                permissions: Some(&[WidgetPermission::Storage]),
                ..WidgetEdit::default()
            },
        )
        .unwrap();
    storage::authorize(&store, "note").unwrap();
}

#[test]
fn values_persist_across_store_instances_and_can_be_removed() {
    let dir = root();
    let store = Store::new(dir.path());
    store.ensure_layout().unwrap();
    widget(&store, true, &[WidgetPermission::Storage]);

    assert_eq!(storage::get(&store, "note", "todos").unwrap(), None);
    storage::set(&store, "note", "todos", "one").unwrap();
    storage::set(&store, "note", "other", "two").unwrap();
    assert!(store.state_dir().join("storage/note.json").is_file());

    let reopened = Store::new(dir.path());
    assert_eq!(
        storage::get(&reopened, "note", "todos").unwrap(),
        Some("one".to_string())
    );
    storage::remove(&reopened, "note", "todos").unwrap();
    assert_eq!(storage::get(&reopened, "note", "todos").unwrap(), None);
    assert_eq!(
        storage::get(&reopened, "note", "other").unwrap(),
        Some("two".to_string())
    );

    storage::clear(&reopened, "note").unwrap();
    assert_eq!(storage::get(&reopened, "note", "other").unwrap(), None);
}

#[test]
fn storage_reads_json_with_utf8_bom() {
    let dir = root();
    let store = Store::new(dir.path());
    store.ensure_layout().unwrap();
    widget(&store, true, &[WidgetPermission::Storage]);
    storage::set(&store, "note", "todos", "one").unwrap();
    let path = store.state_dir().join("storage/note.json");
    let mut bytes = b"\xEF\xBB\xBF".to_vec();
    bytes.extend_from_slice(&fs::read(&path).unwrap());
    fs::write(path, bytes).unwrap();

    assert_eq!(
        storage::get(&store, "note", "todos").unwrap(),
        Some("one".to_string())
    );
}

#[test]
fn storage_rejects_empty_keys_and_payloads_over_the_limit() {
    let dir = root();
    let store = Store::new(dir.path());
    store.ensure_layout().unwrap();
    widget(&store, true, &[WidgetPermission::Storage]);

    let error = storage::set(&store, "note", "", "value").unwrap_err();
    assert!(matches!(error, Error::InvalidParams(_)), "got {error}");

    let error = storage::set(&store, "note", "large", &"x".repeat(MAX_PAYLOAD)).unwrap_err();
    assert!(matches!(error, Error::InvalidParams(_)), "got {error}");
}

#[test]
fn delete_and_restore_keep_storage_and_purge_drops_it() {
    let dir = root();
    let store = Store::new(dir.path());
    store.ensure_layout().unwrap();
    widget(&store, true, &[WidgetPermission::Storage]);
    storage::set(&store, "note", "todos", "kept").unwrap();

    let trash = store.delete("note").unwrap();
    let trash_name = trash.file_name().unwrap().to_string_lossy().to_string();
    store.restore(&trash_name).unwrap();
    assert_eq!(
        storage::get(&store, "note", "todos").unwrap(),
        Some("kept".to_string())
    );

    let trash = store.delete("note").unwrap();
    store
        .purge(trash.file_name().unwrap().to_string_lossy().as_ref())
        .unwrap();
    assert_eq!(storage::get(&store, "note", "todos").unwrap(), None);
}

#[test]
fn recreating_an_id_clears_previous_storage() {
    let dir = root();
    let store = Store::new(dir.path());
    store.ensure_layout().unwrap();
    widget(&store, true, &[WidgetPermission::Storage]);
    storage::set(&store, "note", "todos", "old").unwrap();

    let trash = store.delete("note").unwrap();
    widget(&store, true, &[WidgetPermission::Storage]);

    assert_eq!(storage::get(&store, "note", "todos").unwrap(), None);
    assert!(trash.exists());
}
