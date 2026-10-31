use aidget::community::model::Registry;
use serde_json::{json, Value};

fn registry() -> Value {
    let mut value: Value =
        serde_json::from_str(include_str!("../../widgets/registry.json")).unwrap();
    for entry in value["widgets"].as_array_mut().unwrap() {
        entry["publishedAt"] = json!("2026-10-25T06:49:57Z");
    }
    value
}

#[test]
fn discovery_types_allow_unknown_fields() {
    let mut value = registry();
    value["future"] = json!(true);
    value["widgets"][0]["future"] = json!({"enabled":true});
    value["widgets"][0]["files"][0]["future"] = json!(42);
    value["widgets"][0]["config"]["future"] = json!(true);
    let parsed: Registry = serde_json::from_value(value).unwrap();
    assert_eq!(
        serde_json::to_value(parsed).unwrap()["widgets"][0]["publishedAt"],
        "2026-10-25T06:49:57Z"
    );
}

#[test]
fn published_at_is_required_and_must_be_utc_rfc3339() {
    for invalid in [
        json!(null),
        json!("not-a-date"),
        json!("2026-02-30T00:00:00Z"),
        json!("2026-10-25T08:00:00+08:00"),
        json!(17),
    ] {
        let mut value = registry();
        value["widgets"][0]["publishedAt"] = invalid;
        assert!(serde_json::from_value::<Registry>(value).is_err());
    }
    let mut value = registry();
    value["widgets"][0]
        .as_object_mut()
        .unwrap()
        .remove("publishedAt");
    assert!(serde_json::from_value::<Registry>(value).is_err());
}

#[test]
fn discovery_still_validates_known_fields_and_widget_configuration() {
    let mut value = registry();
    value["widgets"][0]["config"]["permissions"] = json!(["unsupported"]);
    assert!(serde_json::from_value::<Registry>(value).is_err());
    let mut value = registry();
    value["widgets"][0]["files"][0]["size"] = json!("large");
    assert!(serde_json::from_value::<Registry>(value).is_err());
}
