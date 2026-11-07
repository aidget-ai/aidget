//! Wiring the updater into the app: the launch check, the token it may use, and
//! the install that happens as the app quits.
//!
//! Policy that mixes the release feed with the user's settings lives here;
//! `crate::update::UpdateService` knows only about releases and downloads.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use std::time::Duration;

use tauri::{Emitter, Manager};

use crate::runtime::ui;
use crate::settings;
use crate::update::github::PackageKind;
use crate::update::service::InstallOutcome;
use crate::update::{UpdateService, UpdateSnapshot};

/// The event a published snapshot is announced on.
pub const UPDATE_EVENT: &str = "aidget://update";

/// What a normal exit request should do while an update is being replaced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitAction {
    /// A window close, not a quit: the tray app must stay resident.
    Prevent,
    /// A quit arrived while the bundle or package was being replaced.
    Defer(i32),
    /// Nothing is replacing the app, so the quit may proceed.
    Allow,
}

/// Decide whether a quit can proceed, based purely on the request and the
/// replacement gate.
pub fn exit_action(code: Option<i32>, installing: bool) -> ExitAction {
    match code {
        None => ExitAction::Prevent,
        Some(code) if installing => ExitAction::Defer(code),
        Some(_) => ExitAction::Allow,
    }
}

/// A quit requested while the installer was running.
///
/// The first request wins, and the recorded code survives until the install
/// settles. Once released, `install_on_quit` must not retry the installation
/// that just finished: that would make quitting wait for a second attempt.
#[derive(Default)]
pub struct DeferredQuit {
    code: Mutex<Option<i32>>,
    skip_install_on_quit: AtomicBool,
}

impl DeferredQuit {
    /// Remember a quit code unless one is already waiting.
    pub fn defer(&self, code: i32) {
        let mut waiting = self.code.lock().unwrap();
        if waiting.is_none() {
            *waiting = Some(code);
        }
    }

    /// Take the pending quit exactly once.
    pub fn take(&self) -> Option<i32> {
        self.code.lock().unwrap().take()
    }

    /// Consume the instruction to skip the quit-time install retry.
    pub fn skip_install_on_quit(&self) -> bool {
        self.skip_install_on_quit.swap(false, Ordering::SeqCst)
    }
}

/// Whether the service is replacing files right now.
pub fn install_in_progress<R: tauri::Runtime>(handle: &tauri::AppHandle<R>) -> bool {
    handle
        .try_state::<UpdateService>()
        .is_some_and(|service| service.is_installing())
}

/// Record a quit that has to wait for the active installation.
pub fn defer_quit<R: tauri::Runtime>(handle: &tauri::AppHandle<R>, code: i32) {
    if let Some(state) = handle.try_state::<DeferredQuit>() {
        state.defer(code);
    }
    // The install may have settled between the caller's check and this record.
    // Release the quit here instead of waiting for an owner that already left.
    if !install_in_progress(handle) {
        finish_deferred_quit(handle);
    }
}

/// Release a deferred quit after the installation has settled.
///
/// Successful installs leave through `relaunch_and_exit`, so this is reached by
/// a failed install or a failed relaunch. The skip flag prevents the normal
/// `Exit` handler from trying the same installation again.
pub fn finish_deferred_quit<R: tauri::Runtime>(handle: &tauri::AppHandle<R>) {
    let Some(state) = handle.try_state::<DeferredQuit>() else {
        return;
    };
    let Some(code) = state.take() else {
        return;
    };
    state.skip_install_on_quit.store(true, Ordering::SeqCst);
    handle.exit(code);
}

/// Forward every snapshot the service publishes to the main window.
pub fn announce(app: &tauri::AppHandle, service: &UpdateService) {
    let handle = app.clone();
    service.announce(move |snapshot| {
        let _ = handle.emit_to(ui::MAIN_LABEL, UPDATE_EVENT, snapshot);
    });
}

/// Ask the release feed once, shortly after launch.
///
/// One check only: there is no poller or scheduler, so every later check comes
/// from the Check button. A settings file that cannot be read is reported and
/// ends the launch check: without it there is no token and no download policy to
/// apply.
pub fn start_check(app: &tauri::AppHandle) {
    let service = app.state::<UpdateService>().inner().clone();
    if service.is_debug_build() {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // Let the windows and widgets settle before touching the network.
        tokio::time::sleep(Duration::from_secs(5)).await;
        match settings::load(&root_of(&app)) {
            Ok(settings) => {
                let token = settings
                    .github_token
                    .map(|token| token.as_str().to_string());
                let _ = check_on_launch(service, token.as_deref(), settings.auto_download).await;
            }
            Err(error) => eprintln!("aidget: cannot read settings for the update check: {error}"),
        }
    });
}

