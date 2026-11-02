use super::{encoded_package, store, Community, Endpoints, Server};
use aidget::community::model::DownloadCount;
use serde_json::json;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc, Arc, Condvar, Mutex,
    },
    thread,
    time::Duration,
};

struct ConcurrentServer {
    base: String,
    started: mpsc::Receiver<()>,
    gate: Arc<(Mutex<(bool, Option<usize>)>, Condvar)>,
    peak: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}
impl ConcurrentServer {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let (sender, started) = mpsc::channel();
        let gate = Arc::new((Mutex::new((false, None)), Condvar::new()));
        let peak = Arc::new(AtomicUsize::new(0));
        let active = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (g, p, s) = (gate.clone(), peak.clone(), stop.clone());
        let handle = thread::spawn(move || {
            thread::scope(|scope| {
                for stream in listener.incoming() {
                    if s.load(Ordering::SeqCst) {
                        break;
                    }
                    let (gate, peak, active, sender) =
                        (g.clone(), p.clone(), active.clone(), sender.clone());
                    scope.spawn(move || {
                        let mut stream = stream.unwrap();
                        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                        let mut buffer = [0; 8192];
                        let n = stream.read(&mut buffer).unwrap();
                        let request = String::from_utf8_lossy(&buffer[..n]);
                        let path = request.split_whitespace().nth(1).unwrap();
                        let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                        peak.fetch_max(current, Ordering::SeqCst);
                        let _ = sender.send(());
                        let index: usize = path.rsplit("%2D").next().unwrap().parse().unwrap();
                        let (lock, ready) = &*gate;
                        drop(ready.wait_while(lock.lock().unwrap(), |(open, selected)| !*open && *selected != Some(index)).unwrap());
                        let (status, body) = if index == 7 {
                            (503, vec![])
                        } else {
                            (200, serde_json::to_vec(&json!({"downloads": index})).unwrap())
                        };
                        // End the in-flight interval before the client can start its next request.
                        active.fetch_sub(1, Ordering::SeqCst);
                        write!(stream, "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
                        let _ = stream.write_all(&body);
                    });
                }
            });
        });
        Self {
            base,
            started,
            gate,
            peak,
            stop,
            thread: Some(handle),
        }
    }
    fn release(&self) {
        self.gate.0.lock().unwrap().0 = true;
        self.gate.1.notify_all();
    }
    fn release_one(&self, index: usize) {
        self.gate.0.lock().unwrap().1 = Some(index);
        self.gate.1.notify_all();
    }
    fn client(&self) -> Community {
        Community::with_endpoints(Endpoints {
            downloads: self.base.clone(),
            ..Endpoints::default()
        })
        .unwrap()
    }
}
impl Drop for ConcurrentServer {
    fn drop(&mut self) {
        self.release();
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.base.trim_start_matches("http://"));
        self.thread.take().unwrap().join().unwrap();
    }
}

#[test]
fn downloads_overlap_with_at_most_six_workers_and_keep_input_order() {
    let server = ConcurrentServer::new();
    let (_root, store) = store();
    let names: Vec<_> = (0..14)
        .map(|i| format!("@aidget-community/test-{i}"))
        .collect();
    let client = server.client();
    let (started, counts) = thread::scope(|scope| {
        let pending = scope.spawn(|| client.downloads(&store, &names, |_| Ok(())));
        let started = (0..6).all(|_| server.started.recv_timeout(Duration::from_secs(3)).is_ok());
        server.release();
        (started, pending.join().unwrap().unwrap())
    });
    assert!(
        started,
        "six independent requests must reach the server before any response is released"
    );
    assert_eq!(server.peak.load(Ordering::SeqCst), 6);
    assert_eq!(
        counts.iter().map(|v| &v.package).collect::<Vec<_>>(),
        names.iter().collect::<Vec<_>>()
    );
    for (index, count) in counts.iter().enumerate() {
        if index == 7 {
            assert_eq!(count.weekly, None);
            assert!(count.error.is_some());
        } else {
            assert_eq!(count.weekly, Some(index as u64));
            assert!(count.error.is_none());
        }
    }
}

