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
                "capabilities": {},
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
