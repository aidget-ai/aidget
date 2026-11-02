//! What the main window reads: the current state and what is available.
//!
//! Split from the service that produces them so the payload the window depends
//! on has one definition, and so the state machine file stays about the machine.

use serde::Serialize;

use crate::update::github::Release;

/// Where the update flow currently stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateState {
    /// No check has run yet.
    Idle,
    /// A check is in flight.
    Checking,
    /// The running version is the newest release.
    Latest,
    /// This build did not come from a release tag, so it cannot update itself:
    /// a swap would replace the build its developer already has. Distinct from
    /// [`UpdateState::Latest`], which means the check ran and found nothing newer.
    Source,
    /// The repository has published no release at all, so there is nothing to
    /// compare against and nothing to install.
    NoRelease,
    /// A newer release exists but has not been downloaded.
    Found,
    /// A newer release is downloading.
    Downloading,
    /// A newer release is downloaded and verified, waiting to install.
    Ready,
    /// The verified update is being installed.
    Installing,
    /// A step failed; `error` says which.
    Error,
}

impl UpdateState {
    /// Whether a step is running, so another must not start.
    ///
    /// `Installing` counts: a download started while the bundle is being swapped
    /// would delete the archive the swap is reading and clear `staged` beneath
    /// it.
    pub(crate) fn in_flight(&self) -> bool {
        matches!(
            self,
            UpdateState::Checking | UpdateState::Downloading | UpdateState::Installing
        )
    }
}

/// The payload the main window renders.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSnapshot {
    pub state: UpdateState,
    /// What this build is; see [`build_stamp`].
    pub current: String,
    /// The release version found by the last check, once one has run.
    pub latest: Option<String>,
    pub error: Option<String>,
    /// Bytes downloaded so far, while `state` is `Downloading`.
    pub done: Option<u64>,
    /// The asset's size in bytes, when the release says.
    pub total: Option<u64>,
    /// What the release says about itself, shown beside the update.
    ///
    /// Boxed: a `String` here pushed the struct past the size Rust warns about
    /// for an `Err` variant, and these are only ever read.
    pub notes: Option<Box<str>>,
    /// Whether a verified download is waiting on disk.
    ///
    /// `state` says what is happening; this says what is available. Without it
    /// the window has to read availability off the last outcome, so a failed
    /// check, a failed download or a failed install each hid work that was still
    /// possible.
    pub staged: bool,
}

/// A release's notes, or `None` when it has none worth showing.
pub(crate) fn notes_of(release: &Release) -> Option<Box<str>> {
    let notes = release.body.trim();
    (!notes.is_empty()).then(|| notes.into())
}
