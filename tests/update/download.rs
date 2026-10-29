use aidget::update::{UpdateService, UpdateState};

/// The bytes the mock release serves as its asset.
const BODY: &[u8] = b"new aidget app";
/// sha256 of [`BODY`].
const BODY_SHA256: &str = "e22e140d2bf9041a4c0b831c7da156b5706996a3acd0d44f9af614f9ceb24973";

#[test]
fn a_verified_download_leaves_the_file_staged_and_ready() {
    let base = release_server(BODY_SHA256);
    let service = UpdateService::for_build(&base, "owner/repo", "0.1.0");
    block_on(service.check(None));

    let snapshot = block_on(service.download(None));

    assert_eq!(snapshot.state, UpdateState::Ready);
    let staged = service.staged().expect("a staged download");
    assert_eq!(std::fs::read(&staged).unwrap(), BODY);
    remove_download(staged);
}

#[test]
fn a_mismatched_checksum_fails_and_leaves_nothing_staged() {
    let base = release_server(&"0".repeat(64));
    let service = UpdateService::for_build(&base, "owner/repo", "0.1.0");
    block_on(service.check(None));

    let snapshot = block_on(service.download(None));

    assert_eq!(snapshot.state, UpdateState::Error);
    assert!(snapshot.error.unwrap().contains("checksum mismatch"));
    assert!(service.staged().is_none());
}

#[test]
fn a_release_without_a_digest_uses_the_sums_file() {
    let body = b"sums verified";
    let digest = super::sha256(body);
    let release = format!(
        r#"{{"tag_name": "v99.0.0", "html_url": "", "assets": [
            {{"name": "aidget_v99.0.0_aarch64.app.zip", "browser_download_url": "__BASE__/asset", "size": {}, "digest": null}},
            {{"name": "SHA256SUMS", "browser_download_url": "__BASE__/SHA256SUMS", "size": 100, "digest": null}}
        ]}}"#,
        body.len()
    );
    let sums = format!("{digest}  aidget_v99.0.0_aarch64.app.zip\n");
    let base = super::serve_routes(vec![
        ("/releases/latest", release.into_bytes()),
        ("SHA256SUMS", sums.into_bytes()),
        ("/asset", body.to_vec()),
    ]);
    let service = UpdateService::for_build(&base, "owner/repo", "0.1.0");
    block_on(service.check(None));

    let snapshot = block_on(service.download(None));

    assert_eq!(snapshot.state, UpdateState::Ready);
    if let Some(staged) = service.staged() {
        remove_download(staged);
    }
}

#[test]
fn a_download_before_a_check_fails_without_touching_the_network() {
    let service = UpdateService::for_build("http://127.0.0.1:1", "owner/repo", "0.1.0");

    let snapshot = block_on(service.download(None));

    assert_eq!(snapshot.state, UpdateState::Error);
    assert!(snapshot.error.unwrap().contains("no release to download"));
}

/// A release whose single asset is `BODY`, carrying `digest`.
fn release_server(digest: &str) -> String {
    let body_len = BODY.len();
    let release = format!(
        r#"{{"tag_name": "v99.0.0", "html_url": "", "assets": [
            {{"name": "aidget_v99.0.0_aarch64.app.zip", "browser_download_url": "__BASE__/asset", "size": {body_len}, "digest": "sha256:{digest}"}}
        ]}}"#
    );
    super::serve_routes(vec![
        ("/releases/latest", release.into_bytes()),
        ("/asset", BODY.to_vec()),
    ])
}

/// Take the whole per-download directory away, as an install does.
fn remove_download(staged: std::path::PathBuf) {
    if let Some(dir) = staged.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}

#[test]
fn a_plaintext_download_from_the_network_is_refused() {
    let root = tempfile::TempDir::new().unwrap();
    let dest = root.path().join("asset");

    // Nothing listens there: the refusal has to happen before any request.
    let error = aidget::update::download::to_file(
        "http://downloads.example.com/aidget.zip",
        None,
        &dest,
        &mut |_, _| {},
    )
    .unwrap_err();

    assert!(
        error.to_string().contains("refusing to download"),
        "{error}"
    );
    assert!(!dest.exists());
}

#[test]
fn a_loopback_server_is_allowed_for_the_tests() {
    let base = super::serve_once(200, "bytes");
    let root = tempfile::TempDir::new().unwrap();
    let dest = root.path().join("asset");

    let digest = aidget::update::download::to_file(&base, None, &dest, &mut |_, _| {}).unwrap();

    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "bytes");
    assert_eq!(digest.len(), 64);
}
