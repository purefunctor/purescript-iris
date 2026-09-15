use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
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
use lsp_types::notification::{LogMessage, PublishDiagnostics, ShowMessage};
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
    configuration_acknowledgements: mpsc::Sender<()>,
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
    configuration_acknowledgements: Receiver<()>,
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
        let expects_configuration = configuration.is_some();
        let mut server = LanguageServer::start_loading_with_capabilities(
            workspace,
            directory,
            arguments,
            root,
            capabilities,
            configuration,
        );
        if expects_configuration {
            server
                .configuration_acknowledgements
                .recv_timeout(Duration::from_secs(10))
                .expect("timed out waiting for initial workspace configuration request");
        }
        server.wait_for_ready();
        server
    }

    fn start_loading_with_capabilities(
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
        let (configuration_acknowledgements, configuration_acknowledgement_messages) =
            mpsc::channel();
        let client = Arc::new(ClientState {
            configuration: Mutex::new(configuration),
            configuration_requests: AtomicUsize::new(0),
            configuration_acknowledgements,
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
                    state.configuration_acknowledgements.send(()).unwrap();
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
                .notification::<LogMessage>(|state, parameters| {
                    state
                        .notifications
                        .send(json!({"method": "window/logMessage", "params": parameters}))
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
            configuration_acknowledgements: configuration_acknowledgement_messages,
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
        self.request_once(method, parameters)
            .unwrap_or_else(|error| panic!("{method} request failed: {error}"))
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

    fn set_configuration_acknowledged(&mut self, configuration: Value) {
        while self.configuration_acknowledgements.try_recv().is_ok() {}
        self.set_configuration(configuration);
        self.configuration_acknowledgements
            .recv_timeout(Duration::from_secs(10))
            .expect("timed out waiting for workspace configuration request");
        self.wait_for_ready();
    }

    fn wait_for_ready(&mut self) {
        self.wait_for_notification_matching("window/logMessage", |notification| {
            notification["params"]["message"] == "Iris workspace is ready"
        });
    }

    fn wait_for_symbol(&mut self, name: &str, present: bool) {
        let symbols = self.request("workspace/symbol", json!({"query": name}));
        let found = symbols.as_array().unwrap().iter().any(|symbol| symbol["name"] == name);
        assert_eq!(found, present, "symbol {name:?}: {symbols}");
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

    fn disconnect(&mut self) {
        let mainloop = self.mainloop.take().unwrap();
        mainloop.abort();
        self.runtime.block_on(async {
            assert!(mainloop.await.unwrap_err().is_cancelled());
            timeout(Duration::from_secs(10), self.child.wait())
                .await
                .expect("server did not retire after transport EOF")
                .unwrap();
        });
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
process.stderr.write("x".repeat(1024 * 1024));
process.stdout.write("selected/*.purs\n".repeat(8192));
"#,
    );
    let launcher = workspace.path().join("launcher/source command.mjs");
    let configuration = json!({
        "sources": {
            "kind": "command",
            "program": "node",
            "arguments": [launcher, "", "path with spaces", "--flag", "λ", "$(not-a-shell)"]
        },
        "diagnostics": {"onOpen": false, "onSave": false, "onChange": true}
    })
    .to_string();
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
            serde_json::from_str(&workspace.read("project/arguments.json")).unwrap();
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
    workspace.write("project/startup.mjs", "console.log('startup/*.purs');\n");
    workspace.write("project/runtime.mjs", "console.log('runtime/*.purs');\n");
    workspace.write("launcher/.keep", "");
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

    server.set_configuration_acknowledged(json!({"diagnostics": {"onOpen": true}}));
    server.wait_for_symbol("fromStartup", true);
    server.wait_for_symbol("fromRuntime", false);
    server.wait_for_symbol("unsavedRuntime", true);
    server.notify("textDocument/didClose", json!({"textDocument": {"uri": runtime_uri}}));
    server.wait_for_symbol("unsavedRuntime", false);
    assert_diagnostic_triggers_for(&mut server, &root, "AfterUpdate.purs", true, true, false);
    assert!(server.client.configuration_requests.load(Ordering::Relaxed) >= 2);
    server.shutdown();
}

#[cfg(unix)]
#[test]
fn source_reconfiguration_reconciles_open_symlink_aliases() {
    use std::os::unix::fs::symlink;

    let workspace = TestWorkspace::empty();
    workspace.write("project/src/Library.purs", "module Library where\nfromDisk = 1\n");
    workspace.write(
        "launcher/discover.mjs",
        "import { realpathSync } from 'node:fs';\nconsole.log(realpathSync(new URL('../project/src', import.meta.url)) + '/*.purs');\n",
    );
    workspace.write("launcher/empty.mjs", "");
    symlink(workspace.path().join("project"), workspace.path().join("alias")).unwrap();
    let root = workspace.path().join("project");
    let discover = workspace.path().join("launcher/discover.mjs");
    let empty = workspace.path().join("launcher/empty.mjs");
    let startup = json!({
        "sources": {"kind": "command", "program": "node", "arguments": [PathBuf::clone(&discover)]}
    });
    let startup = startup.to_string();
    let mut server = LanguageServer::start_with_capabilities(
        &workspace,
        "launcher",
        &["lsp", "--config", &startup],
        &root,
        json!({"workspace": {"configuration": true}}),
        Some(json!({
            "sources": {"kind": "command", "program": "node", "arguments": [discover]}
        })),
    );
    server.wait_for_symbol("fromDisk", true);
    let alias_uri = Url::from_file_path(workspace.path().join("alias/src/Library.purs")).unwrap();
    server.notify(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": alias_uri,
                "languageId": "purescript",
                "version": 1,
                "text": "module AliasBuffer where\nfromAliasBuffer = 2\n"
            }
        }),
    );
    let symbols = server.request("workspace/symbol", json!({"query": "fromAliasBuffer"}));
    assert_eq!(symbols.as_array().unwrap().len(), 1, "{symbols}");
    assert_eq!(symbols[0]["location"]["uri"], alias_uri.as_str());

    server.set_configuration_acknowledged(json!({
        "sources": {"kind": "command", "program": "node", "arguments": [empty]}
    }));
    let symbols = server.request("workspace/symbol", json!({"query": "fromAliasBuffer"}));
    assert_eq!(symbols.as_array().unwrap().len(), 1, "{symbols}");
    assert_eq!(symbols[0]["location"]["uri"], alias_uri.as_str());

    server.notify("textDocument/didClose", json!({"textDocument": {"uri": alias_uri}}));
    let symbols = server.request("workspace/symbol", json!({"query": "fromAliasBuffer"}));
    assert!(symbols.as_array().unwrap().is_empty(), "{symbols}");
    let symbols = server.request("workspace/symbol", json!({"query": "AliasBuffer"}));
    assert!(symbols.as_array().unwrap().is_empty(), "{symbols}");
    server.wait_for_symbol("fromDisk", false);
    let clear =
        server.wait_for_notification_matching("textDocument/publishDiagnostics", |message| {
            message["params"]["uri"] == alias_uri.as_str() && message["params"]["version"].is_null()
        });
    assert_eq!(clear["params"]["diagnostics"], json!([]));

    std::fs::remove_file(workspace.path().join("alias")).unwrap();
    server.set_configuration_acknowledged(json!({
        "sources": {"kind": "command", "program": "node", "arguments": [PathBuf::clone(&empty)]}
    }));
    symlink(workspace.path().join("project"), workspace.path().join("alias")).unwrap();
    server.notify(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": alias_uri,
                "languageId": "purescript",
                "version": 2,
                "text": "module AliasBuffer where\ncanonicalizationFailed = 3\n"
            }
        }),
    );
    server.wait_for_symbol("canonicalizationFailed", true);
    server.notify("textDocument/didClose", json!({"textDocument": {"uri": alias_uri}}));
    server.wait_for_symbol("canonicalizationFailed", false);
    server.wait_for_symbol("fromDisk", false);

    server.set_configuration_acknowledged(json!({
        "sources": {"kind": "command", "program": "node", "arguments": [PathBuf::clone(&discover)]}
    }));
    server.wait_for_symbol("fromDisk", true);
    server.set_configuration_acknowledged(json!({
        "sources": {"kind": "command", "program": "node", "arguments": [PathBuf::clone(&empty)]}
    }));
    server.wait_for_symbol("fromDisk", false);
    server.notify(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": alias_uri,
                "languageId": "purescript",
                "version": 3,
                "text": "module AliasBuffer where\nsecondAliasBuffer = 3\n"
            }
        }),
    );
    server.wait_for_symbol("secondAliasBuffer", true);
    server.notify("textDocument/didClose", json!({"textDocument": {"uri": alias_uri}}));
    server.wait_for_symbol("secondAliasBuffer", false);
    server.wait_for_symbol("fromDisk", false);
    server.shutdown();
}

