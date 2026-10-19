use std::collections::HashMap;
use std::io::{BufReader, Read};
use std::path::Path;
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Stdio};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::{fmt, thread};

use lsp_server::{ErrorCode, Message, RequestId};
use lsp_types::{
    ClientCapabilities, DocumentSymbolParams, InitializeParams, InitializeResult,
    PartialResultParams, ProgressToken, TextDocumentIdentifier, WorkDoneProgressCancelParams,
    WorkDoneProgressCreateParams, WorkDoneProgressParams, WorkspaceFolder, WorkspaceFolders,
    WorkspaceFoldersInitializeParams, WorkspaceSymbolParams,
};
use process_control::{ChildExt, Control};
use serde::Serialize;
use serde_json::{Value, json};
use url::Url;

use super::snapshot_json;
use super::support::TestWorkspace;

pub struct ClientState {
    pub progress_tokens: Mutex<Vec<ProgressToken>>,
    /// Server requests the client does not handle, and messages it could not accept.
    unexpected_messages: Mutex<Vec<String>>,
}

/// Why a request produced no result.
#[derive(Debug)]
pub enum Error {
    Response(ResponseError),
    /// The connection ended before the response arrived.
    Disconnected,
}

/// A response error as the tests assert and snapshot it. Absent `data` is snapshotted as `null`.
#[derive(Debug, Serialize)]
pub struct ResponseError {
    pub code: i32,
    message: String,
    data: Option<Value>,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Response(error) => {
                write!(formatter, "{} (jsonrpc error {})", error.message, error.code)
            }
            Error::Disconnected => write!(formatter, "the language server disconnected"),
        }
    }
}

pub struct LanguageServer {
    child: Child,
    pub connection: Connection,
    stderr: Arc<Mutex<Vec<u8>>>,
    messages: Receiver<Value>,
    pub notifications: Vec<Value>,
    pub client: Arc<ClientState>,
}

impl LanguageServer {
    pub fn start(
        workspace: &TestWorkspace,
        directory: &str,
        arguments: &[&str],
        root: &Path,
    ) -> LanguageServer {
        LanguageServer::start_with_capabilities(workspace, directory, arguments, root, json!({}))
    }

    pub fn start_with_capabilities(
        workspace: &TestWorkspace,
        directory: &str,
        arguments: &[&str],
        root: &Path,
        capabilities: Value,
    ) -> LanguageServer {
        let mut arguments = arguments.to_vec();
        arguments.extend(["--lsp-log", "off"]);
        let client = Arc::new(ClientState {
            progress_tokens: Mutex::new(Vec::new()),
            unexpected_messages: Mutex::new(Vec::new()),
        });

        let mut command = workspace.command_builder(directory, &arguments);
        command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = command.spawn().unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let stderr = drain(child.stderr.take().unwrap());
        let (notifications, messages) = mpsc::channel();
        let connection = Connection::start(stdin, stdout, Arc::clone(&client), notifications);
        // Constructed before initialization so that a failing assertion still stops the child.
        let server = LanguageServer {
            child,
            connection,
            stderr,
            messages,
            notifications: Vec::new(),
            client,
        };

        let capabilities = serde_json::from_value::<ClientCapabilities>(capabilities).unwrap();
        let parameters = InitializeParams {
            capabilities,
            workspace_folders_initialize_params: WorkspaceFoldersInitializeParams {
                workspace_folders: Some(WorkspaceFolders::WorkspaceFolderList(vec![
                    WorkspaceFolder {
                        uri: Url::from_directory_path(root).unwrap(),
                        name: "project".to_owned(),
                    },
                ])),
            },
            ..InitializeParams::default()
        };
        let response =
            server.connection.request("initialize", serde_json::to_value(parameters).unwrap());
        let result = response
            .recv_timeout(Duration::from_secs(10))
            .expect("invariant violated: timed out waiting for initialize response")
            .unwrap();
        let result = serde_json::from_value::<InitializeResult>(result).unwrap();
        snapshot_json("server_capabilities", &serde_json::to_value(result.capabilities).unwrap());
        server.connection.notify("initialized", json!({}));
        server
    }

    pub fn start_with_progress(workspace: &TestWorkspace) -> LanguageServer {
        let capabilities = json!({"window": {"workDoneProgress": true}});
        let root = workspace.path();
        LanguageServer::start_with_capabilities(workspace, "", &["lsp"], root, capabilities)
    }

    pub fn notify(&mut self, method: &str, parameters: Value) {
        self.connection.notify(method, parameters);
    }

