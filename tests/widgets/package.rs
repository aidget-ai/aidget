use aidget::widgets::package::{
    HostCompatibility, HostCompatibilityState, PackageManifest, WidgetConfig,
};

#[test]
fn config_ignores_unused_fields_without_changing_current_values() {
    let expected = WidgetConfig::new("clock", "Clock");
    let mut value = serde_json::to_value(&expected).unwrap();
    value["allowScripts"] = serde_json::json!(true);
    value["version"] = serde_json::json!("0.1.0");
    value["source"] = serde_json::json!({"kind": "builtin", "id": "welcome"});
    value["unused"] = serde_json::json!({"anything": [1, 2, 3]});
    value["size"]["unused"] = serde_json::json!(true);

    let config: WidgetConfig = serde_json::from_value(value).unwrap();
    config.validate().unwrap();
    assert_eq!(config, expected);
}

#[test]
fn new_widgets_declare_the_current_host_and_version_comparison_uses_semver_precedence() {
    let mut config = WidgetConfig::new("clock", "Clock");
    assert_eq!(
        config.min_host_version.as_deref(),
        Some(env!("CARGO_PKG_VERSION"))
    );
    config.min_host_version = Some("1.2.0+build.7".into());
    let compatible = HostCompatibility::evaluate(&config, "1.2.0+other", false).unwrap();
    assert_eq!(compatible.state, HostCompatibilityState::Compatible);
    assert!(!compatible.allowed_by_override);

    config.min_host_version = Some("1.2.0".into());
    let prerelease = HostCompatibility::evaluate(&config, "1.2.0-beta.2", false).unwrap();
    assert_eq!(prerelease.state, HostCompatibilityState::HostTooOld);
    assert!(!prerelease.permits_runtime());
    let bypassed = HostCompatibility::evaluate(&config, "1.2.0-beta.2", true).unwrap();
    assert!(bypassed.allowed_by_override);
    assert!(bypassed.permits_runtime());

    config.min_host_version = None;
    assert_eq!(
        HostCompatibility::evaluate(&config, "1.2.0", false)
            .unwrap()
            .state,
        HostCompatibilityState::Undeclared
    );
}

#[test]
fn minimum_host_versions_reject_tags_ranges_and_invalid_semver() {
    for value in ["v1.0.0", "^1.0.0", "latest", "1.0", ""] {
        let mut config = WidgetConfig::new("clock", "Clock");
        config.min_host_version = Some(value.into());
        assert!(config.validate().is_err(), "{value}");
    }
    for value in ["0.0.1", "1.2.3-beta.1", "1.2.3+sha.abc"] {
        let mut config = WidgetConfig::new("clock", "Clock");
        config.min_host_version = Some(value.into());
        assert!(config.validate().is_ok(), "{value}");
    }
}

#[test]
fn initial_sizes_reject_oversized_and_nonfinite_dimensions() {
    for dimension in [8192.1, 1e308, f64::INFINITY, f64::NAN] {
        for width in [true, false] {
            let mut config = WidgetConfig::new("clock", "Clock");
            if width {
                config.size.width = dimension;
            } else {
                config.size.height = dimension;
            }
            assert!(config.validate().is_err(), "{dimension}, width={width}");
        }
    }
    let mut config = WidgetConfig::new("clock", "Clock");
    config.size.width = 8192.0;
    config.size.height = 8192.0;
    assert!(config.validate().is_ok());
}
#[test]
fn only_the_exact_community_scope_is_classified_as_community() {
    let package = |name: &str| {
        serde_json::from_value::<PackageManifest>(
            serde_json::json!({"name":name,"version":"1.0.0"}),
        )
        .unwrap()
    };
    assert!(package("@aidget-community/clock").is_community());
    assert!(!package("@aidget-community-fake/clock").is_community());
    assert!(!package("clock").is_community());
}
#[test]
fn network_declarations_are_exact_https_origins() {
    for origin in ["https://api.example.com", "https://example.com:8443"] {
        let mut config = WidgetConfig::new("clock", "Clock");
        config.network = vec![origin.into()];
        assert!(config.validate().is_ok());
    }
    for origin in [
        "https:",
        "https://*.example.com",
        "http://example.com",
        "https://example.com/api",
        "https://user@example.com",
        "https://example.com/",
        "https://example.com?x=1",
    ] {
        let mut config = WidgetConfig::new("clock", "Clock");
        config.network = vec![origin.into()];
        assert!(config.validate().is_err(), "{origin}");
    }
}
