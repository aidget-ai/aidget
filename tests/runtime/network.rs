use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::Duration,
};

use aidget::{
    runtime::network::{authorize_url, FetchOptions, NetworkClient, NetworkFailure},
    widgets::model::{WidgetMeta, WidgetPermission},
};
use reqwest::Url;

fn widget(permissions: &[WidgetPermission], network: &[&str]) -> WidgetMeta {
    let mut meta = WidgetMeta::new("hot-list", "Hot List");
    meta.permissions = permissions.to_vec();
    meta.network = network.iter().map(|origin| (*origin).to_string()).collect();
    meta
}

#[test]
fn authorization_requires_script_and_an_exact_declared_origin() {
    let meta = widget(
        &[WidgetPermission::Script],
        &["https://api.example.com", "https://www.example.com"],
    );

    assert_eq!(
        authorize_url(&meta, "https://api.example.com/items?limit=10")
            .unwrap()
            .origin()
            .ascii_serialization(),
        "https://api.example.com"
    );
    assert_eq!(
        authorize_url(&meta, "https://api.example.com.evil.test/items")
            .unwrap_err()
            .code,
        "permission_denied"
    );
    assert_eq!(
        authorize_url(&meta, "https://undeclared.example.com/items")
            .unwrap_err()
            .code,
        "permission_denied"
    );
}

#[test]
fn authorization_rejects_unsafe_url_shapes_and_missing_script_grant() {
    let secure = widget(&[WidgetPermission::Script], &["https://api.example.com"]);
    for url in [
        "http://api.example.com/items",
        "https://user:secret@api.example.com/items",
        "not a URL",
    ] {
        assert_eq!(
            authorize_url(&secure, url).unwrap_err().code,
            "invalid_params"
        );
    }

    let no_script = widget(&[], &["https://api.example.com"]);
    let failure = authorize_url(&no_script, "https://api.example.com/items").unwrap_err();
    assert_eq!(failure.code, "permission_denied");
    assert!(failure.message.contains("script"));
}

#[test]
fn authorization_preserves_fragments_for_the_system_browser() {
    let meta = widget(&[WidgetPermission::Script], &["https://www.example.com"]);

    let url = authorize_url(&meta, "https://www.example.com/story#details").unwrap();

    assert_eq!(url.fragment(), Some("details"));
}