#[cfg(unix)]
#[test]
fn source_reconfiguration_refreshes_closed_aliases_from_their_own_paths() {
    use std::os::unix::fs::symlink;

    let workspace = TestWorkspace::empty();
    workspace.write("project/src/Library.purs", "module Library where\noldValue = 1\n");
    workspace.write(
        "launcher/physical.mjs",
        "import { realpathSync } from 'node:fs';\nconsole.log(realpathSync(new URL('../project/src', import.meta.url)) + '/*.purs');\n",
    );
    workspace.write(
        "launcher/alias.mjs",
        "import { fileURLToPath } from 'node:url';\nconsole.log(fileURLToPath(new URL('../alias/src', import.meta.url)) + '/*.purs');\n",
    );
    symlink(workspace.path().join("project"), workspace.path().join("alias")).unwrap();
    let root = workspace.path().join("project");
    let physical = workspace.path().join("launcher/physical.mjs");
    let alias = workspace.path().join("launcher/alias.mjs");
    let configuration = json!({
        "sources": {"kind": "command", "program": "node", "arguments": [PathBuf::clone(&physical)]}
    });
    let startup = configuration.to_string();
    let mut server = LanguageServer::start_with_capabilities(
        &workspace,
        "launcher",
        &["lsp", "--config", &startup],
        &root,
        json!({"workspace": {"configuration": true}}),
        Some(configuration),
    );
    server.wait_for_symbol("oldValue", true);
    workspace.write("project/src/Library.purs", "module Library where\nnewValue = 2\n");

    server.set_configuration_acknowledged(json!({
        "sources": {"kind": "command", "program": "node", "arguments": [alias]}
    }));
    server.wait_for_symbol("newValue", true);
    server.wait_for_symbol("oldValue", false);
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
process.stderr.write("source command stderr\n");
process.exit(1);
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
    let message = message["params"]["message"].as_str().unwrap();
    assert!(message.contains("Failed to apply Iris settings"), "{message}");
    assert!(message.contains("source command stderr"), "{message}");
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
fn initial_preparation_queues_documents_while_requests_are_cancelled() {
    let workspace = TestWorkspace::empty();
    workspace.write("src/Library.purs", "module Library where\nfromDisk = 1\n");
    let (connections, program, port) = gated_source_command(&workspace, 1);
    let root = workspace.path();
    let source = root.join("src/Library.purs");
    let configuration = json!({
        "sources": {
            "kind": "command",
            "program": "node",
            "arguments": [program, root, source, port]
        }
    })
    .to_string();
    let mut server = LanguageServer::start_loading_with_capabilities(
        &workspace,
        "",
        &["lsp", "--config", &configuration],
        root,
        json!({}),
        None,
    );
    let mut command = connections.recv_timeout(Duration::from_secs(10)).unwrap();

    let error = server
        .request_once("workspace/symbol", json!({"query": "fromDisk"}))
        .expect_err("workspace request unexpectedly succeeded during initial preparation");
    match error {
        Error::Response(response) => {
            assert_eq!(response.code, ErrorCode::REQUEST_CANCELLED);
            assert_eq!(response.message, "Workspace is loading");
        }
        error => panic!("workspace request returned the wrong error: {error}"),
    }

    let uri = Url::from_file_path(&source).unwrap();
    server.notify(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": uri,
                "languageId": "purescript",
                "version": 1,
                "text": "module Library where\nopenedOnly = 2\n"
            }
        }),
    );
    server.notify(
        "textDocument/didChange",
        json!({
            "textDocument": {"uri": uri, "version": 2},
            "contentChanges": [{
                "range": {
                    "start": {"line": 1, "character": 0},
                    "end": {"line": 1, "character": 14}
                },
                "text": "finalUnsaved = 3"
            }]
        }),
    );
    command.write_all(b"release").unwrap();
    server.wait_for_ready();
    server.wait_for_symbol("finalUnsaved", true);
    server.wait_for_symbol("openedOnly", false);
    server.wait_for_symbol("fromDisk", false);
    server.shutdown();
}

