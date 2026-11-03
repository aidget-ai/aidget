use aidget::agents::jsonc::{read_path, remove_path, set_path};
use serde_json::json;

#[test]
fn reads_jsonc_and_ignores_comments() {
    let text = "{\n  // servers\n  \"mcpServers\": {\"aidget\": {\"command\": \"/a\"}},\n}\n";
    assert_eq!(
        read_path(text, &["mcpServers", "aidget", "command"]).unwrap(),
        Some(json!("/a"))
    );
}

#[test]
fn reads_jsonc_with_utf8_bom() {
    let text = "\u{feff}{\"mcpServers\":{\"aidget\":{\"command\":\"/a\"}}}";
    assert_eq!(
        read_path(text, &["mcpServers", "aidget", "command"]).unwrap(),
        Some(json!("/a"))
    );
}

#[test]
fn sets_and_removes_an_entry_without_touching_other_keys() {
    let text = "{\n  // keep me\n  \"theme\": \"dark\",\n  \"mcpServers\": {\n    \"other\": {\"command\": \"x\"}\n  }\n}\n";
    let set = set_path(text, &["mcpServers", "aidget"], json!({"command":"/a"})).unwrap();
    assert!(set.contains("// keep me"));
    assert!(set.contains("\"other\""));
    assert_eq!(
        read_path(&set, &["mcpServers", "aidget", "command"]).unwrap(),
        Some(json!("/a"))
    );
    let removed = remove_path(&set, &["mcpServers", "aidget"]).unwrap();
    assert_eq!(
        read_path(&removed, &["mcpServers", "aidget"]).unwrap(),
        None
    );
    assert!(removed.contains("\"theme\""));
}
