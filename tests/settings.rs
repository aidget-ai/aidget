use std::fs;

use aidget::settings::{
    ensure, load, save_github_token, save_preferences, GithubToken, Language, MainPage, Theme,
    TrayClickAction, UiPreferences, SETTINGS_FILE,
};
use tempfile::TempDir;

fn root() -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let root = dir.path().to_path_buf();
    ensure(&root).unwrap();
    (dir, root)
}

#[test]
fn settings_are_created_with_retention_and_preference_defaults() {
    let (_dir, root) = root();
    assert_eq!(SETTINGS_FILE, "settings.toml");

    let settings = load(&root).unwrap();
    assert_eq!(settings.screenshot_retention.get(), 30);
    assert_eq!(settings.trash_retention.get(), 20);
    assert_eq!(settings.theme, Theme::System);
    assert_eq!(settings.language, Language::System);
    assert_eq!(settings.tray_click_action, TrayClickAction::Window);
    assert_eq!(settings.last_page, MainPage::Integrations);
    assert_eq!(settings.github_token, None);

    let document = fs::read_to_string(root.join(SETTINGS_FILE)).unwrap();
    assert!(document.contains("screenshot_retention = 30"));
    assert!(document.contains("trash_retention = 20"));
    assert!(document.contains("theme = \"system\""));
    assert!(document.contains("language = \"system\""));
    assert!(document.contains("tray_click_action = \"window\""));
    assert!(document.contains("last_page = \"integrations\""));
    assert!(document.contains("auto_download = true"));
    assert!(settings.auto_download, "downloading unasked is the default");
}

#[test]
fn a_settings_file_without_the_auto_download_key_downloads_unasked() {
    let (dir, root) = root();
    // What a file written before the updater existed looks like.
    fs::write(
        root.join(SETTINGS_FILE),
        "screenshot_retention = 30\ntrash_retention = 20\n",
    )
    .unwrap();

    assert!(load(&root).unwrap().auto_download);
    ensure(&root).unwrap();
    assert!(fs::read_to_string(root.join(SETTINGS_FILE))
        .unwrap()
        .contains("auto_download = true"));
    drop(dir);
}

#[test]
fn ensure_adds_missing_preferences_without_losing_retention_comments() {
    let (dir, root) = root();
    let path = root.join(SETTINGS_FILE);
    let custom = "\
# Keep enough screenshots for the current debugging session.\n\
screenshot_retention = 12\n\
\n\
# Pruned widgets remain recoverable for a shorter period.\n\
trash_retention = 7\n";
    fs::write(&path, custom).unwrap();

    ensure(&root).unwrap();

    let document = fs::read_to_string(&path).unwrap();
    assert!(document.contains("# Keep enough screenshots"));
    assert!(document.contains("screenshot_retention = 12"));
    assert!(document.contains("# Pruned widgets remain"));
    assert!(document.contains("trash_retention = 7"));
    assert!(document.contains("theme = \"system\""));
    assert!(document.contains("language = \"system\""));
    assert!(document.contains("tray_click_action = \"window\""));
    assert!(document.contains("last_page = \"integrations\""));
    drop(dir);
}

#[test]
fn saving_preferences_preserves_retention_and_unrelated_comments() {
    let (dir, root) = root();
    let path = root.join(SETTINGS_FILE);
    fs::write(
        &path,
        "\
# Captured screenshots are retained briefly.\n\
screenshot_retention = 9\n\
trash_retention = 4\n\
\n\
# The UI can follow the operating system.\n\
theme = \"system\"\n\
language = \"system\"\n\
tray_click_action = \"window\"\n",
    )
    .unwrap();

    let preferences = UiPreferences {
        theme: Theme::Dark,
        language: Language::ZhCn,
        tray_click_action: TrayClickAction::Menu,
        last_page: MainPage::About,
        auto_download: false,
    };
    save_preferences(&root, preferences).unwrap();

    let document = fs::read_to_string(&path).unwrap();
    assert!(document.contains("# Captured screenshots are retained briefly."));
    assert!(document.contains("# The UI can follow the operating system."));
    assert!(document.contains("screenshot_retention = 9"));
    assert!(document.contains("trash_retention = 4"));
    assert!(document.contains("theme = \"dark\""));
    assert!(document.contains("language = \"zh-CN\""));
    assert!(document.contains("tray_click_action = \"menu\""));
    assert!(document.contains("last_page = \"about\""));
    assert!(document.contains("auto_download = false"));
    assert_eq!(load(&root).unwrap().preferences(), preferences);
    drop(dir);
}

