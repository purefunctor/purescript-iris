//! Tests where the test plays both the editor and the workspace actor.

use std::collections::VecDeque;
use std::time::Duration;

use lsp_server::{Connection, ErrorCode, Message, Notification, Request, RequestId, Response};
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::service::{
    ControlMessage, OrderedMessage, Rejection, WorkspaceEvent, WorkspaceEventSender,
    WorkspaceFailure, WorkspaceReceivers, WorkspaceSenders,
};
use crate::{Server, ServerError, Transport};

const PATIENCE: Duration = Duration::from_secs(10);

struct ServerHarness {
    editor: Editor,
    workspace: FakeWorkspace,
    server: JoinHandle<Result<(), ServerError>>,
}

/// The editor's end of an in-memory connection.
struct Editor {
    connection: Connection,
    unclaimed: VecDeque<Message>,
}

/// The test's stand-in for the workspace actor.
///
/// Like the real actor, its task ends when `iris-lsp-server` closes the channels, or when the test
/// stops it.
struct FakeWorkspace {
    receivers: WorkspaceReceivers,
    events: WorkspaceEventSender,
    stop: Option<oneshot::Sender<Result<(), WorkspaceFailure>>>,
}

impl ServerHarness {
    fn start() -> ServerHarness {
        let (editor, server) = Connection::memory();
        let (workspace_events, workspace_event_receiver) = WorkspaceEventSender::channel();
        let (workspace_senders, relay_receivers) = WorkspaceSenders::channel();
        let (relay_senders, workspace_receivers) = WorkspaceSenders::channel();
        let (stop, stopped) = oneshot::channel();
        let workspace_task = tokio::spawn(relay(relay_receivers, relay_senders, stopped));
        let server = Server::new(
            Transport::memory(server),
            workspace_senders,
            workspace_event_receiver,
            workspace_task,
        );
        let server = tokio::spawn(server.run());
        ServerHarness {
            editor: Editor { connection: editor, unclaimed: VecDeque::new() },
            workspace: FakeWorkspace {
                receivers: workspace_receivers,
                events: workspace_events,
                stop: Some(stop),
            },
            server,
        }
    }

    /// Starts a server that completed `initialize` and received `initialized`.
    async fn running() -> ServerHarness {
        ServerHarness::running_with(json!({})).await
    }

    async fn running_with(initialize: Value) -> ServerHarness {
        let mut harness = ServerHarness::start();
        harness.initialize(initialize).await;
        harness.editor.notify("initialized", json!({}));
        let OrderedMessage::Initialized = harness.workspace.ordered().await else {
            panic!("expected initialized to reach the workspace actor");
        };
        harness
    }

    async fn initialize(&mut self, params: Value) {
        self.editor.request(0, "initialize", params);
        let OrderedMessage::Initialize { reply, .. } = self.workspace.ordered().await else {
            panic!("expected initialize to reach the workspace actor");
        };
        reply.send(Ok(json!({"capabilities": {}}))).unwrap();
        assert_eq!(self.editor.result(0).await, json!({"capabilities": {}}));
    }

    async fn shutdown(&mut self, id: i32) {
        self.editor.request(id, "shutdown", Value::Null);
        assert_eq!(self.editor.result(id).await, Value::Null);
        assert_eq!(self.workspace.control().await, ControlMessage::Shutdown);
    }

    async fn stopped(self) -> Result<(), ServerError> {
        stopped(self.server).await
    }
}

/// Forwards messages from the server to the test until the server closes the ordered channel.
async fn relay(
    mut receivers: WorkspaceReceivers,
    senders: WorkspaceSenders,
    mut stopped: oneshot::Receiver<Result<(), WorkspaceFailure>>,
) -> Result<(), WorkspaceFailure> {
    loop {
        tokio::select! {
            result = &mut stopped => return result.unwrap_or(Ok(())),
            message = receivers.ordered.recv() => match message {
                Some(message) => senders.send(message),
                None => return Ok(()),
            },
            Some(message) = receivers.control.recv() => senders.control(message),
        }
    }
}