#[test]
fn validates_all_names_before_sending_any_requests() {
    let server = Server::new();
    let (_root, store) = store();
    assert!(server
        .client()
        .downloads(&store, &["valid".into(), "../invalid".into()], |_| Ok(()))
        .is_err());
    assert!(server.requests.lock().unwrap().is_empty());
}

#[test]
fn cache_errors_are_not_converted_to_missing_statistics() {
    let server = Server::new();
    let (_root, store) = store();
    let cache = store.state_dir().join("community/downloads");
    std::fs::create_dir_all(&cache).unwrap();
    let path = cache.join(format!("{}.json", super::sha256_hex(b"note")));
    std::fs::write(&path, b"invalid json").unwrap();
    assert!(server
        .client()
        .downloads(&store, &["note".into()], |_| Ok(()))
        .is_err());
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    assert!(server
        .client()
        .downloads(&store, &["note".into()], |_| Ok(()))
        .is_err());
    assert!(server.requests.lock().unwrap().is_empty());
}

#[test]
fn cached_zero_skips_http_while_missing_counts_keep_their_error() {
    let server = Server::new();
    let (_root, store) = store();
    let name = "@aidget-community/note".to_owned();
    let path = format!("/downloads/point/last-week/{}", encoded_package(&name));
    server.put(&path, 200, json!({"downloads":0}));
    let client = server.client();
    assert_eq!(
        client
            .downloads(&store, std::slice::from_ref(&name), |_| Ok(()))
            .unwrap()[0]
            .weekly,
        Some(0)
    );
    let requests = server.requests.lock().unwrap().len();
    server.bytes(&path, 503, vec![]);
    assert_eq!(
        client.downloads(&store, &[name], |_| Ok(())).unwrap()[0].weekly,
        Some(0)
    );
    assert_eq!(server.requests.lock().unwrap().len(), requests);
    let unavailable = client
        .downloads(&store, &["other".into()], |_| Ok(()))
        .unwrap();
    assert_eq!(unavailable[0].weekly, None);
    assert!(unavailable[0].error.is_some());
}

#[test]
fn fresh_and_expired_caches_preserve_input_order_with_uncached_results() {
    let server = Server::new();
    let (_root, store) = store();
    let names = vec![
        "expired".to_string(),
        "missing".to_string(),
        "cached".to_string(),
    ];
    let cache = store.state_dir().join("community/downloads");
    std::fs::create_dir_all(&cache).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    for (name, at, count) in [("expired", now - 3601, 99), ("cached", now, 0)] {
        std::fs::write(
            cache.join(format!("{}.json", super::sha256_hex(name.as_bytes()))),
            serde_json::to_vec(&json!({"at": at, "count": count})).unwrap(),
        )
        .unwrap();
    }
    server.put(
        "/downloads/point/last-week/expired",
        200,
        json!({"downloads": 42}),
    );
    let values = server
        .client()
        .downloads(&store, &names, |_| Ok(()))
        .unwrap();
    assert_eq!(
        values.iter().map(|v| &v.package).collect::<Vec<_>>(),
        names.iter().collect::<Vec<_>>()
    );
    assert_eq!(
        values.iter().map(|v| v.weekly).collect::<Vec<_>>(),
        vec![Some(42), None, Some(0)]
    );
    let requests = server.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(!requests.iter().any(|p| p.ends_with("/cached")));
}

#[test]
fn cache_write_errors_propagate_after_a_successful_http_response() {
    let server = ConcurrentServer::new();
    let (_root, store) = store();
    let client = server.client();
    let names = vec!["test-0".into()];
    let result = thread::scope(|scope| {
        let pending = scope.spawn(|| client.downloads(&store, &names, |_| Ok(())));
        let started = server.started.recv_timeout(Duration::from_secs(3));
        if started.is_ok() {
            let cache = store.state_dir().join("community/downloads");
            std::fs::remove_dir(&cache).unwrap();
            std::fs::write(&cache, b"not a directory").unwrap();
        }
        server.release();
        let result = pending.join().unwrap();
        assert!(started.is_ok());
        result
    });
    assert!(result.is_err());
}

