use std::io::{Read, Write};
use std::net::TcpListener;
use std::ops::ControlFlow;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use async_lsp::router::Router;
use async_lsp::{
    Error, ErrorCode, LanguageServer as LanguageServerClient, ResponseError, ServerSocket,
};
use lsp_types::notification::{PublishDiagnostics, ShowMessage};
use lsp_types::request::{RegisterCapability, WorkspaceConfiguration, WorkspaceSymbolRequest};
use lsp_types::{
    ClientCapabilities, InitializeParams, InitializedParams, Registration, WorkspaceFolder,
    WorkspaceSymbolParams,
};
use serde_json::{Value, json};
use tokio::runtime::{Builder, Runtime};
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use url::Url;

#[path = "support.rs"]
mod support;

use support::TestWorkspace;

struct ClientState {
    configuration: Mutex<Option<Value>>,
    configuration_requests: AtomicUsize,
    registrations: Mutex<Vec<Registration>>,
    unexpected_requests: Mutex<Vec<String>>,
    notifications: mpsc::Sender<Value>,
}

struct LanguageServer {
    child: tokio::process::Child,
    runtime: Runtime,
    server: ServerSocket,
    mainloop: Option<JoinHandle<async_lsp::Result<()>>>,
    messages: Receiver<Value>,
    notifications: Vec<Value>,
    client: Arc<ClientState>,
}

impl LanguageServer {
    fn start(
        workspace: &TestWorkspace,
        directory: &str,
        arguments: &[&str],
        root: &Path,
    ) -> LanguageServer {
        LanguageServer::start_with_capabilities(
            workspace,
            directory,
            arguments,
            root,
            json!({}),
            None,
        )
    }

    fn start_with_capabilities(
        workspace: &TestWorkspace,
        directory: &str,
        arguments: &[&str],
        root: &Path,
        capabilities: Value,
        configuration: Option<Value>,
    ) -> LanguageServer {
        let mut arguments = arguments.to_vec();
        arguments.extend(["--lsp-log", "off"]);
        let (notifications, messages) = mpsc::channel();
        let client = Arc::new(ClientState {
            configuration: Mutex::new(configuration),
            configuration_requests: AtomicUsize::new(0),
            registrations: Mutex::new(vec![]),
            unexpected_requests: Mutex::new(vec![]),
            notifications,
        });
        let client_state = Arc::clone(&client);
        let (mainloop, server) = async_lsp::MainLoop::new_client(move |_| {
            let mut router = Router::new(client_state);
            router
                .request::<WorkspaceConfiguration, _>(|state, parameters| {
                    assert_eq!(parameters.items.len(), 1);
                    assert_eq!(parameters.items[0].section.as_deref(), Some("iris.server"));
                    assert!(parameters.items[0].scope_uri.is_some());
                    state.configuration_requests.fetch_add(1, Ordering::Relaxed);
                    let configuration = state.configuration.lock().unwrap().clone();
                    async move { Ok(vec![configuration.unwrap_or(Value::Null)]) }
                })
                .request::<RegisterCapability, _>(|state, parameters| {
                    state.registrations.lock().unwrap().extend(parameters.registrations);
                    async { Ok(()) }
                })
                .notification::<PublishDiagnostics>(|state, parameters| {
                    state
                        .notifications
                        .send(json!({
                            "method": "textDocument/publishDiagnostics",
                            "params": parameters
                        }))
                        .unwrap();
                    ControlFlow::Continue(())
                })
                .notification::<ShowMessage>(|state, parameters| {
                    state
                        .notifications
                        .send(json!({"method": "window/showMessage", "params": parameters}))
                        .unwrap();
                    ControlFlow::Continue(())
                })
                .unhandled_request(|state, request| {
                    let method = request.method;
                    state.unexpected_requests.lock().unwrap().push(method.clone());
                    async move {
                        Err(ResponseError::new(
                            ErrorCode::METHOD_NOT_FOUND,
                            format_args!("unexpected server request {method}"),
                        ))
                    }
                });
            router
        });

        let runtime = Builder::new_multi_thread().worker_threads(1).enable_all().build().unwrap();
        let command = workspace.command_builder(directory, &arguments);
        let mut command = tokio::process::Command::from(command);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = runtime.block_on(async { command.spawn().unwrap() });
        let stdout = child.stdout.take().unwrap().compat();
        let stdin = child.stdin.take().unwrap().compat_write();
        let mainloop = runtime.spawn(mainloop.run_buffered(stdout, stdin));
        let mut server = LanguageServer {
            child,
            runtime,
            server,
            mainloop: Some(mainloop),
            messages,
            notifications: vec![],
            client,
        };
        let capabilities = serde_json::from_value::<ClientCapabilities>(capabilities).unwrap();
        let result = server.runtime.block_on(async {
            timeout(
                Duration::from_secs(10),
                server.server.initialize(InitializeParams {
                    capabilities,
                    workspace_folders: Some(vec![WorkspaceFolder {
                        uri: Url::from_directory_path(root).unwrap(),
                        name: "project".to_owned(),
                    }]),
                    ..InitializeParams::default()
                }),
            )
            .await
            .expect("invariant violated: timed out waiting for initialize response")
            .unwrap()
        });
        assert_ne!(result.capabilities, Default::default());
        server.server.initialized(InitializedParams {}).unwrap();
        server
    }

