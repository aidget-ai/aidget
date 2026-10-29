use aidget::{
    common::utils::sha256_hex,
    community::{
        model::*,
        service::{encoded_package, split_spec, Community, Endpoints},
    },
    settings::GithubToken,
    widgets::{
        package::{WidgetConfig, WidgetPlatform},
        store::Store,
    },
};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use sha2::{Digest, Sha512};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
};
const MAIN: &str = "1111111111111111111111111111111111111111";
const NAME: &str = "@aidget-community/note";
type Routes = BTreeMap<String, (u16, Vec<u8>)>;

struct Server {
    base: String,
    routes: Arc<Mutex<Routes>>,
    requests: Arc<Mutex<Vec<String>>>,
    authorizations: Arc<Mutex<Vec<(String, Option<String>)>>>,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let routes = Arc::new(Mutex::new(BTreeMap::<String, (u16, Vec<u8>)>::new()));
        let requests = Arc::new(Mutex::new(vec![]));
        let authorizations = Arc::new(Mutex::new(vec![]));
        let stop = Arc::new(AtomicBool::new(false));
        let (r, q, a, s) = (
            routes.clone(),
            requests.clone(),
            authorizations.clone(),
            stop.clone(),
        );
        let handle = thread::spawn(move || {
            for stream in listener.incoming() {
                if s.load(Ordering::SeqCst) {
                    break;
                }
                let mut stream = stream.unwrap();
                let mut buffer = [0; 8192];
                let n = stream.read(&mut buffer).unwrap();
                let request = String::from_utf8_lossy(&buffer[..n]);
                let path = request.split_whitespace().nth(1).unwrap_or("/").to_owned();
                let authorization = request.lines().find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("authorization")
                        .then(|| value.trim().to_owned())
                });
                q.lock().unwrap().push(path.clone());
                a.lock().unwrap().push((path.clone(), authorization));
                let (status, body) = r
                    .lock()
                    .unwrap()
                    .get(&path)
                    .cloned()
                    .unwrap_or((404, vec![]));
                write!(
                    stream,
                    "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                let _ = stream.write_all(&body);
            }
        });
        Self {
            base: format!("http://{address}"),
            routes,
            requests,
            authorizations,
            stop,
            thread: Some(handle),
        }
    }
    fn put(&self, path: &str, status: u16, value: Value) {
        self.bytes(path, status, serde_json::to_vec(&value).unwrap());
    }
    fn bytes(&self, path: &str, status: u16, bytes: Vec<u8>) {
        self.routes
            .lock()
            .unwrap()
            .insert(path.into(), (status, bytes));
    }
    fn client(&self) -> Community {
        self.client_with_token(None)
    }
    fn client_with_token(&self, token: Option<&GithubToken>) -> Community {
        Community::with_endpoints_and_token(
            Endpoints {
                github: self.base.clone(),
                raw: self.base.clone(),
                npm: self.base.clone(),
                downloads: self.base.clone(),
            },
            token.cloned(),
        )
        .unwrap()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.base.trim_start_matches("http://"));
        self.thread.take().unwrap().join().unwrap();
    }
}
fn fixture(server: &Server) -> (RegistryEntry, Vec<u8>, String) {
    fixture_named(server, NAME)
}
fn fixture_named(server: &Server, name: &str) -> (RegistryEntry, Vec<u8>, String) {
    let mut config = WidgetConfig::new("note", "Note");
    config.platforms = vec![
        WidgetPlatform::Macos,
        WidgetPlatform::Linux,
        WidgetPlatform::Windows,
    ];
    config.screenshots = vec!["assets/screen.png".into()];
    let package = json!({"name":name,"version":"1.0.0","description":"Test widget","author":"Test","files":["config.json","index.html","assets"]});
    let files: BTreeMap<String, Vec<u8>> = BTreeMap::from([
        ("config.json".into(), serde_json::to_vec(&config).unwrap()),
        ("package.json".into(), serde_json::to_vec(&package).unwrap()),
        ("index.html".into(), b"<p>Note</p>".to_vec()),
        (
            "assets/screen.png".into(),
            b"\x89PNG\r\n\x1a\nfixture".to_vec(),
        ),
    ]);
    let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
        vec![],
        flate2::Compression::default(),
    ));
    for (path, bytes) in &files {
        let mut header = tar::Header::new_gnu();
        header.set_mode(0o644);
        header.set_size(bytes.len() as u64);
        header.set_cksum();
        builder
            .append_data(&mut header, format!("package/{path}"), bytes.as_slice())
            .unwrap();
    }
    let archive = builder.into_inner().unwrap().finish().unwrap();
    let integrity = format!("sha512-{}", STANDARD.encode(Sha512::digest(&archive)));
    let entry = RegistryEntry {
        published_at: "2026-10-25T06:49:57Z".into(),
        package: serde_json::from_value(package).unwrap(),
        config,
        files: files
            .iter()
            .map(|(path, bytes)| FileDigest {
                path: path.clone(),
                size: bytes.len() as u64,
                sha256: sha256_hex(bytes),
            })
            .collect(),
    };
    server.put("/branches/main", 200, json!({"commit":{"sha":MAIN}}));
    server.put(
        &format!("/{MAIN}/registry.json"),
        200,
        json!(Registry {
            widgets: vec![entry.clone()]
        }),
    );
    for (path, bytes) in files {
        server.bytes(&format!("/{MAIN}/packages/note/{path}"), 200, bytes);
    }
    server.bytes("/npm.tgz", 200, archive.clone());
    server.put(&format!("/{}/1.0.0",encoded_package(name)),200,json!({"name":name,"version":"1.0.0","dist":{"tarball":format!("{}/npm.tgz",server.base),"integrity":integrity}}));
    (entry, archive, integrity)
}
fn store() -> (tempfile::TempDir, Store) {
    let root = tempfile::tempdir().unwrap();
    let store = Store::new(root.path());
    store.ensure_layout().unwrap();
    (root, store)
}
#[test]
fn exact_versions_are_required_and_prefix_lookalikes_remain_third_party() {
    assert_eq!(
        split_spec("@aidget-community/note@1.2.3").unwrap(),
        (NAME.into(), Some("1.2.3".into()))
    );
    assert!(split_spec("@aidget-community/note@latest").is_err());
    assert!(split_spec("x@^1.0.0").is_err());
    assert!(split_spec("https://example.com/x").is_err());
}