    pub fn cancel_progress(&self, token: ProgressToken) {
        let parameters = WorkDoneProgressCancelParams { token };
        let parameters = serde_json::to_value(parameters).unwrap();
        self.connection.notify("window/workDoneProgress/cancel", parameters);
    }

    #[track_caller]
    pub fn request(&mut self, method: &str, parameters: Value) -> Value {
        for _ in 0..10 {
            match self.request_once(method, parameters.clone()) {
                Ok(result) => return result,
                Err(Error::Response(response))
                    if response.code == ErrorCode::ContentModified as i32
                        || response.code == ErrorCode::RequestCanceled as i32 => {}
                Err(error) => panic!("{method} request failed: {error}"),
            }
        }
        panic!("request {method} was repeatedly reported as stale");
    }

    #[track_caller]
    pub fn request_once(&mut self, method: &str, parameters: Value) -> Result<Value, Error> {
        self.request_async(method, parameters)
            .recv_timeout(Duration::from_secs(10))
            .unwrap_or_else(|_| panic!("timed out waiting for response to {method} request"))
    }

    /// Sends the request now and returns the receiver of its response.
    pub fn request_async(&self, method: &str, parameters: Value) -> Receiver<Result<Value, Error>> {
        assert_eq!(method, "workspace/symbol");
        let parameters: WorkspaceSymbolParams = serde_json::from_value(parameters).unwrap();
        self.connection.request(method, serde_json::to_value(parameters).unwrap())
    }

    /// Sends the request now and returns the receiver of its response.
    pub fn document_symbols_async(&self, uri: Url) -> Receiver<Result<Value, Error>> {
        let parameters = DocumentSymbolParams {
            text_document: TextDocumentIdentifier { uri },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };
        let parameters = serde_json::to_value(parameters).unwrap();
        self.connection.request("textDocument/documentSymbol", parameters)
    }

    #[track_caller]
    pub fn wait_for_notification(&mut self, method: &str) -> Value {
        self.wait_for_notification_matching(method, |_| true)
    }

    #[track_caller]
    pub fn wait_for_notification_matching(
        &mut self,
        method: &str,
        predicate: impl Fn(&Value) -> bool,
    ) -> Value {
        self.wait_for_notification_matching_with_timeout(method, Duration::from_secs(10), predicate)
    }

    #[track_caller]
    pub fn wait_for_notification_matching_with_timeout(
        &mut self,
        method: &str,
        timeout: Duration,
        predicate: impl Fn(&Value) -> bool,
    ) -> Value {
        let deadline = Instant::now() + timeout;
        let waiting_for = format!("notification {method}");
        loop {
            if let Some(index) = self.notifications.iter().position(|notification| {
                notification["method"] == method && predicate(notification)
            }) {
                return self.notifications.remove(index);
            }
            let message = self.receive_before(deadline, &waiting_for);
            self.notifications.push(message);
        }
    }

    #[track_caller]
    pub fn wait_for_progress(&mut self, kind: &str) -> Value {
        self.wait_for_notification_matching("$/progress", |notification| {
            notification["params"]["value"]["kind"] == kind
        })
    }

    #[track_caller]
    fn receive_before(&self, deadline: Instant, waiting_for: &str) -> Value {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .unwrap_or_else(|| panic!("timed out waiting for {waiting_for}"));
        match self.messages.recv_timeout(remaining) {
            Ok(message) => message,
            Err(RecvTimeoutError::Timeout) => panic!("timed out waiting for {waiting_for}"),
            Err(RecvTimeoutError::Disconnected) => {
                panic!("language server disconnected while waiting for {waiting_for}")
            }
        }
    }

    pub fn collect_notifications(&mut self) {
        self.notifications.extend(self.messages.try_iter());
    }

