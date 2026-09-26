//! The local socket: a Unix domain socket on Unix and a named pipe on Windows.

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

pub use platform::{Endpoint, Listener, connect};

/// A connected byte stream from either platform's socket.
pub struct BoxedConnection(Box<dyn Connection>);

trait Connection: AsyncRead + AsyncWrite + Send + Unpin + 'static {}

impl<T: AsyncRead + AsyncWrite + Send + Unpin + 'static> Connection for T {}

impl BoxedConnection {
    fn new(connection: impl AsyncRead + AsyncWrite + Send + Unpin + 'static) -> BoxedConnection {
        BoxedConnection(Box::new(connection))
    }
}

impl AsyncRead for BoxedConnection {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.0).poll_read(context, buffer)
    }
}

impl AsyncWrite for BoxedConnection {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut *self.0).poll_write(context, buffer)
    }

    fn poll_flush(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.0).poll_flush(context)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.0).poll_shutdown(context)
    }
}

#[cfg(unix)]
mod platform {
    use std::path::PathBuf;
    use std::{env, io};

    use tempfile::TempDir;
    use tokio::net::{UnixListener, UnixStream};

    use super::BoxedConnection;

    /// macOS limits socket paths to 104 bytes including the terminator, and Linux to 108.
    const MAXIMUM_SOCKET_PATH: usize = 100;

    /// A socket path inside a new private directory, removed with the directory when dropped.
    ///
    /// The socket does not live in the output directory because a project's output directory can
    /// be deeper than a socket path may be long.
    pub struct Endpoint {
        _directory: TempDir,
        name: String,
    }

    impl Endpoint {
        pub fn create() -> io::Result<Endpoint> {
            // macOS's temporary directory is long enough that a deep one may not fit.
            for base in [env::temp_dir(), PathBuf::from("/tmp")] {
                let directory = tempfile::Builder::new().prefix("iris-watch-").tempdir_in(base)?;
                let name = directory.path().join("socket").to_string_lossy().into_owned();
                if name.len() <= MAXIMUM_SOCKET_PATH {
                    return Ok(Endpoint { _directory: directory, name });
                }
            }
            Err(io::Error::other("no temporary directory gives a short enough socket path"))
        }

        pub fn name(&self) -> &str {
            &self.name
        }
    }

    pub struct Listener {
        listener: UnixListener,
    }

    impl Listener {
        pub fn bind(endpoint: &Endpoint) -> io::Result<Listener> {
            Ok(Listener { listener: UnixListener::bind(endpoint.name())? })
        }

        pub async fn accept(&mut self) -> io::Result<BoxedConnection> {
            let (stream, _) = self.listener.accept().await?;
            Ok(BoxedConnection::new(stream))
        }
    }

    pub async fn connect(name: &str) -> io::Result<BoxedConnection> {
        Ok(BoxedConnection::new(UnixStream::connect(name).await?))
    }
}

#[cfg(windows)]
mod platform {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    use std::time::Duration;
    use std::{io, mem};

    use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeServer, ServerOptions};
    use tokio::time::sleep;

    use super::BoxedConnection;

    /// Every instance of the pipe is connected to a client.
    const ERROR_PIPE_BUSY: i32 = 231;

    /// A pipe name. The pipe disappears when its last handle closes, so nothing needs removing.
    pub struct Endpoint {
        name: String,
    }

    impl Endpoint {
        pub fn create() -> io::Result<Endpoint> {
            let random = RandomState::new().build_hasher().finish();
            let name = format!(r"\\.\pipe\iris-watch-{}-{random:016x}", std::process::id());
            Ok(Endpoint { name })
        }

        pub fn name(&self) -> &str {
            &self.name
        }
    }

    /// A named pipe instance serves one client, so the listener keeps one unconnected instance
    /// waiting for the next.
    pub struct Listener {
        name: String,
        waiting: NamedPipeServer,
    }

    impl Listener {
        pub fn bind(endpoint: &Endpoint) -> io::Result<Listener> {
            let waiting = ServerOptions::new().first_pipe_instance(true).create(endpoint.name())?;
            Ok(Listener { name: endpoint.name().to_string(), waiting })
        }

        pub async fn accept(&mut self) -> io::Result<BoxedConnection> {
            let connected = self.waiting.connect().await;
            // The instance is replaced whether or not the client connected: one that a client
            // opened and closed before `connect` fails every later `connect` too.
            let next = ServerOptions::new().create(&self.name)?;
            let instance = mem::replace(&mut self.waiting, next);
            connected?;
            Ok(BoxedConnection::new(instance))
        }
    }

    pub async fn connect(name: &str) -> io::Result<BoxedConnection> {
        loop {
            match ClientOptions::new().open(name) {
                Ok(client) => return Ok(BoxedConnection::new(client)),
                Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY) => {
                    sleep(Duration::from_millis(20)).await;
                }
                Err(error) => return Err(error),
            }
        }
    }
}
