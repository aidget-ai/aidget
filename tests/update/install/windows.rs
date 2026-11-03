use aidget::update::install::windows::installer_args;

#[test]
fn an_update_restarts_the_app_through_the_installer() {
    // The flags Tauri's own updater passes to its NSIS installer.
    assert_eq!(installer_args(true, &[]), "/S /UPDATE /R");
}

#[test]
fn an_update_installed_while_quitting_does_not_start_the_app_again() {
    assert_eq!(installer_args(false, &[]), "/S /UPDATE");
}

#[test]
fn nsis_restarts_with_the_selected_home_and_preserves_a_trailing_separator() {
    let args = vec!["--home".into(), r"C:\data space\".into()];
    assert_eq!(
        installer_args(true, &args),
        r#"/S /UPDATE /R /ARGS "--home" "C:\data space\\""#
    );
    assert_eq!(installer_args(false, &args), "/S /UPDATE");
}