    pub fn assert_diagnostics(&mut self, uri: &Url, version: i32, enabled: bool) {
        // A response proves the preceding document notification reached the server before
        // the bounded wait for asynchronous diagnostics, including when none are expected.
        self.request("workspace/symbol", json!({"query": "noSuchSymbol"}));
        if enabled {
            let message = self.wait_for_notification_matching(
                "textDocument/publishDiagnostics",
                |notification| notification["params"]["uri"] == uri.as_str(),
            );
            assert_eq!(message["params"]["version"], version);
            let diagnostics = message["params"]["diagnostics"].as_array().unwrap();
            snapshot_json("invalid_string_diagnostics", &Value::Array(diagnostics.clone()));
        } else {
            assert!(!self.notifications.iter().any(|notification| {
                notification["method"] == "textDocument/publishDiagnostics"
                    && notification["params"]["uri"] == uri.as_str()
            }));
            let deadline = Instant::now() + Duration::from_millis(500);
            while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
                match self.messages.recv_timeout(remaining) {
                    Ok(message) => {
                        assert!(
                            message["method"] != "textDocument/publishDiagnostics"
                                || message["params"]["uri"] != uri.as_str(),
                            "{message}"
                        );
                        self.notifications.push(message);
                    }
                    Err(RecvTimeoutError::Timeout) => break,
                    Err(error) => panic!("language server disconnected: {error}"),
                }
            }
        }
    }

    pub fn request_shutdown(&mut self) {
        self.connection
            .request("shutdown", Value::Null)
            .recv_timeout(Duration::from_secs(10))
            .expect("invariant violated: timed out waiting for shutdown response")
            .unwrap();
    }

    pub fn finish_shutdown(&mut self) {
        self.connection.notify("exit", Value::Null);
        self.connection.close();
        let deadline = Instant::now() + Duration::from_secs(10);
        assert!(
            self.connection.wait_for_end_of_input(deadline),
            "invariant violated: timed out stopping language client"
        );
        let status = self
            .child
            .controlled()
            .time_limit(deadline.saturating_duration_since(Instant::now()))
            .wait()
            .unwrap()
            .expect("invariant violated: timed out stopping language server");
        let unexpected_messages = self.client.unexpected_messages.lock().unwrap();
        assert!(
            unexpected_messages.is_empty(),
            "unexpected server messages: {unexpected_messages:?}"
        );
        let stderr = String::from_utf8_lossy(&self.stderr.lock().unwrap()).into_owned();
        assert!(status.success(), "language server exited with {status}\nstderr:\n{stderr}");
    }

    pub fn shutdown(&mut self) {
        self.request_shutdown();
        self.finish_shutdown();
    }
}

impl Drop for LanguageServer {
    fn drop(&mut self) {
        self.connection.close();
        let _ = self.child.kill();
        let _ = self.child.wait();
        if thread::panicking()
            && let Ok(stderr) = self.stderr.lock()
        {
            eprintln!("language server stderr:\n{}", String::from_utf8_lossy(&stderr));
        }
    }
}

/// The client end of the connection to an `iris lsp` process.
///
/// Every message the client sends goes through one queue in the order it was sent, so a request
/// reaches the server after every message sent before it. A reader thread matches responses to
/// requests by ID, answers requests from the server, and forwards notifications.
pub struct Connection {
    outgoing: Sender<Outgoing>,
    waiting: Arc<Mutex<WaitingResponses>>,
    next_id: AtomicI32,
    input_ended: Receiver<()>,
}

enum Outgoing {
    Message(Message),
    /// Closes the server's standard input.
    Close,
}

/// Requests sent to the server that are waiting for a response. Once the server's output ends,
/// the connection is closed and new requests fail immediately.
struct WaitingResponses {
    open: bool,
    responses: HashMap<RequestId, Sender<Result<Value, Error>>>,
}

impl Connection {
    pub fn start(
        stdin: ChildStdin,
        stdout: ChildStdout,
        client: Arc<ClientState>,
        notifications: Sender<Value>,
    ) -> Connection {
        let (outgoing, outgoing_receiver) = mpsc::channel();
        let waiting =
            Arc::new(Mutex::new(WaitingResponses { open: true, responses: HashMap::new() }));
        let (input_ended_sender, input_ended) = mpsc::channel();
        thread::spawn(move || write_messages(stdin, outgoing_receiver));
        let reader = Reader {
            outgoing: Sender::clone(&outgoing),
            waiting: Arc::clone(&waiting),
            client,
            notifications,
        };
        thread::spawn(move || {
            reader.read_messages(stdout);
            let _ = input_ended_sender.send(());
        });
        Connection { outgoing, waiting, next_id: AtomicI32::new(0), input_ended }
    }

    /// Sends a request before returning, and returns the receiver of its response.
    pub fn request(&self, method: &str, params: Value) -> Receiver<Result<Value, Error>> {
        let id = RequestId::from(self.next_id.fetch_add(1, Ordering::Relaxed));
        let (response, result) = mpsc::channel();
        {
            let mut waiting = self.waiting.lock().unwrap();
            if !waiting.open {
                let _ = response.send(Err(Error::Disconnected));
                return result;
            }
            waiting.responses.insert(RequestId::clone(&id), response);
        }
        let request = lsp_server::Request { id, method: method.to_owned(), params };
        self.send(Message::Request(request));
        result
    }

