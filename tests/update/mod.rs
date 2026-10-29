mod download;
mod fetch;
mod github;
mod install;
mod service;
mod snapshot;
mod stage;
mod target;
mod verify;
mod version;

use std::io::{Read, Write};
use std::net::TcpListener;

/// A one-shot HTTP server that answers every request with `body`.
///
/// Returns the base URL to hand to `UpdateService::pointing_at`, so a check can
/// run against a local feed instead of GitHub.
pub fn serve_once(status: u16, body: &str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let body = body.to_string();
    std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut request = [0u8; 2048];
        let _ = stream.read(&mut request);
        let response = format!(
            "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    });
    format!("http://{address}")
}

/// A server that answers after `delay`, so a check stays in flight long enough
/// for the test to look at it.
pub fn serve_slow(delay: std::time::Duration, body: &str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let body = body.to_string();
    std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut request = [0u8; 2048];
        let _ = stream.read(&mut request);
        std::thread::sleep(delay);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    });
    format!("http://{address}")
}

/// Replace every occurrence of `needle` in `haystack`.
/// Fill the placeholders a mock body may carry: the server's own address, and the
/// asset name this machine would install.
fn fill(body: &[u8], address: std::net::SocketAddr) -> Vec<u8> {
    let body = replace_bytes(body, b"__BASE__", format!("http://{address}").as_bytes());
    replace_bytes(&body, b"__ASSET__", release_asset_name().as_bytes())
}

fn replace_bytes(haystack: &[u8], needle: &[u8], with: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(haystack.len());
    let mut rest = haystack;
    while let Some(at) = rest
        .windows(needle.len())
        .position(|window| window == needle)
    {
        out.extend_from_slice(&rest[..at]);
        out.extend_from_slice(with);
        rest = &rest[at + needle.len()..];
    }
    out.extend_from_slice(rest);
    out
}

/// A server that answers each request whose path contains one of `routes`,
/// in any order, for as long as the test binary runs.
pub fn serve_routes(routes: Vec<(&'static str, Vec<u8>)>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut request = [0u8; 4096];
            let read = stream.read(&mut request).unwrap_or(0);
            let request = String::from_utf8_lossy(&request[..read]).to_string();
            let path = request.split_whitespace().nth(1).unwrap_or("").to_string();
            let body = routes
                .iter()
                .find(|(needle, _)| path.contains(needle))
                .map(|(_, body)| body.clone())
                .unwrap_or_default();
            // A served document can point back at this server without knowing
            // its address when the test was written. Replaced on the raw bytes:
            // an asset body need not be UTF-8, and decoding it would corrupt it.
            let body = fill(&body, address);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.write_all(&body);
            let _ = stream.flush();
        }
    });
    format!("http://{address}")
}

/// A server whose release feed answers `body` once and then fails with `status`
/// and `message`, so a later check fails while an earlier download stands.
pub fn serve_feed_then(status: u16, message: &str, body: &str, asset: Vec<u8>) -> String {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let body = body.to_string();
    let failure = format!(r#"{{"message":"{message}"}}"#);
    let hits = Arc::new(AtomicUsize::new(0));
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut request = [0u8; 4096];
            let read = stream.read(&mut request).unwrap_or(0);
            let request = String::from_utf8_lossy(&request[..read]).to_string();
            let path = request.split_whitespace().nth(1).unwrap_or("").to_string();
            let (status, extra, payload) = if path.contains("/releases/latest") {
                if hits.fetch_add(1, Ordering::SeqCst) == 0 {
                    let body = fill(body.as_bytes(), address);
                    (200, String::new(), body)
                } else if status == 403 || status == 429 {
                    (
                        status,
                        "x-ratelimit-remaining: 0\r\nx-ratelimit-reset: 0\r\n".to_string(),
                        failure.clone().into_bytes(),
                    )
                } else {
                    (status, String::new(), failure.clone().into_bytes())
                }
            } else {
                (200, String::new(), asset.clone())
            };
            let head = format!(
                "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n",
                payload.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&payload);
            let _ = stream.flush();
        }
    });
    format!("http://{address}")
}

/// Remove a staged download and the directory it lives in.
pub fn cleanup(staged: std::path::PathBuf) {
    if let Some(dir) = staged.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// The sha-256 a release would publish for `bytes`, as lowercase hex.
pub fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The asset name a release would carry for the machine running the test.
///
/// A release published for this platform, so a test that downloads goes through
/// the same asset selection the app does. Hard-coding one platform's name made
/// every download test fail elsewhere, with the service correctly refusing an
/// asset that does not belong to the machine.
pub fn release_asset_name() -> String {
    let target = aidget::update::target::current().expect("this platform has an install shape");
    format!("aidget_{VERSION}_{}{}", target.arch, target.kind.suffix())
}

/// The version the mock releases claim.
pub const VERSION: &str = "v99.0.0";
