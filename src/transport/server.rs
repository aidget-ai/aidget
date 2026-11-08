use std::path::Path;
use std::sync::Arc;

use serde_json::Value;
use tauri::AppHandle;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::common::error::Error;
use crate::transport::bridge::{Request, Response};
use crate::transport::handlers;
use crate::transport::ipc::{BoxedStream, Endpoint, Listener};
use crate::widgets::store::Store;

pub fn bind(path: &Path) -> std::io::Result<Listener> {
    Endpoint::new(path).bind()
}

pub fn serve(
    app: AppHandle,
    store: Arc<Store>,
    listener: Listener,
    endpoint: Endpoint,
) -> std::io::Result<()> {
    // Report thread-side startup failures back to `setup` instead of leaving
    // a bound socket that nothing is accepting on.
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
    let endpoint_for_thread = endpoint.clone();
    std::thread::Builder::new()
        .name("aidget-bridge".to_string())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    let _ = ready_tx.send(Err(error));
                    endpoint_for_thread.remove_stale();
                    return;
                }
            };
            let accepting = match platform::listener(&runtime, listener, &endpoint_for_thread) {
                Ok(accepting) => accepting,
                Err(error) => {
                    let _ = ready_tx.send(Err(error));
                    endpoint_for_thread.remove_stale();
                    return;
                }
            };
            if ready_tx.send(Ok(())).is_err() {
                return;
            }
            runtime.block_on(async move {
                // One startup line that confirms the bridge thread is live.
                eprintln!(
                    "aidget: bridge accepting on {}",
                    endpoint_for_thread.address()
                );
                accepting.run(app, store, endpoint_for_thread).await;
            });
        })
        .map_err(std::io::Error::other)?;
    match ready_rx.recv() {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(error),
        Err(_) => Err(std::io::Error::other("bridge thread exited during startup")),
    }
}

#[cfg(unix)]
mod platform {
    use std::sync::Arc;

    use tauri::AppHandle;
    use tokio::net::UnixListener;

    use super::{handle, BoxedStream, Endpoint, Listener, Store};

    pub struct Accepting {
        listener: UnixListener,
    }

    /// `from_std` needs the runtime's reactor, so build it while entered.
    pub fn listener(
        runtime: &tokio::runtime::Runtime,
        listener: Listener,
        _endpoint: &Endpoint,
    ) -> std::io::Result<Accepting> {
        let Listener::Unix(listener) = listener;
        let _guard = runtime.enter();
        let listener = UnixListener::from_std(listener)?;
        Ok(Accepting { listener })
    }

    impl Accepting {
        pub async fn run(self, app: AppHandle, store: Arc<Store>, _endpoint: Endpoint) {
            loop {
                match self.listener.accept().await {
                    Ok((stream, _)) => {
                        let app = app.clone();
                        let store = Arc::clone(&store);
                        let stream: BoxedStream = crate::transport::ipc::into_stream(stream);
                        tokio::spawn(async move {
                            if let Err(error) = handle(stream, app, store).await {
                                eprintln!("aidget: bridge connection error: {error}");
                            }
                        });
                    }
                    Err(error) => {
                        eprintln!("aidget: bridge accept error: {error}");
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    }
                }
            }
        }
    }
}

#[cfg(windows)]
mod platform {
    use std::sync::Arc;

    use tauri::AppHandle;
    use tokio::net::windows::named_pipe::NamedPipeServer;

    use super::{handle, BoxedStream, Endpoint, Listener, Store};
    use crate::transport::ipc;

    /// The first pipe instance. Holding it until `run` starts keeps the name
    /// claimed: `first_pipe_instance` fails if another process got there first.
    pub struct Accepting {
        instance: NamedPipeServer,
    }

    pub fn listener(
        runtime: &tokio::runtime::Runtime,
        listener: Listener,
        endpoint: &Endpoint,
    ) -> std::io::Result<Accepting> {
        let Listener::Windows = listener;
        let _guard = runtime.enter();
        Ok(Accepting {
            instance: ipc::first_instance(endpoint)?,
        })
    }

