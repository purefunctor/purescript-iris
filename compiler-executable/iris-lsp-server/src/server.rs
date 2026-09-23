//! The protocol actor: lifecycle, request IDs, cancellation, and the order of steps when the
//! server stops.

use std::collections::HashMap;
use std::time::Duration;

use lsp_server::{ErrorCode, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{
    self as notifications, Notification as _, PublishDiagnostics, ShowMessage,
};
use lsp_types::request::{RegisterCapability, Request as _, WorkspaceConfiguration};
use lsp_types::{
    CancelParams, ClientCapabilities, DidChangeConfigurationParams, MessageType, NumberOrString,
    ShowMessageParams, WorkDoneProgressCancelParams, WorkspaceFolder,
};
use serde_json::{Value, json};
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use tokio::task::{AbortHandle, JoinError, JoinHandle, JoinSet};
use tokio::time::{Instant, sleep_until, timeout_at};

use crate::outgoing::{EditorConnection, Outcome, OutgoingPurpose, Registration};
use crate::parent;
use crate::progress::{Progress, creation_result};
use crate::service::{
    Answer, ControlMessage, OrderedMessage, Rejection, SettingsResponse, WorkspaceEvent,
    WorkspaceFailure, WorkspaceSenders,
};
use crate::settings::{CONFIGURATION_DEADLINE, Settings, to_value};
use crate::transport::{Transport, TransportError};

/// How long cleanup may take after the connection to the editor ends.
const CLEANUP_LIMIT: Duration = Duration::from_secs(5);

/// The only component that talks to the editor.
pub struct Server {
    editor: EditorConnection,
    workspace: WorkspaceSenders,
    workspace_events: mpsc::UnboundedReceiver<WorkspaceEvent>,
    workspace_task: Option<JoinHandle<Result<(), WorkspaceFailure>>>,
    lifecycle: Lifecycle,
    requests: Requests,
    /// What the latest `initialize` negotiated; replaced if that `initialize` is rejected and
    /// another one arrives.
    session: Session,
    editor_exited: Option<oneshot::Receiver<()>>,
}

#[derive(Default)]
struct Session {
    settings: Settings,
    progress: Progress,
    process_id: Option<i32>,
}

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("received exit before shutdown")]
    ExitBeforeShutdown,
    #[error("the editor closed the connection without sending exit")]
    EndOfInput,
    #[error("Client process exited")]
    EditorExited,
    #[error("invalid {method} notification: {error}")]
    InvalidNotification { method: String, error: serde_json::Error },
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
    EditorExited,
    InvalidNotification { method: String, error: serde_json::Error },
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
/// that returns the sequence number, so an answer for a request that is no longer recorded, such
/// as a cancelled one whose ID the editor reused, is discarded.
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
            editor: EditorConnection::new(transport),
            workspace,
            workspace_events,
            workspace_task: Some(workspace_task),
            lifecycle: Lifecycle::Uninitialized,
            requests: Requests::default(),
            session: Session::default(),
            editor_exited: None,
        }
    }

    pub async fn run(mut self) -> Result<(), ServerError> {
        let stop = self.serve().await;
        self.stop(stop).await
    }

    async fn serve(&mut self) -> Stop {
        loop {
            let deadline = self.editor.next_deadline();
            tokio::select! {
                message = self.editor.receive() => {
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
                () = sleep_until(deadline.unwrap_or_else(Instant::now)), if deadline.is_some() => {
                    for (purpose, outcome) in self.editor.expire(Instant::now()) {
                        self.complete_outgoing(purpose, outcome);
                    }
                }
                exited = async { self.editor_exited.as_mut().expect("invariant violated: no editor monitor").await }, if self.editor_exited.is_some() => {
                    if exited.is_ok() {
                        tracing::error!("The editor process exited");
                        return Stop::EditorExited;
                    }
                    self.editor_exited = None;
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
                if let Some((purpose, outcome)) = self.editor.complete(response) {
                    self.complete_outgoing(purpose, outcome);
                }
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
                self.session = Session::negotiate(&params);
                let answer = self.workspace.initialize(params);
                self.requests.wait(id, method, answer);
            }
            (Lifecycle::Uninitialized | Lifecycle::Initializing | Lifecycle::Initialized, _) => {
                let message = "Server is not initialized yet".to_string();
                self.respond_error(id, ErrorCode::ServerNotInitialized, message);
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
                self.editor.respond(Response::new_ok(id, Value::Null));
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
        match method.as_str() {
            notifications::Exit::METHOD => return Some(Stop::Exit),
            // Requests may wait for answers in every lifecycle stage, including after shutdown.
            notifications::Cancel::METHOD => {
                self.cancel(params);
                return None;
            }
            _ => {}
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
            notifications::Initialized::METHOD => self.initialized(),
            notifications::DidChangeConfiguration::METHOD => {
                // The payload is ignored: settings are always requested with
                // `workspace/configuration`, but the parameters must still be well-formed.
                if let Err(error) = serde_json::from_value::<DidChangeConfigurationParams>(params) {
                    return Some(Stop::InvalidNotification { method, error });
                }
                self.request_settings();
            }
            notifications::WorkDoneProgressCancel::METHOD => {
                let parameters =
                    match serde_json::from_value::<WorkDoneProgressCancelParams>(params) {
                        Ok(parameters) => parameters,
                        Err(error) => return Some(Stop::InvalidNotification { method, error }),
                    };
                if let Some(generation) = self.session.progress.cancel(&parameters.token) {
                    self.workspace.control(ControlMessage::CancelPreparation { generation });
                }
            }
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
        for (registration, parameters) in self.session.settings.registrations() {
            let purpose = OutgoingPurpose::Registration(registration);
            self.editor.request(RegisterCapability::METHOD, to_value(parameters), purpose, None);
        }
        if self.session.settings.supports_workspace_configuration() {
            self.request_settings();
        } else {
            self.workspace.send(OrderedMessage::Settings(SettingsResponse::Unsupported));
        }
    }

    fn request_settings(&mut self) {
        if !self.session.settings.supports_workspace_configuration() {
            return;
        }
        let (generation, parameters) = self.session.settings.next_request();
        let purpose = OutgoingPurpose::Configuration { generation };
        let deadline = Some(Instant::now() + CONFIGURATION_DEADLINE);
        self.editor.request(
            WorkspaceConfiguration::METHOD,
            to_value(parameters),
            purpose,
            deadline,
        );
    }

    fn cancel(&mut self, params: Value) {
        let Ok(CancelParams { id }) = serde_json::from_value::<CancelParams>(params) else {
            tracing::warn!("Ignored a malformed $/cancelRequest notification");
            return;
        };
        let id = match id {
            NumberOrString::Number(id) => RequestId::from(id),
            NumberOrString::String(id) => RequestId::from(id),
        };
        let Some(request) = self.requests.cancel(&id) else {
            return;
        };
        if request.method == "initialize" {
            self.lifecycle = Lifecycle::Uninitialized;
        }
        let message = "Client cancelled the request".to_string();
        self.respond_error(id, ErrorCode::RequestCanceled, message);
    }

    fn complete_outgoing(&mut self, purpose: OutgoingPurpose, outcome: Outcome) {
        match purpose {
            OutgoingPurpose::Registration(registration) => {
                if let Outcome::Response(Err(error)) = outcome {
                    let registration = match registration {
                        Registration::WatchedFiles => "source file watcher",
                        Registration::ConfigurationChanges => "workspace configuration changes",
                    };
                    tracing::warn!("Failed to register {registration}: {}", error.message);
                }
            }
            OutgoingPurpose::Configuration { generation } => {
                if let Some(response) = self.session.settings.accept(generation, outcome) {
                    self.workspace.send(OrderedMessage::Settings(response));
                }
            }
            OutgoingPurpose::ProgressCreation { generation } => {
                let Outcome::Response(result) = outcome else {
                    unreachable!("invariant violated: progress creation has no deadline")
                };
                self.session.progress.created(&self.editor, generation, creation_result(result));
            }
        }
    }

    fn deliver(
        &mut self,
        answer: Result<(u64, Result<Answer, oneshot::error::RecvError>), JoinError>,
    ) {
        let (sequence, answer) = match answer {
            Ok(answer) => answer,
            // Cancelling a request removes its record before aborting its task.
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
            self.initialize_answered(answer.is_ok());
        }
        self.respond(id, &request.method, answer);
    }

    fn initialize_answered(&mut self, accepted: bool) {
        if !accepted {
            self.lifecycle = Lifecycle::Uninitialized;
            return;
        }
        self.lifecycle = Lifecycle::Initialized;
        self.editor_exited = self.session.process_id.and_then(parent::monitor);
    }

    fn receive_event(&mut self, event: WorkspaceEvent) {
        match event {
            WorkspaceEvent::Diagnostics { uri, version, diagnostics } => {
                let mut parameters = json!({"uri": uri, "diagnostics": diagnostics});
                if let Some(version) = version {
                    parameters["version"] = json!(version);
                }
                self.editor.notify(PublishDiagnostics::METHOD, parameters);
            }
            WorkspaceEvent::PreparationStarted { generation, title, message } => {
                self.session.progress.started(&mut self.editor, generation, title, message);
            }
            WorkspaceEvent::PreparationProgress { generation, message, percentage } => {
                self.session.progress.report(&self.editor, generation, message, percentage);
            }
            WorkspaceEvent::PreparationEnded { generation, message } => {
                self.session.progress.ended(&self.editor, generation, message);
            }
            WorkspaceEvent::Error { message } => {
                let parameters = ShowMessageParams { typ: MessageType::ERROR, message };
                self.editor.notify(ShowMessage::METHOD, to_value(parameters));
            }
        }
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
        self.editor.respond(response);
    }

    fn respond_error(&self, id: RequestId, code: ErrorCode, message: String) {
        self.editor.respond(Response::new_err(id, code as i32, message));
    }

    /// Runs cleanup within [`CLEANUP_LIMIT`] and reports why the server stopped.
    ///
    /// Closing the workspace channels starts the workspace actor's cleanup: it kills and reaps
    /// Spago process trees, waits for blocking preparation work, and waits for its analysis and
    /// diagnostic workers. Dropping the waiting requests drops their reply channels, and dropping
    /// the connection aborts pending requests to the editor.
    async fn stop(self, stop: Stop) -> Result<(), ServerError> {
        let deadline = Instant::now() + CLEANUP_LIMIT;
        let Server {
            editor,
            workspace,
            workspace_events,
            workspace_task,
            lifecycle,
            requests,
            session,
            editor_exited,
        } = self;

        drop(workspace);
        drop(requests);
        drop(workspace_events);
        drop(session);
        drop(editor_exited);

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
        if let Err(error) = editor.close(deadline, input_ended).await
            && cleanup.is_ok()
        {
            cleanup = Err(match error {
                TransportError::Timeout => ServerError::CleanupTimeout,
                error => ServerError::Transport(error),
            });
        }
        if let Err(error) = &cleanup {
            tracing::error!("Cleanup failed: {error}");
        }

        let stop = match stop {
            Stop::Exit if lifecycle == Lifecycle::ShuttingDown => Ok(()),
            Stop::Exit => Err(ServerError::ExitBeforeShutdown),
            Stop::EndOfInput => Err(ServerError::EndOfInput),
            Stop::EditorExited => Err(ServerError::EditorExited),
            Stop::InvalidNotification { method, error } => {
                Err(ServerError::InvalidNotification { method, error })
            }
            Stop::WorkspaceStopped(reason) => Err(ServerError::WorkspaceStopped(reason)),
        };
        stop.and(cleanup)
    }
}

impl Session {
    /// Reads what `iris-lsp-server` owns from the `initialize` parameters. The workspace actor
    /// validates the parameters as a whole; a part that does not decode here falls back to its
    /// default.
    fn negotiate(initialize: &Value) -> Session {
        let capabilities = initialize
            .get("capabilities")
            .and_then(|capabilities| {
                serde_json::from_value::<ClientCapabilities>(Value::clone(capabilities)).ok()
            })
            .unwrap_or_default();
        let workspace_folders = initialize.get("workspaceFolders").and_then(|folders| {
            serde_json::from_value::<Option<Vec<WorkspaceFolder>>>(Value::clone(folders)).ok()?
        });
        let work_done_progress = capabilities
            .window
            .as_ref()
            .and_then(|window| window.work_done_progress)
            .unwrap_or(false);
        Session {
            settings: Settings::new(&capabilities, workspace_folders.as_deref()),
            progress: Progress::new(work_done_progress),
            process_id: parent::process_id(initialize),
        }
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

    /// Removes the record for `sequence`, or returns `None` if the request was cancelled.
    fn finish(&mut self, sequence: u64) -> Option<(RequestId, WaitingRequest)> {
        let id = self.by_sequence.remove(&sequence)?;
        let request = self.waiting.remove(&id)?;
        Some((id, request))
    }

    /// Removes the record for `id` and drops the receiving end of its reply channel.
    fn cancel(&mut self, id: &RequestId) -> Option<WaitingRequest> {
        let request = self.waiting.remove(id)?;
        self.by_sequence.remove(&request.sequence);
        request.abort.abort();
        Some(request)
    }

    fn drain(&mut self) -> Vec<(RequestId, WaitingRequest)> {
        self.by_sequence.clear();
        self.waiting.drain().collect()
    }
}
