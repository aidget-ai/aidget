use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::common::error::{Error, Result};
use crate::update::github::{self, Release};
mod install;
use crate::update::snapshot::{notes_of, UpdateSnapshot, UpdateState};
use crate::update::stage::{self, Download};
use crate::update::target::Target;
use crate::update::{target, version};
pub use install::InstallOutcome;

/// Holds the outcome of the last check.
///
/// Cloning shares the same snapshot, so an async command can take an owned copy
/// before awaiting instead of holding the Tauri state borrow across it.
#[derive(Clone)]
pub struct UpdateService {
    snapshot: Arc<Mutex<UpdateSnapshot>>,
    /// Where the release feed is read from; production uses [`github::API_BASE`].
    base: Arc<str>,
    /// The `owner/repo` the feed is read for.
    repository: Arc<str>,
    /// What this build is; see [`build_stamp`].
    build: Arc<str>,
    /// The release the last check found, kept so a download needs no second
    /// lookup.
    release: Arc<Mutex<Option<Release>>>,
    /// A verified download waiting to install.
    staged: Arc<Mutex<Option<Staged>>>,
    /// Where a published snapshot is reported.
    ///
    /// The launch check runs without anyone asking, so the window has to be told
    /// when it settles — otherwise it keeps showing what it read before the check
    /// started.
    announce: Arc<Mutex<Option<Sink>>>,
}

impl Default for UpdateService {
    fn default() -> Self {
        UpdateService::new()
    }
}

impl UpdateService {
    pub fn new() -> Self {
        UpdateService::for_build(
            github::API_BASE,
            github::repository(),
            version::build_stamp(),
        )
    }

    /// A service reading `base` for `repository`, claiming to run as `build`.
    ///
    /// Unit tests use it to run against a local server and to choose whether the
    /// build counts as a release, instead of depending on the working tree.
    pub fn for_build(base: &str, repository: &str, build: &str) -> Self {
        UpdateService {
            snapshot: Arc::new(Mutex::new(UpdateSnapshot {
                state: UpdateState::Idle,
                current: build.to_string(),
                latest: None,
                error: None,
                done: None,
                total: None,
                notes: None,
                staged: false,
            })),
            base: Arc::from(base),
            repository: Arc::from(repository),
            build: Arc::from(build),
            release: Arc::new(Mutex::new(None)),
            staged: Arc::new(Mutex::new(None)),
            announce: Arc::new(Mutex::new(None)),
        }
    }

    pub fn snapshot(&self) -> UpdateSnapshot {
        let mut snapshot = self.snapshot.lock().unwrap().clone();
        // Derived, not stored: the download on disk is the fact, and every
        // published snapshot has to agree with it.
        snapshot.staged = self.staged().is_some();
        snapshot
    }

    /// The verified download waiting to install, if there is one.
    pub fn staged(&self) -> Option<PathBuf> {
        self.staged
            .lock()
            .unwrap()
            .as_ref()
            .map(|staged| staged.path.clone())
    }

    /// The version that download carries.
    fn staged_version(&self) -> Option<String> {
        self.staged
            .lock()
            .unwrap()
            .as_ref()
            .map(|staged| staged.version.clone())
    }

    /// Run one check and return the state it settled in.
    ///
    /// There is no timer behind this: the caller drives it from launch and from
    /// the Check button, as `docs/self-update.md` requires. A check already in
    /// flight is not repeated, so a click during the launch check reads the
    /// running one instead of starting a second request.
    pub async fn check(&self, token: Option<&str>) -> UpdateSnapshot {
        self.check_with_policy(token, false).await
    }

    /// Check with download policy known before replacing a staged release target.
    pub async fn check_with_policy(
        &self,
        token: Option<&str>,
        auto_download: bool,
    ) -> UpdateSnapshot {
        if !self.begin() {
            return self.snapshot();
        }

        let (base, repository) = (self.base.clone(), self.repository.clone());
        let download_token = token.map(str::to_string);
        let token = download_token.clone();
        let fetched = tokio::task::spawn_blocking(move || {
            github::latest(&base, &repository, token.as_deref())
        })
        .await;

        let settled = match fetched {
            Ok(Ok(Some(release))) => self.settle(release, auto_download),
            Ok(Ok(None)) => self.settle_nothing(),
            Ok(Err(error)) => self.fail_check(&error),
            Err(error) => self.fail_check(&Error::Other(format!("update check: {error}"))),
        };
        if settled.state == UpdateState::Found && auto_download {
            // Retain check ownership through the handoff: no install may claim
            // the old staged object between feed settlement and revocation.
            self.set(UpdateSnapshot {
                state: UpdateState::Checking,
                ..settled
            });
            if let Some(prepared) = self.prepare_claimed_download() {
                return self.finish_download(prepared, download_token).await;
            }
        } else {
            self.set(settled);
        }
        self.snapshot()
    }

