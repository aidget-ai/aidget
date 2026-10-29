//! Wiring the updater into the app: the launch check, the token it may use, and
//! the install that happens as the app quits.
//!
//! Policy that mixes the release feed with the user's settings lives here;
//! `crate::update::UpdateService` knows only about releases and downloads.

use std::time::Duration;

use tauri::Manager;

use crate::runtime::ui;
use crate::settings;
use crate::update::github::PackageKind;
use crate::update::{UpdateService, UpdateSnapshot, UpdateState};

/// Ask the release feed once, shortly after launch.
///
/// One check only: there is no poller or scheduler, so every later check comes
/// from the Check button. A settings file that cannot be read is reported and
/// ends the launch check: without it there is no token and no download policy to
/// apply.
pub fn start_check(app: &tauri::AppHandle) {
    let service = app.state::<UpdateService>().inner().clone();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // Let the windows and widgets settle before touching the network.
        tokio::time::sleep(Duration::from_secs(5)).await;
        match settings::load(&root_of(&app)) {
            Ok(settings) => {
                let token = settings
                    .github_token
                    .map(|token| token.as_str().to_string());
                check_and_maybe_download(service, token.as_deref(), settings.auto_download).await;
            }
            Err(error) => eprintln!("aidget: cannot read settings for the update check: {error}"),
        }
    });
}

/// Run one check, then download when a newer release was found and automatic
/// downloading is on.
///
/// `token` is the user's GitHub token when configured: 5000 requests an hour
/// instead of 60, which is the difference between "updates work" and "GitHub rate
/// limit reached" behind a shared address.
pub async fn check_and_maybe_download(
    service: UpdateService,
    token: Option<&str>,
    auto_download: bool,
) -> UpdateSnapshot {
    let started = std::time::Instant::now();
    let mut snapshot = service.check(token).await;
    if snapshot.state == UpdateState::Found && auto_download {
        snapshot = service.download(token).await;
    }
    let repository = crate::update::github::repository();
    match &snapshot.error {
        Some(error) => println!(
            "aidget: update check {:?} from {repository} in {:?}: {error}",
            snapshot.state,
            started.elapsed()
        ),
        None => println!(
            "aidget: update check {:?} from {repository} in {:?}",
            snapshot.state,
            started.elapsed()
        ),
    }
    snapshot
}

/// Swap a downloaded update in as the app quits.
///
/// Goes through the service so a failure lands on the same `Error` state and the
/// same message as an install the user asked for.
pub fn install_on_quit(handle: &tauri::AppHandle) {
    let Some(service) = handle.try_state::<UpdateService>() else {
        return;
    };
    if service.staged().is_none() {
        return;
    }
    // A distribution package is installed with the administrator's password.
    // Asking for it as the window disappears is worse than keeping the download:
    // Magpie does not ask on quit either, and the Install button tries again.
    if needs_administrator() {
        println!(
            "aidget: the downloaded update needs an administrator; it waits for the Install button"
        );
        return;
    }
    let snapshot = service.install_blocking();
    match &snapshot.error {
        Some(error) => eprintln!("aidget: cannot install on quit: {error}"),
        None => println!("aidget: installed the downloaded update on quit"),
    }
}

/// Start the version that was just swapped in, then leave.
pub fn relaunch_and_exit(app: &tauri::AppHandle) {
    match crate::update::install::relaunch() {
        Ok(()) => app.exit(0),
        Err(error) => eprintln!("aidget: cannot relaunch: {error}"),
    }
}

/// Whether installing what was downloaded needs elevation.
fn needs_administrator() -> bool {
    matches!(
        crate::update::target::current().map(|target| target.kind),
        Some(PackageKind::Deb | PackageKind::Rpm)
    )
}

/// Where the app's data directory is.
pub fn root_of(app: &tauri::AppHandle) -> std::path::PathBuf {
    app.state::<ui::UiState>().root.lock().unwrap().clone()
}
