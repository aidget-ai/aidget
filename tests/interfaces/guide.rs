use std::process::Command;

use aidget::{
    interfaces::{guide, mcp::AidgetMcp},
    widgets::store::Store,
};

#[test]
fn markdown_guide_documents_the_bridge_and_platform_contracts() {
    let markdown = guide::markdown();

    assert!(markdown.starts_with("# Aidget Widget Guide\n"));
    assert!(markdown.contains(&format!("Version {}", env!("CARGO_PKG_VERSION"))));
    for heading in [
        "## Data root",
        "## config.json",
        "## Visual surfaces",
        "## window.aidget bridge",
        "## Platform capabilities",
        "## Error codes",
    ] {
        assert!(
            markdown.contains(heading),
            "missing guide heading: {heading}"
        );
    }
    for method in [
        "ready()",
        "permissions",
        "requestLocation()",
        "storage",
        "fetch(url, options?)",
        "openUrl(url)",
    ] {
        assert!(markdown.contains(method), "missing bridge method: {method}");
    }
    assert!(markdown.contains("Every bridge method below requires `script`"));
    assert!(markdown.contains("Requires `location` in addition to `script`"));
    assert!(markdown.contains("Requires `storage` in addition to `script`"));
}

#[test]
fn markdown_guide_documents_network_limits_and_error_codes() {
    let markdown = guide::markdown();

    for detail in [
        "request timeout: 12 seconds",
        "connect timeout: 5 seconds",
        "response body: 1 MiB",
        "request body: 256 KiB",
        "application headers: 32 KiB",
        "redirects: at most 3",
        "`permission_denied`",
        "`http_error`",
        "`invalid_response`",
        "`unavailable`",
    ] {
        assert!(markdown.contains(detail), "missing guide detail: {detail}");
    }
}

#[test]
fn cli_prints_the_shared_markdown_guide() {
    let home = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_aidget"))
        .arg("guide")
        .env("AIDGET_HOME", home.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout, guide::markdown());
}

#[tokio::test]
async fn mcp_returns_the_same_markdown_guide() {
    let home = tempfile::tempdir().unwrap();
    let server = AidgetMcp::new(Store::new(home.path()));
    let result = server.guide().await;

    assert!(!result.is_error.unwrap_or(false));
    assert!(result.structured_content.is_none());
    assert_eq!(result.content.len(), 1);
    assert_eq!(result.content[0].as_text().unwrap().text, guide::markdown());
}

#[test]
fn guide_rejects_the_removed_json_mode() {
    let home = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_aidget"))
        .args(["guide", "--json"])
        .env("AIDGET_HOME", home.path())
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("\"code\":\"invalid_params\""));
    assert!(stderr.contains("guide always prints Markdown"));
}