#[test]
fn fetch_json_sends_no_application_json_default_and_parses_the_body() {
    let (origin, request) = serve(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\n\r\n{\"ok\":true}",
    );
    let client = NetworkClient::new().unwrap();
    let value = client
        .fetch_json(
            Url::parse(&format!("{origin}/api?limit=10")).unwrap(),
            &FetchOptions::default(),
        )
        .unwrap();

    assert_eq!(value["ok"], true);
    let headers = request.join().unwrap()[0].to_ascii_lowercase();
    assert!(!headers.contains("accept: application/json"));
    assert!(!headers.contains("referer:"));
}

#[test]
fn fetch_json_uses_caller_method_headers_and_body() {
    let (origin, request) = serve(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\n\r\n{\"ok\":true}",
    );
    let value = NetworkClient::new()
        .unwrap()
        .fetch_json(
            Url::parse(&format!("{origin}/submit")).unwrap(),
            &FetchOptions {
                method: "POST".into(),
                headers: BTreeMap::from([
                    (
                        "Accept".to_string(),
                        "application/vnd.example+json".to_string(),
                    ),
                    ("X-Token".to_string(), "secret".to_string()),
                ]),
                body: Some("{\"sent\":true}".to_string()),
            },
        )
        .unwrap();

    assert_eq!(value["ok"], true);
    let request = request.join().unwrap().remove(0);
    let lower = request.to_ascii_lowercase();
    assert!(request.starts_with("POST /submit "));
    assert!(lower.contains("accept: application/vnd.example+json"));
    assert!(lower.contains("x-token: secret"));
    assert!(request.ends_with("{\"sent\":true}"));
}

#[test]
fn fetch_json_rejects_invalid_methods_headers_and_get_bodies() {
    let url = Url::parse("http://127.0.0.1:9/unreachable").unwrap();
    let client = NetworkClient::new().unwrap();
    let cases = [
        FetchOptions {
            method: "BAD METHOD".to_string(),
            ..FetchOptions::default()
        },
        FetchOptions {
            method: "HEAD".to_string(),
            ..FetchOptions::default()
        },
        FetchOptions {
            headers: BTreeMap::from([("Host".to_string(), "example.com".to_string())]),
            ..FetchOptions::default()
        },
        FetchOptions {
            body: Some("{}".to_string()),
            ..FetchOptions::default()
        },
        FetchOptions {
            method: "POST".to_string(),
            body: Some("x".repeat(256 * 1024 + 1)),
            ..FetchOptions::default()
        },
    ];

    for options in cases {
        assert_eq!(
            client.fetch_json(url.clone(), &options).unwrap_err().code,
            "invalid_params"
        );
    }
}

#[test]
fn fetch_json_rejects_http_errors_and_non_json_bodies() {
    let (status_origin, status_request) =
        serve("HTTP/1.1 403 Forbidden\r\nContent-Length: 9\r\n\r\nforbidden");
    let status = NetworkClient::new()
        .unwrap()
        .fetch_json(
            Url::parse(&format!("{status_origin}/api")).unwrap(),
            &FetchOptions::default(),
        )
        .unwrap_err();
    assert_eq!(
        status,
        NetworkFailure {
            code: "http_error",
            message: format!("GET {status_origin}/api: HTTP 403 Forbidden"),
        }
    );
    status_request.join().unwrap();

    let (json_origin, json_request) =
        serve("HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 8\r\n\r\nnot json");
    let invalid = NetworkClient::new()
        .unwrap()
        .fetch_json(
            Url::parse(&format!("{json_origin}/api")).unwrap(),
            &FetchOptions::default(),
        )
        .unwrap_err();
    assert_eq!(invalid.code, "invalid_response");
    json_request.join().unwrap();
}

#[test]
fn fetch_json_follows_same_origin_redirects_and_rejects_oversized_bodies() {
    let (origin, request) = serve_sequence(&[
        "HTTP/1.1 302 Found\r\nLocation: /ok\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\n\r\n{\"ok\":true}",
    ]);
    let value = NetworkClient::new()
        .unwrap()
        .fetch_json(
            Url::parse(&format!("{origin}/redirect")).unwrap(),
            &FetchOptions::default(),
        )
        .unwrap();
    assert_eq!(value["ok"], true);
    let requests = request.join().unwrap();
    assert!(requests[1].starts_with("GET /ok "));

    let (origin, request) = serve("HTTP/1.1 200 OK\r\nContent-Length: 1048577\r\n\r\n");
    let failure = NetworkClient::new()
        .unwrap()
        .fetch_json(
            Url::parse(&format!("{origin}/large")).unwrap(),
            &FetchOptions::default(),
        )
        .unwrap_err();
    assert_eq!(failure.code, "invalid_response");
    request.join().unwrap();
}

#[test]
fn fetch_json_allows_three_same_origin_redirects() {
    let (origin, request) = serve_sequence(&[
        "HTTP/1.1 302 Found\r\nLocation: /one\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        "HTTP/1.1 302 Found\r\nLocation: /two\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        "HTTP/1.1 302 Found\r\nLocation: /three\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\n\r\n{\"ok\":true}",
    ]);
    let result = NetworkClient::new().unwrap().fetch_json(
        Url::parse(&format!("{origin}/start")).unwrap(),
        &FetchOptions::default(),
    );

    if result.is_ok() {
        assert_eq!(request.join().unwrap().len(), 4);
    }
    assert_eq!(result.unwrap()["ok"], true);
}

#[test]
fn fetch_json_reports_a_stalled_body_as_timeout() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut request = Vec::new();
        let mut chunk = [0; 1024];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let read = stream.read(&mut chunk).unwrap();
            if read == 0 {
                break;
            }
            request.extend_from_slice(&chunk[..read]);
        }
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\n\r\n",
            )
            .unwrap();
        // Hold the body open past the client's 12-second request deadline.
        thread::sleep(Duration::from_millis(12_500));
    });

    let failure = NetworkClient::new()
        .unwrap()
        .fetch_json(
            Url::parse(&format!("{origin}/slow")).unwrap(),
            &FetchOptions::default(),
        )
        .unwrap_err();

    assert_eq!(failure.code, "timeout");
    assert_eq!(failure.message, format!("GET {origin}/slow: timed out"));
    server.join().unwrap();
}

fn serve(response: &str) -> (String, thread::JoinHandle<Vec<String>>) {
    serve_sequence(&[response])
}

fn serve_sequence(responses: &[&str]) -> (String, thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let responses: Vec<String> = responses
        .iter()
        .map(|response| response.to_string())
        .collect();
    let handle = thread::spawn(move || {
        let mut requests = Vec::new();
        for response in responses {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut chunk = [0; 1024];
            while !bytes.windows(4).any(|window| window == b"\r\n\r\n") {
                let read = stream.read(&mut chunk).unwrap();
                if read == 0 {
                    break;
                }
                bytes.extend_from_slice(&chunk[..read]);
            }
            let head_end = bytes
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .map_or(bytes.len(), |index| index + 4);
            let content_length = String::from_utf8_lossy(&bytes[..head_end])
                .lines()
                .find_map(|line| {
                    line.split_once(':').and_then(|(name, value)| {
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().unwrap())
                    })
                })
                .unwrap_or(0);
            while bytes.len() < head_end + content_length {
                let read = stream.read(&mut chunk).unwrap();
                if read == 0 {
                    break;
                }
                bytes.extend_from_slice(&chunk[..read]);
            }
            stream.write_all(response.as_bytes()).unwrap();
            requests.push(String::from_utf8(bytes).unwrap());
        }
        requests
    });
    (origin, handle)
}