    fn notify(&mut self, method: &str, parameters: Value) {
        match method {
            "workspace/didChangeConfiguration" => self
                .server
                .did_change_configuration(serde_json::from_value(parameters).unwrap())
                .unwrap(),
            "textDocument/didOpen" => {
                self.server.did_open(serde_json::from_value(parameters).unwrap()).unwrap()
            }
            "textDocument/didChange" => {
                self.server.did_change(serde_json::from_value(parameters).unwrap()).unwrap()
            }
            "textDocument/didSave" => {
                self.server.did_save(serde_json::from_value(parameters).unwrap()).unwrap()
            }
            "textDocument/didClose" => {
                self.server.did_close(serde_json::from_value(parameters).unwrap()).unwrap()
            }
            _ => panic!("unsupported test notification {method}"),
        }
    }

    #[track_caller]
    fn request(&mut self, method: &str, parameters: Value) -> Value {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match self.request_once(method, parameters.clone()) {
                Ok(result) => return result,
                Err(Error::Response(response)) if response.code == ErrorCode::REQUEST_CANCELLED => {
                }
                Err(error) => panic!("{method} request failed: {error}"),
            }
            assert!(Instant::now() < deadline, "request {method} was repeatedly cancelled");
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[track_caller]
    fn request_once(&mut self, method: &str, parameters: Value) -> Result<Value, Error> {
        assert_eq!(method, "workspace/symbol");
        let parameters: WorkspaceSymbolParams = serde_json::from_value(parameters).unwrap();
        let result = self.runtime.block_on(async {
            timeout(
                Duration::from_secs(10),
                self.server.request::<WorkspaceSymbolRequest>(parameters),
            )
            .await
            .unwrap_or_else(|_| panic!("timed out waiting for response to {method} request"))
        });
        result.map(|result| serde_json::to_value(result).unwrap())
    }

    fn set_configuration(&mut self, configuration: Value) {
        *self.client.configuration.lock().unwrap() = Some(configuration);
        self.notify("workspace/didChangeConfiguration", json!({"settings": null}));
    }

    fn wait_for_symbol(&mut self, name: &str, present: bool) {
        for _ in 0..20 {
            let symbols = self.request("workspace/symbol", json!({"query": name}));
            let found = symbols.as_array().unwrap().iter().any(|symbol| symbol["name"] == name);
            if found == present {
                return;
            }
            thread::sleep(Duration::from_millis(50));
        }
        panic!("symbol {name:?} presence did not become {present}");
    }

    #[track_caller]
    fn wait_for_notification(&mut self, method: &str) -> Value {
        self.wait_for_notification_matching(method, |_| true)
    }

    #[track_caller]
    fn wait_for_notification_matching(
        &mut self,
        method: &str,
        predicate: impl Fn(&Value) -> bool,
    ) -> Value {
        let deadline = Instant::now() + Duration::from_secs(10);
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

    fn assert_diagnostics(&mut self, uri: &Url, version: i32, enabled: bool) {
        // A response proves the preceding document notification reached the server before
        // the bounded wait for asynchronous diagnostics, including when none are expected.
        self.request("workspace/symbol", json!({"query": "noSuchSymbol"}));
        if enabled {
            let message = self.wait_for_notification_matching(
                "textDocument/publishDiagnostics",
                |notification| notification["params"]["uri"] == uri.as_str(),
            );
            assert_eq!(message["params"]["uri"], uri.as_str());
            assert_eq!(message["params"]["version"], version);
            let diagnostics = message["params"]["diagnostics"].as_array().unwrap();
            assert!(diagnostics.iter().any(|diagnostic| diagnostic["severity"] == 1), "{message}");
        } else {
            assert!(!self.notifications.iter().any(|notification| {
                notification["method"] == "textDocument/publishDiagnostics"
                    && notification["params"]["uri"] == uri.as_str()
            }));
            let deadline = Instant::now() + Duration::from_millis(500);
            loop {
                let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                    break;
                };
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

    fn shutdown(&mut self) {
        self.runtime.block_on(async {
            timeout(Duration::from_secs(10), self.server.shutdown(()))
                .await
                .expect("invariant violated: timed out waiting for shutdown response")
                .unwrap();
        });
        self.server.exit(()).unwrap();
        let mainloop = self.mainloop.take().unwrap();
        let mainloop = self
            .runtime
            .block_on(async { timeout(Duration::from_secs(10), mainloop).await })
            .expect("invariant violated: timed out stopping language client")
            .unwrap();
        match mainloop {
            Ok(()) | Err(Error::Eof) => {}
            Err(error) => panic!("language client failed while stopping: {error}"),
        }
        let status = self
            .runtime
            .block_on(async { timeout(Duration::from_secs(10), self.child.wait()).await })
            .expect("invariant violated: timed out stopping language server")
            .unwrap();
        let unexpected_requests = self.client.unexpected_requests.lock().unwrap();
        assert!(
            unexpected_requests.is_empty(),
            "unexpected server requests: {unexpected_requests:?}"
        );
        assert!(status.success(), "language server exited with {status}");
    }
}

impl Drop for LanguageServer {
    fn drop(&mut self) {
        if let Some(mainloop) = self.mainloop.take() {
            mainloop.abort();
        }
        self.runtime.block_on(async {
            let _ = self.child.kill().await;
            let _ = self.child.wait().await;
        });
    }
}

fn assert_diagnostic_triggers(
    server: &mut LanguageServer,
    root: &Path,
    on_open: bool,
    on_save: bool,
    on_change: bool,
) {
    assert_diagnostic_triggers_for(server, root, "Main.purs", on_open, on_save, on_change);
}

fn assert_diagnostic_triggers_for(
    server: &mut LanguageServer,
    root: &Path,
    file: &str,
    on_open: bool,
    on_save: bool,
    on_change: bool,
) {
    let uri = Url::from_file_path(root.join(file)).unwrap();
    let module = file.strip_suffix(".purs").unwrap();
    let text = format!("module {module} where\nvalue :: Int\nvalue = \"invalid\"\n");
    server.notify(
        "textDocument/didOpen",
        json!({
            "textDocument": {"uri": uri, "languageId": "purescript", "version": 1, "text": text}
        }),
    );
    server.assert_diagnostics(&uri, 1, on_open);
    server.notify(
        "textDocument/didChange",
        json!({
            "textDocument": {"uri": uri, "version": 2},
            "contentChanges": [{"text": format!("{text}\n")}]
        }),
    );
    server.assert_diagnostics(&uri, 2, on_change);
    server.notify("textDocument/didSave", json!({"textDocument": {"uri": uri}}));
    server.assert_diagnostics(&uri, 2, on_save);
}

#[test]
fn empty_configuration_preserves_spago_and_default_diagnostics() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "spago.lock",
        r#"{"workspace":{"packages":{"application":{"path":"."}}},"packages":{}}"#,
    );
    workspace.write("src/Library.purs", "module Library where\nfromSpago = 42\n");
    workspace.write("config/empty.json", "{}");

    let cases: &[&[&str]] = &[
        &["lsp"],
        &["lsp", "--stdio"],
        &["lsp", "--config", "null"],
        &["lsp", "--config-file", "config/empty.json"],
        &[
            "lsp",
            "--config",
            r#"{"sources":{"kind":"spago"},"diagnostics":{"onOpen":null,"onSave":null,"onChange":null}}"#,
        ],
    ];
    for arguments in cases {
        let mut server = LanguageServer::start(&workspace, "", arguments, workspace.path());
        let symbols = server.request("workspace/symbol", json!({"query": "fromSpago"}));
        assert_eq!(symbols.as_array().unwrap().len(), 1, "{symbols}");
        assert_eq!(symbols[0]["name"], "fromSpago");
        assert_diagnostic_triggers(&mut server, workspace.path(), true, true, false);
        server.shutdown();
    }
}

#[test]
fn json_inputs_configure_source_commands_and_diagnostic_triggers() {
    let workspace = TestWorkspace::empty();
    workspace.write("project/selected/Library.purs", "module Library where\nfromCommand = 42\n");
    workspace.write(
        "launcher/source command.mjs",
        r#"
import { writeFileSync } from "node:fs";
writeFileSync("arguments.json", JSON.stringify(process.argv.slice(2)));
console.log("selected/*.purs");
"#,
    );
    let configuration = json!({
        "sources": {
            "kind": "command",
            "program": "node",
            "arguments": ["source command.mjs", "", "path with spaces", "--flag", "λ", "$(not-a-shell)"]
        },
        "diagnostics": {"onOpen": false, "onSave": false, "onChange": true}
    }).to_string();
    let absolute_path = workspace.path().join("settings/server config.json");
    let root = workspace.path().join("project");
    let cases: &[&[&str]] = &[
        &["lsp", "--config", &configuration],
        &["lsp", "--config-file", "../settings/server config.json"],
        &["lsp", "--config-file", absolute_path.to_str().unwrap()],
    ];
    for arguments in cases {
        workspace.write("settings/server config.json", &configuration);
        let mut server = LanguageServer::start(&workspace, "launcher", arguments, &root);
        workspace.write("settings/server config.json", "invalid after startup");
        let symbols = server.request("workspace/symbol", json!({"query": "fromCommand"}));
        assert_eq!(symbols.as_array().unwrap().len(), 1, "{symbols}");
        assert_eq!(symbols[0]["name"], "fromCommand");
        let expected_uri = Url::from_file_path(root.join("selected/Library.purs")).unwrap();
        assert_eq!(symbols[0]["location"]["uri"], expected_uri.as_str());
        let arguments: Value =
            serde_json::from_str(&workspace.read("launcher/arguments.json")).unwrap();
        assert_eq!(arguments, json!(["", "path with spaces", "--flag", "λ", "$(not-a-shell)"]));
        assert_diagnostic_triggers(&mut server, &root, false, false, true);
        server.shutdown();
    }
}

#[test]
fn partial_diagnostic_configuration_preserves_omitted_triggers() {
    let workspace = TestWorkspace::empty();
    workspace.write("spago.lock", r#"{"workspace":{"packages":{}},"packages":{}}"#);
    let mut server = LanguageServer::start(
        &workspace,
        "",
        &["lsp", "--config", r#"{"diagnostics":{"onOpen":false}}"#],
        workspace.path(),
    );
    assert_diagnostic_triggers(&mut server, workspace.path(), false, true, false);
    server.shutdown();
}

#[test]
fn workspace_configuration_applies_initial_and_runtime_snapshots() {
    let workspace = TestWorkspace::empty();
    workspace.write("project/startup/Library.purs", "module Library where\nfromStartup = 1\n");
    workspace.write("project/runtime/Library.purs", "module Library where\nfromRuntime = 2\n");
    workspace.write("launcher/startup.mjs", "console.log('../project/startup/*.purs');\n");
    workspace.write("launcher/runtime.mjs", "console.log('../project/runtime/*.purs');\n");
    let startup = r#"{"sources":{"kind":"command","program":"node","arguments":["startup.mjs"]}}"#;
    let runtime = json!({
        "sources": {"kind": "command", "program": "node", "arguments": ["runtime.mjs"]},
        "diagnostics": {"onOpen": false, "onSave": false, "onChange": true}
    });
    let root = workspace.path().join("project");
    let mut server = LanguageServer::start_with_capabilities(
        &workspace,
        "launcher",
        &["lsp", "--config", startup],
        &root,
        json!({"workspace": {"configuration": true}}),
        Some(runtime),
    );

    server.wait_for_symbol("fromRuntime", true);
    server.wait_for_symbol("fromStartup", false);
    assert_diagnostic_triggers(&mut server, &root, false, false, true);
    let runtime_uri = Url::from_file_path(root.join("runtime/Library.purs")).unwrap();
    server.notify(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": runtime_uri,
                "languageId": "purescript",
                "version": 1,
                "text": "module Library where\nunsavedRuntime = 3\n"
            }
        }),
    );
    server.wait_for_symbol("unsavedRuntime", true);

    server.set_configuration(json!({"diagnostics": {"onOpen": true}}));
    server.wait_for_symbol("fromStartup", true);
    server.wait_for_symbol("fromRuntime", false);
    server.wait_for_symbol("unsavedRuntime", true);
    server.notify("textDocument/didClose", json!({"textDocument": {"uri": runtime_uri}}));
    server.wait_for_symbol("unsavedRuntime", false);
    assert_diagnostic_triggers_for(&mut server, &root, "AfterUpdate.purs", true, true, false);
    assert!(server.client.configuration_requests.load(Ordering::Relaxed) >= 2);
    server.shutdown();
}

#[test]
fn invalid_runtime_configuration_preserves_the_previous_workspace() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "spago.lock",
        r#"{"workspace":{"packages":{"application":{"path":"."}}},"packages":{}}"#,
    );
    workspace.write("src/Library.purs", "module Library where\nstillLoaded = 42\n");
    workspace.write(
        "slow failure.mjs",
        r#"
setTimeout(() => {
  process.stderr.write("slow failure\n");
  process.exit(1);
}, 1000);
"#,
    );
    let mut server = LanguageServer::start_with_capabilities(
        &workspace,
        "",
        &["lsp"],
        workspace.path(),
        json!({
            "workspace": {
                "configuration": true,
                "didChangeConfiguration": {"dynamicRegistration": true}
            }
        }),
        Some(json!({})),
    );
    server.wait_for_symbol("stillLoaded", true);

    server.set_configuration(json!({"diagnostics": {"onOpen": "invalid"}}));
    let message = server.wait_for_notification("window/showMessage");
    assert_eq!(message["params"]["type"], 1);
    assert!(
        message["params"]["message"]
            .as_str()
            .unwrap()
            .contains("previous Iris settings remain active")
    );
    server.wait_for_symbol("stillLoaded", true);

    server.set_configuration(json!({
        "sources": {
            "kind": "command",
            "program": "node",
            "arguments": ["slow failure.mjs"]
        }
    }));
    let message = server.wait_for_notification("window/showMessage");
    assert!(
        message["params"]["message"].as_str().unwrap().contains("Failed to apply Iris settings")
    );
    server.wait_for_symbol("stillLoaded", true);
    assert!(
        server
            .client
            .registrations
            .lock()
            .unwrap()
            .iter()
            .any(|registration| registration.method == "workspace/didChangeConfiguration")
    );
    server.shutdown();
}