#[test]
fn empty_requests_and_overlarge_batches_do_not_send_http_requests() {
    let server = Server::new();
    let (_root, store) = store();
    let client = server.client();
    assert!(client
        .downloads(&store, &[], |_| Ok(()))
        .unwrap()
        .is_empty());
    assert!(client
        .downloads(&store, &vec!["note".into(); 101], |_| Ok(()))
        .is_err());
    assert!(server.requests.lock().unwrap().is_empty());
}

#[test]
fn overlapping_batches_can_cache_the_same_package() {
    let server = ConcurrentServer::new();
    let (_root, store) = store();
    let names = vec!["test-0".into()];
    let client = server.client();
    thread::scope(|scope| {
        let batches: Vec<_> = (0..6)
            .map(|_| scope.spawn(|| client.downloads(&store, &names, |_| Ok(()))))
            .collect();
        let started = (0..6).all(|_| server.started.recv_timeout(Duration::from_secs(3)).is_ok());
        server.release();
        for batch in batches {
            let values = batch
                .join()
                .unwrap()
                .expect("concurrent cache writes must succeed");
            assert_eq!(values[0].weekly, Some(0));
        }
        assert!(started);
    });
}

#[test]
fn streams_cached_and_finished_counts_before_the_slowest_request() {
    let server = ConcurrentServer::new();
    let (_root, store) = store();
    let names = vec![
        "cached".into(),
        "test-0".into(),
        "test-7".into(),
        "test-1".into(),
    ];
    let cache = store.state_dir().join("community/downloads");
    std::fs::create_dir_all(&cache).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    std::fs::write(
        cache.join(format!("{}.json", super::sha256_hex(b"cached"))),
        serde_json::to_vec(&json!({"at":now,"count":15})).unwrap(),
    )
    .unwrap();
    let (sender, received) = mpsc::channel::<DownloadCount>();
    let client = server.client();
    let callback = |count: &DownloadCount| {
        sender.send(count.clone()).unwrap();
        Ok(())
    };
    let (cached, zero, unavailable, values) = thread::scope(|scope| {
        let pending = scope.spawn(|| client.downloads(&store, &names, callback));
        let cached = received.recv_timeout(Duration::from_secs(3));
        let started = (0..3).all(|_| server.started.recv_timeout(Duration::from_secs(3)).is_ok());
        server.release_one(0);
        let zero = received.recv_timeout(Duration::from_secs(3));
        server.release_one(7);
        let unavailable = received.recv_timeout(Duration::from_secs(3));
        let still_pending = !pending.is_finished();
        server.release();
        let values = pending.join().unwrap().unwrap();
        assert!(started);
        assert!(
            still_pending,
            "progress must arrive while the slowest request is blocked"
        );
        (cached, zero, unavailable, values)
    });
    assert_eq!(
        cached
            .expect("cached counts must stream immediately")
            .weekly,
        Some(15)
    );
    let zero = zero.expect("a completed request must stream before the batch ends");
    assert_eq!(zero.package, "test-0");
    assert_eq!(zero.weekly, Some(0));
    let unavailable = unavailable.expect("failed statistics must stream as missing");
    assert_eq!(unavailable.package, "test-7");
    assert_eq!(unavailable.weekly, None);
    assert!(unavailable.error.is_some());
    assert_eq!(
        values.iter().map(|v| &v.package).collect::<Vec<_>>(),
        names.iter().collect::<Vec<_>>()
    );
}

#[test]
fn callback_errors_propagate_without_becoming_missing_statistics() {
    let server = Server::new();
    let (_root, store) = store();
    server.put(
        "/downloads/point/last-week/note",
        200,
        json!({"downloads":1}),
    );
    let error = server
        .client()
        .downloads(&store, &["note".into()], |_| {
            Err(aidget::common::error::Error::Other(
                "progress channel closed".into(),
            ))
        })
        .unwrap_err();
    assert!(error.to_string().contains("progress channel closed"));
}