async fn stopped(server: JoinHandle<Result<(), ServerError>>) -> Result<(), ServerError> {
    tokio::time::timeout(PATIENCE, server)
        .await
        .expect("timed out waiting for the server to stop")
        .unwrap()
}

impl Editor {
    fn send(&self, message: impl Into<Message>) {
        self.connection.sender.send(message.into()).unwrap();
    }

    fn request(&self, id: i32, method: &str, params: Value) {
        self.send(Request { id: RequestId::from(id), method: method.to_string(), params });
    }

    fn notify(&self, method: &str, params: Value) {
        self.send(Notification { method: method.to_string(), params });
    }

    fn cancel(&self, id: i32) {
        self.notify("$/cancelRequest", json!({"id": id}));
    }

    fn exit(&self) {
        self.notify("exit", Value::Null);
    }

    async fn receive(&mut self) -> Option<Message> {
        let receiver = self.connection.receiver.clone();
        tokio::task::spawn_blocking(move || receiver.recv_timeout(PATIENCE).ok()).await.unwrap()
    }

    /// Waits for the first unclaimed message that `claim` accepts.
    async fn claim<T>(&mut self, description: &str, claim: impl Fn(&Message) -> Option<T>) -> T {
        if let Some(index) = self.unclaimed.iter().position(|message| claim(message).is_some()) {
            let message = self.unclaimed.remove(index).unwrap();
            return claim(&message).unwrap();
        }
        loop {
            let message = self
                .receive()
                .await
                .unwrap_or_else(|| panic!("timed out waiting for {description}"));
            if let Some(claimed) = claim(&message) {
                return claimed;
            }
            self.unclaimed.push_back(message);
        }
    }

    async fn response(&mut self, id: i32) -> Response {
        let id = RequestId::from(id);
        self.claim(&format!("the response to request {id}"), |message| match message {
            Message::Response(response) if response.id == id => Some(Response::clone(response)),
            _ => None,
        })
        .await
    }

    async fn result(&mut self, id: i32) -> Value {
        let response = self.response(id).await;
        response.response_result.unwrap_or_else(|error| panic!("request {id} failed: {error:?}"))
    }

    async fn error(&mut self, id: i32) -> (i32, String) {
        let response = self.response(id).await;
        let error = response.response_result.expect_err("expected an error response");
        (error.code, error.message)
    }

    async fn error_code(&mut self, id: i32) -> i32 {
        self.error(id).await.0
    }

    /// Asserts that no unclaimed message and no message within a short period satisfies `check`.
    async fn assert_no_message(&mut self, description: &str, check: impl Fn(&Message) -> bool) {
        assert!(!self.unclaimed.iter().any(&check), "unexpected {description}");
        let receiver = self.connection.receiver.clone();
        let messages = tokio::task::spawn_blocking(move || {
            let mut messages = vec![];
            while let Ok(message) = receiver.recv_timeout(Duration::from_millis(200)) {
                messages.push(message);
            }
            messages
        })
        .await
        .unwrap();
        assert!(!messages.iter().any(&check), "unexpected {description}: {messages:?}");
        self.unclaimed.extend(messages);
    }
}

impl FakeWorkspace {
    async fn ordered(&mut self) -> OrderedMessage {
        tokio::time::timeout(PATIENCE, self.receivers.ordered.recv())
            .await
            .expect("timed out waiting for an ordered message")
            .expect("the ordered channel closed")
    }

    async fn control(&mut self) -> ControlMessage {
        tokio::time::timeout(PATIENCE, self.receivers.control.recv())
            .await
            .expect("timed out waiting for a control message")
            .expect("the control channel closed")
    }

    async fn request(&mut self) -> (String, oneshot::Sender<crate::Answer>) {
        match self.ordered().await {
            OrderedMessage::Request { method, reply, .. } => (method, reply),
            _ => panic!("expected a request"),
        }
    }

