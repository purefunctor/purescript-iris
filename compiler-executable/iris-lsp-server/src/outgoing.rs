//! Messages sent to the editor, and requests sent to the editor: IDs, deadlines, and matching
//! responses.

use std::collections::HashMap;

use lsp_server::{Message, Notification, Request, RequestId, Response, ResponseError};
use serde_json::Value;
use tokio::time::Instant;

use crate::transport::{Transport, TransportError};

/// Why `iris-lsp-server` sent a request to the editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OutgoingPurpose {
    Registration(Registration),
    Configuration { generation: u64 },
    ProgressCreation { generation: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Registration {
    WatchedFiles,
    ConfigurationChanges,
}

/// The connection to the editor, and the requests sent to it that are waiting for a response.
pub(crate) struct EditorConnection {
    transport: Transport,
    next_id: i32,
    waiting: HashMap<RequestId, WaitingResponse>,
}

struct WaitingResponse {
    purpose: OutgoingPurpose,
    deadline: Option<Instant>,
}

/// How a request sent to the editor ended.
pub(crate) enum Outcome {
    Response(Result<Value, ResponseError>),
    Expired,
}

impl EditorConnection {
    pub(crate) fn new(transport: Transport) -> EditorConnection {
        EditorConnection { transport, next_id: 0, waiting: HashMap::new() }
    }

    pub(crate) async fn receive(&mut self) -> Option<Message> {
        self.transport.receive().await
    }

    pub(crate) fn respond(&self, response: Response) {
        self.transport.send(response);
    }

    pub(crate) fn notify(&self, method: &str, params: Value) {
        self.transport.send(Notification { method: method.to_string(), params });
    }

    /// Sends a request to the editor. Its response is matched by [`EditorConnection::complete`];
    /// if it has a deadline and no response arrives in time, [`EditorConnection::expire`]
    /// reports it instead and a later response is ignored.
    pub(crate) fn request(
        &mut self,
        method: &str,
        params: Value,
        purpose: OutgoingPurpose,
        deadline: Option<Instant>,
    ) {
        let id = RequestId::from(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        self.waiting.insert(RequestId::clone(&id), WaitingResponse { purpose, deadline });
        self.transport.send(Request { id, method: method.to_string(), params });
    }

    /// Matches a response from the editor with the request it answers.
    ///
    /// Returns `None` for a response to an unknown request, including a late response after
    /// the deadline and a repeated response.
    pub(crate) fn complete(&mut self, response: Response) -> Option<(OutgoingPurpose, Outcome)> {
        let Some(waiting) = self.waiting.remove(&response.id) else {
            tracing::warn!("Ignored a response to an unknown request {}", response.id);
            return None;
        };
        Some((waiting.purpose, Outcome::Response(response.response_result)))
    }

    pub(crate) fn next_deadline(&self) -> Option<Instant> {
        self.waiting.values().filter_map(|waiting| waiting.deadline).min()
    }

    /// Removes the requests whose deadline passed.
    pub(crate) fn expire(&mut self, now: Instant) -> Vec<(OutgoingPurpose, Outcome)> {
        let expired = self
            .waiting
            .iter()
            .filter(|(_, waiting)| waiting.deadline.is_some_and(|deadline| deadline <= now))
            .map(|(id, _)| RequestId::clone(id));
        let expired = expired.collect::<Vec<_>>();
        let expired = expired.into_iter().filter_map(|id| self.waiting.remove(&id));
        expired.map(|waiting| (waiting.purpose, Outcome::Expired)).collect()
    }

    /// Stops waiting for every response, then closes the transport.
    pub(crate) async fn close(
        self,
        deadline: Instant,
        input_ended: bool,
    ) -> Result<(), TransportError> {
        let EditorConnection { transport, waiting, .. } = self;
        drop(waiting);
        transport.close(deadline, input_ended).await
    }
}
