//! Tests that run the protocol actor and the workspace actor together over an in-memory
//! connection.

use std::collections::VecDeque;
use std::time::Duration;

use iris_lsp_workspace::{WorkspaceConfig, WorkspaceService};
use lsp_server::{Connection, ErrorCode, Message, Notification, Request, RequestId, Response};
use serde_json::{Value, json};
use tokio::task::JoinHandle;
use url::Url;

use crate::connect;

const PATIENCE: Duration = Duration::from_secs(10);

struct Session {
    editor: Connection,
    unclaimed: VecDeque<Message>,
    server: JoinHandle<Result<(), iris_lsp_server::ServerError>>,
    root: tempfile::TempDir,
}

impl Session {
    async fn start() -> Session {
        Session::start_with(json!({})).await
    }

    async fn start_with(capabilities: Value) -> Session {
        let (editor, server) = Connection::memory();
        let config = WorkspaceConfig {
            name: "iris".to_string(),
            version: "test".to_string(),
            analysis_permits: 2,
            diagnostic_permits: 2,
        };
        let server = tokio::spawn(connect(iris_lsp_server::Transport::memory(server), |events| {
            WorkspaceService::with_builtin_workspace(config, events)
        }));
        let root = tempfile::tempdir().unwrap();
        let mut session = Session { editor, unclaimed: VecDeque::new(), server, root };
        let root_uri = Url::from_directory_path(session.root.path()).unwrap();
        session.request(
            0,
            "initialize",
            json!({
                "capabilities": capabilities,
                "workspaceFolders": [{"uri": root_uri, "name": "workspace"}]
            }),
        );
        let result = session.result(0).await;
        assert_eq!(result["serverInfo"], json!({"name": "iris", "version": "test"}));
        session.notify("initialized", json!({}));
        session
    }

    fn uri(&self, name: &str) -> Url {
        Url::from_file_path(self.root.path().join(name)).unwrap()
    }

    fn reply(&self, request: &Request, result: Value) {
        let response = Response::new_ok(RequestId::clone(&request.id), result);
        self.editor.sender.send(Message::Response(response)).unwrap();
    }

    async fn server_request(&mut self, method: &str) -> Request {
        self.claim(method, |message| match message {
            Message::Request(request) if request.method == method => Some(Request::clone(request)),
            _ => None,
        })
        .await
    }

    fn open(&self, uri: &Url, version: i32, text: &str) {
        self.notify(
            "textDocument/didOpen",
            json!({"textDocument": {
                "uri": uri, "languageId": "purescript", "version": version, "text": text
            }}),
        );
    }

    fn change(&self, uri: &Url, version: i32, text: &str) {
        self.notify(
            "textDocument/didChange",
            json!({"textDocument": {"uri": uri, "version": version}, "contentChanges": [{"text": text}]}),
        );
    }

    fn save(&self, uri: &Url) {
        self.notify("textDocument/didSave", json!({"textDocument": {"uri": uri}}));
    }

    /// Answers the next `workspace/configuration` request with one settings item.
    async fn answer_configuration(&mut self, settings: Value) {
        let request = self.server_request("workspace/configuration").await;
        assert_eq!(request.params["items"][0]["section"], "iris.server");
        self.reply(&request, json!([settings]));
    }

    /// Waits for diagnostics for `uri` and returns their version.
    async fn diagnostics(&mut self, uri: &Url) -> Value {
        let params = self
            .claim("diagnostics", |message| match message {
                Message::Notification(notification)
                    if notification.method == "textDocument/publishDiagnostics"
                        && notification.params["uri"] == uri.as_str() =>
                {
                    Some(Value::clone(&notification.params))
                }
                _ => None,
            })
            .await;
        assert_eq!(params["diagnostics"][0]["code"], "CannotUnify");
        Value::clone(&params["version"])
    }

    /// Asserts that no diagnostics for `uri` arrive after every earlier notification was handled.
    async fn assert_no_diagnostics(&mut self, id: i32, uri: &Url) {
        // The answer proves that the preceding notifications reached the workspace actor.
        self.request(id, "workspace/symbol", json!({"query": ""}));
        self.result(id).await;
        let receiver = self.editor.receiver.clone();
        let messages = tokio::task::spawn_blocking(move || {
            let mut messages = Vec::new();
            while let Ok(message) = receiver.recv_timeout(Duration::from_millis(300)) {
                messages.push(message);
            }
            messages
        })
        .await
        .unwrap();
        self.unclaimed.extend(messages);
        let published = self.unclaimed.iter().any(|message| {
            matches!(message, Message::Notification(notification)
                if notification.method == "textDocument/publishDiagnostics"
                    && notification.params["uri"] == uri.as_str())
        });
        assert!(!published, "unexpected diagnostics for {uri}");
    }