#[test]
fn clients_without_workspace_configuration_keep_startup_settings() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "spago.lock",
        r#"{"workspace":{"packages":{"application":{"path":"."}}},"packages":{}}"#,
    );
    workspace.write("src/Library.purs", "module Library where\nstartupOnly = 42\n");
    let mut server = LanguageServer::start(&workspace, "", &["lsp"], workspace.path());
    server.notify(
        "workspace/didChangeConfiguration",
        json!({"settings": {"sources": {"kind": "command", "program": "missing"}}}),
    );
    let symbols = server.request("workspace/symbol", json!({"query": "startupOnly"}));
    assert_eq!(symbols.as_array().unwrap().len(), 1, "{symbols}");
    assert_eq!(symbols[0]["name"], "startupOnly");
    assert_eq!(server.client.configuration_requests.load(Ordering::Relaxed), 0);
    server.shutdown();
}

fn blocking_configuration(workspace: &TestWorkspace, sources: &str) -> (TcpListener, Value) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    workspace.write(
        "blocking-source-command.mjs",
        r#"
import net from "node:net";
const socket = net.connect(Number(process.argv[2]), "127.0.0.1", () => socket.write("started"));
socket.once("data", () => {
  console.log(process.argv[3]);
  socket.end();
});
"#,
    );
    let configuration = json!({
        "sources": {
            "kind": "command",
            "program": "node",
            "arguments": ["blocking-source-command.mjs", port.to_string(), sources]
        }
    });
    (listener, configuration)
}

