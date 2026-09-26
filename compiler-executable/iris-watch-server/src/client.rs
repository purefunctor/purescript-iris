//! Connecting to a running watcher through the socket file in its output directory.

use std::io;
use std::path::{Path, PathBuf};

use serde_json::json;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, ReadHalf, WriteHalf};

use crate::discovery::{self, Discovery, DiscoveryError};
use crate::protocol::{Query, Request, Response, ResponseBody};
use crate::transport::{self, BoxedConnection};

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

pub struct Client {
    reader: BufReader<ReadHalf<BoxedConnection>>,
    writer: WriteHalf<BoxedConnection>,
    line: String,
}

impl Client {
    /// Connects to the watcher that published its socket in `output`. A socket file whose socket
    /// cannot be reached was left by a watcher that exited without cleaning up.
    pub async fn connect(output: &Path) -> Result<(Client, Discovery), ClientError> {
        let no_watcher = || ClientError::NoWatcher { output: output.to_path_buf() };
        let discovery = discovery::read(output)?.ok_or_else(no_watcher)?;
        let connection = transport::connect(&discovery.socket).await.map_err(|error| {
            tracing::debug!(?error, socket = discovery.socket, "Failed to connect to the watcher");
            no_watcher()
        })?;
        let (reader, writer) = tokio::io::split(connection);
        let client = Client { reader: BufReader::new(reader), writer, line: String::new() };
        Ok((client, discovery))
    }

    pub async fn send(&mut self, request: &Request) -> Result<(), ClientError> {
        let mut line = serde_json::to_string(request)?;
        line.push('\n');
        self.writer.write_all(line.as_bytes()).await?;
        self.writer.flush().await?;
        Ok(())
    }

    /// Reads the next response, which may answer any request sent on this connection.
    pub async fn receive(&mut self) -> Result<Response, ClientError> {
        self.line.clear();
        if self.reader.read_line(&mut self.line).await? == 0 {
            return Err(ClientError::Closed);
        }
        Ok(serde_json::from_str(&self.line)?)
    }
}

/// Sends `query` to the watcher for `output` and waits for its answer. A query cancelled by a
/// change to the watcher's inputs is sent again, so the answer is a result or an error.
pub fn query(output: &Path, query: &Query) -> Result<(Discovery, Response), ClientError> {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    runtime.block_on(async {
        let (mut client, discovery) = Client::connect(output).await?;
        for id in 0.. {
            let request = Request { id: json!(id), query: Query::clone(query) };
            client.send(&request).await?;
            let response = client.receive().await?;
            if response.body != ResponseBody::Cancelled {
                return Ok((discovery, response));
            }
        }
        unreachable!("invariant violated: request IDs ran out")
    })
}