/// Run the launch policy without loading a debug build's update metadata.
pub async fn check_on_launch(
    service: UpdateService,
    token: Option<&str>,
    auto_download: bool,
) -> Option<UpdateSnapshot> {
    if service.is_debug_build() {
        return None;
    }
    Some(check_and_maybe_download(service, token, auto_download).await)
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
    let snapshot = service.check_with_policy(token, auto_download).await;
    let repository = crate::update::github::REPOSITORY;
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
    if handle
        .try_state::<DeferredQuit>()
        .is_some_and(|state| state.skip_install_on_quit())
    {
        println!("aidget: the deferred installation already settled; quitting");
        return;
    }
    let Some(service) = handle.try_state::<UpdateService>() else {
        return;
    };
    // macOS native quit (Cmd+Q) reaches `Exit` without `ExitRequested`, so it
    // cannot be deferred there. Keep the event loop alive until the replacement
    // has finished; exiting mid-swap is what this guard exists to prevent.
    if service.is_installing() {
        println!("aidget: waiting for the active update installation before quitting");
        service.wait_for_install();
        println!("aidget: the active update installation settled; quitting");
        return;
    }
    if service.staged().is_none() {
        return;
    }
    // Do not request administrator access as the window disappears; keep the
    // download for the Update action instead.
    if needs_administrator() {
        println!(
            "aidget: the downloaded update needs an administrator; it waits for the Update action"
        );
        return;
    }
    match service.install_blocking() {
        InstallOutcome::Completed(..) => {
            println!("aidget: installed the downloaded update on quit")
        }
        InstallOutcome::NotStarted(_) => {
            // A native quit can race the install's claim: the first check saw
            // no replacement, then the claim won. The gate is set inside that
            // claim, so after a lost claim it is safe to read and wait on.
            if service.is_installing() {
                println!("aidget: waiting for the active update installation before quitting");
                service.wait_for_install();
                println!("aidget: the active update installation settled; quitting");
            } else {
                println!("aidget: skipped install on quit: another update operation is active");
            }
        }
        InstallOutcome::Failed(snapshot) => eprintln!(
            "aidget: cannot install on quit: {}",
            snapshot.error.unwrap()
        ),
    }
}

/// Schedule the updated executable and exit only if scheduling succeeds.
///
/// On failure, keep the old process running and report that the installed update
/// needs a manual launch.
pub fn relaunch_and_exit(
    app: &tauri::AppHandle,
    installation: &crate::update::install::Installation,
) -> crate::common::error::Result<()> {
    match installation.relaunch(&root_of(app)) {
        Ok(()) => {
            app.exit(0);
            Ok(())
        }
        Err(error) => Err(crate::common::error::Error::Other(format!(
            "the update is in place but could not be started ({error}); quit and start aidget again"
        ))),
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

#[tauri::command]
pub(crate) fn update_state(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> std::result::Result<UpdateSnapshot, String> {
    ui::require_main_window_label(window.label()).map_err(|error| error.to_string())?;
    Ok(app.state::<UpdateService>().inner().snapshot())
}

#[tauri::command]
pub(crate) async fn update_check(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> std::result::Result<UpdateSnapshot, String> {
    ui::require_main_window_label(window.label()).map_err(|error| error.to_string())?;
    let service = app.state::<UpdateService>().inner().clone();
    let settings = settings::load(&root_of(&app)).map_err(|error| error.to_string())?;
    let token = settings
        .github_token
        .map(|token| token.as_str().to_string());
    Ok(check_and_maybe_download(service, token.as_deref(), settings.auto_download).await)
}

/// Recheck the latest release, then download or install as needed.
#[tauri::command]
pub(crate) async fn update_apply(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> std::result::Result<UpdateSnapshot, String> {
    ui::require_main_window_label(window.label()).map_err(|error| error.to_string())?;
    let service = app.state::<UpdateService>().inner().clone();
    let settings = settings::load(&root_of(&app)).map_err(|error| error.to_string())?;
    let token = settings
        .github_token
        .map(|token| token.as_str().to_string());
    let outcome = service.apply(token.as_deref()).await;
    finish_update_request(&app, outcome)
}

fn finish_update_request(
    app: &tauri::AppHandle,
    outcome: InstallOutcome,
) -> std::result::Result<UpdateSnapshot, String> {
    let completed = matches!(&outcome, InstallOutcome::Completed(..));
    let failed = matches!(&outcome, InstallOutcome::Failed(_));
    let result = complete_install(outcome, |installation| relaunch_and_exit(app, installation));
    if failed || (completed && result.is_err()) {
        // The installer has stopped, so a quit that arrived while it ran can
        // now finish. `NotStarted` is deliberately absent: another request
        // still owns the replacement and will release the quit itself.
        finish_deferred_quit(&app);
    }
    result.map_err(|error| error.to_string())
}

/// Authorize process exit only for the request that completed installation.
pub(crate) fn complete_install(
    outcome: InstallOutcome,
    relaunch: impl FnOnce(&crate::update::install::Installation) -> crate::Result<()>,
) -> crate::Result<UpdateSnapshot> {
    match outcome {
        InstallOutcome::Completed(snapshot, installation) => {
            relaunch(&installation)?;
            Ok(snapshot)
        }
        InstallOutcome::NotStarted(snapshot) | InstallOutcome::Failed(snapshot) => Ok(snapshot),
    }
}
