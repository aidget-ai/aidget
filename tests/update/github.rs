#[cfg(debug_assertions)]
use aidget::update::github::DEV_REPOSITORY;
use aidget::update::github::{repository, Asset, PackageKind, Release, REPOSITORY};
use aidget::update::target::Target;

fn asset(name: &str) -> Asset {
    Asset {
        name: name.to_string(),
        browser_download_url: format!("https://example.invalid/{name}"),
        size: 1,
        digest: Some("sha256:00".to_string()),
    }
}

fn release(names: &[&str]) -> Release {
    Release {
        tag_name: "v0.1.1".to_string(),
        body: "notes".to_string(),
        html_url: "https://example.invalid/release".to_string(),
        assets: names.iter().map(|name| asset(name)).collect(),
    }
}

fn target(kind: PackageKind, arch: &'static str) -> Target {
    Target { kind, arch }
}

#[test]
#[cfg(debug_assertions)]
fn a_debug_build_reads_the_fork() {
    assert_eq!(repository(), DEV_REPOSITORY);
    assert_ne!(DEV_REPOSITORY, REPOSITORY);
}

#[test]
#[cfg(not(debug_assertions))]
fn a_release_build_reads_upstream() {
    assert_eq!(repository(), REPOSITORY);
}

#[test]
fn reads_the_fields_the_payload_carries() {
    let release: Release = serde_json::from_str(
        r#"{
            "tag_name": "v0.1.1",
            "body": "- faster",
            "html_url": "https://github.com/aidget-ai/aidget/releases/tag/v0.1.1",
            "assets": [
                {
                    "name": "aidget_v0.1.1_aarch64.dmg",
                    "browser_download_url": "https://github.com/aidget-ai/aidget/releases/download/v0.1.1/aidget_v0.1.1_aarch64.dmg",
                    "size": 26512345,
                    "digest": "sha256:abc123"
                }
            ]
        }"#,
    )
    .unwrap();

    assert_eq!(release.tag_name, "v0.1.1");
    assert_eq!(release.body, "- faster");
    assert_eq!(release.assets.len(), 1);
    assert_eq!(release.assets[0].size, 26_512_345);
    assert_eq!(release.assets[0].digest.as_deref(), Some("sha256:abc123"));
}

#[test]
fn a_null_digest_and_a_missing_body_are_accepted() {
    let release: Release = serde_json::from_str(
        r#"{
            "tag_name": "v0.1.1",
            "html_url": "https://example.invalid/release",
            "assets": [
                {
                    "name": "aidget_v0.1.1_aarch64.dmg",
                    "browser_download_url": "https://example.invalid/a.dmg",
                    "size": 1,
                    "digest": null
                }
            ]
        }"#,
    )
    .unwrap();

    assert_eq!(release.body, "");
    assert_eq!(release.assets[0].digest, None);
}

#[test]
fn selects_the_asset_for_the_running_architecture() {
    let release = release(&[
        "aidget_v0.1.1_aarch64.dmg",
        "aidget_v0.1.1_x64.dmg",
        "aidget_v0.1.1_aarch64.app.zip",
        "aidget_v0.1.1_x64.app.zip",
    ]);

    let arm = release
        .asset_for(target(PackageKind::MacApp, "aarch64"))
        .unwrap();
    let intel = release
        .asset_for(target(PackageKind::MacApp, "x64"))
        .unwrap();

    assert_eq!(arm.name, "aidget_v0.1.1_aarch64.app.zip");
    assert_eq!(intel.name, "aidget_v0.1.1_x64.app.zip");
}

#[test]
fn never_hands_out_a_dmg_as_an_app_zip() {
    let release = release(&["aidget_v0.1.1_aarch64.dmg"]);

    assert!(release
        .asset_for(target(PackageKind::MacApp, "aarch64"))
        .is_none());
}

