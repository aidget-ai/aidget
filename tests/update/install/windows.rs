use aidget::update::install::windows::installer_args;

#[test]
fn an_update_restarts_the_app_through_the_installer() {
    // The flags Tauri's own updater passes to its NSIS installer.
    assert_eq!(installer_args(true), ["/S", "/UPDATE", "/R"]);
}

#[test]
fn an_update_installed_while_quitting_does_not_start_the_app_again() {
    assert_eq!(installer_args(false), ["/S", "/UPDATE"]);
}