    fn emit(&self, event: WorkspaceEvent) {
        self.events.send(event);
    }

    fn stop(&mut self, result: Result<(), WorkspaceFailure>) {
        let _ = self.stop.take().expect("the fake workspace already stopped").send(result);
    }

    fn assert_no_ordered_message(&mut self) {
        assert!(
            matches!(self.receivers.ordered.try_recv(), Err(mpsc::error::TryRecvError::Empty)),
            "unexpected ordered message"
        );
    }
}

fn is_response_to(message: &Message, id: i32) -> bool {
    matches!(message, Message::Response(response) if response.id == RequestId::from(id))
}

#[tokio::test]
async fn initialize_is_answered_by_the_workspace_actor() {
    let mut harness = ServerHarness::start();
    harness.editor.request(1, "initialize", json!({"capabilities": {}}));
    let OrderedMessage::Initialize { params, reply } = harness.workspace.ordered().await else {
        panic!("expected initialize");
    };
    assert_eq!(params, json!({"capabilities": {}}));
    reply.send(Ok(json!({"capabilities": {"hoverProvider": true}}))).unwrap();
    assert_eq!(harness.editor.result(1).await, json!({"capabilities": {"hoverProvider": true}}));
}

#[tokio::test]
async fn rejected_initialize_leaves_the_server_uninitialized() {
    let mut harness = ServerHarness::start();
    harness.editor.request(1, "initialize", json!({"capabilities": 42}));
    let OrderedMessage::Initialize { reply, .. } = harness.workspace.ordered().await else {
        panic!("expected initialize");
    };
    reply.send(Err(Rejection::InvalidParams("bad capabilities".to_string()))).unwrap();
    assert_eq!(
        harness.editor.error(1).await,
        (ErrorCode::InvalidParams as i32, "bad capabilities".to_string())
    );

    harness.initialize(json!({})).await;
}

#[tokio::test]
async fn lifecycle_errors_before_initialize_is_answered_and_after_shutdown() {
    let mut harness = ServerHarness::start();
    harness.editor.request(1, "textDocument/hover", json!({}));
    assert_eq!(
        harness.editor.error(1).await,
        (ErrorCode::ServerNotInitialized as i32, "Server is not initialized yet".to_string())
    );

    harness.editor.request(2, "initialize", json!({}));
    let OrderedMessage::Initialize { reply, .. } = harness.workspace.ordered().await else {
        panic!("expected initialize");
    };
    harness.editor.request(3, "initialize", json!({}));
    assert_eq!(
        harness.editor.error(3).await,
        (ErrorCode::ServerNotInitialized as i32, "Server is not initialized yet".to_string())
    );
    harness.editor.request(4, "shutdown", Value::Null);
    assert_eq!(harness.editor.error_code(4).await, ErrorCode::ServerNotInitialized as i32);
    reply.send(Ok(json!({}))).unwrap();
    harness.editor.result(2).await;

    // Between the initialize answer and `initialized`, requests are still rejected.
    harness.editor.request(5, "textDocument/hover", json!({}));
    assert_eq!(harness.editor.error_code(5).await, ErrorCode::ServerNotInitialized as i32);

    harness.editor.notify("initialized", json!({}));
    let OrderedMessage::Initialized = harness.workspace.ordered().await else {
        panic!("expected initialized");
    };
    harness.editor.request(6, "initialize", json!({}));
    assert_eq!(
        harness.editor.error(6).await,
        (ErrorCode::InvalidRequest as i32, "Server is already initialized".to_string())
    );

    harness.shutdown(7).await;
    harness.editor.request(8, "textDocument/hover", json!({}));
    assert_eq!(
        harness.editor.error(8).await,
        (ErrorCode::InvalidRequest as i32, "Server is shutting down".to_string())
    );
    harness.editor.request(9, "initialize", json!({}));
    assert_eq!(
        harness.editor.error(9).await,
        (ErrorCode::InvalidRequest as i32, "Server is already initialized".to_string())
    );
    harness.editor.exit();
    harness.stopped().await.unwrap();
}

