//! One-click self-update: version comparison, release lookup, download,
//! checksum verification and per-platform installation.
//!
//! See `docs/self-update.md` for the intended scope.

pub mod download;
pub mod fetch;
pub mod github;
pub mod install;
pub mod service;
pub mod snapshot;
pub mod stage;
pub mod target;
pub mod verify;
pub mod version;

pub use service::UpdateService;
pub use snapshot::{UpdateSnapshot, UpdateState};
pub use version::current_version;
