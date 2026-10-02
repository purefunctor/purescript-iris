//! Finding a running watcher, and making sure only one watcher writes to an output directory.
//!
//! A watcher holds an exclusive OS lock on `.iris-watch.lock` in the output directory while it
//! runs; the OS releases it when the process exits for any reason. It names its socket in
//! `.iris-watch`, a separate file because Windows locks are mandatory and would stop clients from
//! reading a locked file.

use std::fs::{self, File, TryLockError};
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const SOCKET_FILE: &str = ".iris-watch";
pub const LOCK_FILE: &str = ".iris-watch.lock";

/// The contents of the socket file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Discovery {
    /// The socket's path on Unix, or the pipe's name on Windows.
    pub socket: String,
    pub pid: u32,
    /// The watcher's Iris version.
    pub version: String,
    /// The watcher's executable, which a client from a different Iris version can point to.
    pub executable: Option<PathBuf>,
}

#[derive(Debug, Error)]
pub enum DiscoveryError {
    #[error(
        "another `iris watch`{} is already writing to {}",
        .pid.map(|pid| format!(" (process {pid})")).unwrap_or_default(),
        .output.display()
    )]
    Locked { output: PathBuf, pid: Option<u32> },
    #[error("failed to lock {}: {source}", .path.display())]
    Lock {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to read {}: {source}", .path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse {}: {source}", .path.display())]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to write {}: {source}", .path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// The exclusive lock on an output directory, held until dropped.
pub struct OutputLock {
    output: PathBuf,
    _file: File,
}

impl OutputLock {
    /// Locks `output`, creating it if needed.
    pub fn acquire(output: &Path) -> Result<OutputLock, DiscoveryError> {
        let path = output.join(LOCK_FILE);
        let lock_error = |source| DiscoveryError::Lock { path: PathBuf::clone(&path), source };
        fs::create_dir_all(output).map_err(lock_error)?;
        let file = File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(lock_error)?;
        match file.try_lock() {
            Ok(()) => Ok(OutputLock { output: output.to_path_buf(), _file: file }),
            Err(TryLockError::WouldBlock) => {
                let pid = read(output).ok().flatten().map(|discovery| discovery.pid);
                Err(DiscoveryError::Locked { output: output.to_path_buf(), pid })
            }
            Err(TryLockError::Error(error)) => Err(lock_error(error)),
        }
    }

    /// Writes the socket file, replacing one a previous watcher left behind. It is written to a
    /// temporary file and renamed, so a client never reads a partial file.
    pub fn publish(&self, discovery: &Discovery) -> Result<Published, DiscoveryError> {
        let path = self.output.join(SOCKET_FILE);
        let temporary = self.output.join(format!("{SOCKET_FILE}.{}", std::process::id()));
        let write_error = |source| DiscoveryError::Write { path: PathBuf::clone(&path), source };
        let content = serde_json::to_string_pretty(discovery)
            .expect("invariant violated: a socket file failed to serialize");
        fs::write(&temporary, content).map_err(write_error)?;
        fs::rename(&temporary, &path).map_err(write_error)?;
        Ok(Published { path })
    }
}

/// A published socket file, removed when dropped.
pub struct Published {
    path: PathBuf,
}

impl Drop for Published {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_file(&self.path) {
            tracing::warn!(?error, path = %self.path.display(), "Failed to remove the socket file");
        }
    }
}

/// Reads the socket file in `output`, or `None` if no watcher published one.
pub fn read(output: &Path) -> Result<Option<Discovery>, DiscoveryError> {
    let path = output.join(SOCKET_FILE);
    let content = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(DiscoveryError::Read { path, source }),
    };
    let discovery = serde_json::from_str(&content);
    discovery.map(Some).map_err(|source| DiscoveryError::Parse { path, source })
}
