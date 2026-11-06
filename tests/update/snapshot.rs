use aidget::update::{ReleaseNote, UpdateNotes, UpdateSnapshot, UpdateState};

#[test]
fn the_snapshot_serializes_to_the_field_names_the_main_window_reads() {
    let value = serde_json::to_value(UpdateSnapshot {
        state: UpdateState::Found,
        is_debug_build: false,
        current: "0.1.0".to_string(),
        latest: Some("0.2.0".to_string()),
        error: None,
        done: None,
        total: None,
        notes: Some(Box::new(UpdateNotes {
            from: "0.1.0".to_string(),
            to: "0.2.0".to_string(),
            releases: vec![ReleaseNote {
                version: "0.2.0".to_string(),
                body: "notes".to_string(),
            }],
        })),
        staged: true,
    })
    .unwrap();

    assert_eq!(value["state"], "found");
    assert_eq!(value["isDebugBuild"], false);
    assert_eq!(value["latest"], "0.2.0");
    assert_eq!(value["notes"]["from"], "0.1.0");
    assert_eq!(value["notes"]["releases"][0]["body"], "notes");
    assert_eq!(value["staged"], true);
}
