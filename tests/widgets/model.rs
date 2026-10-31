use aidget::widgets::model::WidgetFrame;

#[test]
fn script_execution_is_a_permission_not_a_separate_switch() {
    let value =
        serde_json::to_value(aidget::widgets::model::WidgetMeta::new("note", "Note")).unwrap();
    assert!(value.get("allowScripts").is_none());
    assert_eq!(value["network"], serde_json::json!([]));
    assert!(value.get("source").is_none());
}

#[test]
fn metadata_ignores_unused_fields_without_granting_script_permission() {
    use aidget::widgets::model::{WidgetMeta, WidgetPermission};
    use serde_json::json;

    for unused in [json!(true), json!(false), json!({"obsolete": true})] {
        let mut value = serde_json::to_value(WidgetMeta::new("note", "Note")).unwrap();
        value["allowScripts"] = unused;
        value["version"] = json!("0.1.0");
        value["source"] = json!({"kind": "builtin", "id": "welcome"});
        let meta: WidgetMeta = serde_json::from_value(value.clone()).unwrap();
        assert!(!meta.grants(WidgetPermission::Script));
        assert_eq!(meta.revision, 1);

        value["permissions"] = json!(["script"]);
        let meta: WidgetMeta = serde_json::from_value(value).unwrap();
        assert!(meta.grants(WidgetPermission::Script));
    }
}

#[test]
fn unused_fields_do_not_supply_missing_or_invalid_current_fields() {
    use aidget::widgets::model::WidgetMeta;
    use serde_json::json;

    let mut value = serde_json::to_value(WidgetMeta::new("note", "Note")).unwrap();
    value["allowScripts"] = json!(true);
    value.as_object_mut().unwrap().remove("permissions");
    assert!(serde_json::from_value::<WidgetMeta>(value.clone()).is_err());
    value["permissions"] = json!(["unsupported"]);
    assert!(serde_json::from_value::<WidgetMeta>(value).is_err());
}

#[test]
fn drag_mode_is_explicit_and_rejects_unknown_values() {
    use aidget::widgets::model::WidgetMeta;
    use serde_json::json;

    let mut value = serde_json::to_value(WidgetMeta::new("note", "Note")).unwrap();
    assert_eq!(value["dragMode"], "all");
    value.as_object_mut().unwrap().remove("dragMode");
    assert!(serde_json::from_value::<WidgetMeta>(value.clone()).is_err());
    value["dragMode"] = json!("handle");
    let meta: WidgetMeta = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(meta).unwrap()["dragMode"], "handle");
    value["dragMode"] = json!("disabled");
    assert!(serde_json::from_value::<WidgetMeta>(value).is_err());
}

#[test]
fn clamps_frames_into_the_work_area() {
    let frame = WidgetFrame {
        display: "primary".to_string(),
        x: 5000.0,
        y: -80.0,
        w: 900.0,
        h: 100.0,
    };
    let clamped = frame.clamped((1440.0, 900.0, 1440.0, 900.0));
    assert_eq!(clamped.x, 540.0);
    assert_eq!(clamped.y, 0.0);
    assert_eq!(clamped.w, 900.0);
    assert_eq!(clamped.h, 120.0);
}

#[test]
fn default_frames_do_not_overlap() {
    let a = WidgetFrame::default_for(0);
    let b = WidgetFrame::default_for(1);
    assert!(a.x + a.w <= b.x);
}

#[test]
fn fitting_a_frame_keeps_cross_display_offsets() {
    let frame = WidgetFrame {
        display: "Monitor #2".to_string(),
        x: -120.0,
        y: -300.0,
        w: 900.0,
        h: 100.0,
    };
    let fitted = frame.fitted((1512.0, 893.0, 1512.0, 893.0));
    assert_eq!(fitted.x, -120.0);
    assert_eq!(fitted.y, -300.0);
    assert_eq!(fitted.w, 900.0);
    assert_eq!(fitted.h, 120.0);
}