#[test]
fn dirty_runtime_preparation_restarts_except_for_document_changes() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "spago.lock",
        r#"{"workspace":{"packages":{"application":{"path":"."}}},"packages":{}}"#,
    );
    workspace.write("src/Library.purs", "module Library where\nfromDisk = 1\n");
    let (connections, program, port) = gated_source_command(&workspace, 3);
    let root = workspace.path();
    let source = root.join("src/Library.purs");
    let configuration = json!({
        "sources": {
            "kind": "command",
            "program": "node",
            "arguments": [program, root, source, port]
        }
    });
    let mut server = LanguageServer::start_with_capabilities(
        &workspace,
        "",
        &["lsp"],
        root,
        json!({"workspace": {"configuration": true}}),
        Some(json!({})),
    );
    server.set_configuration(configuration);
    let first = connections.recv_timeout(Duration::from_secs(10)).unwrap();
    let uri = Url::from_file_path(&source).unwrap();
    server.notify(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": uri,
                "languageId": "purescript",
                "version": 1,
                "text": "module Library where\nopenContent = 2\n"
            }
        }),
    );
    assert_stream_closed(first);
    let second = connections.recv_timeout(Duration::from_secs(10)).unwrap();
    server.notify("textDocument/didSave", json!({"textDocument": {"uri": uri}}));
    assert_stream_closed(second);
    let mut third = connections.recv_timeout(Duration::from_secs(10)).unwrap();
    server.notify(
        "textDocument/didChange",
        json!({
            "textDocument": {"uri": uri, "version": 2},
            "contentChanges": [{"text": "module Library where\nchangedContent = 3\n"}]
        }),
    );
    // This response acknowledges DidChange. Completing this same gated command
    // must then install the workspace; an incorrect restart cannot reach ready.
    server.wait_for_symbol("changedContent", true);
    third.write_all(b"release").unwrap();
    server.wait_for_ready();
    server.wait_for_symbol("changedContent", true);
    server.wait_for_symbol("openContent", false);
    server.wait_for_symbol("fromDisk", false);
    server.shutdown();
}

