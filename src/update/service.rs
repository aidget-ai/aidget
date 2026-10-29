use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::common::error::{Error, Result};
use crate::update::github::{self, Release};
use crate::update::install;
use crate::update::snapshot::{notes_of, UpdateSnapshot, UpdateState};
use crate::update::stage::{self, Download};
use crate::update::target::Target;
use crate::update::{download, target, verify, version};

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
        if !self.begin() {
            return self.snapshot();
        }

        let (base, repository) = (self.base.clone(), self.repository.clone());
        let token = token.map(str::to_string);
        let fetched = tokio::task::spawn_blocking(move || {
            github::latest(&base, &repository, token.as_deref())
        })
        .await;

        let settled = match fetched {
            Ok(Ok(Some(release))) => self.settle(release),
            Ok(Ok(None)) => self.settle_nothing(),
            Ok(Err(error)) => self.fail_check(&error),
            Err(error) => self.fail_check(&Error::Other(format!("update check: {error}"))),
        };
        self.set(settled);
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
        let target = match self.prepare_allowed() {
            Ok(target) => target,
            Err(error) => {
                self.set(self.fail(&error));
                return None;
            }
        };
        if !self.begin() {
            return None;
        }
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
        let progress = self.snapshot.clone();
        let outcome =
            tokio::task::spawn_blocking(move || prepared.run(&progress, token.as_deref())).await;

        let settled = match outcome {
            Ok(Ok(path)) => self.settle_download(path),
            Ok(Err(error)) => self.fail(&error),
            Err(error) => self.fail(&Error::Other(format!("download: {error}"))),
        };
        self.set(settled);
        self.snapshot()
    }

    /// Install the verified download and start the new version.
    ///
    /// The caller must not expect to keep running: the app exits once the swap
    /// is done. A swap that fails leaves the previous install in place and the
    /// state on `Error`.
    pub async fn install(&self) -> UpdateSnapshot {
        let staged = match self.begin_install() {
            Some(staged) => staged,
            None => return self.snapshot(),
        };
        let outcome = tokio::task::spawn_blocking(move || install::apply(&staged, true)).await;
        self.settle_install(outcome)
    }

    /// Install without waiting, for the app's exit path.
    ///
    /// Same swap as [`install`], minus the async hop: nothing is left to await
    /// once the process is going away.
    pub fn install_blocking(&self) -> UpdateSnapshot {
        let staged = match self.begin_install() {
            Some(staged) => staged,
            None => return self.snapshot(),
        };
        // On quit the new version must not come back: the user closed the app.
        self.settle_install(Ok(install::apply(&staged, false)))
    }

    /// Mark an install as under way and return what to install.
    ///
    /// The path is handed back once: re-reading it later let a concurrent
    /// download clear `staged` between the check and the swap.
    fn begin_install(&self) -> Option<PathBuf> {
        let staged = match self.staged() {
            Some(staged) => staged,
            None => {
                let failed =
                    self.fail(&Error::Other("no downloaded update to install".to_string()));
                self.set(failed);
                return None;
            }
        };
        if !self.begin() {
            return None;
        }
        self.set(UpdateSnapshot {
            state: UpdateState::Installing,
            error: None,
            ..self.snapshot()
        });
        Some(staged)
    }

    fn settle_install(
        &self,
        outcome: std::result::Result<Result<()>, tokio::task::JoinError>,
    ) -> UpdateSnapshot {
        let settled = match outcome {
            Ok(Ok(())) => {
                if let Some(dir) = self
                    .staged()
                    .and_then(|path| path.parent().map(Path::to_path_buf))
                {
                    let _ = std::fs::remove_dir_all(dir);
                }
                *self.staged.lock().unwrap() = None;
                // Stay on `Installing`: the process exits next, so a `Ready`
                // here would only offer to install what is already in place.
                UpdateSnapshot {
                    state: UpdateState::Installing,
                    error: None,
                    ..self.snapshot()
                }
            }
            Ok(Err(error)) => self.fail(&error),
            Err(error) => self.fail(&Error::Other(format!("install: {error}"))),
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
        let prepared = stage::prepare(&release, target, self.staged().as_deref())?;
        *self.staged.lock().unwrap() = None;
        Ok(prepared)
    }

    /// Mark a step as under way, unless one already is.
    fn begin(&self) -> bool {
        let mut snapshot = self.snapshot.lock().unwrap();
        if snapshot.state.in_flight() {
            return false;
        }
        snapshot.state = UpdateState::Checking;
        snapshot.error = None;
        snapshot.done = None;
        snapshot.total = None;
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
    fn settle(&self, release: Release) -> UpdateSnapshot {
        let latest = version::normalised(&release.tag_name);
        let notes = notes_of(&release);
        let current = self.build.to_string();
        // A download already on disk stays installable while the feed names
        // nothing newer, the way Magpie keeps `ready` in that case. Without
        // this a check that finds the same version would take the install away.
        if let Some(downloaded) = self.staged_version() {
            if !version::is_newer(&latest, &downloaded) {
                *self.release.lock().unwrap() = Some(release);
                return UpdateSnapshot {
                    state: UpdateState::Ready,
                    latest: Some(downloaded),
                    error: None,
                    notes: notes.or_else(|| self.snapshot().notes),
                    ..self.snapshot()
                };
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

    fn settle_download(&self, path: PathBuf) -> UpdateSnapshot {
        let version = self.snapshot().latest.unwrap_or_default();
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
    }
}

/// A download that passed verification.
struct Staged {
    path: PathBuf,
    version: String,
}

impl Download {
    /// Fetch the asset, verify it, and return where it landed.
    ///
    /// A file that fails verification is removed: nothing unverified is left
    /// looking installable, and a failed attempt takes its download directory
    /// with it instead of leaving an empty folder in the temp directory.
    fn run(self, progress: &Arc<Mutex<UpdateSnapshot>>, token: Option<&str>) -> Result<PathBuf> {
        let path = self.path.clone();
        let fetched = self.fetch(progress, token);
        if fetched.is_err() {
            if let Some(dir) = path.parent() {
                let _ = std::fs::remove_dir_all(dir);
            }
        }
        fetched
    }

    fn fetch(self, progress: &Arc<Mutex<UpdateSnapshot>>, token: Option<&str>) -> Result<PathBuf> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let sums = match self.asset.digest {
            Some(_) => None,
            None => {
                let url = self.sums_url.as_deref().ok_or_else(|| {
                    Error::Other("the release publishes no SHA256SUMS".to_string())
                })?;
                Some(download::text(url, token)?)
            }
        };
        let expected = verify::expected(&self.asset, sums.as_deref())?;

        let progress = progress.clone();
        let actual = download::to_file(
            &self.asset.browser_download_url,
            token,
            &self.path,
            &mut |done, total| {
                let mut snapshot = progress.lock().unwrap();
                snapshot.done = Some(done);
                if total > 0 {
                    snapshot.total = Some(total);
                }
            },
        )?;

        if actual != expected {
            let _ = std::fs::remove_file(&self.path);
            return Err(Error::Other(format!(
                "{}: checksum mismatch, expected {expected}, got {actual}",
                self.asset.name
            )));
        }
        Ok(self.path)
    }
}