#[test]
fn github_api_requests_use_the_configured_bearer_token_only() {
    let server = Server::new();
    fixture(&server);
    let token = GithubToken::parse("samplevalueABCDEFGHIJKLMNOPQRSTUVWXYZ").unwrap();
    let (_root, store) = store();

    server.client_with_token(Some(&token)).list(&store).unwrap();
    server
        .client_with_token(Some(&token))
        .install_reviewed(&store, MAIN, NAME)
        .unwrap();
    server.put(
        &format!("/downloads/point/last-week/{}", encoded_package(NAME)),
        200,
        json!({"downloads": 7}),
    );
    server
        .client_with_token(Some(&token))
        .downloads(&store, &[NAME.to_string()])
        .unwrap();

    let authorizations = server.authorizations.lock().unwrap().clone();
    assert!(authorizations.iter().any(|(path, authorization)| {
        path == "/branches/main"
            && authorization.as_deref() == Some("Bearer samplevalueABCDEFGHIJKLMNOPQRSTUVWXYZ")
    }));
    assert!(authorizations
        .iter()
        .filter(|(path, _)| path != "/branches/main")
        .all(|(_, authorization)| authorization.is_none()));
}

#[test]
fn anonymous_github_requests_never_send_an_authorization_header() {
    let server = Server::new();
    fixture(&server);
    let (_root, store) = store();

    server.client().list(&store).unwrap();

    assert!(server
        .authorizations
        .lock()
        .unwrap()
        .iter()
        .all(|(_, authorization)| authorization.is_none()));
}