#[test]
fn startup_source_command_failure_falls_back_without_configuration_support() {
    startup_source_command_failure_falls_back(false);
}

#[test]
fn diagnostic_overrides_do_not_repeat_a_failed_startup_command() {
    startup_source_command_failure_falls_back(true);
}

fn startup_source_command_failure_falls_back(workspace_configuration: bool) {
    let workspace = TestWorkspace::empty();
    workspace.write("Open.purs", "module Open where\nfromOpenBuffer = 1\n");
    workspace.write(
        "fail.mjs",
        r#"
import { readFileSync, writeFileSync } from "node:fs";
const count = Number(readFileSync(process.argv[2], "utf8"));
writeFileSync(process.argv[2], String(count + 1));
process.stderr.write("deterministic startup failure\n");
process.exit(1);
"#,
    );
    workspace.write("count", "0");
    let program = workspace.path().join("fail.mjs");
    let count = workspace.path().join("count");
    let configuration = json!({
        "sources": {"kind": "command", "program": "node", "arguments": [program, count]}
    })
    .to_string();
    let mut server = LanguageServer::start_loading_with_capabilities(
        &workspace,
        "",
        &["lsp", "--config", &configuration],
        workspace.path(),
        json!({"workspace": {"configuration": workspace_configuration}}),
        workspace_configuration.then(|| json!({"diagnostics": {"onOpen": false}})),
    );
    let uri = Url::from_file_path(workspace.path().join("Open.purs")).unwrap();
    server.notify(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": uri,
                "languageId": "purescript",
                "version": 1,
                "text": "module Open where\nfromOpenBuffer = 1\n"
            }
        }),
    );
    let message = server.wait_for_notification("window/showMessage");
    let message = message["params"]["message"].as_str().unwrap();
    assert!(message.contains("deterministic startup failure"), "{message}");
    server.wait_for_ready();
    assert_eq!(workspace.read("count"), "1");
    server.wait_for_symbol("fromOpenBuffer", true);
    server.shutdown();
}

