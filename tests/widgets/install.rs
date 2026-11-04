use std::fs;

use aidget::widgets::install;
use aidget::widgets::package::{PackageManifest, WidgetConfig};
use aidget::widgets::store::{Store, CONFIG_FILE, PACKAGE_FILE};
use serde_json::json;

#[test]
fn installation_does_not_inherit_package_host_state_or_a_previous_disabled_id() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::new(temp.path());
    store.ensure_layout().unwrap();
    let mut config = serde_json::to_value(WidgetConfig::new("note", "Note")).unwrap();
    config["enabled"] = json!(false);
    config["revision"] = json!(999);
    let config: WidgetConfig = serde_json::from_value(config).unwrap();
    let package: PackageManifest = serde_json::from_value(json!({
        "name": "@aidget-community/note", "version": "1.0.0"
    }))
    .unwrap();
    for _ in 0..2 {
        let staged = temp.path().join("staged");
        fs::create_dir(&staged).unwrap();
        fs::write(
            staged.join(PACKAGE_FILE),
            serde_json::to_vec(&package).unwrap(),
        )
        .unwrap();
        let meta = install::commit(&store, &staged, config.clone(), &package, "note").unwrap();
        assert!(meta.enabled);
        assert_eq!(meta.revision, 1);
        assert!(store.load_meta("note").unwrap().enabled);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(
                &fs::read(store.widget_dir("note").unwrap().join(CONFIG_FILE)).unwrap()
            )
            .unwrap()["enabled"],
            true
        );
        store
            .update(
                "note",
                1,
                aidget::widgets::store::WidgetEdit {
                    enabled: Some(false),
                    ..Default::default()
                },
            )
            .unwrap();
        let deleted = store.delete("note").unwrap();
        store
            .purge(deleted.file_name().unwrap().to_str().unwrap())
            .unwrap();
    }
}