#[test]
fn rejecting_github_credentials_reports_the_status_without_a_fallback() {
    let server = Server::new();
    server.put("/branches/main", 403, json!({"message": "rate limited"}));
    let token = GithubToken::parse("samplevalueABCDEFGHIJKLMNOPQRSTUVWXYZ").unwrap();

    let error = server
        .client_with_token(Some(&token))
        .verify_github_access()
        .unwrap_err();

    assert!(error.to_string().contains("HTTP 403 Forbidden"));
}

#[test]
fn installs_the_reviewed_npm_version_and_rejects_a_second_copy() {
    let server = Server::new();
    fixture(&server);
    let (_root, store) = store();
    let client = server.client();
    client.install_reviewed(&store, MAIN, NAME).unwrap();
    assert_eq!(store.load_package("note").unwrap().unwrap().name, NAME);
    assert_eq!(store.load_board().unwrap().widgets["note"].w, 360.0);
    assert!(client.install_reviewed(&store, MAIN, NAME).is_err());
    assert!(!server
        .requests
        .lock()
        .unwrap()
        .iter()
        .any(|p| p.ends_with("index.json")));
}

#[test]
fn official_versions_outside_the_current_review_are_rejected_before_npm() {
    for listed in [false, true] {
        let server = Server::new();
        let (mut entry, _, _) = fixture(&server);
        entry.package.version = "2.0.0".into();
        server.put(
            &format!("/{MAIN}/registry.json"),
            200,
            json!(Registry {
                widgets: if listed { vec![entry] } else { vec![] }
            }),
        );
        let (_root, store) = store();
        let result = server.client().install(&store, &format!("{NAME}@1.0.0"));
        assert!(
            result.is_err(),
            "an unreviewed official version was installed"
        );
        assert!(store.widget_ids().unwrap().is_empty());
        assert!(server
            .requests
            .lock()
            .unwrap()
            .iter()
            .all(|path| { path == "/branches/main" || path == &format!("/{MAIN}/registry.json") }));
    }
}
#[test]
fn missing_npm_versions_and_temporary_errors_use_the_pinned_snapshot() {
    for status in [404, 429, 503] {
        let server = Server::new();
        fixture(&server);
        server.bytes(&format!("/{}/1.0.0", encoded_package(NAME)), status, vec![]);
        let (_root, store) = store();
        server
            .client()
            .install_reviewed(&store, MAIN, NAME)
            .unwrap();
        assert!(store.load_meta("note").is_ok());
        assert!(server
            .requests
            .lock()
            .unwrap()
            .iter()
            .any(|p| p == &format!("/{MAIN}/packages/note/index.html")));
    }
}
#[test]
fn integrity_errors_never_switch_to_snapshot_or_leave_partial_widgets() {
    let server = Server::new();
    fixture(&server);
    server.bytes("/npm.tgz", 200, b"corrupt".to_vec());
    let (_root, store) = store();
    assert!(server
        .client()
        .install_reviewed(&store, MAIN, NAME)
        .unwrap_err()
        .to_string()
        .contains("integrity"));
    assert!(store.widget_ids().unwrap().is_empty());
    assert!(!server
        .requests
        .lock()
        .unwrap()
        .iter()
        .any(|p| p.starts_with(&format!("/{MAIN}/packages/"))));
}
#[test]
fn reviewed_content_mismatch_fails_even_with_valid_tarball_integrity() {
    let server = Server::new();
    let (mut entry, _archive, _integrity) = fixture(&server);
    entry
        .config
        .permissions
        .push(aidget::widgets::model::WidgetPermission::Script);
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
        .install_reviewed(&store, MAIN, NAME)
        .unwrap_err()
        .to_string()
        .contains("reviewed"));
    assert!(store.widget_ids().unwrap().is_empty());
}
#[test]
fn main_snapshot_is_available_without_waiting_for_npm_publication() {
    let server = Server::new();
    let (entry, _, _) = fixture(&server);
    server.bytes(&format!("/{}/1.0.0", encoded_package(NAME)), 404, vec![]);
    let (_root, store) = store();
    let snapshot = server.client().list(&store).unwrap();
    assert_eq!(snapshot.widgets.len(), 1);
    assert_eq!(
        snapshot.widgets[0].entry.package.version,
        entry.package.version
    );
    assert!(!server
        .requests
        .lock()
        .unwrap()
        .iter()
        .any(|p| p.contains("dist") || p.contains(&encoded_package(NAME))));
}
#[test]
fn npm_zero_is_cached_and_unavailable_counts_are_not_zero() {
    let server = Server::new();
    let (_root, store) = store();
    let name = "@aidget-community/note".to_owned();
    let path = format!("/downloads/point/last-week/{}", encoded_package(&name));
    server.put(&path, 200, json!({"downloads":0}));
    let client = server.client();
    assert_eq!(
        client
            .downloads(&store, std::slice::from_ref(&name))
            .unwrap()[0]
            .weekly,
        Some(0)
    );
    server.bytes(&path, 503, vec![]);
    assert_eq!(
        client.downloads(&store, &[name]).unwrap()[0].weekly,
        Some(0)
    );
    let unavailable = client.downloads(&store, &["other".into()]).unwrap();
    assert_eq!(unavailable[0].weekly, None);
    assert!(unavailable[0].error.is_some());
}