    impl Accepting {
        pub async fn run(self, app: AppHandle, store: Arc<Store>, endpoint: Endpoint) {
            let mut instance = self.instance;
            loop {
                if let Err(error) = instance.connect().await {
                    eprintln!("aidget: bridge connect error: {error}");
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    continue;
                }
                // The next instance must exist before this one is handed to a
                // connection task, otherwise a client can arrive in between.
                let next: std::io::Result<NamedPipeServer> = ipc::next_instance(&endpoint);
                let connected = std::mem::replace(
                    &mut instance,
                    match next {
                        Ok(next) => next,
                        Err(error) => {
                            eprintln!("aidget: bridge cannot create pipe: {error}");
                            return;
                        }
                    },
                );
                let app = app.clone();
                let store = Arc::clone(&store);
                let connected: BoxedStream = crate::transport::ipc::into_stream(connected);
                tokio::spawn(async move {
                    if let Err(error) = handle(connected, app, store).await {
                        eprintln!("aidget: bridge connection error: {error}");
                    }
                });
            }
        }
    }
}

pub mod protocol {
    use std::path::{Component, Path, PathBuf};

    use tauri::http::{Request, Response, StatusCode};
    use tauri::UriSchemeContext;

    use crate::transport::origin;
    use crate::widgets::store::{Store, HTML_FILE};

    static UI_DIST: include_dir::Dir<'_> = include_dir::include_dir!("$CARGO_MANIFEST_DIR/ui/dist");

    pub fn handle<R: tauri::Runtime>(
        _ctx: &UriSchemeContext<'_, R>,
        request: Request<Vec<u8>>,
        store: &Store,
    ) -> Response<Vec<u8>> {
        handle_with_override(_ctx, request, store, false)
    }

    pub fn handle_with_override<R: tauri::Runtime>(
        _ctx: &UriSchemeContext<'_, R>,
        request: Request<Vec<u8>>,
        store: &Store,
        allow_incompatible: bool,
    ) -> Response<Vec<u8>> {
        let host = request.uri().host().unwrap_or_default();
        if !origin::is_scheme_host(host) {
            return not_found();
        }
        let path = request.uri().path();
        if path == "/" || path == "/main" || path == "/main/" {
            return serve_embedded("main.html");
        }
        if path == "/widget" || path == "/widget/" {
            return serve_embedded("widget.html");
        }
        if let Some(rest) = path.strip_prefix("/content/") {
            return serve_widget_with_override(store, rest, allow_incompatible);
        }
        let asset = path.trim_start_matches('/');
        if asset.ends_with(".html") {
            return not_found();
        }
        serve_embedded(asset)
    }

    fn serve_embedded(path: &str) -> Response<Vec<u8>> {
        match UI_DIST.get_file(path) {
            Some(file) => response(
                file.contents().to_vec(),
                content_type(path),
                None,
                embedded_cache_control(path),
            ),
            None => not_found(),
        }
    }

