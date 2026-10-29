use aidget::update::version::{is_newer, is_released};

#[test]
fn a_leading_v_is_stripped() {
    assert!(!is_newer("v0.1.1", "0.1.1"));
    assert!(is_newer("v0.1.1", "0.1.0"));
}

#[test]
fn build_metadata_does_not_affect_the_order() {
    assert!(!is_newer("0.1.1+sha.5d0c1dc", "0.1.1"));
}

#[test]
fn a_patch_bump_is_newer_and_an_equal_version_is_not() {
    assert!(is_newer("0.1.1", "0.1.0"));
    assert!(!is_newer("0.1.1", "0.1.1"));
    assert!(!is_newer("0.1.0", "0.1.1"));
}

#[test]
fn numbers_compare_numerically_not_as_text() {
    assert!(is_newer("0.10.0", "0.9.0"));
    assert!(is_newer("0.1.10", "0.1.9"));
}

#[test]
fn a_pre_release_sorts_below_the_release_it_leads_to() {
    assert!(is_newer("0.1.0", "0.1.0-beta.1"));
    assert!(!is_newer("0.1.0-beta.1", "0.1.0"));
}

#[test]
fn pre_release_identifiers_compare_the_way_semver_says() {
    assert!(is_newer("0.1.0-beta.2", "0.1.0-beta.1"));
    assert!(is_newer("0.1.0-beta.10", "0.1.0-beta.9"));
    assert!(is_newer("0.1.0-beta.1", "0.1.0-alpha.1"));
    assert!(is_newer("0.1.0-beta.1.1", "0.1.0-beta.1"));
    assert!(is_newer("0.1.0-alpha.1", "0.1.0-alpha"));
}

#[test]
fn a_number_sorts_before_a_word() {
    assert!(is_newer("0.1.0-alpha.a", "0.1.0-alpha.1"));
}

#[test]
fn a_version_without_three_numbers_is_rejected() {
    for text in ["1.0", "0.1.0.1", "", "abc", "0.1.x", "0..1"] {
        assert!(!is_newer(text, "0.1.0"), "{text}");
        assert!(!is_released(text), "{text}");
    }
}

#[test]
fn unparseable_versions_never_count_as_an_update() {
    assert!(!is_newer("0.1.1", "dev"));
    assert!(!is_newer("dev", "0.1.0"));
    assert!(!is_newer("0.1.1", ""));
}

#[test]
fn a_git_describe_build_is_still_not_a_release() {
    assert!(!is_released("0.1.0-3-gabc1234"));
}

#[test]
fn only_release_tags_update_themselves() {
    assert!(is_released("0.1.0"));
    assert!(is_released("v0.1.0"));
    assert!(is_released("0.1.0-beta.1"));
    assert!(!is_released("dev"));
    assert!(!is_released("0.1.0-3-g0bcb2cc"));
    assert!(!is_released("0.1.0-dirty"));
    assert!(!is_released("0.1.0-3-g0bcb2cc-dirty"));
}
