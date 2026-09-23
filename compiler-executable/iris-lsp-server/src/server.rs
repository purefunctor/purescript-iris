//! The protocol actor: lifecycle, request IDs, and the order of steps when the server stops.

use std::collections::HashMap;
use std::time::Duration;

use lsp_server::{ErrorCode, Message, Notification, Request, RequestId, Response};
use serde_json::Value;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use tokio::task::{AbortHandle, JoinError, JoinHandle, JoinSet};
use tokio::time::{Instant, timeout_at};

use crate::service::{
    Answer, ControlMessage, OrderedMessage, Rejection, WorkspaceEvent, WorkspaceFailure,
    WorkspaceSenders,
};
use crate::transport::{Transport, TransportError};

/// How long cleanup may take after the connection to the editor ends.
const CLEANUP_LIMIT: Duration = Duration::from_secs(5);

/// The only component that talks to the editor.
pub struct Server {
    transport: Transport,
    workspace: WorkspaceSenders,
    workspace_events: mpsc::UnboundedReceiver<WorkspaceEvent>,
    workspace_task: Option<JoinHandle<Result<(), WorkspaceFailure>>>,
    lifecycle: Lifecycle,
    requests: Requests,
}

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("received exit before shutdown")]
    ExitBeforeShutdown,
    #[error("the editor closed the connection without sending exit")]
    EndOfInput,
    #[error("Workspace service stopped: {0}")]
    WorkspaceStopped(String),
    #[error("stdio transport failed: {0}")]
    Transport(#[from] TransportError),
    #[error("cleanup did not finish within {} seconds", CLEANUP_LIMIT.as_secs())]
    CleanupTimeout,
}

