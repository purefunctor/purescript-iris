//! Accepting clients and forwarding their requests to the watcher.
//!
//! Each connection has a task that reads request lines and a task that writes responses, so a
//! client can send several requests and receive each response as soon as it is ready.

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot};

use crate::protocol::{Query, Request, Response, ResponseBody};
use crate::transport::{BoxedConnection, Listener};

/// A request, forwarded to the watcher with the channel for its response.
pub struct QueryRequest {
    pub query: Query,
    pub reply: oneshot::Sender<ResponseBody>,
}

/// Accepts clients on `listener` until the watcher stops receiving requests.
pub async fn serve(mut listener: Listener, requests: mpsc::UnboundedSender<QueryRequest>) {
    loop {
        match listener.accept().await {
            Ok(connection) => {
                tokio::spawn(serve_connection(connection, mpsc::UnboundedSender::clone(&requests)));
            }
            Err(error) => tracing::warn!(?error, "Failed to accept a connection"),
        }
        if requests.is_closed() {
            return;
        }
    }
}

async fn serve_connection(
    connection: BoxedConnection,
    requests: mpsc::UnboundedSender<QueryRequest>,
) {
    let (reader, mut writer) = tokio::io::split(connection);
    let (responses, mut queued) = mpsc::unbounded_channel::<Response>();
    tokio::spawn(async move {
        while let Some(response) = queued.recv().await {
            let line = response.to_line();
            if writer.write_all(line.as_bytes()).await.is_err() || writer.flush().await.is_err() {
                return;
            }
        }
    });

    let mut lines = BufReader::new(reader).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim().is_empty() {
            continue;
        }
        let Request { id, query } = match serde_json::from_str(&line) {
            Ok(request) => request,
            Err(error) => {
                let id = serde_json::from_str::<Value>(&line)
                    .ok()
                    .and_then(|mut value| value.get_mut("id").map(Value::take))
                    .unwrap_or(Value::Null);
                let message = format!("invalid request: {error}");
                let _ = responses.send(Response { id, body: ResponseBody::Error { message } });
                continue;
            }
        };
        let (reply, answer) = oneshot::channel();
        if requests.send(QueryRequest { query, reply }).is_err() {
            return;
        }
        let responses = mpsc::UnboundedSender::clone(&responses);
        tokio::spawn(async move {
            let body = answer.await.unwrap_or_else(|_| {
                let message = "the watcher failed to answer; see its output".to_string();
                ResponseBody::Error { message }
            });
            let _ = responses.send(Response { id, body });
        });
    }
}