#[test]
fn saving_and_removing_a_github_token_preserves_other_settings() {
    let (dir, root) = root();
    let path = root.join(SETTINGS_FILE);
    let token = GithubToken::parse("samplevalueABCDEFGHIJKLMNOPQRSTUVWXYZ").unwrap();

    save_github_token(&root, Some(&token)).unwrap();

    let document = fs::read_to_string(&path).unwrap();
    assert!(document.contains("github_token = \"samplevalueABCDEFGHIJKLMNOPQRSTUVWXYZ\""));
    assert!(document.contains("# Maximum number of screenshots"));
    assert!(document.contains("screenshot_retention = 30"));
    assert!(document.contains("last_page = \"integrations\""));
    assert_eq!(load(&root).unwrap().github_token, Some(token));

    save_github_token(&root, None).unwrap();

    let document = fs::read_to_string(&path).unwrap();
    assert!(!document.contains("github_token"));
    assert!(document.contains("# Maximum number of screenshots"));
    assert!(document.contains("trash_retention = 20"));
    assert!(document.contains("theme = \"system\""));
    assert_eq!(load(&root).unwrap().github_token, None);
    drop(dir);
}

#[cfg(unix)]
#[test]
fn settings_writes_use_owner_only_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().unwrap();
    let root = dir.path().to_path_buf();
    let path = root.join(SETTINGS_FILE);
    ensure(&root).unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );

    let preferences = UiPreferences {
        theme: Theme::Dark,
        language: Language::ZhCn,
        tray_click_action: TrayClickAction::Menu,
        last_page: MainPage::About,
        auto_download: true,
    };
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    save_preferences(&root, preferences).unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );

    let token = GithubToken::parse("samplevalueABCDEFGHIJKLMNOPQRSTUVWXYZ").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    save_github_token(&root, Some(&token)).unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );

    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    save_github_token(&root, None).unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn github_tokens_reject_ambiguous_or_unsafe_values_and_redact_debug_output() {
    let token = GithubToken::parse("  samplevalueABCDEFGHIJKLMNOPQRSTUVWXYZ  ").unwrap();
    assert_eq!(token.masked(), "ghp_…WXYZ");
    assert!(!format!("{token:?}").contains("samplevalueABCDEFGHIJKLMNOPQRSTUVWXYZ"));

    for invalid in [
        "",
        "short",
        "ghp_1234567890 abcdefghijklmnopqrstuvwxyz",
        "ghp_1234567890\nabcdefghijklmnopqrstuvwxyz",
        "ghp_1234567890\tabcdefghijklmnopqrstuvwxyz",
    ] {
        assert!(GithubToken::parse(invalid).is_err(), "{invalid:?}");
    }
    assert!(GithubToken::parse("x".repeat(513)).is_err());
}

#[test]
fn settings_reject_invalid_retention_or_preference_shapes() {
    let (dir, root) = root();
    let path = root.join(SETTINGS_FILE);
    for invalid in [
        "",
        "screenshot_retention = 30\ntrash_retention = 0\n",
        "screenshot_retention = 30\ntrash_retention = 20\nunknown = true\n",
        "screenshot_retention = 30\ntrash_retention = 20\ntheme = \"sepia\"\n",
        "screenshot_retention = 30\ntrash_retention = 20\nlanguage = \"fr\"\n",
        "screenshot_retention = 30\ntrash_retention = 20\ntray_click_action = \"popup\"\n",
        "screenshot_retention = 30\ntrash_retention = 20\nlast_page = \"settings\"\n",
        "not = [valid",
    ] {
        fs::write(&path, invalid).unwrap();
        assert!(load(&root).is_err(), "{invalid}");
    }
    drop(dir);
}
