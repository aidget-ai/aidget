use aidget::update::{UpdateSnapshot, UpdateState};

#[test]
fn the_snapshot_serializes_to_the_field_names_the_main_window_reads() {
    let value = serde_json::to_value(UpdateSnapshot {
        state: UpdateState::Found,
        current: "0.1.0".to_string(),
        latest: Some("0.2.0".to_string()),
        error: None,
        done: None,
        total: None,
        notes: Some("notes".into()),
        staged: true,
    })
    .unwrap();

    assert_eq!(value["state"], "found");
    assert_eq!(value["latest"], "0.2.0");
    assert_eq!(value["notes"], "notes");
    assert_eq!(value["staged"], true);
}
