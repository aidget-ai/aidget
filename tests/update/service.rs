use aidget::update::{UpdateService, UpdateState};

#[test]
fn a_new_service_is_idle_and_reports_its_build() {
    let snapshot =
        UpdateService::for_build("https://example.invalid", "owner/repo", "0.1.0").snapshot();

    assert_eq!(snapshot.state, UpdateState::Idle);
    assert_eq!(snapshot.current, "0.1.0");
    assert_eq!(snapshot.latest, None);
    assert_eq!(snapshot.error, None);
}

#[test]
fn a_release_ahead_of_this_build_is_found() {
    let base = super::serve_once(
        200,
        r#"{
            "tag_name": "v99.0.0",
            "body": "- faster",
            "html_url": "https://example.invalid/release",
            "assets": []
        }"#,
    );
    let service = UpdateService::for_build(&base, "owner/repo", "0.1.0");

    let snapshot = block_on(service.check(None));

    assert_eq!(snapshot.state, UpdateState::Found);
    assert_eq!(snapshot.latest.as_deref(), Some("99.0.0"));
    assert_eq!(snapshot.error, None);
}

#[test]
fn the_release_tag_loses_its_leading_v() {
    let base = super::serve_once(
        200,
        r#"{"tag_name": "v99.0.0", "html_url": "", "assets": []}"#,
    );
    let service = UpdateService::for_build(&base, "owner/repo", "0.1.0");

    let snapshot = block_on(service.check(None));

    assert_eq!(snapshot.latest.as_deref(), Some("99.0.0"));
}

#[test]
fn the_running_build_is_latest_when_the_feed_names_it() {
    let tag = "v0.1.0";
    let base = super::serve_once(
        200,
        &format!(r#"{{"tag_name": "{tag}", "html_url": "", "assets": []}}"#),
    );
    let service = UpdateService::for_build(&base, "owner/repo", "0.1.0");

    let snapshot = block_on(service.check(None));

    assert_eq!(snapshot.state, UpdateState::Latest);
}

#[test]
fn an_older_release_in_the_feed_is_not_an_update() {
    let base = super::serve_once(
        200,
        r#"{"tag_name": "v0.0.1", "html_url": "", "assets": []}"#,
    );
    let service = UpdateService::for_build(&base, "owner/repo", "0.1.0");

    let snapshot = block_on(service.check(None));

    assert_eq!(snapshot.state, UpdateState::Latest);
}

#[test]
fn a_feed_that_fails_settles_on_error() {
    // Not the 404 of an empty repository: this one carries a reason, and a
    // failure the window should show.
    let base = super::serve_once(404, r#"{"message": "Not Found for another reason"}"#);
    let service = UpdateService::for_build(&base, "owner/repo", "0.1.0");

    let snapshot = block_on(service.check(None));

    assert_eq!(snapshot.state, UpdateState::Error);
    assert!(snapshot.error.is_some());
}

#[test]
fn a_second_check_is_refused_while_one_is_in_flight() {
    let base = super::serve_slow(
        std::time::Duration::from_millis(300),
        r#"{"tag_name": "v99.0.0", "html_url": "", "assets": []}"#,
    );
    let service = UpdateService::for_build(&base, "owner/repo", "0.1.0");
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    let running = {
        let service = service.clone();
        runtime.spawn(async move { service.check(None).await })
    };
    wait_until_checking(&service);

    let second = runtime.block_on(service.check(None));

    assert_eq!(second.state, UpdateState::Checking);
    assert_eq!(runtime.block_on(running).unwrap().state, UpdateState::Found);
}

#[test]
fn a_build_from_a_working_tree_says_it_cannot_update_itself() {
    let base = super::serve_once(
        200,
        r#"{"tag_name": "v99.0.0", "html_url": "", "assets": []}"#,
    );
    let service = UpdateService::for_build(&base, "owner/repo", "0.1.0-3-g5de53c8-dirty");

    let snapshot = block_on(service.check(None));

    assert_eq!(snapshot.state, UpdateState::Source, "{snapshot:?}");
    assert_eq!(snapshot.current, "0.1.0-3-g5de53c8-dirty");
    assert_eq!(
        snapshot.latest, None,
        "a source build offers nothing to download"
    );
}

#[test]
fn a_release_build_at_the_feed_version_is_latest() {
    let base = super::serve_once(
        200,
        r#"{"tag_name": "v0.1.0", "html_url": "", "assets": []}"#,
    );
    let service = UpdateService::for_build(&base, "owner/repo", "0.1.0");

    let snapshot = block_on(service.check(None));

    assert_eq!(snapshot.state, UpdateState::Latest, "{snapshot:?}");
    assert_eq!(snapshot.latest.as_deref(), Some("0.1.0"));
}

fn wait_until_checking(service: &UpdateService) {
    for _ in 0..500 {
        if service.snapshot().state == UpdateState::Checking {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("the first check never started");
}

#[test]
fn clones_share_one_snapshot() {
    let base = super::serve_once(
        200,
        r#"{"tag_name": "v99.0.0", "html_url": "", "assets": []}"#,
    );
    let service = UpdateService::for_build(&base, "owner/repo", "0.1.0");
    let clone = service.clone();

    block_on(clone.check(None));

    assert_eq!(service.snapshot().state, UpdateState::Found);
}

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}

/// A release whose asset is served with a matching digest.
fn release_with_asset() -> String {
    let body = b"new build";
    let digest = super::sha256(body);
    let release = format!(
        r#"{{"tag_name": "v99.0.0", "html_url": "", "assets": [
            {{"name": "__ASSET__", "browser_download_url": "__BASE__/asset", "size": {}, "digest": "sha256:{digest}"}}
        ]}}"#,
        body.len()
    );
    super::serve_routes(vec![
        ("/releases/latest", release.into_bytes()),
        ("/asset", body.to_vec()),
    ])
}