#[test]
fn third_party_installation_resolves_latest_once_without_official_snapshot() {
    let server = Server::new();
    let name = "@someone/note";
    fixture_named(&server, name);
    server.put(
        &format!("/{}/latest", encoded_package(name)),
        200,
        json!({"name":name,"version":"1.0.0"}),
    );
    let (_root, store) = store();
    server.client().install(&store, name).unwrap();
    assert!(!store.load_package("note").unwrap().unwrap().is_community());
    assert!(!server
        .requests
        .lock()
        .unwrap()
        .iter()
        .any(|p| p.starts_with("/branches/")));
}
#[test]
fn third_party_npm_failures_never_query_the_official_snapshot() {
    let server = Server::new();
    let name = "@someone/note";
    fixture_named(&server, name);
    server.bytes(&format!("/{}/1.0.0", encoded_package(name)), 503, vec![]);
    let (_root, store) = store();
    assert!(server
        .client()
        .install(&store, "@someone/note@1.0.0")
        .is_err());
    assert!(!server
        .requests
        .lock()
        .unwrap()
        .iter()
        .any(|p| p.starts_with("/branches/")));
}
#[test]
fn snapshot_mismatch_fails_without_committing_partial_content() {
    let server = Server::new();
    fixture(&server);
    server.bytes(&format!("/{}/1.0.0", encoded_package(NAME)), 404, vec![]);
    server.bytes(
        &format!("/{MAIN}/packages/note/index.html"),
        200,
        b"changed".to_vec(),
    );
    let (_root, store) = store();
    let error = server
        .client()
        .install_reviewed(&store, MAIN, NAME)
        .unwrap_err();
    assert!(error.to_string().contains("reviewed"));
    assert!(store.widget_ids().unwrap().is_empty());
    assert_eq!(
        std::fs::read_dir(store.state_dir())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().unwrap().is_dir()
                && e.file_name().to_string_lossy().starts_with(".tmp"))
            .count(),
        0
    );
}

#[test]
fn a_missing_snapshot_file_never_downgrades_or_installs_partial_content() {
    let server = Server::new();
    fixture(&server);
    server.bytes(&format!("/{}/1.0.0", encoded_package(NAME)), 503, vec![]);
    server.bytes(&format!("/{MAIN}/packages/note/index.html"), 404, vec![]);
    let (_root, store) = store();
    assert!(server
        .client()
        .install_reviewed(&store, MAIN, NAME)
        .is_err());
    assert!(store.widget_ids().unwrap().is_empty());
    assert!(!server
        .requests
        .lock()
        .unwrap()
        .iter()
        .any(|p| p.contains("latest") || p.contains("dist")));
}

