use aidget::update::github::{Asset, PackageKind, Release};
use aidget::update::stage::prepare;
use aidget::update::target::Target;

fn release(names: &[&str]) -> Release {
    Release {
        tag_name: "v0.2.0".to_string(),
        body: String::new(),
        html_url: "https://example.invalid/release".to_string(),
        assets: names
            .iter()
            .map(|name| Asset {
                name: name.to_string(),
                browser_download_url: format!("https://example.invalid/{name}"),
                size: 7,
                digest: None,
            })
            .collect(),
    }
}

fn target(kind: PackageKind, arch: &'static str) -> Target {
    Target { kind, arch }
}

#[test]
fn the_download_lands_under_the_asset_name_in_a_directory_of_its_own() {
    let release = release(&["aidget_0.2.0_aarch64.app.zip", "SHA256SUMS"]);

    let prepared = prepare(&release, target(PackageKind::MacApp, "aarch64"), None).unwrap();

    assert_eq!(
        prepared.path.file_name().unwrap(),
        "aidget_0.2.0_aarch64.app.zip"
    );
    let dir = prepared.path.parent().unwrap();
    assert!(dir.exists(), "the directory is ready for the download");
    assert_eq!(
        prepared.sums_url.as_deref(),
        Some("https://example.invalid/SHA256SUMS")
    );

    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_previous_download_is_dropped_before_the_next_one_starts() {
    let release = release(&["aidget_0.2.0_aarch64.app.zip"]);
    let first = prepare(&release, target(PackageKind::MacApp, "aarch64"), None).unwrap();
    let first_dir = first.path.parent().unwrap().to_path_buf();
    std::fs::write(&first.path, b"stale").unwrap();

    let second = prepare(
        &release,
        target(PackageKind::MacApp, "aarch64"),
        Some(&first.path),
    )
    .unwrap();

    assert!(!first_dir.exists(), "the stale download is gone");
    assert_ne!(second.path.parent().unwrap(), first_dir);

    std::fs::remove_dir_all(second.path.parent().unwrap()).unwrap();
}

#[test]
fn a_release_without_this_platforms_asset_is_refused() {
    let release = release(&["aidget_0.2.0_x64-setup.exe"]);

    let error = prepare(&release, target(PackageKind::MacApp, "aarch64"), None).unwrap_err();

    assert!(
        error.to_string().contains("has no .app.zip asset"),
        "{error}"
    );
}
