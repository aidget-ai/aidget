use aidget::update::fetch::{client, ensure_secure, github_api_get};
use reqwest::header::AUTHORIZATION;

#[test]
fn an_https_asset_url_is_allowed() {
    assert!(ensure_secure("https://github.com/owner/repo/releases/download/v1/a.zip").is_ok());
}

#[test]
fn a_plaintext_asset_url_from_the_network_is_refused() {
    let error = ensure_secure("http://downloads.example.com/aidget.zip").unwrap_err();

    assert!(
        error.to_string().contains("refusing to download"),
        "{error}"
    );
}

#[test]
fn loopback_is_allowed_for_the_local_test_server() {
    for url in [
        "http://127.0.0.1:8080/asset",
        "http://localhost:8080/asset",
        "http://[::1]:8080/asset",
    ] {
        assert!(ensure_secure(url).is_ok(), "{url}");
    }
}

#[test]
fn a_scheme_that_is_not_http_is_refused() {
    for url in ["ftp://example.com/a.zip", "file:///tmp/a.zip"] {
        assert!(ensure_secure(url).is_err(), "{url}");
    }
}

#[test]
fn a_token_is_only_attached_to_the_official_https_api_host() {
    let client = client().unwrap();
    let request = github_api_get(
        &client,
        "https://api.github.com/repos/aidget-ai/aidget/releases/latest",
        Some("secret-token"),
    )
    .unwrap()
    .build()
    .unwrap();
    assert_eq!(
        request.headers().get(AUTHORIZATION).unwrap(),
        "Bearer secret-token"
    );

    for url in [
        "https://github.com/aidget-ai/aidget/releases/download/v1/a.zip",
        "http://api.github.com/repos/aidget-ai/aidget/releases/latest",
        "https://api.github.com.example.invalid/repos/aidget-ai/aidget",
    ] {
        assert!(
            github_api_get(&client, url, Some("secret-token")).is_err(),
            "{url}"
        );
    }
}