#[test]
fn a_downloaded_update_is_the_one_install_tries_to_use() {
    let base = release_with_asset();
    let service = UpdateService::for_build(&base, "owner/repo", "0.1.0");
    block_on(service.check(None));
    let downloaded = block_on(service.download(None));

    assert_eq!(downloaded.state, UpdateState::Ready);
    assert!(
        service.staged().is_some(),
        "nothing staged after a download"
    );

    let installed = block_on(service.install());
    let error = installed.error.expect("an error outside an .app bundle");
    assert!(!error.contains("no downloaded update"), "{error}");
}

#[test]
fn a_failed_check_keeps_a_downloaded_update_installable() {
    let body = b"ready to install";
    let digest = super::sha256(body);
    let release = format!(
        r#"{{"tag_name": "v99.0.0", "html_url": "", "assets": [
            {{"name": "__ASSET__", "browser_download_url": "__BASE__/asset", "size": {}, "digest": "sha256:{digest}"}}
        ]}}"#,
        body.len()
    );
    let base = super::serve_feed_then(403, "API rate limit exceeded", &release, body.to_vec());
    let service = UpdateService::for_build(&base, "owner/repo", "0.1.0");
    block_on(service.check(None));
    assert_eq!(block_on(service.download(None)).state, UpdateState::Ready);

    eprintln!("DEBUG staged after download: {:?}", service.staged());
    let rechecked = block_on(service.check(None));
    eprintln!(
        "DEBUG rechecked: {:?}, staged: {:?}",
        rechecked,
        service.staged()
    );

    assert_eq!(rechecked.state, UpdateState::Ready, "{rechecked:?}");
    assert!(rechecked.error.unwrap().contains("rate limit"));
    assert!(service.staged().is_some(), "the download was discarded");

    if let Some(staged) = service.staged() {
        super::cleanup(staged);
    }
}

