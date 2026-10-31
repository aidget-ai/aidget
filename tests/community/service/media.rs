use super::{fixture, store, Server, MAIN, NAME};
use aidget::{
    common::{error::Error, utils::sha256_hex},
    community::model::Registry,
};
use serde_json::json;
use std::{
    fs,
    time::{Duration, SystemTime},
};

#[test]
fn disk_media_survives_clients_and_reuses_identical_content_across_commits() {
    let server = Server::new();
    let (entry, _, _) = fixture(&server);
    let (_root, store) = store();
    let first = server
        .client()
        .media(&store, MAIN, NAME, "assets/screen.png", false)
        .unwrap();
    let next = "2222222222222222222222222222222222222222";
    server.put(
        &format!("/{next}/registry.json"),
        200,
        json!(Registry {
            widgets: vec![entry.clone()]
        }),
    );
    server.bytes(
        &format!("/{MAIN}/packages/note/assets/screen.png"),
        500,
        vec![],
    );
    assert_eq!(
        server
            .client()
            .media(&store, next, NAME, "assets/screen.png", false)
            .unwrap(),
        first
    );
    let file = entry
        .files
        .iter()
        .find(|file| file.path == "assets/screen.png")
        .unwrap();
    assert_eq!(
        fs::read(store.state_dir().join("community/media").join(&file.sha256)).unwrap(),
        b"\x89PNG\r\n\x1a\nfixture"
    );
    assert_eq!(
        server
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|path| path.ends_with("/assets/screen.png"))
            .count(),
        1
    );
}

#[test]
fn new_digest_downloads_new_bytes_and_explicit_retry_repairs_corruption() {
    let server = Server::new();
    let (mut entry, _, _) = fixture(&server);
    let (_root, store) = store();
    server
        .client()
        .media(&store, MAIN, NAME, "assets/screen.png", false)
        .unwrap();
    let bytes = b"\x89PNG\r\n\x1a\nnew content";
    let file = entry
        .files
        .iter_mut()
        .find(|file| file.path == "assets/screen.png")
        .unwrap();
    file.size = bytes.len() as u64;
    file.sha256 = sha256_hex(bytes);
    let destination = store.state_dir().join("community/media").join(&file.sha256);
    let next = "2222222222222222222222222222222222222222";
    server.put(
        &format!("/{next}/registry.json"),
        200,
        json!(Registry {
            widgets: vec![entry]
        }),
    );
    server.bytes(
        &format!("/{next}/packages/note/assets/screen.png"),
        200,
        bytes.to_vec(),
    );
    server
        .client()
        .media(&store, next, NAME, "assets/screen.png", false)
        .unwrap();
    fs::write(&destination, b"corrupted").unwrap();
    let before = server.requests.lock().unwrap().len();
    assert!(server
        .client()
        .media(&store, next, NAME, "assets/screen.png", false)
        .unwrap_err()
        .to_string()
        .contains("differs"));
    assert_eq!(server.requests.lock().unwrap().len(), before);
    server
        .client()
        .media(&store, next, NAME, "assets/screen.png", true)
        .unwrap();
    assert_eq!(fs::read(destination).unwrap(), bytes);
}

#[test]
fn eviction_preserves_recently_used_files_and_enforces_128_mib() {
    let server = Server::new();
    fixture(&server);
    let (_root, store) = store();
    let directory = store.state_dir().join("community/media");
    fs::create_dir_all(&directory).unwrap();
    for (name, age) in [("a".repeat(64), 30), ("b".repeat(64), 10)] {
        let file = fs::File::create(directory.join(name)).unwrap();
        file.set_len(64 * 1024 * 1024).unwrap();
        file.set_times(
            fs::FileTimes::new().set_modified(SystemTime::now() - Duration::from_secs(age)),
        )
        .unwrap();
    }
    server
        .client()
        .media(&store, MAIN, NAME, "assets/screen.png", false)
        .unwrap();
    assert!(!directory.join("a".repeat(64)).exists());
    assert!(directory.join("b".repeat(64)).exists());
    let size: u64 = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().metadata().unwrap().len())
        .sum();
    assert!(size <= 128 * 1024 * 1024);
}

#[test]
fn invalid_remote_bytes_and_cache_errors_do_not_leave_entries() {
    let server = Server::new();
    fixture(&server);
    let (_root, store) = store();
    let directory = store.state_dir().join("community/media");
    server.bytes(
        &format!("/{MAIN}/packages/note/assets/screen.png"),
        200,
        b"invalid".to_vec(),
    );
    assert!(server
        .client()
        .media(&store, MAIN, NAME, "assets/screen.png", false)
        .is_err());
    assert!(!directory.exists());
    fs::write(&directory, b"not a directory").unwrap();
    let before = server.requests.lock().unwrap().len();
    assert!(server
        .client()
        .media(&store, MAIN, NAME, "assets/screen.png", false)
        .is_err());
    assert_eq!(server.requests.lock().unwrap().len(), before);
}

#[test]
fn a_cache_directory_replaced_by_a_file_fails_before_http_even_on_refresh() {
    let server = Server::new();
    fixture(&server);
    let (_root, store) = store();
    let client = server.client();
    client
        .media(&store, MAIN, NAME, "assets/screen.png", false)
        .unwrap();
    let directory = store.state_dir().join("community/media");
    fs::remove_dir_all(&directory).unwrap();
    fs::write(&directory, b"not a directory").unwrap();
    let before = server.requests.lock().unwrap().len();

    for refresh in [false, true] {
        let error = client
            .media(&store, MAIN, NAME, "assets/screen.png", refresh)
            .unwrap_err();
        assert_eq!(server.requests.lock().unwrap().len(), before);
        assert!(
            matches!(error, Error::Io(error) if error.kind() == std::io::ErrorKind::NotADirectory)
        );
        assert_eq!(fs::read(&directory).unwrap(), b"not a directory");
    }
}
