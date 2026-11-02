use super::{fixture, store, Server, MAIN, NAME};
use aidget::{common::utils::sha256_hex, community::model::Registry};
use serde_json::json;
use std::fs;

#[test]
fn cached_catalogue_is_optional_and_recomputes_current_installation_state_without_http() {
    let server = Server::new();
    fixture(&server);
    let (_root, store) = store();
    assert!(server.client().cached(&store).unwrap().is_none());
    assert!(server.requests.lock().unwrap().is_empty());
    server.client().list(&store).unwrap();
    server
        .client()
        .install_reviewed(&store, MAIN, NAME)
        .unwrap();
    server.requests.lock().unwrap().clear();
    let cached = server.client().cached(&store).unwrap().unwrap();
    assert_eq!(cached.snapshot.commit, MAIN);
    assert!(cached.checked_at > 0);
    assert!(cached.snapshot.widgets[0].installed);
    assert!(cached.snapshot.widgets[0].id_conflict);
    assert!(server.requests.lock().unwrap().is_empty());
}

#[test]
fn failed_update_preserves_last_success_and_corrupt_disk_data_is_explicit() {
    let server = Server::new();
    fixture(&server);
    let (_root, store) = store();
    server.client().list(&store).unwrap();
    let next = "2222222222222222222222222222222222222222";
    server.put("/branches/main", 200, json!({"commit":{"sha":next}}));
    assert!(server.client().list(&store).is_err());
    assert_eq!(
        server
            .client()
            .cached(&store)
            .unwrap()
            .unwrap()
            .snapshot
            .commit,
        MAIN
    );
    let directory = store
        .state_dir()
        .join("community/registries")
        .join(sha256_hex(server.base.as_bytes()));
    fs::write(directory.join(format!("{MAIN}.json")), b"broken json").unwrap();
    assert!(server.client().cached(&store).is_err());
}

#[test]
fn directory_retention_keeps_sixteen_revisions_and_the_latest_pointer_is_readable() {
    let server = Server::new();
    fixture(&server);
    let (_root, store) = store();
    for index in 0..18 {
        let commit = format!("{index:040x}");
        server.put("/branches/main", 200, json!({"commit":{"sha":commit}}));
        server.put(
            &format!("/{commit}/registry.json"),
            200,
            json!(Registry { widgets: vec![] }),
        );
        server.client().list(&store).unwrap();
    }
    let directory = store
        .state_dir()
        .join("community/registries")
        .join(sha256_hex(server.base.as_bytes()));
    assert_eq!(fs::read_dir(directory).unwrap().count(), 17);
    assert_eq!(
        server
            .client()
            .cached(&store)
            .unwrap()
            .unwrap()
            .snapshot
            .commit,
        format!("{:040x}", 17)
    );
}

#[test]
fn pinned_disk_registry_remains_installable_when_memory_is_evicted_and_main_is_unavailable() {
    let server = Server::new();
    fixture(&server);
    let (_root, store) = store();
    server.client().list(&store).unwrap();
    // Other stores evict process memory without pruning this snapshot's disk entry.
    for _ in 0..17 {
        let (_other_root, other_store) = super::store();
        server.client().list(&other_store).unwrap();
    }
    server.bytes("/branches/main", 500, vec![]);
    server.bytes(&format!("/{MAIN}/registry.json"), 500, vec![]);
    server.bytes(
        &format!("/{}/1.0.0", super::encoded_package(NAME)),
        503,
        vec![],
    );
    server.requests.lock().unwrap().clear();
    server
        .client()
        .install_reviewed(&store, MAIN, NAME)
        .unwrap();
    assert!(store.load_package("note").unwrap().is_some());
    assert!(!server
        .requests
        .lock()
        .unwrap()
        .iter()
        .any(|path| path == "/branches/main" || path.ends_with("/registry.json")));
}

#[test]
fn delayed_checks_cannot_replace_a_snapshot_published_by_a_later_started_check() {
    let server = Server::new();
    fixture(&server);
    let (_root, store) = store();
    server.client().list(&store).unwrap();
    let pointer = store
        .state_dir()
        .join("community/registries")
        .join(sha256_hex(server.base.as_bytes()))
        .join("latest.json");
    let mut latest: serde_json::Value =
        serde_json::from_slice(&fs::read(&pointer).unwrap()).unwrap();
    // A later-started check has already published its result before this delayed check finishes.
    latest["requested_at"] = json!(u64::MAX);
    fs::write(&pointer, serde_json::to_vec(&latest).unwrap()).unwrap();
    let next = "2222222222222222222222222222222222222222";
    server.put("/branches/main", 200, json!({"commit":{"sha":next}}));
    server.put(
        &format!("/{next}/registry.json"),
        200,
        json!(Registry { widgets: vec![] }),
    );
    assert_eq!(server.client().list(&store).unwrap().commit, next);
    assert_eq!(
        server
            .client()
            .cached(&store)
            .unwrap()
            .unwrap()
            .snapshot
            .commit,
        MAIN
    );
}

#[test]
fn missing_media_digests_are_rejected_before_publishing_a_catalogue() {
    let server = Server::new();
    let (mut entry, _, _) = fixture(&server);
    entry.files.retain(|file| file.path != "assets/screen.png");
    server.put(
        &format!("/{MAIN}/registry.json"),
        200,
        json!(Registry {
            widgets: vec![entry]
        }),
    );
    let (_root, store) = store();
    assert!(server
        .client()
        .list(&store)
        .unwrap_err()
        .to_string()
        .contains("media missing"));
    assert!(server.client().cached(&store).unwrap().is_none());
}
