use aidget::update::{UpdateService, UpdateState};

#[test]
fn a_new_service_is_idle_and_reports_its_build() {
    let snapshot =
        UpdateService::for_build("https://example.invalid", "owner/repo", "0.1.0").snapshot();

    assert_eq!(snapshot.state, UpdateState::Idle);
    assert!(!snapshot.is_debug_build);
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
fn a_source_build_can_still_show_stable_release_notes() {
    let base = super::serve_once(
        200,
        &release_list(&[
            ("v2.0.0", "two", false, false),
            ("v3.0.0", "preview", true, false),
        ]),
    );
    let service = UpdateService::for_build(&base, "owner/repo", "0.1.0-3-g5de53c8-dirty");

    let snapshot = block_on(service.check(None));

    assert_eq!(snapshot.state, UpdateState::Source);
    assert_eq!(snapshot.latest, None);
    let notes = snapshot.notes.expect("stable notes");
    assert_eq!((notes.from.as_str(), notes.to.as_str()), ("2.0.0", "2.0.0"));
    assert_eq!(notes.releases[0].body, "two");
}

#[test]
fn a_debug_build_never_downloads_even_at_a_release_version() {
    let base = release_with_asset();
    let service = UpdateService::for_debug_build(&base, "owner/repo", "0.1.0");

    let snapshot = block_on(service.check_with_policy(None, true));

    assert!(snapshot.is_debug_build);
    assert_eq!(snapshot.state, UpdateState::Source);
    assert_eq!(snapshot.latest, None);
    assert!(!snapshot.staged);
    assert!(service.staged().is_none());
}

#[test]
fn a_debug_build_loads_the_five_newest_stable_release_notes() {
    let body = release_list(&[
        ("v9.0.0", "preview", true, false),
        ("v8.0.0", "eight", false, false),
        ("v7.0.0", "seven", false, false),
        ("v6.0.0", "six", false, false),
        ("v5.0.0", "five", false, false),
        ("v4.0.0", "four", false, false),
        ("v3.0.0", "three", false, false),
    ]);
    let base = super::serve_once(200, &body);
    let service = UpdateService::for_debug_build(&base, "owner/repo", "1.0.0");

    let snapshot = block_on(service.check(None));
    let notes = snapshot.notes.expect("recent stable notes");

    assert_eq!(snapshot.state, UpdateState::Source);
    assert_eq!((notes.from.as_str(), notes.to.as_str()), ("4.0.0", "8.0.0"));
    assert_eq!(notes.releases.len(), 5);
    assert_eq!(notes.releases[0].body, "eight");
    assert!(!notes
        .releases
        .iter()
        .any(|release| release.body == "preview"));
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
    let downloaded = block_on(service.download());

    assert_eq!(downloaded.state, UpdateState::Ready);
    assert!(
        service.staged().is_some(),
        "nothing staged after a download"
    );

    let installed = block_on(service.install());
    let error = installed
        .snapshot()
        .error
        .clone()
        .expect("an error outside an .app bundle");
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
    assert_eq!(block_on(service.download()).state, UpdateState::Ready);

    let rechecked = block_on(service.check(None));

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
    assert_eq!(block_on(service.download()).state, UpdateState::Ready);

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
    let ready = block_on(service.download());

    assert_eq!(
        found
            .notes
            .as_deref()
            .and_then(|notes| notes.releases.first())
            .map(|release| release.body.as_str()),
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
fn release_notes_use_stable_semver_versions_in_the_update_range() {
    let base = super::serve_once(
        200,
        &release_list(&[
            ("v4.0.0-beta.1", "preview", false, false),
            ("v2.0.0", "stable two", false, false),
            ("v5.0.0", "draft", false, true),
            ("v3.0.0", "stable three", false, false),
        ]),
    );
    let service = UpdateService::for_build(&base, "owner/repo", "1.0.0");

    let snapshot = block_on(service.check(None));
    let notes = snapshot.notes.expect("the stable releases have notes");

    assert_eq!(snapshot.latest.as_deref(), Some("3.0.0"));
    assert_eq!(notes.from, "1.0.0");
    assert_eq!(notes.to, "3.0.0");
    assert_eq!(
        notes
            .releases
            .iter()
            .map(|release| (release.version.as_str(), release.body.as_str()))
            .collect::<Vec<_>>(),
        vec![("3.0.0", "stable three"), ("2.0.0", "stable two")]
    );
}

#[test]
fn an_installed_target_reuses_the_retained_range() {
    let directory = tempfile::tempdir().unwrap();
    let cache = directory.path().join("update/releases.json");
    let body = release_list(&[
        ("v3.0.0", "three", false, false),
        ("v2.0.0", "two", false, false),
        ("v1.0.0", "one", false, false),
    ]);

    let first_base = super::serve_once(200, &body);
    let first =
        UpdateService::for_build_with_cache(&first_base, "owner/repo", "1.0.0", &cache).unwrap();
    let found = block_on(first.check(None));
    assert_eq!(
        found.notes.as_deref(),
        Some(&aidget::update::UpdateNotes {
            from: "1.0.0".into(),
            to: "3.0.0".into(),
            releases: vec![
                aidget::update::ReleaseNote {
                    version: "3.0.0".into(),
                    body: "three".into(),
                },
                aidget::update::ReleaseNote {
                    version: "2.0.0".into(),
                    body: "two".into(),
                },
            ],
        })
    );
    drop(first);

    let second_base = super::serve_once(200, &body);
    let second =
        UpdateService::for_build_with_cache(&second_base, "owner/repo", "3.0.0", &cache).unwrap();
    let latest = block_on(second.check(None));
    assert_eq!(latest.state, UpdateState::Latest);
    assert_eq!(latest.notes.as_deref(), found.notes.as_deref());
}

#[test]
fn an_installed_release_does_not_include_the_previous_release_notes() {
    let body = release_list(&[
        ("v8.0.0", "eight", false, false),
        ("v7.0.0", "seven", false, false),
        ("v6.0.0", "six", false, false),
        ("v5.0.0", "five", false, false),
        ("v4.0.0", "four", false, false),
        ("v3.0.0", "three", false, false),
    ]);
    let base = super::serve_once(200, &body);
    let service = UpdateService::for_build(&base, "owner/repo", "8.0.0");

    let snapshot = block_on(service.check(None));
    let notes = snapshot.notes.expect("the target release keeps its notes");

    assert_eq!(snapshot.state, UpdateState::Latest);
    assert_eq!(notes.from, "7.0.0");
    assert_eq!(notes.to, "8.0.0");
    assert_eq!(
        notes
            .releases
            .iter()
            .map(|release| release.version.as_str())
            .collect::<Vec<_>>(),
        vec!["8.0.0"]
    );
}

#[test]
fn a_retained_cache_excludes_the_starting_release_before_a_check() {
    let directory = tempfile::tempdir().unwrap();
    let cache = directory.path().join("update/releases.json");
    std::fs::create_dir_all(cache.parent().unwrap()).unwrap();
    std::fs::write(
        &cache,
        serde_json::json!({
            "schemaVersion": 1,
            "lastRange": null,
            "recent": {
                "from": "7.0.0",
                "to": "8.0.0",
                "releases": [
                    { "version": "8.0.0", "body": "eight" },
                    { "version": "7.0.0", "body": "seven" }
                ]
            }
        })
        .to_string(),
    )
    .unwrap();

    let service =
        UpdateService::for_build_with_cache("unused", "owner/repo", "8.0.0", &cache).unwrap();
    let notes = service.snapshot().notes.expect("the target release notes");

    assert_eq!(notes.from, "7.0.0");
    assert_eq!(notes.to, "8.0.0");
    assert_eq!(
        notes
            .releases
            .iter()
            .map(|release| release.version.as_str())
            .collect::<Vec<_>>(),
        vec!["8.0.0"]
    );
}

#[test]
fn a_pending_retained_range_survives_a_restart() {
    let directory = tempfile::tempdir().unwrap();
    let cache = directory.path().join("update/releases.json");
    std::fs::create_dir_all(cache.parent().unwrap()).unwrap();
    std::fs::write(
        &cache,
        serde_json::json!({
            "schemaVersion": 1,
            "lastRange": {
                "from": "1.0.0",
                "to": "8.0.0",
                "releases": [
                    { "version": "8.0.0", "body": "eight" },
                    { "version": "7.0.0", "body": "seven" },
                    { "version": "6.0.0", "body": "six" },
                    { "version": "5.0.0", "body": "five" },
                    { "version": "4.0.0", "body": "four" },
                    { "version": "3.0.0", "body": "three" },
                    { "version": "2.0.0", "body": "two" }
                ]
            },
            "recent": {
                "from": "4.0.0",
                "to": "8.0.0",
                "releases": [
                    { "version": "8.0.0", "body": "eight" },
                    { "version": "7.0.0", "body": "seven" },
                    { "version": "6.0.0", "body": "six" },
                    { "version": "5.0.0", "body": "five" },
                    { "version": "4.0.0", "body": "four" }
                ]
            }
        })
        .to_string(),
    )
    .unwrap();

    let service =
        UpdateService::for_build_with_cache("unused", "owner/repo", "1.0.0", &cache).unwrap();
    let notes = service.snapshot().notes.expect("the pending range");

    assert_eq!(notes.from, "1.0.0");
    assert_eq!(notes.to, "8.0.0");
    assert_eq!(notes.releases.len(), 7);
}

#[test]
fn empty_latest_notes_do_not_show_the_previous_release() {
    let directory = tempfile::tempdir().unwrap();
    let cache = directory.path().join("update/releases.json");
    std::fs::create_dir_all(cache.parent().unwrap()).unwrap();
    std::fs::write(
        &cache,
        serde_json::json!({
            "schemaVersion": 1,
            "lastRange": null,
            "recent": {
                "from": "7.0.0",
                "to": "8.0.0",
                "releases": [
                    { "version": "7.0.0", "body": "seven" }
                ]
            }
        })
        .to_string(),
    )
    .unwrap();

    let service =
        UpdateService::for_build_with_cache("unused", "owner/repo", "8.0.0", &cache).unwrap();

    assert_eq!(service.snapshot().notes, None);
}

#[test]
fn a_source_build_falls_back_to_the_five_newest_stable_releases() {
    let body = release_list(&[
        ("v8.0.0", "eight", false, false),
        ("v7.0.0", "seven", false, false),
        ("v6.0.0", "six", false, false),
        ("v5.0.0", "five", false, false),
        ("v4.0.0", "four", false, false),
        ("v3.0.0", "three", false, false),
    ]);
    let base = super::serve_once(200, &body);
    let service = UpdateService::for_build(&base, "owner/repo", "0.1.0-3-g5de53c8-dirty");

    let snapshot = block_on(service.check(None));
    let notes = snapshot.notes.expect("recent stable notes");

    assert_eq!(notes.from, "4.0.0");
    assert_eq!(notes.to, "8.0.0");
    assert_eq!(notes.releases.len(), 5);
}

#[test]
fn corrupt_or_unwritable_note_caches_are_not_silently_ignored() {
    let directory = tempfile::tempdir().unwrap();
    let corrupt = directory.path().join("corrupt.json");
    std::fs::write(&corrupt, b"{not json").unwrap();
    let error = UpdateService::for_build_with_cache("unused", "owner/repo", "1.0.0", &corrupt)
        .err()
        .expect("the corrupt cache must fail");
    assert!(matches!(error, aidget::common::error::Error::Json(_)));

    let cache = directory.path().join("blocked/releases.json");
    let body = release_list(&[("v2.0.0", "two", false, false)]);
    let base = super::serve_once(200, &body);
    let service =
        UpdateService::for_build_with_cache(&base, "owner/repo", "1.0.0", &cache).unwrap();
    std::fs::write(directory.path().join("blocked"), b"not a directory").unwrap();

    let failed = block_on(service.check(None));
    assert_eq!(failed.state, UpdateState::Error);
    assert!(failed.error.is_some());
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
        block_on(service.download()).staged,
        "the download is installable"
    );
    // The window decides the Update action from this, whatever the state is.
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

    let failed = block_on(service.download());

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
    block_on(service.download());

    // The test binary does not run from a bundle the updater can replace, so this
    // install fails the way a real one would on a read-only or unusual install.
    let failed = block_on(service.install());

    assert_eq!(failed.snapshot().state, UpdateState::Error, "{failed:?}");
    assert!(
        failed.snapshot().staged,
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
    assert_eq!(block_on(service.download()).state, UpdateState::Ready);

    let rechecked = block_on(service.check(None));

    assert_eq!(rechecked.state, UpdateState::Ready, "{rechecked:?}");
    assert_eq!(rechecked.error, None);

    if let Some(staged) = service.staged() {
        super::cleanup(staged);
    }
}

#[test]
fn every_published_state_reaches_the_listener() {
    // The launch check runs without anyone asking, so the window learns about it
    // through this listener rather than by polling from `idle`.
    let base = super::serve_once(
        200,
        r#"{"tag_name": "v99.0.0", "html_url": "", "assets": []}"#,
    );
    let service = UpdateService::for_build(&base, "owner/repo", "0.1.0");
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = seen.clone();
    service.announce(move |snapshot| sink.lock().unwrap().push(snapshot.state.clone()));

    block_on(service.check(None));

    let states = seen.lock().unwrap().clone();
    assert!(states.contains(&UpdateState::Checking), "{states:?}");
    assert!(states.contains(&UpdateState::Found), "{states:?}");
}

#[test]
fn an_install_refused_during_check_does_not_change_the_active_operation() {
    let base = super::serve_once(200, r#"{"tag_name":"v99.0.0","html_url":"","assets":[]}"#);
    let service = UpdateService::for_build(&base, "owner/repo", "0.1.0");
    let (entered, waiting) = std::sync::mpsc::channel();
    let (resume, resumed) = std::sync::mpsc::channel();
    let resumed = std::sync::Mutex::new(resumed);
    service.announce(move |snapshot| {
        if snapshot.state == UpdateState::Checking {
            entered.send(()).unwrap();
            resumed.lock().unwrap().recv().unwrap();
        }
    });
    let checking = service.clone();
    let thread = std::thread::spawn(move || block_on(checking.check(None)));
    waiting.recv().unwrap();
    let refused = block_on(service.install());
    assert_eq!(refused.snapshot().state, UpdateState::Checking);
    assert_eq!(service.snapshot().state, UpdateState::Checking);
    resume.send(()).unwrap();
    assert_eq!(thread.join().unwrap().state, UpdateState::Found);
}

#[test]
fn user_apply_rechecks_and_keeps_a_downloaded_update_when_the_check_fails() {
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
    assert_eq!(block_on(service.download()).state, UpdateState::Ready);

    let outcome = block_on(service.apply(None));

    assert!(matches!(
        outcome,
        aidget::update::service::InstallOutcome::NotStarted(_)
    ));
    assert_eq!(outcome.snapshot().state, UpdateState::Ready);
    assert!(outcome
        .snapshot()
        .error
        .as_deref()
        .is_some_and(|error| error.contains("rate limit")));
    assert!(service.staged().is_some());
    cleanup(&service);
}

#[test]
fn user_apply_installs_the_same_staged_version_after_rechecking() {
    let base = release_with_asset();
    let service = UpdateService::for_build(&base, "owner/repo", "0.1.0");
    block_on(service.check(None));
    assert_eq!(block_on(service.download()).state, UpdateState::Ready);

    let outcome = block_on(service.apply(None));

    assert!(matches!(
        outcome,
        aidget::update::service::InstallOutcome::Failed(_)
    ));
    assert!(
        !outcome
            .snapshot()
            .error
            .as_deref()
            .is_some_and(|error| error.contains("no downloaded update")),
        "{outcome:?}"
    );
    cleanup(&service);
}

#[test]
fn user_apply_downloads_the_newer_release_before_installing() {
    let base = super::serve_release_sequence(vec![
        ("98.0.0", b"old build".as_slice()),
        ("99.0.0", b"new build".as_slice()),
    ]);
    let service = UpdateService::for_build(&base, "owner/repo", "0.1.0");
    block_on(service.check(None));
    assert_eq!(block_on(service.download()).state, UpdateState::Ready);

    let outcome = block_on(service.apply(None));

    assert!(matches!(
        outcome,
        aidget::update::service::InstallOutcome::Failed(_)
    ));
    let staged = service
        .staged()
        .expect("the replacement remains installable");
    assert_eq!(std::fs::read(staged).unwrap(), b"new build");
    cleanup(&service);
}

fn cleanup(service: &UpdateService) {
    if let Some(staged) = service.staged() {
        super::cleanup(staged);
    }
}

fn release_list(releases: &[(&str, &str, bool, bool)]) -> String {
    serde_json::to_string(
        &releases
            .iter()
            .map(|(tag, body, prerelease, draft)| {
                serde_json::json!({
                    "tag_name": tag,
                    "body": body,
                    "draft": draft,
                    "prerelease": prerelease,
                    "html_url": "",
                    "assets": [],
                })
            })
            .collect::<Vec<_>>(),
    )
    .unwrap()
}
