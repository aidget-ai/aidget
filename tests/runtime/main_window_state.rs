use aidget::runtime::main_window_state;
use tempfile::TempDir;

#[test]
fn preview_cache_survives_until_explicitly_removed() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    assert_eq!(
        main_window_state::load_preview(root, "weather").unwrap(),
        None
    );
    main_window_state::save_preview(root, "weather", b"png bytes").unwrap();
    assert_eq!(
        main_window_state::load_preview(root, "weather").unwrap(),
        Some(b"png bytes".to_vec())
    );
    main_window_state::save_preview(root, "weather", b"new bytes").unwrap();
    assert_eq!(
        main_window_state::load_preview(root, "weather").unwrap(),
        Some(b"new bytes".to_vec())
    );
    main_window_state::forget_widget(root, "weather").unwrap();
    main_window_state::forget_widget(root, "weather").unwrap();
    assert_eq!(
        main_window_state::load_preview(root, "weather").unwrap(),
        None
    );
    assert!(!root.join("state/widgets.json").exists());
}

#[test]
fn rejects_invalid_widget_ids_before_building_preview_paths() {
    let dir = TempDir::new().unwrap();
    assert!(main_window_state::load_preview(dir.path(), "../outside").is_err());
    assert!(main_window_state::save_preview(dir.path(), "../outside", b"png").is_err());
    assert!(main_window_state::forget_widget(dir.path(), "../outside").is_err());
}