#[test]
fn a_failed_check_with_nothing_downloaded_is_an_error() {
    let base = super::serve_feed_then(
        403,
        "API rate limit exceeded",
        r#"{"tag_name": "v99.0.0"}"#,
        Vec::new(),
    );
    let service = UpdateService::for_build(&base, "owner/repo", "0.1.0");
    block_on(service.check(None));

    let snapshot = block_on(service.check(None));

    assert_eq!(snapshot.state, UpdateState::Error);
}

#[test]
fn a_check_that_finds_the_same_version_keeps_the_download_installable() {
    let body = b"ready to install";
    let digest = super::sha256(body);
    let release = format!(
        r#"{{"tag_name": "v99.0.0", "html_url": "", "assets": [
            {{"name": "__ASSET__", "browser_download_url": "__BASE__/asset", "size": {}, "digest": "sha256:{digest}"}}
        ]}}"#,
        body.len()
    );
    let base = super::serve_routes(vec![
        ("/releases/latest", release.into_bytes()),
        ("/asset", body.to_vec()),
    ]);
    let service = UpdateService::for_build(&base, "owner/repo", "0.1.0");
    block_on(service.check(None));
    assert_eq!(block_on(service.download(None)).state, UpdateState::Ready);

    // The feed still names what was downloaded, so nothing newer is out.
    let rechecked = block_on(service.check(None));

    assert_eq!(rechecked.state, UpdateState::Ready, "{rechecked:?}");
    assert_eq!(rechecked.latest.as_deref(), Some("99.0.0"));
    assert!(service.staged().is_some());

    if let Some(staged) = service.staged() {
        super::cleanup(staged);
    }
}

#[test]
fn a_release_brings_its_notes_and_keeps_them_while_downloading() {
    let body = b"ready to install";
    let digest = super::sha256(body);
    let release = format!(
        r#"{{"tag_name": "v99.0.0", "html_url": "", "body": "  Fixes the updater.\n\n- one\n- two  ", "assets": [
            {{"name": "__ASSET__", "browser_download_url": "__BASE__/asset", "size": {}, "digest": "sha256:{digest}"}}
        ]}}"#,
        body.len()
    );
    let base = super::serve_routes(vec![
        ("/releases/latest", release.into_bytes()),
        ("/asset", body.to_vec()),
    ]);
    let service = UpdateService::for_build(&base, "owner/repo", "0.1.0");

    let found = block_on(service.check(None));
    let ready = block_on(service.download(None));

    assert_eq!(
        found.notes.as_deref(),
        Some("Fixes the updater.\n\n- one\n- two")
    );
    assert_eq!(ready.notes, found.notes);

    if let Some(staged) = service.staged() {
        super::cleanup(staged);
    }
}

#[test]
fn a_release_without_notes_says_nothing() {
    let base = super::serve_once(
        200,
        r#"{"tag_name": "v99.0.0", "html_url": "", "body": "   ", "assets": []}"#,
    );
    let service = UpdateService::for_build(&base, "owner/repo", "0.1.0");

    assert_eq!(block_on(service.check(None)).notes, None);
}

#[test]
fn a_download_is_reported_as_installable_before_it_is_installed() {
    let body = b"ready to install";
    let digest = super::sha256(body);
    let release = format!(
        r#"{{"tag_name": "v99.0.0", "html_url": "", "assets": [
            {{"name": "__ASSET__", "browser_download_url": "__BASE__/asset", "size": {}, "digest": "sha256:{digest}"}}
        ]}}"#,
        body.len()
    );
    let base = super::serve_routes(vec![
        ("/releases/latest", release.into_bytes()),
        ("/asset", body.to_vec()),
    ]);
    let service = UpdateService::for_build(&base, "owner/repo", "0.1.0");

    assert!(
        !block_on(service.check(None)).staged,
        "nothing downloaded yet"
    );
    assert!(
        block_on(service.download(None)).staged,
        "the download is installable"
    );
    // The window decides the Install button from this, whatever the state is.
    assert!(service.snapshot().staged);

    if let Some(staged) = service.staged() {
        super::cleanup(staged);
    }
}