#[test]
fn fallback_stays_on_the_confirmed_commit_after_main_advances() {
    let server = Server::new();
    fixture(&server);
    server.bytes(&format!("/{}/1.0.0", encoded_package(NAME)), 404, vec![]);
    server.put(
        "/branches/main",
        200,
        json!({"commit":{"sha":"3333333333333333333333333333333333333333"}}),
    );
    let (_root, store) = store();
    server
        .client()
        .install_reviewed(&store, MAIN, NAME)
        .unwrap();
    assert_eq!(
        store.load_package("note").unwrap().unwrap().version,
        "1.0.0"
    );
    assert!(!server
        .requests
        .lock()
        .unwrap()
        .iter()
        .any(|p| p.starts_with("/branches/")));
}

#[test]
fn media_paths_are_encoded_without_changing_the_selected_commit() {
    let server = Server::new();
    let (mut entry, _, _) = fixture(&server);
    let path = "assets/screen #1.png";
    entry.config.screenshots = vec![path.into()];
    entry
        .files
        .iter_mut()
        .find(|f| f.path == "assets/screen.png")
        .unwrap()
        .path = path.into();
    server.put(
        &format!("/{MAIN}/registry.json"),
        200,
        json!(Registry {
            widgets: vec![entry]
        }),
    );
    server.bytes(
        &format!("/{MAIN}/packages/note/assets/screen%20%231.png"),
        200,
        b"\x89PNG\r\n\x1a\nfixture".to_vec(),
    );
    assert!(server
        .client()
        .media(MAIN, NAME, path)
        .unwrap()
        .starts_with("data:image/png;base64,"));
}

#[test]
fn media_commands_share_the_list_snapshot_across_clients_and_concurrent_requests() {
    for load_list in [false, true] {
        let server = Server::new();
        fixture(&server);
        let (_root, store) = store();
        if load_list {
            server.client().list(&store).unwrap();
        }
        thread::scope(|scope| {
            for _ in 0..4 {
                scope.spawn(|| {
                    assert!(server
                        .client()
                        .media(MAIN, NAME, "assets/screen.png")
                        .unwrap()
                        .starts_with("data:image/png;base64,"));
                });
            }
        });
        let requests = server.requests.lock().unwrap();
        assert_eq!(
            requests
                .iter()
                .filter(|path| **path == format!("/{MAIN}/registry.json"))
                .count(),
            1
        );
    }
}

#[test]
fn registry_cache_keeps_endpoints_and_commits_separate_and_rejects_invalid_content() {
    let server = Server::new();
    fixture(&server);
    let (_root, store) = store();
    server.client().list(&store).unwrap();

    let second = Server::new();
    let (mut entry, _, _) = fixture(&second);
    entry.config.network = vec!["http://invalid.example".into()];
    second.put(
        &format!("/{MAIN}/registry.json"),
        200,
        json!(Registry {
            widgets: vec![entry]
        }),
    );
    assert!(second.client().list(&store).is_err());
    let (mut entry, _, _) = fixture(&second);
    entry.config.title = "Another endpoint".into();
    second.put(
        &format!("/{MAIN}/registry.json"),
        200,
        json!(Registry {
            widgets: vec![entry]
        }),
    );
    assert_eq!(
        second.client().list(&store).unwrap().widgets[0]
            .entry
            .config
            .title,
        "Another endpoint"
    );

    let next_commit = "2222222222222222222222222222222222222222";
    server.put("/branches/main", 200, json!({"commit":{"sha":next_commit}}));
    server.put(
        &format!("/{next_commit}/registry.json"),
        200,
        json!(Registry { widgets: vec![] }),
    );
    assert!(server.client().list(&store).unwrap().widgets.is_empty());
    assert!(server
        .client()
        .media(MAIN, NAME, "assets/screen.png")
        .is_ok());
}