    async fn document_symbol_names(&mut self, id: i32, uri: &Url) -> Vec<String> {
        self.document_symbols(id, uri);
        let symbols = self.result(id).await;
        symbol_names(&symbols).into_iter().map(str::to_string).collect()
    }

    fn request(&self, id: i32, method: &str, params: Value) {
        let request = Request { id: RequestId::from(id), method: method.to_string(), params };
        self.editor.sender.send(Message::Request(request)).unwrap();
    }

    fn notify(&self, method: &str, params: Value) {
        let notification = Notification { method: method.to_string(), params };
        self.editor.sender.send(Message::Notification(notification)).unwrap();
    }

    fn document_symbols(&self, id: i32, uri: &Url) {
        self.request(id, "textDocument/documentSymbol", json!({"textDocument": {"uri": uri}}));
    }

    async fn claim<T>(&mut self, description: &str, claim: impl Fn(&Message) -> Option<T>) -> T {
        if let Some(index) = self.unclaimed.iter().position(|message| claim(message).is_some()) {
            let message = self.unclaimed.remove(index).unwrap();
            return claim(&message).unwrap();
        }
        loop {
            let receiver = self.editor.receiver.clone();
            let message = tokio::task::spawn_blocking(move || receiver.recv_timeout(PATIENCE).ok())
                .await
                .unwrap()
                .unwrap_or_else(|| panic!("timed out waiting for {description}"));
            if let Some(claimed) = claim(&message) {
                return claimed;
            }
            self.unclaimed.push_back(message);
        }
    }

    async fn response(&mut self, id: i32) -> Response {
        let id = RequestId::from(id);
        self.claim(&format!("the response to {id}"), |message| match message {
            Message::Response(response) if response.id == id => Some(Response::clone(response)),
            _ => None,
        })
        .await
    }

    async fn result(&mut self, id: i32) -> Value {
        self.response(id).await.response_result.expect("the request failed")
    }

    async fn notification(&mut self, method: &str) -> Value {
        self.claim(method, |message| match message {
            Message::Notification(notification) if notification.method == method => {
                Some(Value::clone(&notification.params))
            }
            _ => None,
        })
        .await
    }

    async fn stopped(self) -> Result<(), iris_lsp_server::ServerError> {
        tokio::time::timeout(PATIENCE, self.server)
            .await
            .expect("timed out waiting for the server to stop")
            .unwrap()
    }
}

fn symbol_names(symbols: &Value) -> Vec<&str> {
    let symbols = symbols.as_array().expect("expected a list of symbols");
    symbols.iter().map(|symbol| symbol["name"].as_str().unwrap()).collect()
}

#[tokio::test]
async fn requests_see_the_edits_sent_before_them() {
    let mut session = Session::start().await;
    let uri = session.uri("Main.purs");
    session.notify(
        "textDocument/didOpen",
        json!({"textDocument": {
            "uri": uri, "languageId": "purescript", "version": 1,
            "text": "module Main where\nopened = 1\n"
        }}),
    );
    session.document_symbols(1, &uri);
    session.notify(
        "textDocument/didChange",
        json!({
            "textDocument": {"uri": uri, "version": 2},
            "contentChanges": [{"text": "module Main where\nvalue :: Int\nvalue = \"changed\"\n"}]
        }),
    );
    session.document_symbols(2, &uri);

    match session.response(1).await.response_result {
        Ok(symbols) => assert_eq!(symbol_names(&symbols), ["opened"]),
        Err(error) => assert_eq!(
            (error.code, error.message.as_str()),
            (ErrorCode::ContentModified as i32, "Content modified")
        ),
    }
    assert_eq!(symbol_names(&session.result(2).await), ["value"]);

    // Diagnostics on save are enabled by default.
    session.notify("textDocument/didSave", json!({"textDocument": {"uri": uri}}));
    let diagnostics = loop {
        let diagnostics = session.notification("textDocument/publishDiagnostics").await;
        if diagnostics["version"] == 2 {
            break diagnostics;
        }
    };
    assert_eq!(diagnostics["uri"], uri.as_str());
    assert_eq!(diagnostics["diagnostics"][0]["code"], "CannotUnify");

    session.request(3, "shutdown", Value::Null);
    session.result(3).await;
    session.notify("exit", Value::Null);
    session.stopped().await.unwrap();
}

