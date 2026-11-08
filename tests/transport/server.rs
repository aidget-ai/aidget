use aidget::{
    transport::server::protocol::{
        embedded_cache_control, serve_widget, serve_widget_with_override, widget_csp_for,
    },
    widgets::{
        model::WidgetPermission,
        package::WidgetConfig,
        store::{Store, WidgetEdit},
    },
};
#[test]
fn every_resource_type_is_bounded_by_the_declared_network_list() {
    let mut config = WidgetConfig::new("note", "Note");
    let policy = widget_csp_for(&config, "http://aidget.localhost");
    assert!(policy.contains("connect-src 'none'"));
    assert!(policy.contains("script-src 'none'"));
    assert!(!policy.contains("https:"));
    config.permissions.push(WidgetPermission::Script);
    config.network.push("https://api.example.com".into());
    let policy = widget_csp_for(&config, "http://aidget.localhost");
    for directive in [
        "style-src",
        "img-src",
        "font-src",
        "media-src",
        "connect-src",
    ] {
        let value = policy
            .split(';')
            .find(|s| s.trim().starts_with(directive))
            .unwrap();
        assert!(value.contains("https://api.example.com"));
    }
    assert!(policy.contains("http://aidget.localhost/content/note/"));
    assert!(!policy.contains("connect-src http:"));
    let scripts = policy
        .split(';')
        .find(|s| s.trim().starts_with("script-src "))
        .unwrap();
    assert!(!scripts.contains("https://api.example.com"));
    assert!(!scripts.contains("'unsafe-inline'"));
    assert!(!scripts.contains("'unsafe-eval'"));
}

#[test]
fn html_responses_authorize_only_their_existing_inline_scripts() {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use sha2::{Digest, Sha256};

    let root = tempfile::tempdir().unwrap();
    let store = Store::new(root.path());
    store.ensure_layout().unwrap();
    store.create("note", "Note", WidgetEdit {
        html: Some("<script>window.answer = 42;\r\n</script><script src=\"assets/app.js\"></script>"),
        permissions: Some(&[WidgetPermission::Script]),
        ..WidgetEdit::default()
    }).unwrap();
    let response = serve_widget(&store, "note/index.html");
    assert_eq!(response.status(), 200);
    let policy = response.headers()["content-security-policy"]
        .to_str()
        .unwrap();
    let hash = STANDARD.encode(Sha256::digest(b"window.answer = 42;\n"));
    assert!(policy.contains(&format!("'sha256-{hash}'")), "{policy}");
    assert!(!policy.contains("script-src 'unsafe-inline'"));
    assert!(!policy.contains("'unsafe-eval'"));
    let html = store.widget_dir("note").unwrap().join("index.html");
    std::fs::write(&html, "<script>window.answer = 43;</script>").unwrap();
    let response = serve_widget(&store, "note/index.html");
    assert!(!response.headers()["content-security-policy"]
        .to_str()
        .unwrap()
        .contains(&hash));
    std::fs::write(&html, b"<script>\xff</script>").unwrap();
    assert_eq!(serve_widget(&store, "note/index.html").status(), 500);
}
#[test]
fn widget_files_never_expose_package_or_another_widget() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::new(root.path());
    store.ensure_layout().unwrap();
    store.create("note", "Note", WidgetEdit::default()).unwrap();
    std::fs::write(
        store.widget_dir("note").unwrap().join("assets/evil.svg"),
        b"<svg/>",
    )
    .unwrap();
    let response = serve_widget(&store, "note/assets/evil.svg");
    assert_eq!(response.status(), 200);
    assert!(response.headers()["content-security-policy"]
        .to_str()
        .unwrap()
        .contains("script-src 'none'"));
    for path in [
        "note/config.json",
        "note/package.json",
        "note/../other/index.html",
        "note/assets/%2e%2e/config.json",
    ] {
        assert_eq!(serve_widget(&store, path).status(), 404, "{path}");
    }
}
#[test]
fn embedded_html_revalidates_while_hashed_assets_are_immutable() {
    assert_eq!(embedded_cache_control("main.html"), "no-cache");
    assert_eq!(
        embedded_cache_control("assets/main-abc.js"),
        "public, max-age=31536000, immutable"
    );
}

#[test]
fn widget_content_is_blocked_when_the_declared_host_version_is_newer() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::new(root.path());
    store.ensure_layout().unwrap();
    store
        .create("future", "Future", WidgetEdit::default())
        .unwrap();
    let mut meta = store.load_meta("future").unwrap();
    meta.min_host_version = Some("99.0.0".into());
    std::fs::write(
        store.widget_dir("future").unwrap().join("config.json"),
        serde_json::to_vec_pretty(&meta).unwrap(),
    )
    .unwrap();

    assert_eq!(serve_widget(&store, "future/index.html").status(), 404);
    assert_eq!(
        serve_widget_with_override(&store, "future/index.html", true).status(),
        200
    );
}
