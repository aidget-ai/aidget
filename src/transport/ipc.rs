//! The local bridge between the CLI/MCP and the resident UI.
//!
//! Unix targets use a filesystem socket under `<root>/run`; Windows has no Unix
//! sockets in `std`, so the same `<root>/run/ui.sock` path is mapped onto a named
//! pipe whose name is derived from that path. Callers only ever see an
//! [`Endpoint`], which prints the address that actually exists on this platform.

use std::io;
use std::path::{Path, PathBuf};

/// Address of the resident UI's bridge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    path: PathBuf,
}

impl Endpoint {
    /// Build an endpoint from the `<root>/run/ui.sock` path stored on disk.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Endpoint { path: path.into() }
    }

    /// The Unix socket path, or the named pipe name on Windows.
    pub fn address(&self) -> String {
        #[cfg(windows)]
        {
            format!(r"\\.\pipe\{}", pipe_name(&self.path))
        }
        #[cfg(not(windows))]
        {
            self.path.to_string_lossy().to_string()
        }
    }

    /// The on-disk path that names this endpoint. Only Unix binds it.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Whether a process is listening, without requiring it to answer.
    pub fn is_connectable(&self) -> bool {
        platform::is_connectable(self)
    }

    /// Bind a listener. On Windows this only validates the name; the pipe is
    /// created inside the serving runtime.
    pub fn bind(&self) -> io::Result<Listener> {
        platform::bind(self)
    }

    /// Remove the Unix socket file left behind by a crashed instance.
    pub fn remove_stale(&self) {
        #[cfg(unix)]
        let _ = std::fs::remove_file(&self.path);
    }

    /// Send one request line and return the single response line.
    pub fn exchange(&self, line: &str, timeout: std::time::Duration) -> io::Result<String> {
        platform::exchange(self, line, timeout)
    }
}

/// A connection the bridge can line-read and line-write over.
///
/// Unix sockets and Windows named pipes are different objects, so the accept
/// code hands the server one boxed stream and only accepting stays per-platform.
pub trait Stream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Unpin + 'static {}
impl<T> Stream for T where T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Unpin + 'static {}

/// One accepted bridge connection.
pub type BoxedStream = Box<dyn Stream>;

/// A listener ready to accept bridge connections.
pub enum Listener {
    #[cfg(unix)]
    Unix(std::os::unix::net::UnixListener),
    #[cfg(windows)]
    Windows,
}

/// First named pipe instance, claiming the name exclusively.
#[cfg(windows)]
pub fn first_instance(
    endpoint: &Endpoint,
) -> io::Result<tokio::net::windows::named_pipe::NamedPipeServer> {
    platform::first_instance(endpoint)
}

/// Additional named pipe instance for the next accepted connection.
#[cfg(windows)]
pub fn next_instance(
    endpoint: &Endpoint,
) -> io::Result<tokio::net::windows::named_pipe::NamedPipeServer> {
    platform::next_instance(endpoint)
}

/// Wrap a freshly accepted connection for the bridge.
#[cfg(windows)]
pub fn into_stream(server: tokio::net::windows::named_pipe::NamedPipeServer) -> BoxedStream {
    platform::into_stream(server)
}

/// Wrap a freshly accepted connection for the bridge.
#[cfg(unix)]
pub fn into_stream(stream: tokio::net::UnixStream) -> BoxedStream {
    platform::into_stream(stream)
}

/// Named pipe name for a `<root>/run/ui.sock` path.
///
/// Named pipes live in one global namespace, so the name is derived from the
/// path rather than fixed: separate data directories stay separate, and the
/// path itself is not leaked into a name every local process can enumerate.
pub fn pipe_name(socket_path: &Path) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in socket_path.to_string_lossy().bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("aidget-{hash:016x}")
}