    /// A failed check, which is about the feed and not about what is already on
    /// disk.
    ///
    /// A downloaded and verified update stays installable: Magpie keeps its
    /// `ready` state across a failed poll for the same reason, and the
    /// Integrations page here keeps its last answer when a refresh fails. The
    /// message is still reported, next to the update rather than instead of it.
    fn fail_check(&self, error: &Error) -> UpdateSnapshot {
        if self.staged().is_some() {
            return UpdateSnapshot {
                state: UpdateState::Ready,
                error: Some(error.to_string()),
                ..self.snapshot()
            };
        }
        self.fail(error)
    }

    /// Download and verify the release the last check found, waiting for it.
    ///
    /// A click drives this regardless of the `auto_download` setting; that
    /// setting only decides whether a check downloads on its own.
    pub async fn download(&self, token: Option<&str>) -> UpdateSnapshot {
        let Some(prepared) = self.begin_download() else {
            return self.snapshot();
        };
        self.finish_download(prepared, token.map(str::to_string))
            .await
    }

    /// Start the same download without waiting for it.
    ///
    /// The snapshot comes back on `Downloading` at once, so the window can show
    /// progress instead of blocking until the file is on disk. This needs a Tokio
    /// runtime to spawn onto: call it from an async command, never from a
    /// synchronous one, which runs on the main thread and aborts the process when
    /// it panics.
    pub fn start_download(&self, token: Option<&str>) -> UpdateSnapshot {
        let Some(prepared) = self.begin_download() else {
            return self.snapshot();
        };
        let service = self.clone();
        let token = token.map(str::to_string);
        tokio::spawn(async move {
            service.finish_download(prepared, token).await;
        });
        self.snapshot()
    }

    /// Claim the download and put the snapshot on `Downloading`.
    ///
    /// `None` means it did not start, and that the snapshot passed to `set` here
    /// already says why: the caller only has to read it back.
    fn begin_download(&self) -> Option<Download> {
        if !self.begin() {
            return None;
        }
        self.prepare_claimed_download()
    }

    fn prepare_claimed_download(&self) -> Option<Download> {
        let target = match self.prepare_allowed() {
            Ok(target) => target,
            Err(error) => {
                self.set(self.fail(&error));
                return None;
            }
        };
        let prepared = match self.prepare_download(target) {
            Ok(prepared) => prepared,
            Err(error) => {
                self.set(self.fail(&error));
                return None;
            }
        };
        self.set(UpdateSnapshot {
            state: UpdateState::Downloading,
            done: Some(0),
            total: Some(prepared.asset.size),
            error: None,
            ..self.snapshot()
        });
        Some(prepared)
    }

    /// A download needs a release to install, and a platform to install it on.
    fn prepare_allowed(&self) -> Result<Target> {
        if self.release.lock().unwrap().is_none() {
            return Err(Error::Other("no release to download yet".to_string()));
        }
        target::current()
            .ok_or_else(|| Error::Other("this install cannot update itself in place".to_string()))
    }

    async fn finish_download(&self, prepared: Download, token: Option<String>) -> UpdateSnapshot {
        let version = prepared.version.clone();
        let progress = self.snapshot.clone();
        let outcome =
            tokio::task::spawn_blocking(move || prepared.run(&progress, token.as_deref())).await;

        let settled = match outcome {
            Ok(Ok(path)) => self.settle_download(path, version),
            Ok(Err(error)) => self.fail(&error),
            Err(error) => self.fail(&Error::Other(format!("download: {error}"))),
        };
        self.set(settled);
        self.snapshot()
    }