/// Why the server stopped serving the editor.
enum Stop {
    Exit,
    EndOfInput,
    WorkspaceStopped(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lifecycle {
    Uninitialized,
    /// `initialize` was sent to the workspace actor and is not answered yet.
    Initializing,
    /// `initialize` was answered; `initialized` has not arrived.
    Initialized,
    Ready,
    ShuttingDown,
}

/// Requests from the editor that are waiting for an answer.
///
/// Each request gets an internal sequence number. Its answer arrives through a task in `answers`
/// that returns the sequence number, so an answer for a request that is no longer recorded can be
/// discarded even if the editor reused its ID.
struct Requests {
    next_sequence: u64,
    waiting: HashMap<RequestId, WaitingRequest>,
    by_sequence: HashMap<u64, RequestId>,
    answers: JoinSet<(u64, Result<Answer, oneshot::error::RecvError>)>,
}

struct WaitingRequest {
    sequence: u64,
    method: String,
    abort: AbortHandle,
}

impl Server {
    pub fn new(
        transport: Transport,
        workspace: WorkspaceSenders,
        workspace_events: mpsc::UnboundedReceiver<WorkspaceEvent>,
        workspace_task: JoinHandle<Result<(), WorkspaceFailure>>,
    ) -> Server {
        Server {
            transport,
            workspace,
            workspace_events,
            workspace_task: Some(workspace_task),
            lifecycle: Lifecycle::Uninitialized,
            requests: Requests::default(),
        }
    }

    pub async fn run(mut self) -> Result<(), ServerError> {
        let stop = self.serve().await;
        self.stop(stop).await
    }

    async fn serve(&mut self) -> Stop {
        loop {
            tokio::select! {
                message = self.transport.receive() => {
                    let Some(message) = message else { return Stop::EndOfInput };
                    if let Some(stop) = self.receive(message) {
                        return stop;
                    }
                }
                Some(answer) = self.requests.answers.join_next() => {
                    self.deliver(answer);
                }
                Some(event) = self.workspace_events.recv() => {
                    self.receive_event(event);
                }
                result = async { self.workspace_task.as_mut().expect("invariant violated: workspace task already joined").await } => {
                    self.workspace_task = None;
                    return self.workspace_stopped(result);
                }
            }
        }
    }

    fn receive(&mut self, message: Message) -> Option<Stop> {
        match message {
            Message::Request(request) => {
                self.receive_request(request);
                None
            }
            Message::Response(response) => {
                tracing::warn!("Ignored a response to an unknown request {}", response.id);
                None
            }
            Message::Notification(notification) => self.receive_notification(notification),
        }
    }

    fn receive_request(&mut self, request: Request) {
        let Request { id, method, params } = request;
        match (self.lifecycle, method.as_str()) {
            (Lifecycle::Uninitialized, "initialize") => {
                if self.reject_duplicate(&id) {
                    return;
                }
                self.lifecycle = Lifecycle::Initializing;
                let answer = self.workspace.initialize(params);
                self.requests.wait(id, method, answer);
            }
            (Lifecycle::Uninitialized | Lifecycle::Initializing | Lifecycle::Initialized, _) => {
                let error = (ErrorCode::ServerNotInitialized, "Server is not initialized yet");
                self.respond_error(id, error.0, error.1.to_string());
            }
            (_, "initialize") => {
                let message = "Server is already initialized".to_string();
                self.respond_error(id, ErrorCode::InvalidRequest, message);
            }
            (Lifecycle::Ready, "shutdown") => {
                if self.reject_duplicate(&id) {
                    return;
                }
                self.lifecycle = Lifecycle::ShuttingDown;
                self.workspace.control(ControlMessage::Shutdown);
                self.transport.send(Response::new_ok(id, Value::Null));
            }
            (Lifecycle::Ready, _) => {
                if self.reject_duplicate(&id) {
                    return;
                }
                let answer = self.workspace.request(String::clone(&method), params);
                self.requests.wait(id, method, answer);
            }
            (Lifecycle::ShuttingDown, _) => {
                let message = "Server is shutting down".to_string();
                self.respond_error(id, ErrorCode::InvalidRequest, message);
            }
        }
    }

    fn reject_duplicate(&self, id: &RequestId) -> bool {
        if !self.requests.waiting.contains_key(id) {
            return false;
        }
        let message = format!("Request {id} is still waiting for an answer");
        self.respond_error(RequestId::clone(id), ErrorCode::InvalidRequest, message);
        true
    }

    fn receive_notification(&mut self, notification: Notification) -> Option<Stop> {
        let Notification { method, params } = notification;
        if method == "exit" {
            return Some(Stop::Exit);
        }
        if matches!(
            self.lifecycle,
            Lifecycle::Uninitialized | Lifecycle::Initializing | Lifecycle::ShuttingDown
        ) {
            tracing::debug!(
                "Dropped {method} notification in lifecycle stage {:?}",
                self.lifecycle
            );
            return None;
        }
        match method.as_str() {
            "initialized" => self.initialized(),
            _ if method.starts_with("$/") => {}
            _ => self.workspace.send(OrderedMessage::Notification { method, params }),
        }
        None
    }

    fn initialized(&mut self) {
        if self.lifecycle != Lifecycle::Initialized {
            tracing::warn!(
                "Ignored initialized notification in lifecycle stage {:?}",
                self.lifecycle
            );
            return;
        }
        self.lifecycle = Lifecycle::Ready;
        self.workspace.send(OrderedMessage::Initialized);
    }

    fn deliver(
        &mut self,
        answer: Result<(u64, Result<Answer, oneshot::error::RecvError>), JoinError>,
    ) {
        let (sequence, answer) = match answer {
            Ok(answer) => answer,
            // Aborting a waiting request removes its record first.
            Err(error) if error.is_cancelled() => return,
            Err(error) => std::panic::resume_unwind(error.into_panic()),
        };
        let Some((id, request)) = self.requests.finish(sequence) else {
            return;
        };
        let answer = answer.unwrap_or_else(|_| {
            Err(Rejection::Internal("Request was dropped without an answer".to_string()))
        });
        if request.method == "initialize" {
            self.lifecycle = match &answer {
                Ok(_) => Lifecycle::Initialized,
                Err(_) => Lifecycle::Uninitialized,
            };
        }
        self.respond(id, &request.method, answer);
    }

    fn receive_event(&mut self, event: WorkspaceEvent) {
        tracing::debug!("Ignored workspace event {event:?}");
    }

    fn workspace_stopped(
        &mut self,
        result: Result<Result<(), WorkspaceFailure>, JoinError>,
    ) -> Stop {
        let reason = match result {
            Ok(Ok(())) => "the workspace actor returned".to_string(),
            Ok(Err(failure)) => failure.to_string(),
            Err(error) => error.to_string(),
        };
        tracing::error!("Workspace service stopped: {reason}");
        for (id, request) in self.requests.drain() {
            request.abort.abort();
            let rejection = Rejection::Internal("Workspace service stopped".to_string());
            self.respond(id, &request.method, Err(rejection));
        }
        Stop::WorkspaceStopped(reason)
    }

    fn respond(&self, id: RequestId, method: &str, answer: Answer) {
        let response = match answer {
            Ok(result) => Response::new_ok(id, result),
            Err(rejection) => {
                let (code, message) = match rejection {
                    Rejection::MethodNotFound => {
                        (ErrorCode::MethodNotFound, format!("No such method {method}"))
                    }
                    Rejection::InvalidParams(message) => (ErrorCode::InvalidParams, message),
                    Rejection::ContentModified(message) => (ErrorCode::ContentModified, message),
                    Rejection::RequestFailed(message) => (ErrorCode::RequestFailed, message),
                    Rejection::Internal(message) => (ErrorCode::InternalError, message),
                };
                Response::new_err(id, code as i32, message)
            }
        };
        self.transport.send(response);
    }

    fn respond_error(&self, id: RequestId, code: ErrorCode, message: String) {
        self.transport.send(Response::new_err(id, code as i32, message));
    }

    /// Runs cleanup within [`CLEANUP_LIMIT`] and reports why the server stopped.
    async fn stop(self, stop: Stop) -> Result<(), ServerError> {
        let deadline = Instant::now() + CLEANUP_LIMIT;
        let Server { transport, workspace, workspace_events, workspace_task, lifecycle, requests } =
            self;

        drop(workspace);
        drop(requests);
        drop(workspace_events);

        let mut cleanup = Ok(());
        if let Some(workspace_task) = workspace_task {
            match timeout_at(deadline, workspace_task).await {
                Ok(Ok(Ok(()))) => {}
                Ok(Ok(Err(failure))) => tracing::error!("Workspace service failed: {failure}"),
                Ok(Err(error)) => tracing::error!("Workspace service failed: {error}"),
                Err(_) => cleanup = Err(ServerError::CleanupTimeout),
            }
        }
        let input_ended = matches!(stop, Stop::Exit | Stop::EndOfInput);
        if let Err(error) = transport.close(deadline, input_ended).await {
            if cleanup.is_ok() {
                cleanup = Err(match error {
                    TransportError::Timeout => ServerError::CleanupTimeout,
                    error => ServerError::Transport(error),
                });
            }
        }
        if let Err(error) = &cleanup {
            tracing::error!("Cleanup failed: {error}");
        }

        let stop = match stop {
            Stop::Exit if lifecycle == Lifecycle::ShuttingDown => Ok(()),
            Stop::Exit => Err(ServerError::ExitBeforeShutdown),
            Stop::EndOfInput => Err(ServerError::EndOfInput),
            Stop::WorkspaceStopped(reason) => Err(ServerError::WorkspaceStopped(reason)),
        };
        stop.and(cleanup)
    }
}

impl Default for Requests {
    fn default() -> Requests {
        Requests {
            next_sequence: 0,
            waiting: HashMap::new(),
            by_sequence: HashMap::new(),
            answers: JoinSet::new(),
        }
    }
}

impl Requests {
    fn wait(&mut self, id: RequestId, method: String, answer: oneshot::Receiver<Answer>) {
        let sequence = self.next_sequence;
        self.next_sequence += 1;
        let abort = self.answers.spawn(async move { (sequence, answer.await) });
        self.by_sequence.insert(sequence, RequestId::clone(&id));
        self.waiting.insert(id, WaitingRequest { sequence, method, abort });
    }

    /// Removes the record for `sequence`, or returns `None` if it was cancelled.
    fn finish(&mut self, sequence: u64) -> Option<(RequestId, WaitingRequest)> {
        let id = self.by_sequence.remove(&sequence)?;
        let request = self.waiting.remove(&id)?;
        Some((id, request))
    }

    fn drain(&mut self) -> Vec<(RequestId, WaitingRequest)> {
        self.by_sequence.clear();
        self.waiting.drain().collect()
    }
}