#[tokio::test]
async fn notifications_are_dropped_before_initialize_is_answered_and_after_shutdown() {
    let mut harness = ServerHarness::start();
    harness.editor.notify("textDocument/didOpen", json!({"early": true}));
    harness.editor.request(0, "initialize", json!({}));
    let OrderedMessage::Initialize { reply, .. } = harness.workspace.ordered().await else {
        panic!("expected initialize");
    };
    harness.editor.notify("textDocument/didOpen", json!({"initializing": true}));
    // The rejection proves that the server handled the notification before the answer below.
    harness.editor.request(9, "textDocument/hover", json!({}));
    assert_eq!(harness.editor.error_code(9).await, ErrorCode::ServerNotInitialized as i32);
    reply.send(Ok(json!({}))).unwrap();
    harness.editor.result(0).await;

    harness.editor.notify("textDocument/didOpen", json!({"initialized": true}));
    let OrderedMessage::Notification { params, .. } = harness.workspace.ordered().await else {
        panic!("expected the notification sent after initialize was answered");
    };
    assert_eq!(params, json!({"initialized": true}));

    harness.editor.notify("initialized", json!({}));
    let OrderedMessage::Initialized = harness.workspace.ordered().await else {
        panic!("expected initialized");
    };
    harness.shutdown(1).await;
    harness.editor.notify("textDocument/didOpen", json!({"shutdown": true}));
    harness.editor.exit();
    let ServerHarness { editor: _editor, mut workspace, server } = harness;
    stopped(server).await.unwrap();
    assert!(workspace.receivers.ordered.try_recv().is_err());
}

#[tokio::test]
async fn unexpected_and_repeated_initialized_notifications_are_ignored() {
    let mut harness = ServerHarness::start();
    harness.editor.notify("initialized", json!({}));
    harness.initialize(json!({})).await;
    harness.editor.notify("initialized", json!({}));
    let OrderedMessage::Initialized = harness.workspace.ordered().await else {
        panic!("expected initialized");
    };
    harness.editor.notify("initialized", json!({}));
    harness.editor.request(1, "textDocument/hover", json!({}));
    let (method, reply) = harness.workspace.request().await;
    assert_eq!(method, "textDocument/hover");
    reply.send(Ok(Value::Null)).unwrap();
    assert_eq!(harness.editor.result(1).await, Value::Null);
}

#[tokio::test]
async fn requests_and_notifications_reach_the_workspace_actor_in_order() {
    let mut harness = ServerHarness::running().await;
    harness.editor.notify("textDocument/didOpen", json!({"order": 1}));
    harness.editor.request(1, "textDocument/hover", json!({"order": 2}));
    harness.editor.notify("textDocument/didChange", json!({"order": 3}));
    harness.editor.notify("$/unknownNotification", json!({}));
    harness.editor.request(2, "workspace/symbol", json!({"order": 4}));
    harness.editor.notify("custom/unknownNotification", json!({"order": 5}));

    let mut order = vec![];
    for _ in 0..5 {
        match harness.workspace.ordered().await {
            OrderedMessage::Notification { method, params } => order.push((method, params)),
            OrderedMessage::Request { method, params, .. } => order.push((method, params)),
            _ => panic!("unexpected ordered message"),
        }
    }
    assert_eq!(
        order,
        vec![
            ("textDocument/didOpen".to_string(), json!({"order": 1})),
            ("textDocument/hover".to_string(), json!({"order": 2})),
            ("textDocument/didChange".to_string(), json!({"order": 3})),
            ("workspace/symbol".to_string(), json!({"order": 4})),
            ("custom/unknownNotification".to_string(), json!({"order": 5})),
        ]
    );
    harness.workspace.assert_no_ordered_message();
}

