//! Messages between `iris-lsp-server` and the workspace actor.
//!
//! The workspace actor never addresses the editor. It receives [`OrderedMessage`]s and
//! [`ControlMessage`]s, and reports what happened as [`WorkspaceEvent`]s, which
//! `iris-lsp-server` turns into LSP messages.

use serde_json::Value;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use url::Url;

/// The workspace actor's answer to a request.
pub type Answer = Result<Value, Rejection>;

/// Why the workspace actor declined a request. `iris-lsp-server` maps these to JSON-RPC error codes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rejection {
    MethodNotFound,
    InvalidParams(String),
    ContentModified(String),
    RequestFailed(String),
    Internal(String),
}

/// Messages the workspace actor handles one at a time, in the order they were sent.
///
/// A request sees every notification sent before it and none sent after it. A change to the query
/// engine's inputs is applied when the actor reaches it, after cancelling analysis that started
/// earlier; requests sent after the change take their snapshot after it.
pub enum OrderedMessage {
    Initialize {
        params: Value,
        reply: oneshot::Sender<Answer>,
    },
    Initialized,
    Settings(SettingsResponse),
    /// Document notifications, forwarded unchanged.
    Notification {
        method: String,
        params: Value,
    },
    Request {
        method: String,
        params: Value,
        reply: oneshot::Sender<Answer>,
    },
}

/// The outcome of retrieving the `iris.server` settings from the editor.
#[derive(Clone, Debug, PartialEq)]
pub enum SettingsResponse {
    /// The editor does not support `workspace/configuration`. Sent once, after `initialized`.
    Unsupported,
    /// The undecoded `result` of a `workspace/configuration` request for section `iris.server`.
    Received(Value),
    /// The `workspace/configuration` request failed or timed out.
    Failed(String),
}

/// Messages that must not wait behind [`OrderedMessage`]s.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlMessage {
    /// Sent when the editor cancels the progress of this preparation attempt.
    CancelPreparation {
        generation: u64,
    },
    Shutdown,
}

/// Messages from the workspace actor to `iris-lsp-server`, in the workspace actor's own terms.
#[derive(Clone, Debug, PartialEq)]
pub enum WorkspaceEvent {
    /// Becomes `textDocument/publishDiagnostics`. `diagnostics` is the list `iris-analysis` produced.
    Diagnostics { uri: Url, version: Option<i32>, diagnostics: Value },
    /// Preparation attempt `generation` started.
    PreparationStarted { generation: u64, title: String, message: String },
    /// Preparation attempt `generation` reached a new stage.
    PreparationProgress { generation: u64, message: String, percentage: Option<u32> },
    /// Preparation attempt `generation` finished, failed, or was cancelled.
    PreparationEnded { generation: u64, message: String },
    /// Becomes `window/showMessage` with type Error.
    Error { message: String },
}

/// Why the workspace actor stopped before its channels were closed.
#[derive(Debug, Error)]
#[error("{0}")]
pub struct WorkspaceFailure(pub String);

/// Sending ends of the [`OrderedMessage`] and [`ControlMessage`] channels, held by
/// `iris-lsp-server`.
#[derive(Clone)]
pub struct WorkspaceSenders {
    ordered: mpsc::UnboundedSender<OrderedMessage>,
    control: mpsc::UnboundedSender<ControlMessage>,
}

/// Receiving ends of the same channels, held by the workspace actor.
pub struct WorkspaceReceivers {
    pub ordered: mpsc::UnboundedReceiver<OrderedMessage>,
    pub control: mpsc::UnboundedReceiver<ControlMessage>,
}

/// Sending end of the [`WorkspaceEvent`] channel, held by the workspace actor.
#[derive(Clone)]
pub struct WorkspaceEventSender {
    events: mpsc::UnboundedSender<WorkspaceEvent>,
}

impl WorkspaceSenders {
    pub fn channel() -> (WorkspaceSenders, WorkspaceReceivers) {
        let (ordered, ordered_receiver) = mpsc::unbounded_channel();
        let (control, control_receiver) = mpsc::unbounded_channel();
        let senders = WorkspaceSenders { ordered, control };
        let receivers = WorkspaceReceivers { ordered: ordered_receiver, control: control_receiver };
        (senders, receivers)
    }

    /// Sends `initialize` and returns the receiving end of its reply channel.
    pub fn initialize(&self, params: Value) -> oneshot::Receiver<Answer> {
        let (reply, answer) = oneshot::channel();
        self.send(OrderedMessage::Initialize { params, reply });
        answer
    }

    /// Sends a request and returns the receiving end of its reply channel.
    ///
    /// Dropping the receiver cancels the request.
    pub fn request(&self, method: String, params: Value) -> oneshot::Receiver<Answer> {
        let (reply, answer) = oneshot::channel();
        self.send(OrderedMessage::Request { method, params, reply });
        answer
    }

    /// Sends an ordered message. If the workspace actor has stopped, the message is dropped, which
    /// also closes any reply channel it carries.
    pub fn send(&self, message: OrderedMessage) {
        let _ = self.ordered.send(message);
    }

    /// Sends a control message. If the workspace actor has stopped, the message is dropped.
    pub fn control(&self, message: ControlMessage) {
        let _ = self.control.send(message);
    }
}

impl WorkspaceEventSender {
    pub fn channel() -> (WorkspaceEventSender, mpsc::UnboundedReceiver<WorkspaceEvent>) {
        let (events, receiver) = mpsc::unbounded_channel();
        (WorkspaceEventSender { events }, receiver)
    }

    /// Sends an event. Events sent after `iris-lsp-server` stopped reading are dropped.
    pub fn send(&self, event: WorkspaceEvent) {
        let _ = self.events.send(event);
    }
}