    pub fn notify(&self, method: &str, params: Value) {
        let notification = lsp_server::Notification { method: method.to_owned(), params };
        self.send(Message::Notification(notification));
    }

    fn send(&self, message: Message) {
        let _ = self.outgoing.send(Outgoing::Message(message));
    }

    /// Closes the server's standard input after the messages already sent.
    fn close(&self) {
        let _ = self.outgoing.send(Outgoing::Close);
    }

    /// Waits until the server's output ends.
    fn wait_for_end_of_input(&self, deadline: Instant) -> bool {
        let remaining = deadline.saturating_duration_since(Instant::now());
        !matches!(self.input_ended.recv_timeout(remaining), Err(RecvTimeoutError::Timeout))
    }
}

fn write_messages(mut stdin: ChildStdin, outgoing: Receiver<Outgoing>) {
    for message in outgoing {
        match message {
            Outgoing::Message(message) => {
                if message.write(&mut stdin).is_err() {
                    break;
                }
            }
            Outgoing::Close => break,
        }
    }
}

struct Reader {
    outgoing: Sender<Outgoing>,
    waiting: Arc<Mutex<WaitingResponses>>,
    client: Arc<ClientState>,
    notifications: Sender<Value>,
}

impl Reader {
    fn read_messages(self, stdout: ChildStdout) {
        let mut stdout = BufReader::new(stdout);
        loop {
            let message = match Message::read(&mut stdout) {
                Ok(Some(message)) => message,
                Ok(None) => break,
                Err(error) => {
                    self.unexpected(format!("unreadable message: {error}"));
                    break;
                }
            };
            match message {
                Message::Response(response) => self.receive_response(response),
                Message::Request(request) => self.answer(request),
                Message::Notification(notification) => self.receive_notification(notification),
            }
        }
        let mut waiting = self.waiting.lock().unwrap();
        waiting.open = false;
        for (_, response) in waiting.responses.drain() {
            let _ = response.send(Err(Error::Disconnected));
        }
    }

    fn receive_response(&self, response: lsp_server::Response) {
        let Some(sender) = self.waiting.lock().unwrap().responses.remove(&response.id) else {
            self.unexpected(format!("response to unknown request {}", response.id));
            return;
        };
        let result = response.response_result.map_err(|error| {
            let lsp_server::ResponseError { code, message, data } = error;
            Error::Response(ResponseError { code, message, data })
        });
        let _ = sender.send(result);
    }

    fn receive_notification(&self, notification: lsp_server::Notification) {
        let lsp_server::Notification { method, params } = notification;
        match method.as_str() {
            "textDocument/publishDiagnostics" | "window/showMessage" | "$/progress" => {
                let _ = self.notifications.send(json!({"method": method, "params": params}));
            }
            _ if method.starts_with("$/") => {}
            _ => self.unexpected(format!("notification {method}")),
        }
    }

    fn answer(&self, request: lsp_server::Request) {
        let lsp_server::Request { id, method, params } = request;
        match method.as_str() {
            "window/workDoneProgress/create" => {
                let parameters =
                    serde_json::from_value::<WorkDoneProgressCreateParams>(params).unwrap();
                self.client.progress_tokens.lock().unwrap().push(parameters.token);
                self.respond(id, Ok(Value::Null));
            }
            _ => {
                self.unexpected(format!("request {method}"));
                let message = format!("unexpected server request {method}");
                self.respond(id, Err((ErrorCode::MethodNotFound, message)));
            }
        }
    }

    fn respond(&self, id: RequestId, result: Result<Value, (ErrorCode, String)>) {
        let response = match result {
            Ok(result) => lsp_server::Response::new_ok(id, result),
            Err((code, message)) => lsp_server::Response::new_err(id, code as i32, message),
        };
        let _ = self.outgoing.send(Outgoing::Message(Message::Response(response)));
    }

    fn unexpected(&self, description: String) {
        self.client.unexpected_messages.lock().unwrap().push(description);
    }
}

/// Collects a child's standard error so that it can neither block the child nor be lost.
fn drain(mut stderr: ChildStderr) -> Arc<Mutex<Vec<u8>>> {
    let output = Arc::new(Mutex::new(Vec::new()));
    let collected = Arc::clone(&output);
    thread::spawn(move || {
        let mut buffer = [0; 4096];
        while let Ok(read) = stderr.read(&mut buffer) {
            if read == 0 {
                break;
            }
            collected.lock().unwrap().extend_from_slice(&buffer[..read]);
        }
    });
    output
}
