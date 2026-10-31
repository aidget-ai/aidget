use aidget::runtime::update::check_and_maybe_download;
use aidget::update::{UpdateService, UpdateState};
use std::io::{Read, Write};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

struct Feed {
    base: String,
    release: Arc<Mutex<(u16, String)>>,
    downloads: Arc<AtomicUsize>,
    bad_asset: Arc<Mutex<bool>>,
    asset_status: Arc<Mutex<u16>>,
}

impl Feed {
    fn new() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let release = Arc::new(Mutex::new((200, String::new())));
        let downloads = Arc::new(AtomicUsize::new(0));
        let bad_asset = Arc::new(Mutex::new(false));
        let asset_status = Arc::new(Mutex::new(200));
        let feed = Self {
            base,
            release: release.clone(),
            downloads: downloads.clone(),
            bad_asset: bad_asset.clone(),
            asset_status: asset_status.clone(),
        };
        // Weak ownership ends this local server when the test fixture is dropped.
        let live = Arc::downgrade(&feed.release);
        std::thread::spawn(move || {
            while live.strong_count() > 1 {
                let Ok((mut stream, _)) = listener.accept() else {
                    std::thread::yield_now();
                    continue;
                };
                stream.set_nonblocking(false).unwrap();
                let mut buffer = [0; 4096];
                let count = stream.read(&mut buffer).unwrap();
                let request = String::from_utf8_lossy(&buffer[..count]);
                let (status, body) = if request.contains("/releases/latest ") {
                    release.lock().unwrap().clone()
                } else {
                    downloads.fetch_add(1, Ordering::SeqCst);
                    (
                        *asset_status.lock().unwrap(),
                        if *bad_asset.lock().unwrap() {
                            "bad"
                        } else {
                            "verified"
                        }
                        .into(),
                    )
                };
                write!(
                    stream,
                    "HTTP/1.1 {status} OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            }
        });
        feed.set("2.0.0", "v2 notes");
        feed
    }

    fn set(&self, version: &str, notes: &str) {
        let target = aidget::update::target::current().unwrap();
        let name = format!("aidget_v{version}_{}{}", target.arch, target.kind.suffix());
        let digest = crate::update::sha256(b"verified");
        *self.release.lock().unwrap() = (200, serde_json::json!({
            "tag_name": format!("v{version}"), "html_url": "", "body": notes,
            "assets": [{"name":name,"browser_download_url":format!("{}/asset",self.base),"size":8,"digest":format!("sha256:{digest}")}]
        }).to_string());
    }

    fn service(&self) -> UpdateService {
        UpdateService::for_build(&self.base, "owner/repo", "1.0.0")
    }
}

fn run<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(f)
}

#[test]
fn magpie_held_download_keeps_version_notes_asset_and_install_identity() {
    let feed = Feed::new();
    let service = feed.service();
    let ready = run(check_and_maybe_download(service.clone(), None, true));
    assert_eq!(ready.state, UpdateState::Ready);
    let staged = service.staged().unwrap();
    feed.set("3.0.0", "v3 notes");
    let held = run(check_and_maybe_download(service.clone(), None, false));
    assert_eq!(held.state, UpdateState::Ready);
    assert_eq!(held.latest.as_deref(), Some("2.0.0"));
    assert_eq!(held.notes.as_deref(), Some("v2 notes"));
    let contract: serde_json::Value =
        serde_json::from_str(include_str!("../../ui/tests/fixtures/update-staged.json")).unwrap();
    assert_eq!(serde_json::to_value(&held).unwrap(), contract);
    assert_eq!(service.staged().unwrap(), staged);
    assert_eq!(feed.downloads.load(Ordering::SeqCst), 1);
    // The service's download target must still agree, not just its display.
    let again = run(service.download(None));
    assert_eq!(again.latest.as_deref(), Some("2.0.0"));
    assert!(service
        .staged()
        .unwrap()
        .to_string_lossy()
        .contains("v2.0.0"));
    crate::update::cleanup(service.staged().unwrap());
}

#[test]
fn replacing_download_revokes_old_eligibility_and_failed_replacement_retries_new_version() {
    let feed = Feed::new();
    let service = feed.service();
    run(check_and_maybe_download(service.clone(), None, true));
    let old = service.staged().unwrap();
    feed.set("3.0.0", "v3 notes");
    *feed.bad_asset.lock().unwrap() = true;
    let observed = Arc::new(Mutex::new(Vec::new()));
    let seen = observed.clone();
    service.announce(move |snapshot| seen.lock().unwrap().push(snapshot.clone()));
    let failed = run(check_and_maybe_download(service.clone(), None, true));
    assert_eq!(failed.state, UpdateState::Error);
    assert!(!failed.staged);
    assert!(!old.exists());
    assert_eq!(failed.latest.as_deref(), Some("3.0.0"));
    assert_eq!(failed.notes.as_deref(), Some("v3 notes"));
    let states = observed.lock().unwrap();
    assert!(states
        .iter()
        .any(|s| s.state == UpdateState::Downloading && !s.staged));
    assert!(!states.iter().any(|s| s.state == UpdateState::Found));
    drop(states);
    assert!(matches!(
        run(service.install()),
        aidget::update::service::InstallOutcome::Failed(_)
    ));
    *feed.bad_asset.lock().unwrap() = false;
    let retry = run(service.download(None));
    assert_eq!(retry.state, UpdateState::Ready);
    assert_eq!(retry.latest.as_deref(), Some("3.0.0"));
    assert!(service
        .staged()
        .unwrap()
        .to_string_lossy()
        .contains("v3.0.0"));
    crate::update::cleanup(service.staged().unwrap());
}

