use aidget::update::fetch::ensure_secure;

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