#[test]
fn blocked_workspace_preparation_does_not_block_protocol_requests() {
    let workspace = TestWorkspace::empty();
    workspace.write("src/Library.purs", "module Library where\nprepared = 42\n");
    let (listener, configuration) = blocking_configuration(&workspace, "src/*.purs");
    let configuration = configuration.to_string();
    let mut server = LanguageServer::start(
        &workspace,
        "",
        &["lsp", "--config", &configuration],
        workspace.path(),
    );
    let (connected, connection) = mpsc::channel();
    thread::spawn(move || {
        let (mut source_command, _) = listener.accept().unwrap();
        let mut started = [0; 7];
        source_command.read_exact(&mut started).unwrap();
        assert_eq!(&started, b"started");
        connected.send(source_command).unwrap();
    });
    let mut source_command =
        connection.recv_timeout(Duration::from_secs(10)).expect("source command did not connect");

    let response = server.request_once("workspace/symbol", json!({"query": "prepared"}));
    assert!(matches!(
        response,
        Err(Error::Response(response)) if response.code == ErrorCode::REQUEST_CANCELLED
    ));

    source_command.write_all(b"continue").unwrap();
    drop(source_command);
    server.wait_for_symbol("prepared", true);
    server.shutdown();
}

#[test]
fn shutdown_cancels_a_blocked_source_command() {
    let workspace = TestWorkspace::empty();
    let (listener, configuration) = blocking_configuration(&workspace, "src/*.purs");
    let configuration = configuration.to_string();
    let mut server = LanguageServer::start(
        &workspace,
        "",
        &["lsp", "--config", &configuration],
        workspace.path(),
    );
    let (connected, connection) = mpsc::channel();
    thread::spawn(move || {
        let (mut source_command, _) = listener.accept().unwrap();
        let mut started = [0; 7];
        source_command.read_exact(&mut started).unwrap();
        connected.send(source_command).unwrap();
    });
    let mut source_command =
        connection.recv_timeout(Duration::from_secs(10)).expect("source command did not connect");

    server.shutdown();

    source_command.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
    let mut remaining = [0; 1];
    assert_eq!(source_command.read(&mut remaining).unwrap(), 0);
}