#[test]
fn same_version_notes_errors_empty_feed_and_channel_regression_stay_coherent() {
    let feed = Feed::new();
    feed.set("2.0.0", "");
    let service = feed.service();
    run(check_and_maybe_download(service.clone(), None, true));
    let old = service.staged().unwrap();
    feed.set("2.0.0", "supplemented");
    assert_eq!(
        run(check_and_maybe_download(service.clone(), None, false))
            .notes
            .as_deref(),
        Some("supplemented")
    );
    feed.set("2.0.0", "changed");
    assert_eq!(
        run(check_and_maybe_download(service.clone(), None, false))
            .notes
            .as_deref(),
        Some("supplemented")
    );
    *feed.release.lock().unwrap() = (500, "{}".into());
    let failed = run(check_and_maybe_download(service.clone(), None, true));
    assert_eq!(failed.state, UpdateState::Ready);
    assert!(failed.error.is_some());
    assert_eq!(failed.notes.as_deref(), Some("supplemented"));
    *feed.release.lock().unwrap() = (404, r#"{"message":"Not Found"}"#.into());
    let empty = run(check_and_maybe_download(service.clone(), None, true));
    assert_eq!(empty.latest.as_deref(), Some("2.0.0"));
    assert!(empty.staged);
    feed.set("1.5.0", "older feed");
    let regressed = run(check_and_maybe_download(service.clone(), None, false));
    assert_eq!(regressed.state, UpdateState::Found);
    assert_eq!(regressed.latest.as_deref(), Some("1.5.0"));
    assert_eq!(regressed.notes.as_deref(), Some("older feed"));
    assert!(!regressed.staged);
    assert!(!old.exists());
    let downloaded = run(service.download(None));
    assert_eq!(downloaded.latest.as_deref(), Some("1.5.0"));
    crate::update::cleanup(service.staged().unwrap());
    // Without valid staging the empty feed must remove the previous asset target.
    let fresh = feed.service();
    feed.set("3.0.0", "latest");
    run(fresh.check(None));
    *feed.release.lock().unwrap() = (404, r#"{"message":"Not Found"}"#.into());
    assert_eq!(run(fresh.check(None)).state, UpdateState::NoRelease);
    assert!(run(fresh.download(None))
        .error
        .unwrap()
        .contains("no release"));
}

#[test]
fn policy_on_download_rejects_overlapping_install_and_download_without_mutation() {
    let feed = Feed::new();
    let service = feed.service();
    run(check_and_maybe_download(service.clone(), None, true));
    let old = service.staged().unwrap();
    feed.set("3.0.0", "v3 notes");
    let observed = Arc::new(AtomicUsize::new(0));
    let count = observed.clone();
    let active = service.clone();
    service.announce(move |snapshot| {
        if snapshot.state == UpdateState::Downloading {
            assert!(!snapshot.staged);
            assert!(matches!(
                active.install_blocking(),
                aidget::update::service::InstallOutcome::NotStarted(_)
            ));
            let repeated = active.start_download(None);
            assert_eq!(repeated.state, UpdateState::Downloading);
            assert!(!repeated.staged);
            count.fetch_add(1, Ordering::SeqCst);
        }
    });
    let ready = run(check_and_maybe_download(service.clone(), None, true));
    assert_eq!(ready.state, UpdateState::Ready);
    assert_eq!(ready.latest.as_deref(), Some("3.0.0"));
    assert_eq!(ready.notes.as_deref(), Some("v3 notes"));
    service.announce(|_| {});
    assert_eq!(observed.load(Ordering::SeqCst), 1);
    assert_eq!(feed.downloads.load(Ordering::SeqCst), 2);
    assert!(!old.exists());
    crate::update::cleanup(service.staged().unwrap());
}

#[test]
fn replacement_network_and_preparation_failure_never_fall_back_to_old_staging() {
    for missing_asset in [false, true] {
        let feed = Feed::new();
        let service = feed.service();
        run(check_and_maybe_download(service.clone(), None, true));
        let old = service.staged().unwrap();
        feed.set("3.0.0", "v3 notes");
        if missing_asset {
            let mut release = feed.release.lock().unwrap();
            let mut json: serde_json::Value = serde_json::from_str(&release.1).unwrap();
            json["assets"] = serde_json::json!([]);
            release.1 = json.to_string();
        } else {
            *feed.asset_status.lock().unwrap() = 500;
        }
        let failed = run(check_and_maybe_download(service.clone(), None, true));
        assert_eq!(failed.state, UpdateState::Error);
        assert_eq!(failed.latest.as_deref(), Some("3.0.0"));
        assert_eq!(failed.notes.as_deref(), Some("v3 notes"));
        assert!(!failed.staged);
        assert!(!old.exists());
        assert!(service.staged().is_none());
        if !missing_asset {
            *feed.asset_status.lock().unwrap() = 200;
            let retry = run(service.download(None));
            assert_eq!(retry.state, UpdateState::Ready);
            assert_eq!(retry.latest.as_deref(), Some("3.0.0"));
            crate::update::cleanup(service.staged().unwrap());
        }
    }
}
