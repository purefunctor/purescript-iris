//! Tests where the test plays both the editor and the workspace actor.

use std::collections::VecDeque;
use std::time::Duration;

use lsp_server::{Connection, ErrorCode, Message, Notification, Request, RequestId, Response};
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::service::{
    ControlMessage, OrderedMessage, Rejection, SettingsResponse, WorkspaceEvent,
    WorkspaceEventSender, WorkspaceFailure, WorkspaceReceivers, WorkspaceSenders,
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

    /// Starts a server with the given `initialize` parameters. Without `workspace/configuration`
    /// support, the settings message that follows `initialized` is consumed too.
    async fn running_with(initialize: Value) -> ServerHarness {
        let configuration = initialize["capabilities"]["workspace"]["configuration"] == true;
        let mut harness = ServerHarness::start();
        harness.initialize(initialize).await;
        harness.editor.notify("initialized", json!({}));
        let OrderedMessage::Initialized = harness.workspace.ordered().await else {
            panic!("expected initialized to reach the workspace actor");
        };
        if !configuration {
            assert_eq!(harness.workspace.settings().await, SettingsResponse::Unsupported);
        }
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

    /// Waits for a request from the server.
    async fn server_request(&mut self, method: &str) -> Request {
        self.claim(&format!("a {method} request"), |message| match message {
            Message::Request(request) if request.method == method => Some(Request::clone(request)),
            _ => None,
        })
        .await
    }

    async fn notification(&mut self, method: &str) -> Value {
        self.claim(&format!("a {method} notification"), |message| match message {
            Message::Notification(notification) if notification.method == method => {
                Some(Value::clone(&notification.params))
            }
            _ => None,
        })
        .await
    }

    fn reply(&self, request: &Request, result: Value) {
        self.send(Response::new_ok(RequestId::clone(&request.id), result));
    }

    fn reply_error(&self, request: &Request, message: &str) {
        let code = ErrorCode::InternalError as i32;
        self.send(Response::new_err(RequestId::clone(&request.id), code, message.to_string()));
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

    fn assert_no_control_message(&mut self) {
        assert!(
            matches!(self.receivers.control.try_recv(), Err(mpsc::error::TryRecvError::Empty)),
            "unexpected control message"
        );
    }

    async fn settings(&mut self) -> SettingsResponse {
        match self.ordered().await {
            OrderedMessage::Settings(response) => response,
            _ => panic!("expected settings"),
        }
    }
}

fn configuration_capabilities() -> Value {
    json!({
        "capabilities": {"workspace": {"configuration": true}},
        "workspaceFolders": [{"uri": "file:///workspace/", "name": "workspace"}]
    })
}

fn progress_capabilities() -> Value {
    json!({"capabilities": {"window": {"workDoneProgress": true}}})
}

fn started(generation: u64) -> WorkspaceEvent {
    WorkspaceEvent::PreparationStarted {
        generation,
        title: "Preparing Iris workspace".to_string(),
        message: "Discovering Spago workspace".to_string(),
    }
}

fn reported(generation: u64, message: &str, percentage: u32) -> WorkspaceEvent {
    WorkspaceEvent::PreparationProgress {
        generation,
        message: message.to_string(),
        percentage: Some(percentage),
    }
}

fn ended(generation: u64, message: &str) -> WorkspaceEvent {
    WorkspaceEvent::PreparationEnded { generation, message: message.to_string() }
}

fn progress_value(notification: &Message) -> Option<Value> {
    match notification {
        Message::Notification(notification) if notification.method == "$/progress" => {
            Some(Value::clone(&notification.params["value"]))
        }
        _ => None,
    }
}

fn is_progress(message: &Message) -> bool {
    progress_value(message).is_some()
}

impl Editor {
    /// Waits for the next `$/progress` value for `token`.
    async fn progress(&mut self, token: &str) -> Value {
        let params = self.notification("$/progress").await;
        assert_eq!(params["token"], token);
        Value::clone(&params["value"])
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
    assert_eq!(harness.workspace.settings().await, SettingsResponse::Unsupported);
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
    assert_eq!(harness.workspace.settings().await, SettingsResponse::Unsupported);
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

#[tokio::test]
async fn late_answer_after_cancellation_does_not_answer_reused_id() {
    let mut harness = ServerHarness::running().await;
    harness.editor.request(7, "textDocument/hover", json!({}));
    let OrderedMessage::Request { reply: mut stale, .. } = harness.workspace.ordered().await else {
        panic!()
    };

    harness.editor.cancel(7);
    assert_eq!(harness.editor.error_code(7).await, ErrorCode::RequestCanceled as i32);
    tokio::time::timeout(PATIENCE, stale.closed()).await.expect("the reply channel stayed open");

    harness.editor.request(7, "textDocument/hover", json!({}));
    let OrderedMessage::Request { reply: fresh, .. } = harness.workspace.ordered().await else {
        panic!()
    };
    let _ = stale.send(Ok(json!("stale")));
    let _ = fresh.send(Ok(json!("fresh")));
    assert_eq!(harness.editor.result(7).await, json!("fresh"));
    harness.editor.assert_no_message("second response", |message| is_response_to(message, 7)).await;
}

#[tokio::test]
async fn cancellation_is_answered_once_and_ignores_unknown_ids() {
    let mut harness = ServerHarness::running().await;
    harness.editor.request(1, "textDocument/hover", json!({}));
    let (_, reply) = harness.workspace.request().await;
    harness.editor.cancel(1);
    harness.editor.cancel(1);
    harness.editor.cancel(99);
    harness.editor.notify("$/cancelRequest", json!({"id": "not-waiting"}));
    assert_eq!(
        harness.editor.error(1).await,
        (ErrorCode::RequestCanceled as i32, "Client cancelled the request".to_string())
    );
    let _ = reply.send(Ok(Value::Null));
    harness.editor.assert_no_message("second response", |message| is_response_to(message, 1)).await;
    harness
        .editor
        .assert_no_message("response to an unknown ID", |message| is_response_to(message, 99))
        .await;
}

#[tokio::test]
async fn requests_after_shutdown_can_still_be_cancelled() {
    let mut harness = ServerHarness::running().await;
    harness.editor.request(1, "textDocument/hover", json!({}));
    let (_, _reply) = harness.workspace.request().await;
    harness.shutdown(2).await;
    harness.editor.cancel(1);
    assert_eq!(harness.editor.error_code(1).await, ErrorCode::RequestCanceled as i32);
    harness.editor.exit();
    harness.stopped().await.unwrap();
}

#[tokio::test]
async fn registrations_follow_the_client_capabilities() {
    let cases = [
        (json!({}), vec![]),
        (
            json!({"workspace": {"didChangeWatchedFiles": {"dynamicRegistration": true}}}),
            vec!["workspace/didChangeWatchedFiles"],
        ),
        (json!({"workspace": {"didChangeConfiguration": {"dynamicRegistration": true}}}), vec![]),
        (
            json!({"workspace": {
                "configuration": true,
                "didChangeConfiguration": {"dynamicRegistration": true}
            }}),
            vec!["workspace/didChangeConfiguration"],
        ),
        (
            json!({"workspace": {
                "configuration": true,
                "didChangeConfiguration": {"dynamicRegistration": true},
                "didChangeWatchedFiles": {"dynamicRegistration": true}
            }}),
            vec!["workspace/didChangeWatchedFiles", "workspace/didChangeConfiguration"],
        ),
    ];
    for (capabilities, expected) in cases {
        let mut harness = ServerHarness::running_with(json!({"capabilities": capabilities})).await;
        let mut methods = vec![];
        for _ in 0..expected.len() {
            let request = harness.editor.server_request("client/registerCapability").await;
            let registration = &request.params["registrations"][0];
            match registration["method"].as_str().unwrap() {
                "workspace/didChangeWatchedFiles" => {
                    assert_eq!(registration["id"], "purescript-source-files");
                    assert_eq!(
                        registration["registerOptions"],
                        json!({"watchers": [
                            {"globPattern": "**/*.purs"},
                            {"globPattern": "**/*.js"},
                            {"globPattern": "**/*.jsx"}
                        ]})
                    );
                }
                "workspace/didChangeConfiguration" => {
                    assert_eq!(registration["id"], "iris-workspace-configuration");
                    assert!(registration.get("registerOptions").is_none());
                }
                method => panic!("unexpected registration {method}"),
            }
            methods.push(registration["method"].as_str().unwrap().to_string());
            harness.editor.reply(&request, Value::Null);
        }
        assert_eq!(methods, expected);
        harness
            .editor
            .assert_no_message("registration", |message| {
                matches!(message, Message::Request(request) if request.method == "client/registerCapability")
            })
            .await;
    }
}

#[tokio::test]
async fn settings_are_requested_on_initialized_and_forwarded() {
    let mut harness = ServerHarness::running_with(configuration_capabilities()).await;
    let request = harness.editor.server_request("workspace/configuration").await;
    assert_eq!(
        request.params,
        json!({"items": [{"scopeUri": "file:///workspace/", "section": "iris.server"}]})
    );
    harness.editor.reply(&request, json!([{"diagnostics": {"onChange": true}}]));
    assert_eq!(
        harness.workspace.settings().await,
        SettingsResponse::Received(json!([{"diagnostics": {"onChange": true}}]))
    );

    // A repeated response is a response to an unknown request.
    harness.editor.reply(&request, json!([{}]));
    harness.editor.request(1, "textDocument/hover", json!({}));
    let (method, _) = harness.workspace.request().await;
    assert_eq!(method, "textDocument/hover");
}

#[tokio::test]
async fn only_the_latest_settings_generation_is_forwarded() {
    let mut harness = ServerHarness::running_with(configuration_capabilities()).await;
    let first = harness.editor.server_request("workspace/configuration").await;
    harness
        .editor
        .notify("workspace/didChangeConfiguration", json!({"settings": {"ignored": true}}));
    let second = harness.editor.server_request("workspace/configuration").await;
    assert_ne!(first.id, second.id);

    harness.editor.reply(&first, json!([{"stale": true}]));
    harness.editor.reply_error(&second, "boom");
    assert_eq!(
        harness.workspace.settings().await,
        SettingsResponse::Failed("boom (jsonrpc error -32603)".to_string())
    );
    harness.workspace.assert_no_ordered_message();
}

#[tokio::test]
async fn settings_requests_time_out_after_ten_seconds() {
    let mut harness = ServerHarness::running_with(configuration_capabilities()).await;
    let request = harness.editor.server_request("workspace/configuration").await;
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(9)).await;
    harness.workspace.assert_no_ordered_message();
    tokio::time::advance(Duration::from_secs(2)).await;
    assert_eq!(
        harness.workspace.settings().await,
        SettingsResponse::Failed("workspace/configuration request timed out".to_string())
    );
    tokio::time::resume();

    // The late response is ignored.
    harness.editor.reply(&request, json!([{}]));
    harness.editor.request(1, "textDocument/hover", json!({}));
    let (method, _) = harness.workspace.request().await;
    assert_eq!(method, "textDocument/hover");
}

#[tokio::test]
async fn clients_without_workspace_configuration_get_unsupported_settings_once() {
    // `running` consumes the one `SettingsResponse::Unsupported`.
    let mut harness = ServerHarness::running().await;
    harness.editor.notify("workspace/didChangeConfiguration", json!({"settings": {}}));
    harness.editor.request(1, "textDocument/hover", json!({}));
    let (method, _) = harness.workspace.request().await;
    assert_eq!(method, "textDocument/hover");
    harness
        .editor
        .assert_no_message("configuration request", |message| {
            matches!(message, Message::Request(request) if request.method == "workspace/configuration")
        })
        .await;
}

#[tokio::test]
async fn progress_accepted_immediately_reports_in_order() {
    let mut harness = ServerHarness::running_with(progress_capabilities()).await;
    harness.workspace.emit(started(1));
    let create = harness.editor.server_request("window/workDoneProgress/create").await;
    assert_eq!(create.params, json!({"token": "iris/startup/1"}));
    harness.editor.reply(&create, Value::Null);
    assert_eq!(
        harness.editor.progress("iris/startup/1").await,
        json!({
            "kind": "begin",
            "title": "Preparing Iris workspace",
            "cancellable": true,
            "message": "Discovering Spago workspace",
            "percentage": 0
        })
    );
    harness.workspace.emit(reported(1, "Compiling packages (0/2 completed)", 0));
    assert_eq!(
        harness.editor.progress("iris/startup/1").await,
        json!({
            "kind": "report",
            "cancellable": true,
            "message": "Compiling packages (0/2 completed)",
            "percentage": 0
        })
    );
    harness.workspace.emit(ended(1, "Workspace preparation finished"));
    assert_eq!(
        harness.editor.progress("iris/startup/1").await,
        json!({"kind": "end", "message": "Workspace preparation finished"})
    );
    harness.workspace.emit(reported(1, "after the end", 100));
    harness.editor.assert_no_message("progress after the end", is_progress).await;
}

#[tokio::test]
async fn progress_accepted_late_sends_begin_the_latest_report_and_end() {
    let mut harness = ServerHarness::running_with(progress_capabilities()).await;
    harness.workspace.emit(started(1));
    let create = harness.editor.server_request("window/workDoneProgress/create").await;
    harness.workspace.emit(reported(1, "first", 10));
    harness.workspace.emit(reported(1, "latest", 20));
    harness.workspace.emit(ended(1, "Workspace preparation finished"));
    harness.editor.assert_no_message("progress before acceptance", is_progress).await;

    harness.editor.reply(&create, Value::Null);
    let values = [
        harness.editor.progress("iris/startup/1").await,
        harness.editor.progress("iris/startup/1").await,
        harness.editor.progress("iris/startup/1").await,
    ];
    assert_eq!(values[0]["kind"], "begin");
    assert_eq!(
        values[1],
        json!({"kind": "report", "cancellable": true, "message": "latest", "percentage": 20})
    );
    assert_eq!(values[2], json!({"kind": "end", "message": "Workspace preparation finished"}));
    harness.editor.assert_no_message("more progress", is_progress).await;
}

#[tokio::test]
async fn progress_accepted_after_shutdown_is_still_balanced() {
    let mut harness = ServerHarness::running_with(progress_capabilities()).await;
    harness.workspace.emit(started(1));
    let create = harness.editor.server_request("window/workDoneProgress/create").await;
    harness.shutdown(1).await;
    harness.workspace.emit(ended(1, "Workspace preparation cancelled"));
    harness.editor.assert_no_message("progress before acceptance", is_progress).await;

    harness.editor.reply(&create, Value::Null);
    assert_eq!(harness.editor.progress("iris/startup/1").await["kind"], "begin");
    assert_eq!(
        harness.editor.progress("iris/startup/1").await,
        json!({"kind": "end", "message": "Workspace preparation cancelled"})
    );
    harness.editor.exit();
    harness.stopped().await.unwrap();
}

#[tokio::test]
async fn rejected_progress_creation_sends_no_progress() {
    let mut harness = ServerHarness::running_with(progress_capabilities()).await;
    harness.workspace.emit(started(1));
    let create = harness.editor.server_request("window/workDoneProgress/create").await;
    harness.editor.reply_error(&create, "rejected");
    harness.workspace.emit(reported(1, "report", 50));
    harness.workspace.emit(ended(1, "Workspace preparation finished"));
    harness.editor.assert_no_message("progress after rejection", is_progress).await;
}

#[tokio::test]
async fn progress_is_omitted_without_client_support() {
    let mut harness = ServerHarness::running().await;
    harness.workspace.emit(started(1));
    harness.workspace.emit(ended(1, "Workspace preparation finished"));
    harness
        .editor
        .assert_no_message("progress", |message| {
            is_progress(message)
                || matches!(message, Message::Request(request) if request.method == "window/workDoneProgress/create")
        })
        .await;
}

#[tokio::test]
async fn cancelling_the_current_progress_token_cancels_preparation_ahead_of_ordered_messages() {
    let mut harness = ServerHarness::running_with(progress_capabilities()).await;
    harness.workspace.emit(started(1));
    let create = harness.editor.server_request("window/workDoneProgress/create").await;
    harness.editor.reply(&create, Value::Null);
    assert_eq!(harness.editor.progress("iris/startup/1").await["kind"], "begin");
    harness.workspace.emit(ended(1, "Workspace preparation cancelled"));
    assert_eq!(harness.editor.progress("iris/startup/1").await["kind"], "end");

    harness.workspace.emit(started(2));
    let create = harness.editor.server_request("window/workDoneProgress/create").await;
    harness.editor.reply(&create, Value::Null);
    assert_eq!(harness.editor.progress("iris/startup/2").await["kind"], "begin");

    // Ordered messages that the workspace actor has not reached yet.
    harness.editor.notify("textDocument/didOpen", json!({"queued": 1}));
    harness.editor.notify("textDocument/didChange", json!({"queued": 2}));
    for token in [json!("iris/startup/1"), json!("unknown"), json!(2)] {
        harness.editor.notify("window/workDoneProgress/cancel", json!({"token": token}));
    }
    harness.editor.notify("window/workDoneProgress/cancel", json!({"token": "iris/startup/2"}));
    harness.editor.notify("window/workDoneProgress/cancel", json!({"token": "iris/startup/2"}));
    assert_eq!(
        harness.workspace.control().await,
        ControlMessage::CancelPreparation { generation: 2 }
    );

    let OrderedMessage::Notification { params, .. } = harness.workspace.ordered().await else {
        panic!("expected the first queued notification");
    };
    assert_eq!(params, json!({"queued": 1}));
    let OrderedMessage::Notification { params, .. } = harness.workspace.ordered().await else {
        panic!("expected the second queued notification");
    };
    assert_eq!(params, json!({"queued": 2}));
    harness.workspace.assert_no_control_message();

    harness.workspace.emit(ended(2, "Workspace preparation cancelled"));
    assert_eq!(
        harness.editor.progress("iris/startup/2").await,
        json!({"kind": "end", "message": "Workspace preparation cancelled"})
    );
    harness.editor.assert_no_message("a second end", is_progress).await;
}

#[tokio::test]
async fn workspace_events_become_editor_notifications() {
    let mut harness = ServerHarness::running().await;
    let uri = url::Url::parse("file:///workspace/src/Main.purs").unwrap();
    harness.workspace.emit(WorkspaceEvent::Diagnostics {
        uri: url::Url::clone(&uri),
        version: Some(3),
        diagnostics: json!([{"message": "problem"}]),
    });
    assert_eq!(
        harness.editor.notification("textDocument/publishDiagnostics").await,
        json!({"uri": uri, "version": 3, "diagnostics": [{"message": "problem"}]})
    );
    harness.workspace.emit(WorkspaceEvent::Diagnostics {
        uri: url::Url::clone(&uri),
        version: None,
        diagnostics: json!([]),
    });
    assert_eq!(
        harness.editor.notification("textDocument/publishDiagnostics").await,
        json!({"uri": uri, "diagnostics": []})
    );
    harness.workspace.emit(WorkspaceEvent::Error { message: "Invalid Iris settings".to_string() });
    assert_eq!(
        harness.editor.notification("window/showMessage").await,
        json!({"type": 1, "message": "Invalid Iris settings"})
    );
}

#[tokio::test]
async fn malformed_notifications_that_the_server_decodes_stop_it() {
    let harness = ServerHarness::running().await;
    harness.editor.notify("workspace/didChangeConfiguration", json!(42));
    let error = harness.stopped().await.unwrap_err();
    assert!(
        matches!(&error, ServerError::InvalidNotification { method, .. } if method == "workspace/didChangeConfiguration"),
        "{error}"
    );
}

#[tokio::test]
async fn a_process_id_that_cannot_be_represented_installs_no_monitor() {
    // As an `i32`, 2^31 would wrap to a negative ID that names no process; monitoring it would
    // report the editor as exited and stop the server.
    let mut harness = ServerHarness::running_with(json!({"processId": 2_147_483_648_i64})).await;
    harness.editor.request(1, "textDocument/hover", json!({}));
    let (_, reply) = harness.workspace.request().await;
    reply.send(Ok(Value::Null)).unwrap();
    assert_eq!(harness.editor.result(1).await, Value::Null);
    harness.shutdown(2).await;
    harness.editor.exit();
    harness.stopped().await.unwrap();
}

#[tokio::test]
async fn a_live_editor_process_keeps_the_server_running() {
    let mut harness = ServerHarness::running_with(json!({"processId": std::process::id()})).await;
    harness.shutdown(1).await;
    harness.editor.exit();
    harness.stopped().await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn an_exited_editor_process_stops_the_server() {
    let mut child = std::process::Command::new("sh").arg("-c").arg("exit 0").spawn().unwrap();
    let process_id = child.id();
    child.wait().unwrap();
    let mut harness = ServerHarness::start();
    harness.initialize(json!({"processId": process_id})).await;
    assert!(matches!(harness.stopped().await, Err(ServerError::EditorExited)));
}

#[test]
fn lsp_server_rejects_a_response_with_neither_result_nor_error() {
    let text = r#"{"jsonrpc":"2.0","id":1}"#;
    let framed = format!("Content-Length: {}\r\n\r\n{text}", text.len());
    let error = Message::read(&mut framed.as_bytes()).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
}

#[test]
fn lsp_server_decodes_a_response_with_both_fields_from_the_first() {
    let read = |text: &str| {
        let framed = format!("Content-Length: {}\r\n\r\n{text}", text.len());
        let Some(Message::Response(response)) = Message::read(&mut framed.as_bytes()).unwrap()
        else {
            panic!("expected a response");
        };
        response.response_result
    };
    let result_first =
        read(r#"{"jsonrpc":"2.0","id":1,"result":42,"error":{"code":-32603,"message":"no"}}"#);
    assert_eq!(result_first.unwrap(), json!(42));
    let error_first =
        read(r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32603,"message":"no"},"result":42}"#);
    assert_eq!(error_first.unwrap_err().message, "no");
}