#[tokio::test]
async fn rejections_map_to_json_rpc_error_codes() {
    let mut harness = ServerHarness::running().await;
    let rejections = [
        (Rejection::MethodNotFound, ErrorCode::MethodNotFound, "No such method custom/method"),
        (Rejection::InvalidParams("invalid".to_string()), ErrorCode::InvalidParams, "invalid"),
        (
            Rejection::ContentModified("modified".to_string()),
            ErrorCode::ContentModified,
            "modified",
        ),
        (Rejection::RequestFailed("failed".to_string()), ErrorCode::RequestFailed, "failed"),
        (Rejection::Internal("internal".to_string()), ErrorCode::InternalError, "internal"),
    ];
    for (id, (rejection, code, message)) in (1..).zip(rejections) {
        harness.editor.request(id, "custom/method", json!({}));
        let (_, reply) = harness.workspace.request().await;
        reply.send(Err(rejection)).unwrap();
        assert_eq!(harness.editor.error(id).await, (code as i32, message.to_string()));
    }
}

#[tokio::test]
async fn a_request_whose_id_is_still_waiting_is_rejected() {
    let mut harness = ServerHarness::running().await;
    harness.editor.request(7, "textDocument/hover", json!({"first": true}));
    let (_, first) = harness.workspace.request().await;
    harness.editor.request(7, "textDocument/hover", json!({"second": true}));
    assert_eq!(harness.editor.error_code(7).await, ErrorCode::InvalidRequest as i32);
    harness.workspace.assert_no_ordered_message();

    first.send(Ok(json!("first"))).unwrap();
    assert_eq!(harness.editor.result(7).await, json!("first"));
}

#[tokio::test]
async fn a_reply_channel_dropped_without_an_answer_produces_one_internal_error() {
    let mut harness = ServerHarness::running().await;
    harness.editor.request(1, "textDocument/hover", json!({}));
    let (_, reply) = harness.workspace.request().await;
    drop(reply);
    assert_eq!(harness.editor.error_code(1).await, ErrorCode::InternalError as i32);
    harness.editor.assert_no_message("second response", |message| is_response_to(message, 1)).await;
}

#[tokio::test]
async fn the_server_stops_when_the_workspace_actor_stops_while_a_request_waits() {
    let mut harness = ServerHarness::running().await;
    harness.editor.request(1, "textDocument/hover", json!({}));
    let (_, _reply) = harness.workspace.request().await;
    harness.workspace.stop(Err(WorkspaceFailure("the actor failed".to_string())));
    assert_eq!(
        harness.editor.error(1).await,
        (ErrorCode::InternalError as i32, "Workspace service stopped".to_string())
    );
    let error = harness.stopped().await.unwrap_err();
    assert!(matches!(error, ServerError::WorkspaceStopped(reason) if reason == "the actor failed"));
}

#[tokio::test]
async fn shutdown_and_exit_stop_the_server_successfully() {
    let mut harness = ServerHarness::running().await;
    harness.shutdown(1).await;
    harness.editor.exit();
    harness.stopped().await.unwrap();
}

#[tokio::test]
async fn exit_without_shutdown_is_an_error() {
    let harness = ServerHarness::running().await;
    harness.editor.exit();
    assert!(matches!(harness.stopped().await, Err(ServerError::ExitBeforeShutdown)));
}

#[tokio::test]
async fn end_of_input_is_an_error() {
    let ServerHarness { editor, workspace: _workspace, server } = ServerHarness::running().await;
    drop(editor);
    assert!(matches!(stopped(server).await, Err(ServerError::EndOfInput)));
}

#[tokio::test]
async fn shutdown_does_not_reject_requests_still_waiting_for_answers() {
    let mut harness = ServerHarness::running().await;
    harness.editor.request(1, "textDocument/hover", json!({}));
    let (_, reply) = harness.workspace.request().await;
    harness.shutdown(2).await;
    reply.send(Ok(json!("late but valid"))).unwrap();
    assert_eq!(harness.editor.result(1).await, json!("late but valid"));
    harness.editor.exit();
    harness.stopped().await.unwrap();
}
