//! Accepting clients and forwarding their requests to the watcher.
//!
//! Each connection has one task that reads a query, waits for its answer, writes it, and closes.
//! Other connections can submit queries while that task waits.

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot};

use crate::protocol::{Query, Response};
use crate::transport::{BoxedConnection, Listener};

/// A request, forwarded to the watcher with the channel for its response.
pub struct QueryRequest {
    pub query: Query,
    pub reply: oneshot::Sender<Response>,
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
    let mut connection = BufReader::new(connection);
    let mut line = String::new();
    match connection.read_line(&mut line).await {
        Ok(0) | Err(_) => return,
        Ok(_) => {}
    }
    let response = match serde_json::from_str(&line) {
        Ok(query) => {
            let (reply, answer) = oneshot::channel();
            if requests.send(QueryRequest { query, reply }).is_err() {
                return;
            }
            answer.await.unwrap_or_else(|_| {
                let message = "the watcher failed to answer; see its output".to_string();
                Response::Error { message }
            })
        }
        Err(error) => Response::Error { message: format!("invalid request: {error}") },
    };
    let mut connection = connection.into_inner();
    if connection.write_all(response.to_line().as_bytes()).await.is_ok() {
        let _ = connection.shutdown().await;
    }
}
