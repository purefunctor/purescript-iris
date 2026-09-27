//! Connecting to a running watcher through the socket file in its output directory.

use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::discovery::{self, Discovery, DiscoveryError};
use crate::protocol::{Query, Response};
use crate::transport;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("no `iris watch` is running for {}", .output.display())]
    NoWatcher { output: PathBuf },
    #[error(transparent)]
    Discovery(#[from] DiscoveryError),
    #[error("the watcher closed the connection")]
    Closed,
    #[error("failed to communicate with the watcher: {0}")]
    Io(#[from] io::Error),
    #[error("the watcher sent an invalid response: {0}")]
    InvalidResponse(#[from] serde_json::Error),
}

/// Sends `query` to the watcher for `output` and waits for its answer. A query cancelled by a
/// change to the watcher's inputs is sent again on a new connection.
pub fn query(output: &Path, query: &Query) -> Result<(Discovery, Response), ClientError> {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    runtime.block_on(async {
        let mut request = serde_json::to_string(query)?;
        request.push('\n');
        loop {
            let no_watcher = || ClientError::NoWatcher { output: output.to_path_buf() };
            let discovery = discovery::read(output)?.ok_or_else(no_watcher)?;
            let mut connection = transport::connect(&discovery.socket).await.map_err(|error| {
                tracing::debug!(
                    ?error,
                    socket = discovery.socket,
                    "Failed to connect to the watcher"
                );
                no_watcher()
            })?;
            connection.write_all(request.as_bytes()).await?;
            connection.flush().await?;
            let mut line = String::new();
            if BufReader::new(connection).read_line(&mut line).await? == 0 {
                return Err(ClientError::Closed);
            }
            let response = serde_json::from_str(&line)?;
            if response != Response::Cancelled {
                return Ok((discovery, response));
            }
        }
    })
}