    /// What a download needs, resolved from the release the check found.
    fn prepare_download(&self, target: Target) -> Result<Download> {
        let release = self
            .release
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| Error::Other("no release to download yet".to_string()))?;
        let previous = self.staged.lock().unwrap().take();
        if let Some(previous) = previous {
            if let Some(dir) = previous.path.parent() {
                let _ = std::fs::remove_dir_all(dir);
            }
        }
        let prepared = stage::prepare(&release, target, None)?;
        Ok(prepared)
    }

    /// Mark a step as under way, unless one already is.
    fn begin(&self) -> bool {
        {
            let mut snapshot = self.snapshot.lock().unwrap();
            if snapshot.state.in_flight() {
                return false;
            }
            snapshot.state = UpdateState::Checking;
            snapshot.error = None;
            snapshot.done = None;
            snapshot.total = None;
        }
        // Announced like any other publish: a check that starts without a caller
        // asking is exactly the one the window cannot discover by itself.
        self.notify();
        true
    }

    /// Settle when the repository has published nothing at all.
    ///
    /// Nothing newer can exist, so this is not a failure: the state says so
    /// quietly, and a download already on disk stays installable.
    fn settle_nothing(&self) -> UpdateSnapshot {
        if self.staged().is_some() {
            return UpdateSnapshot {
                state: UpdateState::Ready,
                error: None,
                ..self.snapshot()
            };
        }
        *self.release.lock().unwrap() = None;
        UpdateSnapshot {
            state: UpdateState::NoRelease,
            latest: None,
            error: None,
            notes: None,
            ..self.snapshot()
        }
    }

    /// Decide what a fetched release means for this build.
    fn settle(&self, release: Release, auto_download: bool) -> UpdateSnapshot {
        let latest = version::normalised(&release.tag_name);
        let notes = notes_of(&release);
        let current = self.build.to_string();
        if let Some(downloaded) = self.staged_version() {
            if version::is_newer(&downloaded, &latest) {
                // Channel regression revokes staging and reevaluates the feed.
                if let Some(path) = self.staged() {
                    if let Some(dir) = path.parent() {
                        let _ = std::fs::remove_dir_all(dir);
                    }
                }
                *self.staged.lock().unwrap() = None;
            } else if latest == downloaded || !auto_download {
                let mut kept = self.snapshot();
                // Only missing notes from this exact version may supplement it.
                if latest == downloaded && kept.notes.is_none() {
                    if let Some(notes) = notes {
                        if let Some(held) = self.release.lock().unwrap().as_mut() {
                            held.body = notes.to_string();
                        }
                        kept.notes = Some(notes);
                    }
                }
                return UpdateSnapshot {
                    state: UpdateState::Ready,
                    error: None,
                    ..kept
                };
            } else {
                // Replacement begins now: even preparation failure cannot offer
                // the old archive under the newly selected version's metadata.
                let previous = self.staged.lock().unwrap().take().unwrap();
                if let Some(dir) = previous.path.parent() {
                    let _ = std::fs::remove_dir_all(dir);
                }
            }
        }
        // A build from a working tree cannot swap itself for a release, so the
        // feed's answer does not make it updatable. Saying `Latest` there would
        // claim a comparison that never happened.
        if !version::is_released(&current) {
            *self.release.lock().unwrap() = None;
            return UpdateSnapshot {
                state: UpdateState::Source,
                latest: None,
                error: None,
                notes: None,
                ..self.snapshot()
            };
        }
        let state = if version::is_newer(&latest, &current) {
            UpdateState::Found
        } else {
            UpdateState::Latest
        };
        *self.release.lock().unwrap() = Some(release);
        UpdateSnapshot {
            state,
            latest: Some(latest),
            error: None,
            notes,
            ..self.snapshot()
        }
    }

    fn settle_download(&self, path: PathBuf, version: String) -> UpdateSnapshot {
        *self.staged.lock().unwrap() = Some(Staged { path, version });
        // `snapshot()` below derives `staged` from the field just written.
        UpdateSnapshot {
            state: UpdateState::Ready,
            error: None,
            ..self.snapshot()
        }
    }

    fn fail(&self, error: &Error) -> UpdateSnapshot {
        UpdateSnapshot {
            state: UpdateState::Error,
            error: Some(error.to_string()),
            ..self.snapshot()
        }
    }

    fn set(&self, next: UpdateSnapshot) {
        *self.snapshot.lock().unwrap() = next;
        self.notify();
    }

    /// Hand the current snapshot to the listener, if one is registered.
    fn notify(&self) {
        let sink = self.announce.lock().unwrap().clone();
        if let Some(sink) = sink {
            let published = self.snapshot();
            sink(&published);
        }
    }

    /// Report every snapshot this service publishes to `sink`.
    ///
    /// Registering more than one replaces the previous sink: one window reads this,
    /// and a second listener would double every event.
    pub fn announce(&self, sink: impl Fn(&UpdateSnapshot) + Send + Sync + 'static) {
        *self.announce.lock().unwrap() = Some(Arc::new(sink));
    }
}

/// Where a published snapshot goes.
type Sink = Arc<dyn Fn(&UpdateSnapshot) + Send + Sync>;

/// A download that passed verification.
struct Staged {
    path: PathBuf,
    version: String,
}