#[test]
fn the_name_must_end_with_the_token_and_suffix_exactly() {
    let release = release(&["aidget_v0.1.1_aarch64.app.zip.bak"]);

    assert!(release
        .asset_for(target(PackageKind::MacApp, "aarch64"))
        .is_none());
}

#[test]
fn an_architecture_token_inside_the_version_does_not_select_it() {
    let release = release(&["aidget_v0.1.1-x64_aarch64.app.zip"]);

    let found = release
        .asset_for(target(PackageKind::MacApp, "aarch64"))
        .unwrap();

    assert_eq!(found.name, "aidget_v0.1.1-x64_aarch64.app.zip");
    assert!(release
        .asset_for(target(PackageKind::MacApp, "x64"))
        .is_none());
}

#[test]
fn selects_the_windows_installer() {
    let release = release(&["aidget_v0.1.1_x64-setup.exe"]);

    let asset = release
        .asset_for(target(PackageKind::WindowsSetup, "x64"))
        .unwrap();

    assert_eq!(asset.name, "aidget_v0.1.1_x64-setup.exe");
}

#[test]
fn selects_the_linux_packages_by_suffix() {
    let release = release(&[
        "aidget_v0.1.1_amd64.AppImage",
        "aidget_v0.1.1_amd64.deb",
        "aidget_v0.1.1_amd64.rpm",
    ]);

    for (kind, expected) in [
        (PackageKind::AppImage, "aidget_v0.1.1_amd64.AppImage"),
        (PackageKind::Deb, "aidget_v0.1.1_amd64.deb"),
        (PackageKind::Rpm, "aidget_v0.1.1_amd64.rpm"),
    ] {
        assert_eq!(
            release.asset_for(target(kind, "amd64")).unwrap().name,
            expected
        );
    }
}

#[test]
fn a_release_without_the_wanted_asset_selects_nothing() {
    assert!(release(&["aidget_v0.1.1_x64.deb"])
        .asset_for(target(PackageKind::MacApp, "aarch64"))
        .is_none());
}

#[test]
fn a_rate_limited_answer_says_how_long_to_wait() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("x-ratelimit-remaining", "0".parse().unwrap());
    headers.insert(
        "x-ratelimit-reset",
        (now + 600).to_string().parse().unwrap(),
    );

    let message = aidget::update::github::describe_failure(
        403,
        &headers,
        r#"{"message":"API rate limit exceeded for 1.2.3.4."}"#,
    );

    assert!(message.contains("rate limit"), "{message}");
    assert!(message.contains("minute"), "{message}");
}

#[test]
fn a_repository_with_nothing_published_is_not_an_error() {
    let base = super::serve_once(404, r#"{"message":"Not Found"}"#);

    let release = aidget::update::github::latest(&base, "owner/repo", None).unwrap();

    assert!(release.is_none(), "an empty repository answers None");
}

#[test]
fn a_404_with_another_message_is_still_a_failure() {
    let base = super::serve_once(404, r#"{"message":"Not Found for another reason"}"#);

    let error = aidget::update::github::latest(&base, "owner/repo", None).unwrap_err();

    assert!(error.to_string().contains("HTTP 404"), "{error}");
}

#[test]
fn a_published_release_is_read_from_the_feed() {
    let base = super::serve_once(
        200,
        r#"{"tag_name": "v0.2.0", "html_url": "", "assets": [
            {"name": "aidget_0.2.0_aarch64.app.zip", "browser_download_url": "https://example.invalid/a", "size": 1}
        ]}"#,
    );

    let release = aidget::update::github::latest(&base, "owner/repo", None)
        .unwrap()
        .expect("a published release");

    assert_eq!(release.tag_name, "v0.2.0");
}

#[test]
fn a_plain_failure_reports_the_status_and_the_server_message() {
    let headers = reqwest::header::HeaderMap::new();

    let message = aidget::update::github::describe_failure(
        422,
        &headers,
        r#"{"message":"Validation Failed"}"#,
    );

    assert_eq!(message, "HTTP 422: Validation Failed");
}
