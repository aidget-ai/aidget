// Link-verification classification tests for `src/agents/skill.rs`.
//
// Loaded as a child module via `#[path]` from `src/agents/skill.rs` (see
// the `created_link_tests` declaration there), so `super::*` reaches the
// private `verify_created_link` helper without widening its visibility.
// This file is not declared in `tests/agents/mod.rs` and therefore is not
// part of the integration test crate; it lives under `tests/` only to
// mirror the source structure per AGENTS.md rule 6.

use super::*;

#[test]
fn a_materialized_path_counts_as_created() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("real");
    fs::create_dir_all(&dir).unwrap();
    assert!(verify_created_link(&dir).is_ok());
}

#[test]
fn a_missing_link_counts_as_a_refused_privilege() {
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("never-created");
    let error = verify_created_link(&missing).unwrap_err();
    assert_eq!(error.raw_os_error(), Some(1314));
}

#[test]
fn other_metadata_errors_propagate_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let invalid = temp.path().join("bad<name");
    let error = verify_created_link(&invalid).unwrap_err();
    assert_eq!(error.raw_os_error(), Some(123));
}