#[test]
fn shutdown_cancels_blocked_initial_source_command() {
    blocked_initial_source_command_retires(true);
}

#[test]
fn transport_eof_cancels_blocked_initial_source_command() {
    blocked_initial_source_command_retires(false);
}

fn blocked_initial_source_command_retires(orderly: bool) {
    let workspace = TestWorkspace::empty();
    workspace.write("src/Library.purs", "module Library where\nfromDisk = 1\n");
    let (connections, program, port) = gated_source_command(&workspace, 1);
    let root = workspace.path();
    let source = root.join("src/Library.purs");
    let configuration = json!({
        "sources": {
            "kind": "command",
            "program": "node",
            "arguments": [program, root, source, port]
        }
    })
    .to_string();
    let mut server = LanguageServer::start_loading_with_capabilities(
        &workspace,
        "",
        &["lsp", "--config", &configuration],
        root,
        json!({}),
        None,
    );
    let command = connections.recv_timeout(Duration::from_secs(10)).unwrap();

    if orderly {
        server.shutdown();
    } else {
        server.disconnect();
    }
    assert_stream_closed(command);
}

fn gated_source_command(
    workspace: &TestWorkspace,
    attempts: usize,
) -> (Receiver<TcpStream>, PathBuf, String) {
    workspace.write(
        "gated-source.mjs",
        r#"
import { connect } from "node:net";
const [root, literal, port] = process.argv.slice(2);
if (process.cwd() !== root) throw new Error(`wrong root: ${process.cwd()}`);
const connection = connect(Number(port), "127.0.0.1");
await new Promise((resolve, reject) => {
  connection.once("data", resolve);
  connection.once("error", reject);
});
console.log(literal);
connection.destroy();
"#,
    );
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port().to_string();
    let (connected, connections) = mpsc::channel();
    thread::spawn(move || {
        for connection in listener.incoming().take(attempts) {
            if connected.send(connection.unwrap()).is_err() {
                break;
            }
        }
    });
    (connections, workspace.path().join("gated-source.mjs"), port)
}

fn assert_stream_closed(mut connection: TcpStream) {
    connection.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let mut trailing = [0; 1];
    match connection.read(&mut trailing) {
        Ok(0) => {}
        #[cfg(windows)]
        Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => {}
        result => panic!("source command connection was not closed: {result:?}"),
    }
}