#[test]
fn newer_configuration_cancels_blocked_preparation() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "spago.lock",
        r#"{"workspace":{"packages":{"application":{"path":"."}}},"packages":{}}"#,
    );
    workspace.write("src/Library.purs", "module Library where\nnewer = 42\n");
    let (listener, blocked) = blocking_configuration(&workspace, "src/*.purs");
    let mut server = LanguageServer::start_with_capabilities(
        &workspace,
        "",
        &["lsp"],
        workspace.path(),
        json!({"workspace": {"configuration": true}}),
        Some(blocked),
    );
    let (connected, connection) = mpsc::channel();
    thread::spawn(move || {
        let (mut source_command, _) = listener.accept().unwrap();
        let mut started = [0; 7];
        source_command.read_exact(&mut started).unwrap();
        connected.send(source_command).unwrap();
    });
    let mut source_command =
        connection.recv_timeout(Duration::from_secs(10)).expect("source command did not connect");

    server.set_configuration(json!({"sources": {"kind": "spago"}}));
    server.wait_for_symbol("newer", true);

    source_command.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
    let mut remaining = [0; 1];
    assert_eq!(source_command.read(&mut remaining).unwrap(), 0);
    server.shutdown();
}

#[test]
fn notifications_during_reconfiguration_are_reconciled_before_commit() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "spago.lock",
        r#"{"workspace":{"packages":{"application":{"path":"."}}},"packages":{}}"#,
    );
    workspace.write("src/Library.purs", "module Library where\nfromDisk = 42\n");
    workspace.write("replacement/Library.purs", "module Replacement where\nreplacement = 42\n");
    let (listener, blocked) = blocking_configuration(&workspace, "replacement/*.purs");
    let mut server = LanguageServer::start_with_capabilities(
        &workspace,
        "",
        &["lsp"],
        workspace.path(),
        json!({"workspace": {"configuration": true}}),
        Some(json!({})),
    );
    server.wait_for_symbol("fromDisk", true);
    let source_uri = Url::from_file_path(workspace.path().join("src/Library.purs")).unwrap();
    server.notify(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": source_uri,
                "languageId": "purescript",
                "version": 1,
                "text": "module Library where\nfromEditor = 42\n"
            }
        }),
    );
    server.wait_for_symbol("fromEditor", true);
    let (connected, connection) = mpsc::channel();
    thread::spawn(move || {
        for _ in 0..2 {
            let (mut source_command, _) = listener.accept().unwrap();
            let mut started = [0; 7];
            source_command.read_exact(&mut started).unwrap();
            connected.send(source_command).unwrap();
        }
    });

    server.set_configuration(blocked);
    let mut first =
        connection.recv_timeout(Duration::from_secs(10)).expect("source command did not connect");
    server.notify("textDocument/didClose", json!({"textDocument": {"uri": source_uri}}));
    server.wait_for_symbol("fromEditor", false);
    first.write_all(b"continue").unwrap();
    drop(first);

    let mut second = connection
        .recv_timeout(Duration::from_secs(10))
        .expect("source command was not restarted after the workspace changed");
    second.write_all(b"continue").unwrap();
    drop(second);
    server.wait_for_symbol("replacement", true);
    server.wait_for_symbol("fromDisk", false);
    server.shutdown();
}
