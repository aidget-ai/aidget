//! Installation ownership and per-request outcomes.
use super::*;
use crate::update::install::{self, Installation};
use std::path::{Path, PathBuf};

/// Only a completed request carries authorization to restart and exit.
#[derive(Debug)]
pub enum InstallOutcome {
    /// Another operation owns the service; this request did nothing.
    NotStarted(UpdateSnapshot),
    /// Preparation or replacement failed; the download remains retryable.
    Failed(UpdateSnapshot),
    /// Replacement completed using the captured installation paths.
    Completed(UpdateSnapshot, Installation),
}

impl InstallOutcome {
    /// Read the service state without treating it as this request's success.
    pub fn snapshot(&self) -> &UpdateSnapshot {
        match self {
            Self::NotStarted(s) | Self::Failed(s) | Self::Completed(s, _) => s,
        }
    }
}

impl UpdateService {
    /// Install the verified download; only this request's completion permits exit.
    pub async fn install(&self) -> InstallOutcome {
        self.install_with(|staged| {
            let installation = install::prepare()?;
            installation.apply(staged, true)?;
            Ok(installation)
        })
        .await
    }

    async fn install_with(
        &self,
        apply: impl FnOnce(&Path) -> Result<Installation> + Send + 'static,
    ) -> InstallOutcome {
        let staged = match self.begin_install() {
            Ok(path) => path,
            Err(outcome) => return outcome,
        };
        let outcome = tokio::task::spawn_blocking(move || apply(&staged)).await;
        self.settle_install(outcome)
    }

    /// Install on quit without restarting, sharing ownership with manual installs.
    pub fn install_blocking(&self) -> InstallOutcome {
        let staged = match self.begin_install() {
            Ok(path) => path,
            Err(outcome) => return outcome,
        };
        self.settle_install(Ok((|| {
            let installation = install::prepare()?;
            installation.apply(&staged, false)?;
            Ok(installation)
        })()))
    }

    fn begin_install(&self) -> std::result::Result<PathBuf, InstallOutcome> {
        // Claim before reading staging: a refused request cannot mutate its owner.
        if !self.begin() {
            return Err(InstallOutcome::NotStarted(self.snapshot()));
        }
        let Some(staged) = self.staged() else {
            let failed = self.fail(&Error::Other("no downloaded update to install".into()));
            self.set(failed.clone());
            return Err(InstallOutcome::Failed(failed));
        };
        self.set(UpdateSnapshot {
            state: UpdateState::Installing,
            error: None,
            ..self.snapshot()
        });
        Ok(staged)
    }

    fn settle_install(
        &self,
        outcome: std::result::Result<Result<Installation>, tokio::task::JoinError>,
    ) -> InstallOutcome {
        match outcome {
            Ok(Ok(installation)) => {
                if let Some(dir) = self
                    .staged()
                    .and_then(|path| path.parent().map(Path::to_path_buf))
                {
                    let _ = std::fs::remove_dir_all(dir);
                }
                *self.staged.lock().unwrap() = None;
                // Keep ownership until exit, including a restart scheduling failure.
                self.set(UpdateSnapshot {
                    state: UpdateState::Installing,
                    error: None,
                    ..self.snapshot()
                });
                InstallOutcome::Completed(self.snapshot(), installation)
            }
            failed => {
                let error = match failed {
                    Ok(Err(error)) => error,
                    Err(error) => Error::Other(format!("install: {error}")),
                    Ok(Ok(_)) => unreachable!(),
                };
                // Publishing releases ownership; a new operation may clear the
                // shared error before this request returns to its quit caller.
                let failed = self.fail(&error);
                self.set(failed.clone());
                InstallOutcome::Failed(failed)
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/update/service/install.rs"]
mod tests;
