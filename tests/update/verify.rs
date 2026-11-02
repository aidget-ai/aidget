use aidget::update::github::Asset;
use aidget::update::verify::{expected, parse_sums};

fn asset(name: &str, digest: Option<&str>) -> Asset {
    Asset {
        name: name.to_string(),
        browser_download_url: "https://example.invalid/asset".to_string(),
        size: 1,
        digest: digest.map(|digest| digest.to_string()),
    }
}

const HEX: &str = "7f73085d000f823b272aa378801500cea6da802a8045b5c12c5b7fed06a202d2";

#[test]
fn a_sha256_digest_is_read_from_the_asset() {
    let asset = asset(
        "aidget_v0.1.1_aarch64.app.zip",
        Some(&format!("sha256:{HEX}")),
    );

    assert_eq!(expected(&asset, None).unwrap(), HEX);
}

#[test]
fn a_digest_that_is_not_a_sha256_is_refused() {
    let asset = asset("aidget_v0.1.1_aarch64.app.zip", Some("md5:abc"));

    assert!(expected(&asset, None).is_err());
}

#[test]
fn without_a_digest_the_sums_file_is_used() {
    let asset = asset("aidget_v0.1.1_aarch64.app.zip", None);
    let sums = format!("{HEX}  aidget_v0.1.1_aarch64.app.zip\n");

    assert_eq!(expected(&asset, Some(&sums)).unwrap(), HEX);
}

#[test]
fn a_sums_file_without_the_asset_is_refused() {
    let asset = asset("aidget_v0.1.1_aarch64.app.zip", None);
    let sums = format!("{HEX}  aidget_v0.1.1_x64.app.zip\n");

    assert!(expected(&asset, Some(&sums)).is_err());
}

#[test]
fn neither_a_digest_nor_a_sums_file_is_refused() {
    let asset = asset("aidget_v0.1.1_aarch64.app.zip", None);

    assert!(expected(&asset, None).is_err());
}

#[test]
fn the_sums_file_is_read_as_hex_and_name_pairs() {
    let sums = format!(
        "{HEX}  aidget_v0.1.1_aarch64.app.zip\n\
         00aa  *aidget_v0.1.1_x64.app.zip\n\
         \n\
         malformed\n"
    );

    let parsed = parse_sums(&sums);

    assert_eq!(
        parsed
            .get("aidget_v0.1.1_aarch64.app.zip")
            .map(String::as_str),
        Some(HEX)
    );
    assert_eq!(
        parsed.get("aidget_v0.1.1_x64.app.zip").map(String::as_str),
        Some("00aa")
    );
    assert_eq!(parsed.len(), 2);
}
