use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Condvar, Mutex,
};

use crate::common::error::{Error, Result};
use crate::update::github::{self, Release};
mod install;
use crate::update::notes::{self, NotesCache, UpdateNotes};
use crate::update::snapshot::{UpdateSnapshot, UpdateState};
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
    /// Where retained release notes live between runs.
    notes_path: Arc<PathBuf>,
    notes_cache: Arc<Mutex<NotesCache>>,
    /// The release the last check found, kept so a download needs no second
    /// lookup.
    release: Arc<Mutex<Option<Release>>>,
    /// A verified download waiting to install.
    staged: Arc<Mutex<Option<Staged>>>,
    /// Whether a replacement is actually running.
    ///
    /// Separate from `snapshot.state`: `Installing` deliberately survives a
    /// completed replacement until the process exits, so using it as the quit
    /// gate would also defer the exit that starts the new version.
    installing: Arc<AtomicBool>,
    /// Wakes an un-deferable quit (macOS Cmd+Q) once replacement has finished.
    install_settled: Arc<(Mutex<()>, Condvar)>,
    /// Where a published snapshot is reported.
    ///
    /// The launch check runs without anyone asking, so the window has to be told
    /// when it settles — otherwise it keeps showing what it read before the check
    /// started.
    announce: Arc<Mutex<Option<Sink>>>,
}

impl UpdateService {
    /// Build the production service with a persistent release-notes cache.
    pub fn new(notes_path: impl Into<PathBuf>) -> Result<Self> {
        Self::for_build_with_cache(
            github::API_BASE,
            github::REPOSITORY,
            version::build_stamp(),
            notes_path,
        )
    }

    /// A service reading `base` for `repository`, claiming to run as `build`.
    ///
    /// Unit tests use it to run against a local server and to choose whether the
    /// build counts as a release, instead of depending on the working tree.
    pub fn for_build(base: &str, repository: &str, build: &str) -> Self {
        let notes_path = std::env::temp_dir()
            .join("aidget-update-tests")
            .join(format!("{}.json", uuid::Uuid::new_v4()));
        Self::for_build_with_cache(base, repository, build, notes_path)
            .expect("a unique test cache path is readable")
    }

    /// Test seam for services that need the same notes cache across restarts.
    pub fn for_build_with_cache(
        base: &str,
        repository: &str,
        build: &str,
        notes_path: impl Into<PathBuf>,
    ) -> Result<Self> {
        let notes_path = notes_path.into();
        let notes_cache = notes::load(&notes_path)?;
        let retained = notes::initial(&notes_cache).map(Box::new);
        Ok(UpdateService {
            snapshot: Arc::new(Mutex::new(UpdateSnapshot {
                state: UpdateState::Idle,
                current: build.to_string(),
                latest: None,
                error: None,
                done: None,
                total: None,
                notes: retained,
                staged: false,
            })),
            base: Arc::from(base),
            repository: Arc::from(repository),
            build: Arc::from(build),
            notes_path: Arc::new(notes_path),
            notes_cache: Arc::new(Mutex::new(notes_cache)),
            release: Arc::new(Mutex::new(None)),
            staged: Arc::new(Mutex::new(None)),
            installing: Arc::new(AtomicBool::new(false)),
            install_settled: Arc::new((Mutex::new(()), Condvar::new())),
            announce: Arc::new(Mutex::new(None)),
        })
    }

    pub fn snapshot(&self) -> UpdateSnapshot {
        let mut snapshot = self.snapshot.lock().unwrap().clone();
        // Derived, not stored: the download on disk is the fact, and every
        // published snapshot has to agree with it.
        snapshot.staged = self.staged().is_some();
        snapshot
    }

    /// Whether file replacement is running, excluding the settled state that
    /// still owns the service until exit.
    pub fn is_installing(&self) -> bool {
        self.installing.load(Ordering::SeqCst)
    }