#[test]
fn superseded_source_command_retires_descendants() {
    let (workspace, mut server, descendant, configuration) = descendant_source_command(false);
    server.set_configuration(configuration);
    let descendant = descendant.recv_timeout(Duration::from_secs(10)).unwrap();

    server.set_configuration_acknowledged(json!({}));
    server.wait_for_symbol("stillLoaded", true);
    assert_connection_closed(descendant);
    server.shutdown();
    drop(workspace);
}

#[test]
fn shutdown_retires_source_command_descendants() {
    let (workspace, mut server, descendant, configuration) = descendant_source_command(false);
    server.set_configuration(configuration);
    let descendant = descendant.recv_timeout(Duration::from_secs(10)).unwrap();

    server.shutdown();
    assert_connection_closed(descendant);
    drop(workspace);
}

#[test]
fn exited_source_command_retires_descendants_holding_pipes() {
    let (workspace, mut server, descendant, configuration) = descendant_source_command(true);
    server.set_configuration(configuration);
    let descendant = descendant.recv_timeout(Duration::from_secs(10)).unwrap();
    server.wait_for_ready();
    assert_connection_closed(descendant);
    server.wait_for_symbol("stillLoaded", true);
    server.shutdown();
    drop(workspace);
}

fn descendant_source_command(
    exit_leader: bool,
) -> (TestWorkspace, LanguageServer, Receiver<TcpStream>, Value) {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "spago.lock",
        r#"{"workspace":{"packages":{"application":{"path":"."}}},"packages":{}}"#,
    );
    workspace.write("src/Library.purs", "module Library where\nstillLoaded = 42\n");
    workspace.write(
        "descendant.mjs",
        r#"
import { spawn } from "node:child_process";
const descendant = spawn(process.execPath, ["-e", `
  const connection = require("node:net").connect(Number(process.argv[1]), "127.0.0.1");
  connection.on("connect", () => { connection.write("ready"); process.send("ready"); });
`, process.argv[2]], { stdio: ["ignore", "inherit", "inherit", "ipc"] });
descendant.once("message", () => {
  if (process.argv[3] === "true") {
    console.log("src/*.purs");
    process.exit(0);
  }
});
await new Promise(() => {});
"#,
    );
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port().to_string();
    let (connected, descendant) = mpsc::channel();
    thread::spawn(move || {
        let (connection, _) = listener.accept().unwrap();
        connected.send(connection).unwrap();
    });
    let program = workspace.path().join("descendant.mjs");
    let configuration = json!({
        "sources": {"kind": "command", "program": "node", "arguments": [program, port, exit_leader.to_string()]}
    });
    let mut server = LanguageServer::start_with_capabilities(
        &workspace,
        "",
        &["lsp"],
        workspace.path(),
        json!({"workspace": {"configuration": true}}),
        Some(json!({})),
    );
    server.wait_for_symbol("stillLoaded", true);
    (workspace, server, descendant, configuration)
}

fn assert_connection_closed(mut connection: TcpStream) {
    connection.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let mut ready = [0; 5];
    connection.read_exact(&mut ready).unwrap();
    assert_eq!(&ready, b"ready");
    let mut trailing = [0; 1];
    match connection.read(&mut trailing) {
        Ok(0) => {}
        #[cfg(windows)]
        Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => {}
        result => panic!("descendant connection was not closed without trailing data: {result:?}"),
    }
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
    let symbols = server
        .request_once("workspace/symbol", json!({"query": "startupOnly"}))
        .expect("invariant violated: workspace was not ready after the initialized notification");
    assert_eq!(symbols.as_array().unwrap().len(), 1, "{symbols}");
    assert_eq!(symbols[0]["name"], "startupOnly");
    assert_eq!(server.client.configuration_requests.load(Ordering::Relaxed), 0);
    server.shutdown();
}