#[test]
fn a_failed_download_leaves_the_release_downloadable_again() {
    // The asset says one thing, the checksum says another: verification fails.
    let release = r#"{"tag_name": "v99.0.0", "html_url": "", "assets": [
        {"name": "__ASSET__", "browser_download_url": "__BASE__/asset", "size": 4, "digest": "sha256:0000000000000000000000000000000000000000000000000000000000000000"}
    ]}"#;
    let base = super::serve_routes(vec![
        ("/releases/latest", release.as_bytes().to_vec()),
        ("/asset", b"body".to_vec()),
    ]);
    let service = UpdateService::for_build(&base, "owner/repo", "0.1.0");
    block_on(service.check(None));

    let failed = block_on(service.download(None));

    assert_eq!(failed.state, UpdateState::Error);
    assert!(!failed.staged, "a failed verification installs nothing");
    assert_eq!(
        failed.latest.as_deref(),
        Some("99.0.0"),
        "the release is still the one to download"
    );
}

#[test]
fn a_failed_install_keeps_the_download_ready_to_try_again() {
    let body = b"ready to install";
    let digest = super::sha256(body);
    let release = format!(
        r#"{{"tag_name": "v99.0.0", "html_url": "", "assets": [
            {{"name": "__ASSET__", "browser_download_url": "__BASE__/asset", "size": {}, "digest": "sha256:{digest}"}}
        ]}}"#,
        body.len()
    );
    let base = super::serve_routes(vec![
        ("/releases/latest", release.into_bytes()),
        ("/asset", body.to_vec()),
    ]);
    let service = UpdateService::for_build(&base, "owner/repo", "0.1.0");
    block_on(service.check(None));
    block_on(service.download(None));

    // The test binary does not run from a bundle the updater can replace, so this
    // install fails the way a real one would on a read-only or unusual install.
    let failed = block_on(service.install());

    assert_eq!(failed.state, UpdateState::Error, "{failed:?}");
    assert!(
        failed.staged,
        "the verified download is still on disk, so it can be installed again"
    );

    if let Some(staged) = service.staged() {
        super::cleanup(staged);
    }
}

#[test]
fn a_repository_with_nothing_published_is_not_a_failure() {
    let base = super::serve_once(404, r#"{"message":"Not Found"}"#);
    let service = UpdateService::for_build(&base, "owner/repo", "0.1.0");

    let snapshot = block_on(service.check(None));

    assert_eq!(snapshot.state, UpdateState::NoRelease, "{snapshot:?}");
    assert_eq!(
        snapshot.error, None,
        "nothing failed: nothing was published"
    );
    assert_eq!(snapshot.latest, None);
}

#[test]
fn an_empty_repository_keeps_a_downloaded_update_installable() {
    let body = b"ready to install";
    let digest = super::sha256(body);
    let release = format!(
        r#"{{"tag_name": "v99.0.0", "html_url": "", "assets": [
            {{"name": "__ASSET__", "browser_download_url": "__BASE__/asset", "size": {}, "digest": "sha256:{digest}"}}
        ]}}"#,
        body.len()
    );
    let base = super::serve_feed_then(404, "Not Found", &release, body.to_vec());
    let service = UpdateService::for_build(&base, "owner/repo", "0.1.0");
    block_on(service.check(None));
    assert_eq!(block_on(service.download(None)).state, UpdateState::Ready);

    let rechecked = block_on(service.check(None));

    assert_eq!(rechecked.state, UpdateState::Ready, "{rechecked:?}");
    assert_eq!(rechecked.error, None);

    if let Some(staged) = service.staged() {
        super::cleanup(staged);
    }
}
