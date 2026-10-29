use aidget::widgets::store::Store;
use aidget::widgets::template::bootstrap_welcome;

use tempfile::TempDir;

#[test]
fn bootstrap_installs_the_complete_welcome_package_once() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store.ensure_layout().unwrap();
    bootstrap_welcome(&store).unwrap();
    let widget = store.widget_dir("welcome").unwrap();
    for file in [
        "config.json",
        "index.html",
        "style.css",
        "agents.md",
        "package.json",
    ] {
        assert!(widget.join(file).is_file(), "missing {file}");
    }
    assert_eq!(
        store.load_package("welcome").unwrap().unwrap().name,
        "@aidget-community/welcome"
    );
    bootstrap_welcome(&store).unwrap();
    assert_eq!(store.widget_ids().unwrap(), vec!["welcome".to_string()]);
}