#[tokio::test]
async fn shutdown_and_exit_stop_both_actors() {
    let mut session = Session::start().await;
    session.request(1, "workspace/symbol", json!({"query": "Prim"}));
    session.result(1).await;
    session.request(2, "shutdown", Value::Null);
    assert_eq!(session.result(2).await, Value::Null);
    session.request(3, "workspace/symbol", json!({"query": ""}));
    assert_eq!(
        session.response(3).await.response_result.unwrap_err().code,
        ErrorCode::InvalidRequest as i32
    );
    session.notify("exit", Value::Null);
    session.stopped().await.unwrap();
}

fn invalid_module(name: &str, text: &str) -> String {
    format!("module {name} where\nvalue :: Int\nvalue = \"{text}\"\n")
}

#[tokio::test]
async fn workspace_configuration_updates_diagnostic_triggers_without_replacing_documents() {
    let mut session = Session::start_with(json!({"workspace": {
        "configuration": true,
        "didChangeConfiguration": {"dynamicRegistration": true}
    }}))
    .await;
    let registration = session.server_request("client/registerCapability").await;
    assert_eq!(
        registration.params["registrations"][0]["method"],
        "workspace/didChangeConfiguration"
    );
    session.reply(&registration, Value::Null);
    session
        .answer_configuration(
            json!({"diagnostics": {"onOpen": false, "onSave": false, "onChange": true}}),
        )
        .await;

    // Only changes publish diagnostics.
    let main = session.uri("Main.purs");
    session.open(&main, 1, &invalid_module("Main", "one"));
    session.assert_no_diagnostics(1, &main).await;
    session.change(&main, 2, &invalid_module("Main", "two"));
    assert_eq!(session.diagnostics(&main).await, 2);
    session.save(&main);
    session.assert_no_diagnostics(2, &main).await;

    // Runtime settings replace the startup settings over the defaults: opens and saves publish
    // diagnostics again, and changes no longer do.
    session.notify("workspace/didChangeConfiguration", json!({"settings": null}));
    session.answer_configuration(json!({"diagnostics": {"onOpen": true}})).await;
    let other = session.uri("Other.purs");
    session.open(&other, 1, &invalid_module("Other", "one"));
    assert_eq!(session.diagnostics(&other).await, 1);
    session.change(&other, 2, &invalid_module("Other", "two"));
    session.assert_no_diagnostics(3, &other).await;
    session.save(&other);
    assert_eq!(session.diagnostics(&other).await, 2);
    assert_eq!(session.document_symbol_names(4, &main).await, ["value"]);

    // Invalid settings keep the previous ones.
    session.notify("workspace/didChangeConfiguration", json!({"settings": null}));
    session.answer_configuration(json!({"diagnostics": {"onOpen": "invalid"}})).await;
    assert_eq!(
        session.notification("window/showMessage").await,
        json!({
            "type": 1,
            "message": "Invalid Iris settings: invalid type: string \"invalid\", expected a boolean. \
                        The previous Iris settings remain active."
        })
    );
    session.notify("workspace/didChangeConfiguration", json!({"settings": null}));
    session
        .answer_configuration(json!({"sources": {
            "kind": "command", "program": "node", "arguments": ["slow failure.mjs"]
        }}))
        .await;
    assert_eq!(
        session.notification("window/showMessage").await,
        json!({
            "type": 1,
            "message": "Invalid Iris settings: unknown field `sources`, expected `diagnostics`. \
                        The previous Iris settings remain active."
        })
    );
    let third = session.uri("Third.purs");
    session.open(&third, 1, &invalid_module("Third", "one"));
    assert_eq!(session.diagnostics(&third).await, 1);
    assert_eq!(session.document_symbol_names(5, &main).await, ["value"]);

    session.request(6, "shutdown", Value::Null);
    session.result(6).await;
    session.notify("exit", Value::Null);
    session.stopped().await.unwrap();
}