#[cfg(unix)]
mod platform {
    use std::io::{self, BufRead, BufReader, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::time::Duration;

    use tokio::net::UnixStream;

    use super::{BoxedStream, Endpoint, Listener};

    /// Wrap a freshly accepted Unix socket for the bridge.
    pub fn into_stream(stream: UnixStream) -> BoxedStream {
        Box::new(stream)
    }

    pub fn is_connectable(endpoint: &Endpoint) -> bool {
        std::os::unix::net::UnixStream::connect(endpoint.path()).is_ok()
    }

    pub fn bind(endpoint: &Endpoint) -> io::Result<Listener> {
        let path = endpoint.path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if path.exists() {
            if is_connectable(endpoint) {
                return Err(io::Error::new(
                    io::ErrorKind::AddrInUse,
                    "another aidget ui is already listening",
                ));
            }
            // Stale socket from a crashed instance.
            let _ = std::fs::remove_file(path);
        }
        let listener = std::os::unix::net::UnixListener::bind(path)?;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        listener.set_nonblocking(true)?;
        Ok(Listener::Unix(listener))
    }

    pub fn exchange(endpoint: &Endpoint, line: &str, timeout: Duration) -> io::Result<String> {
        let mut stream = std::os::unix::net::UnixStream::connect(endpoint.path())?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        stream.write_all(line.as_bytes())?;
        stream.flush()?;
        let mut response = String::new();
        BufReader::new(&stream).read_line(&mut response)?;
        Ok(response)
    }
}

#[cfg(windows)]
mod platform {
    use std::io;
    use std::time::Duration;

    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeServer, ServerOptions};

    use super::{BoxedStream, Endpoint, Listener};

    /// Wrap a connected pipe instance for the bridge.
    pub fn into_stream(server: NamedPipeServer) -> BoxedStream {
        Box::new(server)
    }

    /// `CreateFile` reports a missing pipe as a raw win32 code, which `std`
    /// classifies as Uncategorized. Normalise it so callers see the same
    /// "the UI is not running" answer they get from `ENOENT` on Unix.
    fn no_pipe(error: io::Error) -> io::Error {
        const ERROR_FILE_NOT_FOUND: i32 = 2;
        const ERROR_PATH_NOT_FOUND: i32 = 3;
        const ERROR_PIPE_BUSY: i32 = 231;
        match error.raw_os_error() {
            Some(code)
                if matches!(
                    code,
                    ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND | ERROR_PIPE_BUSY
                ) =>
            {
                io::Error::new(io::ErrorKind::NotFound, "no aidget ui is listening")
            }
            _ => error,
        }
    }

    pub fn is_connectable(endpoint: &Endpoint) -> bool {
        ClientOptions::new().open(endpoint.address()).is_ok()
    }

    pub fn bind(endpoint: &Endpoint) -> io::Result<Listener> {
        if is_connectable(endpoint) {
            return Err(io::Error::new(
                io::ErrorKind::AddrInUse,
                "another aidget ui is already listening",
            ));
        }
        Ok(Listener::Windows)
    }

    /// The first pipe instance. `first_pipe_instance` closes the race between
    /// the `bind` probe and serving.
    pub fn first_instance(endpoint: &Endpoint) -> io::Result<NamedPipeServer> {
        ServerOptions::new()
            .first_pipe_instance(true)
            .create(endpoint.address())
    }

    pub fn next_instance(endpoint: &Endpoint) -> io::Result<NamedPipeServer> {
        ServerOptions::new().create(endpoint.address())
    }

    pub fn exchange(endpoint: &Endpoint, line: &str, timeout: Duration) -> io::Result<String> {
        let address = endpoint.address();
        let line = line.to_string();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        runtime.block_on(async move {
            let exchange = async {
                let client = ClientOptions::new().open(&address).map_err(no_pipe)?;
                let (read, mut write) = tokio::io::split(client);
                let mut reader = BufReader::new(read);
                write.write_all(line.as_bytes()).await?;
                write.flush().await?;
                let mut response = String::new();
                if reader.read_line(&mut response).await? == 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "ui closed the connection",
                    ));
                }
                Ok(response)
            };
            match tokio::time::timeout(timeout, exchange).await {
                Ok(result) => result,
                Err(_) => Err(io::Error::new(io::ErrorKind::TimedOut, "timeout")),
            }
        })
    }
}