    /// Cache policy for files embedded in the application bundle.
    pub fn embedded_cache_control(path: &str) -> &'static str {
        if path.starts_with("assets/") {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        }
    }

    /// Serve assets with the widget's CSP, including non-HTML document types such as SVG.
    pub fn serve_widget(store: &Store, rest: &str) -> Response<Vec<u8>> {
        serve_widget_with_override(store, rest, false)
    }

    pub fn serve_widget_with_override(
        store: &Store,
        rest: &str,
        allow_incompatible: bool,
    ) -> Response<Vec<u8>> {
        let Ok(decoded) = percent_encoding::percent_decode_str(rest).decode_utf8() else {
            return not_found();
        };
        let mut parts = decoded.splitn(2, '/');
        let Some(id) = parts.next() else {
            return not_found();
        };
        let relative = parts.next().unwrap_or(HTML_FILE);
        let Ok(widget_dir) = store.widget_dir(id) else {
            return not_found();
        };
        let Some(safe) = sanitize(relative) else {
            return not_found();
        };
        if safe != std::path::Path::new("index.html")
            && safe != std::path::Path::new("style.css")
            && !safe.starts_with("assets")
        {
            return not_found();
        }
        let path = widget_dir.join(safe);
        let Ok(real) = path.canonicalize() else {
            return not_found();
        };
        let Ok(root) = widget_dir.canonicalize() else {
            return not_found();
        };
        if !real.starts_with(root) {
            return not_found();
        }
        let Ok(bytes) = std::fs::read(&path) else {
            return not_found();
        };
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let Ok(meta) = store.load_meta(id) else {
            return not_found();
        };
        let Ok(compatibility) = crate::widgets::package::HostCompatibility::evaluate(
            &meta.config,
            env!("CARGO_PKG_VERSION"),
            allow_incompatible,
        ) else {
            return not_found();
        };
        if !compatibility.permits_runtime() {
            return not_found();
        }
        let html = if content_type(name).starts_with("text/html") {
            Some(bytes.as_slice())
        } else {
            None
        };
        let csp = match crate::transport::csp::for_document(&meta.config, origin::origin(), html) {
            Ok(csp) => csp,
            Err(error) => {
                return Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .header("Content-Type", "text/plain; charset=utf-8")
                    .header("Content-Security-Policy", "default-src 'none'")
                    .body(error.to_string().into_bytes())
                    .expect("static error response")
            }
        };
        response(bytes, content_type(name), Some(&csp), "no-store")
    }

    /// Restrict every resource type to this widget and its exact declared origins.
    pub fn widget_csp(config: &crate::widgets::package::WidgetConfig) -> String {
        widget_csp_for(config, origin::origin())
    }

    /// Asset policy without inline-script grants; parameterized for platform origins.
    pub fn widget_csp_for(config: &crate::widgets::package::WidgetConfig, origin: &str) -> String {
        crate::transport::csp::for_document(config, origin, None)
            .expect("a policy without document bytes cannot fail")
    }

    fn sanitize(relative: &str) -> Option<PathBuf> {
        let path = Path::new(relative);
        let mut clean = PathBuf::new();
        for component in path.components() {
            match component {
                Component::Normal(part) => {
                    let text = part.to_str()?;
                    if text.starts_with('.') {
                        return None;
                    }
                    clean.push(part);
                }
                Component::CurDir => {}
                _ => return None,
            }
        }
        if clean.as_os_str().is_empty() {
            None
        } else {
            Some(clean)
        }
    }

    fn content_type(name: &str) -> &'static str {
        match name.rsplit('.').next() {
            Some("html") => "text/html; charset=utf-8",
            Some("css") => "text/css; charset=utf-8",
            Some("js") => "text/javascript; charset=utf-8",
            Some("json") => "application/json; charset=utf-8",
            Some("svg") => "image/svg+xml",
            Some("png") => "image/png",
            Some("jpg") | Some("jpeg") => "image/jpeg",
            Some("webp") => "image/webp",
            Some("woff2") => "font/woff2",
            _ => "application/octet-stream",
        }
    }

    fn response(
        bytes: Vec<u8>,
        mime: &str,
        csp: Option<&str>,
        cache_control: &str,
    ) -> Response<Vec<u8>> {
        let builder = Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", mime)
            .header("Cache-Control", cache_control)
            .header("X-Content-Type-Options", "nosniff")
            .header("Access-Control-Allow-Origin", "*");
        let builder = if let Some(csp) = csp {
            builder.header("Content-Security-Policy", csp)
        } else {
            builder
        };
        builder.body(bytes).unwrap_or_else(|_| not_found())
    }

    fn not_found() -> Response<Vec<u8>> {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header("Content-Type", "text/plain; charset=utf-8")
            .body(b"not found".to_vec())
            .expect("static response")
    }
}

async fn handle(stream: BoxedStream, app: AppHandle, store: Arc<Store>) -> std::io::Result<()> {
    let (read, mut write) = tokio::io::split(stream);
    let mut reader = BufReader::new(read);
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).await? == 0 {
            return Ok(());
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Request>(trimmed) {
            Ok(request) => dispatch(&app, &store, request).await,
            Err(error) => Response::err(
                "unknown",
                &Error::InvalidParams(format!("malformed request: {error}")),
            ),
        };
        let mut encoded = serde_json::to_string(&response)
            .unwrap_or_else(|_| "{\"id\":\"unknown\",\"ok\":false}".to_string());
        encoded.push('\n');
        write.write_all(encoded.as_bytes()).await?;
        write.flush().await?;
    }
}

async fn dispatch(app: &AppHandle, store: &Arc<Store>, request: Request) -> Response {
    let id = request.id.clone();
    let result: Result<Value, Error> =
        handlers::dispatch(app.clone(), store.clone(), &request.method, request.params).await;
    match result {
        Ok(value) => Response::ok(&id, value),
        Err(error) => Response::err(&id, &error),
    }
}
