use std::ops::ControlFlow;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::{fs, thread};

use async_lsp::router::Router;
use async_lsp::{
    Error, ErrorCode, LanguageServer as LanguageServerClient, ResponseError, ServerSocket,
};
use lsp_types::notification::{Progress, PublishDiagnostics, ShowMessage};
use lsp_types::request::{
    RegisterCapability, WorkDoneProgressCreate, WorkspaceConfiguration, WorkspaceSymbolRequest,
};
use lsp_types::{
    ClientCapabilities, InitializeParams, InitializedParams, ProgressToken, Registration,
    WorkspaceFolder, WorkspaceSymbolParams,
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
    progress_tokens: Mutex<Vec<ProgressToken>>,
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
            progress_tokens: Mutex::new(vec![]),
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
                .request::<WorkDoneProgressCreate, _>(|state, parameters| {
                    state.progress_tokens.lock().unwrap().push(parameters.token);
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
                .notification::<Progress>(|state, parameters| {
                    state
                        .notifications
                        .send(json!({"method": "$/progress", "params": parameters}))
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
        for _ in 0..600 {
            match self.request_once(method, parameters.clone()) {
                Ok(result) => return result,
                Err(Error::Response(response))
                    if response.code == ErrorCode::CONTENT_MODIFIED
                        || response.code == ErrorCode::REQUEST_CANCELLED => {}
                Err(error) => panic!("{method} request failed: {error}"),
            }
            thread::sleep(Duration::from_millis(50));
        }
        panic!("request {method} was repeatedly reported as stale");
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
        self.wait_for_notification_matching_with_timeout(method, Duration::from_secs(10), predicate)
    }

    #[track_caller]
    fn wait_for_notification_matching_with_timeout(
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

    fn collect_notifications(&mut self) {
        self.notifications.extend(self.messages.try_iter());
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

    fn request_shutdown(&mut self) {
        self.runtime.block_on(async {
            timeout(Duration::from_secs(10), self.server.shutdown(()))
                .await
                .expect("invariant violated: timed out waiting for shutdown response")
                .unwrap();
        });
    }

    fn finish_shutdown(&mut self) {
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

    fn shutdown(&mut self) {
        self.request_shutdown();
        self.finish_shutdown();
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
    workspace
        .write("spago.yaml", "package:\n  name: application\n  dependencies: []\nworkspace: {}\n");
    workspace.write("src/Library.purs", "module Library where\nfromSpago = 42\n");
    workspace.write("config/empty.json", "{}");

    let cases: &[&[&str]] = &[
        &["lsp"],
        &["lsp", "--stdio"],
        &["lsp", "--config", "null"],
        &["lsp", "--config-file", "config/empty.json"],
        &["lsp", "--config", r#"{"diagnostics":{"onOpen":null,"onSave":null,"onChange":null}}"#],
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
fn discovers_workspace_sources_when_opened_from_a_nested_package() {
    let workspace = TestWorkspace::empty();
    workspace.write("spago.yaml", "workspace: {}\n");
    workspace.write(
        "packages/application/spago.yaml",
        r#"package:
  name: application
  dependencies: []
"#,
    );
    workspace.write(
        "packages/application/src/Library.purs",
        "module Library where\nfromWorkspaceRoot = 42\n",
    );
    let package_root = workspace.path().join("packages/application");
    let mut server =
        LanguageServer::start(&workspace, "packages/application", &["lsp"], &package_root);

    let symbols = server.request("workspace/symbol", json!({"query": "fromWorkspaceRoot"}));

    assert_eq!(symbols.as_array().unwrap().len(), 1, "{symbols}");
    assert_eq!(symbols[0]["name"], "fromWorkspaceRoot");
    server.shutdown();
}

#[test]
fn prepares_the_spago_workspace_before_serving_analysis() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "spago.yaml",
        r#"package:
  name: application
  dependencies: []
workspace: {}
"#,
    );
    workspace.write(
        "src/Library.purs",
        r#"module Library where
fromFreshClone = 42
"#,
    );
    let mut server = LanguageServer::start(&workspace, "", &["lsp"], workspace.path());

    let symbols = server.request("workspace/symbol", json!({"query": "fromFreshClone"}));

    let [symbol] = symbols.as_array().unwrap().as_slice() else {
        panic!("expected one symbol, got {symbols}");
    };
    assert_eq!(symbol["name"], "fromFreshClone");
    workspace.assert_spago_calls("", &[&["fetch", "-p", "application"]]);
    server.shutdown();
}

#[test]
fn reports_workspace_preparation_progress() {
    let workspace = TestWorkspace::empty();
    workspace
        .write("spago.yaml", "package:\n  name: application\n  dependencies: []\nworkspace: {}\n");
    workspace.write("src/Library.purs", "module Library where\nvalue = 42\n");
    let mut server = LanguageServer::start_with_capabilities(
        &workspace,
        "",
        &["lsp"],
        workspace.path(),
        json!({"window": {"workDoneProgress": true}}),
        None,
    );

    let end = server.wait_for_notification_matching_with_timeout(
        "$/progress",
        Duration::from_secs(60),
        |notification| notification["params"]["value"]["kind"] == "end",
    );
    assert_eq!(end["params"]["value"]["message"], "Workspace preparation finished");
    let token = end["params"]["token"].clone();
    let progress = server
        .notifications
        .iter()
        .filter(|notification| notification["method"] == "$/progress")
        .collect::<Vec<_>>();
    assert!(progress.iter().all(|notification| notification["params"]["token"] == token));
    assert!(progress.iter().any(|notification| {
        notification["params"]["value"]["kind"] == "begin"
            && notification["params"]["value"]["title"] == "Preparing Iris workspace"
            && notification["params"]["value"]["percentage"] == 0
    }));
    assert!(progress.iter().any(|notification| {
        notification["params"]["value"]["message"] == "Completed application (1/1 packages)"
            && notification["params"]["value"]["percentage"] == 100
    }));
    assert!(progress.iter().any(|notification| {
        notification["params"]["value"]["message"] == "Finalizing initial compilation"
    }));
    let progress_tokens = server.client.progress_tokens.lock().unwrap();
    assert_eq!(progress_tokens.as_slice(), &[serde_json::from_value(token).unwrap()]);
    drop(progress_tokens);
    server.shutdown();
}

#[test]
fn omits_preparation_progress_when_the_client_does_not_support_it() {
    let workspace = TestWorkspace::empty();
    workspace
        .write("spago.yaml", "package:\n  name: application\n  dependencies: []\nworkspace: {}\n");
    workspace.write("src/Library.purs", "module Library where\nvalue = 42\n");
    let mut server = LanguageServer::start(&workspace, "", &["lsp"], workspace.path());

    server.request("workspace/symbol", json!({"query": "value"}));
    server.collect_notifications();

    assert!(server.client.progress_tokens.lock().unwrap().is_empty());
    assert!(server.notifications.iter().all(|notification| notification["method"] != "$/progress"));
    server.shutdown();
}

fn wait_for_path(path: &Path, description: &str) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !path.exists() {
        assert!(Instant::now() < deadline, "timed out waiting for {description}");
        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(unix)]
fn process_is_running(pid: i32) -> bool {
    std::process::Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn gate_preparation(workspace: &TestWorkspace) -> (std::path::PathBuf, std::path::PathBuf) {
    let started = workspace.path().join("spago-started");
    let release = workspace.path().join("spago-release");
    workspace.set_env("IRIS_E2E_SPAGO_STARTED", started.to_str().unwrap());
    workspace.set_env("IRIS_E2E_SPAGO_RELEASE", release.to_str().unwrap());
    (started, release)
}

#[test]
fn preparation_is_responsive_and_replays_ordered_buffers() {
    let workspace = TestWorkspace::empty();
    workspace
        .write("spago.yaml", "package:\n  name: application\n  dependencies: []\nworkspace: {}\n");
    workspace.write("src/Library.purs", "module Library where\nfromDisk = 0\n");
    let (started, release) = gate_preparation(&workspace);
    let root = dunce::canonicalize(workspace.path()).unwrap();

    let mut server = LanguageServer::start(&workspace, "", &["lsp"], &root);
    wait_for_path(&started, "Spago fetch to start");

    let error = server
        .request_once("workspace/symbol", json!({"query": "anything"}))
        .expect_err("invariant violated: loading workspace answered a request");
    let Error::Response(response) = error else {
        panic!("expected a response error while loading, got {error:?}");
    };
    assert_eq!(response.code, ErrorCode::CONTENT_MODIFIED);

    let uri = Url::from_file_path(root.join("src/Library.purs")).unwrap();
    server.notify(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": uri,
                "languageId": "purescript",
                "version": 1,
                "text": "module Library where\nfromBuffer = 1\n"
            }
        }),
    );
    server.notify(
        "textDocument/didChange",
        json!({
            "textDocument": {"uri": uri, "version": 2},
            "contentChanges": [{"text": "module Library where\nfromBuffer = 2\n"}]
        }),
    );
    server.notify("textDocument/didSave", json!({"textDocument": {"uri": uri}}));

    fs::write(&release, "release\n").unwrap();
    server.wait_for_symbol("fromBuffer", true);
    server.wait_for_symbol("fromDisk", false);
    server.shutdown();
}

#[test]
fn invalid_configuration_while_preparing_preserves_the_last_valid_settings() {
    let workspace = TestWorkspace::empty();
    workspace
        .write("spago.yaml", "package:\n  name: application\n  dependencies: []\nworkspace: {}\n");
    let (started, release) = gate_preparation(&workspace);
    let root = dunce::canonicalize(workspace.path()).unwrap();
    let runtime = json!({
        "diagnostics": {"onOpen": false, "onSave": false, "onChange": true}
    });
    let mut server = LanguageServer::start_with_capabilities(
        &workspace,
        "",
        &["lsp"],
        &root,
        json!({"workspace": {"configuration": true}}),
        Some(runtime),
    );
    wait_for_path(&started, "Spago fetch to start");

    server.set_configuration(json!({"diagnostics": {"onChange": "invalid"}}));
    let message = server.wait_for_notification("window/showMessage");
    assert!(
        message["params"]["message"]
            .as_str()
            .unwrap()
            .contains("previous Iris settings remain active")
    );

    fs::write(&release, "release\n").unwrap();
    assert_diagnostic_triggers(&mut server, &root, false, false, true);
    server.shutdown();
}

#[test]
fn shutdown_retires_the_spago_process_tree_while_preparing() {
    let workspace = TestWorkspace::empty();
    workspace
        .write("spago.yaml", "package:\n  name: application\n  dependencies: []\nworkspace: {}\n");
    workspace.write("src/Library.purs", "module Library where\nfromDisk = 0\n");
    let (started, release) = gate_preparation(&workspace);
    let pid_file = workspace.path().join("spago-pid");
    let descendant_pid_file = workspace.path().join("spago-descendant-pid");
    let descendant_release = workspace.path().join("spago-descendant-release");
    workspace.set_env("IRIS_E2E_SPAGO_PID", pid_file.to_str().unwrap());
    workspace.set_env("IRIS_E2E_SPAGO_DESCENDANT_PID", descendant_pid_file.to_str().unwrap());
    workspace.set_env("IRIS_E2E_SPAGO_DESCENDANT_RELEASE", descendant_release.to_str().unwrap());

    let mut server = LanguageServer::start_with_capabilities(
        &workspace,
        "",
        &["lsp"],
        workspace.path(),
        json!({"window": {"workDoneProgress": true}}),
        None,
    );
    server.wait_for_notification_matching("$/progress", |notification| {
        notification["params"]["value"]["kind"] == "begin"
    });
    wait_for_path(&started, "Spago fetch to start");
    wait_for_path(&descendant_pid_file, "Spago descendant to start");
    let _pid: i32 = fs::read_to_string(&pid_file).unwrap().trim().parse().unwrap();
    let _descendant_pid: i32 =
        fs::read_to_string(&descendant_pid_file).unwrap().trim().parse().unwrap();

    server.request_shutdown();
    let end = server.wait_for_notification_matching("$/progress", |notification| {
        notification["params"]["value"]["kind"] == "end"
    });
    assert_eq!(end["params"]["value"]["message"], "Workspace preparation cancelled");

    let (shutdown_complete, shutdown_result) = mpsc::channel();
    let shutdown = thread::spawn(move || {
        let mut server = server;
        server.finish_shutdown();
        shutdown_complete.send(()).unwrap();
    });
    if shutdown_result.recv_timeout(Duration::from_secs(5)).is_err() {
        fs::write(&descendant_release, "release\n").unwrap();
        shutdown.join().unwrap();
        panic!("language server shutdown waited for a surviving Spago descendant");
    }
    shutdown.join().unwrap();

    assert!(!release.exists());
    assert!(!descendant_release.exists());
    #[cfg(unix)]
    {
        assert!(!process_is_running(_pid), "Spago process {_pid} survived shutdown");
        assert!(
            !process_is_running(_descendant_pid),
            "Spago descendant process {_descendant_pid} survived shutdown"
        );
    }
}

#[test]
fn preparation_failure_is_reported_and_rejects_analysis() {
    let workspace = TestWorkspace::empty();
    workspace
        .write("spago.yaml", "package:\n  name: application\n  dependencies: []\nworkspace: {}\n");
    workspace.write("src/Library.purs", "module Library where\nfromDisk = 0\n");
    workspace.set_env("IRIS_E2E_SPAGO_FAIL", "simulated spago failure\n");

    let mut server = LanguageServer::start_with_capabilities(
        &workspace,
        "",
        &["lsp"],
        workspace.path(),
        json!({"window": {"workDoneProgress": true}}),
        None,
    );

    let message = server.wait_for_notification("window/showMessage");
    assert_eq!(message["params"]["type"], 1);
    let text = message["params"]["message"].as_str().unwrap();
    assert!(text.contains("could not prepare"), "{text}");
    assert!(text.contains("simulated spago failure"), "{text}");
    assert!(text.contains("restart Iris"), "{text}");
    let end = server.wait_for_notification_matching("$/progress", |notification| {
        notification["params"]["value"]["kind"] == "end"
    });
    assert_eq!(end["params"]["value"]["message"], "Workspace preparation failed");

    let error = server
        .request_once("workspace/symbol", json!({"query": "fromDisk"}))
        .expect_err("invariant violated: failed workspace answered a request");
    let Error::Response(response) = error else {
        panic!("expected a response error after failure, got {error:?}");
    };
    assert_eq!(response.code, ErrorCode::REQUEST_FAILED);
    assert_eq!(response.message, "Workspace preparation failed");
    server.shutdown();
}

#[cfg(unix)]
#[test]
fn discovers_workspace_sources_through_a_symlinked_root() {
    use std::os::unix::fs::symlink;

    let workspace = TestWorkspace::empty();
    workspace.write(
        "project/spago.yaml",
        "package:\n  name: application\n  dependencies: []\nworkspace: {}\n",
    );
    workspace.write("project/src/Library.purs", "module Library where\nfromSymlinkRoot = 42\n");
    symlink(workspace.path().join("project"), workspace.path().join("linked-project")).unwrap();
    let linked_root = workspace.path().join("linked-project");
    let mut server = LanguageServer::start(&workspace, "linked-project", &["lsp"], &linked_root);

    let symbols = server.request("workspace/symbol", json!({"query": "fromSymlinkRoot"}));

    assert_eq!(symbols.as_array().unwrap().len(), 1, "{symbols}");
    assert_eq!(symbols[0]["name"], "fromSymlinkRoot");
    server.shutdown();
}

#[test]
fn json_inputs_configure_diagnostic_triggers() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "project/spago.yaml",
        "package:\n  name: application\n  dependencies: []\nworkspace: {}\n",
    );
    workspace.write("project/src/Library.purs", "module Library where\nfromSpago = 42\n");
    let configuration = json!({
        "diagnostics": {"onOpen": false, "onSave": false, "onChange": true}
    })
    .to_string();
    let absolute_path = workspace.path().join("settings/server config.json");
    let root = dunce::canonicalize(workspace.path().join("project")).unwrap();
    let cases: &[&[&str]] = &[
        &["lsp", "--config", &configuration],
        &["lsp", "--config-file", "../settings/server config.json"],
        &["lsp", "--config-file", absolute_path.to_str().unwrap()],
    ];
    for arguments in cases {
        workspace.write("settings/server config.json", &configuration);
        let mut server = LanguageServer::start(&workspace, "launcher", arguments, &root);
        workspace.write("settings/server config.json", "invalid after startup");
        let symbols = server.request("workspace/symbol", json!({"query": "fromSpago"}));
        assert_eq!(symbols.as_array().unwrap().len(), 1, "{symbols}");
        assert_eq!(symbols[0]["name"], "fromSpago");
        let expected_uri = Url::from_file_path(root.join("src/Library.purs")).unwrap();
        assert_eq!(symbols[0]["location"]["uri"], expected_uri.as_str());
        assert_diagnostic_triggers(&mut server, &root, false, false, true);
        server.shutdown();
    }
}

#[test]
fn partial_diagnostic_configuration_preserves_omitted_triggers() {
    let workspace = TestWorkspace::empty();
    workspace
        .write("spago.yaml", "package:\n  name: application\n  dependencies: []\nworkspace: {}\n");
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
    workspace.write(
        "project/spago.yaml",
        "package:\n  name: application\n  dependencies: []\nworkspace: {}\n",
    );
    workspace.write("project/src/Library.purs", "module Library where\nfromSpago = 1\n");
    let startup = r#"{"diagnostics":{"onOpen":false,"onSave":false,"onChange":true}}"#;
    let runtime = json!({
        "diagnostics": {"onOpen": false, "onSave": false, "onChange": true}
    });
    let root = dunce::canonicalize(workspace.path().join("project")).unwrap();
    let mut server = LanguageServer::start_with_capabilities(
        &workspace,
        "launcher",
        &["lsp", "--config", startup],
        &root,
        json!({"workspace": {"configuration": true}}),
        Some(runtime),
    );

    server.wait_for_symbol("fromSpago", true);
    assert_diagnostic_triggers(&mut server, &root, false, false, true);
    let library_uri = Url::from_file_path(root.join("src/Library.purs")).unwrap();
    server.notify(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": library_uri,
                "languageId": "purescript",
                "version": 1,
                "text": "module Library where\nunsavedSpago = 3\n"
            }
        }),
    );
    server.wait_for_symbol("unsavedSpago", true);
    server.wait_for_symbol("fromSpago", false);

    server.set_configuration(json!({"diagnostics": {"onOpen": true}}));
    server.wait_for_symbol("fromSpago", false);
    server.wait_for_symbol("unsavedSpago", true);
    server.notify("textDocument/didClose", json!({"textDocument": {"uri": library_uri}}));
    server.wait_for_symbol("unsavedSpago", false);
    server.wait_for_symbol("fromSpago", true);
    assert_diagnostic_triggers_for(&mut server, &root, "AfterUpdate.purs", true, false, true);
    assert!(server.client.configuration_requests.load(Ordering::Relaxed) >= 2);
    server.shutdown();
}

#[test]
fn invalid_runtime_configuration_preserves_the_previous_workspace() {
    let workspace = TestWorkspace::empty();
    workspace
        .write("spago.yaml", "package:\n  name: application\n  dependencies: []\nworkspace: {}\n");
    workspace.write("src/Library.purs", "module Library where\nstillLoaded = 42\n");
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
    assert!(message["params"]["message"].as_str().unwrap().contains("Invalid Iris settings"));
    assert!(
        message["params"]["message"]
            .as_str()
            .unwrap()
            .contains("previous Iris settings remain active")
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
    workspace
        .write("spago.yaml", "package:\n  name: application\n  dependencies: []\nworkspace: {}\n");
    workspace.write("src/Library.purs", "module Library where\nstartupOnly = 42\n");
    let mut server = LanguageServer::start(&workspace, "", &["lsp"], workspace.path());
    server.notify(
        "workspace/didChangeConfiguration",
        json!({"settings": {"diagnostics": {"onOpen": false}}}),
    );
    let symbols = server.request("workspace/symbol", json!({"query": "startupOnly"}));
    assert_eq!(symbols.as_array().unwrap().len(), 1, "{symbols}");
    assert_eq!(symbols[0]["name"], "startupOnly");
    assert_eq!(server.client.configuration_requests.load(Ordering::Relaxed), 0);
    server.shutdown();
}