    /// Wait until the active replacement has finished.
    ///
    /// Used by the `Exit` handler on macOS, where the native quit gesture never
    /// emits `ExitRequested` and therefore cannot be deferred.
    pub fn wait_for_install(&self) {
        let mut guard = self.install_settled.0.lock().unwrap();
        while self.installing.load(Ordering::SeqCst) {
            guard = self.install_settled.1.wait(guard).unwrap();
        }
    }

    /// Open the quit gate and wake anyone waiting on the replacement.
    fn clear_installing(&self) {
        let _guard = self.install_settled.0.lock().unwrap();
        self.installing.store(false, Ordering::SeqCst);
        self.install_settled.1.notify_all();
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
        let current = self.build.to_string();
        let token = token.map(str::to_string);
        let fetched = tokio::task::spawn_blocking(move || {
            github::stable_releases(&base, &repository, token.as_deref(), Some(&current))
        })
        .await;

        let settled = match fetched {
            Ok(Ok(releases)) if releases.is_empty() => self.settle_nothing(),
            Ok(Ok(releases)) => {
                let selected = {
                    let mut cache = self.notes_cache.lock().unwrap();
                    let current = version::is_released(&self.build).then_some(self.build.as_ref());
                    notes::select(&mut cache, current, &releases).and_then(|selected| {
                        notes::save(&self.notes_path, &cache)?;
                        Ok(selected.map(Box::new))
                    })
                };
                match selected {
                    Ok(notes) => self.settle(releases[0].clone(), &releases, notes, auto_download),
                    Err(error) => self.fail_check(&error),
                }
            }
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
                return self.finish_download(prepared).await;
            }
        } else {
            self.set(settled);
        }
        self.snapshot()
    }

    /// A failed check, which is about the feed and not about what is already on
    /// disk.
    ///
    /// A downloaded and verified update stays installable across a failed poll.
    /// The error is reported alongside the update.
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
    pub async fn download(&self) -> UpdateSnapshot {
        let Some(prepared) = self.begin_download() else {
            return self.snapshot();
        };
        self.finish_download(prepared).await
    }

    /// Start the same download without waiting for it.
    ///
    /// The snapshot comes back on `Downloading` at once, so the window can show
    /// progress instead of blocking until the file is on disk. This needs a Tokio
    /// runtime to spawn onto: call it from an async command, never from a
    /// synchronous one, which runs on the main thread and aborts the process when
    /// it panics.
    pub fn start_download(&self) -> UpdateSnapshot {
        let Some(prepared) = self.begin_download() else {
            return self.snapshot();
        };
        let service = self.clone();
        tokio::spawn(async move {
            service.finish_download(prepared).await;
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

    async fn finish_download(&self, prepared: Download) -> UpdateSnapshot {
        let version = prepared.version.clone();
        let progress = self.snapshot.clone();
        let outcome = tokio::task::spawn_blocking(move || prepared.run(&progress)).await;

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
        self.begin_with(false)
    }

    /// Claim an install and close the quit gate in the same critical section.
    ///
    /// Setting the flag after [`Self::begin`] returns would leave a window where
    /// the state is already in flight but the exit policy still sees no
    /// replacement; `notify()` runs before the caller can store the flag.
    fn begin_replacement(&self) -> bool {
        self.begin_with(true)
    }

    fn begin_with(&self, installing: bool) -> bool {
        {
            let mut snapshot = self.snapshot.lock().unwrap();
            if snapshot.state.in_flight() {
                return false;
            }
            if installing {
                self.installing.store(true, Ordering::SeqCst);
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
    fn settle(
        &self,
        release: Release,
        releases: &[Release],
        notes: Option<Box<UpdateNotes>>,
        auto_download: bool,
    ) -> UpdateSnapshot {
        let latest = version::normalised(&release.tag_name);
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
                let effective = if latest == downloaded {
                    latest
                } else {
                    downloaded
                };
                return UpdateSnapshot {
                    state: UpdateState::Ready,
                    latest: Some(effective.clone()),
                    error: None,
                    notes: notes::range(&current, &effective, releases)
                        .map(Box::new)
                        .or_else(|| self.snapshot().notes)
                        .or(notes),
                    ..self.snapshot()
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
                notes,
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
