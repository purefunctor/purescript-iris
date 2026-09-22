use std::ops::ControlFlow;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, LazyLock, Mutex};
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
    ClientCapabilities, DocumentSymbolParams, InitializeParams, InitializedParams,
    PartialResultParams, ProgressToken, Registration, TextDocumentIdentifier,
    WorkDoneProgressCancelParams, WorkDoneProgressParams, WorkspaceFolder, WorkspaceSymbolParams,
};
use regex::Regex;
use serde_json::{Value, json};
use tokio::runtime::{Builder, Runtime};
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use url::Url;

#[path = "support.rs"]
mod support;

use support::TestWorkspace;

fn snapshot_json(name: &str, value: &Value) {
    let mut value = value.clone();
    normalize_process_status(&mut value);
    insta::with_settings!({omit_expression => true}, {
        insta::assert_snapshot!(name, serde_json::to_string_pretty(&value).unwrap());
    });
}

fn snapshot_workspace_json(name: &str, value: &Value, workspace: &Path) {
    let mut value = value.clone();
    let workspace_prefixes = workspace_prefixes(workspace);
    normalize_workspace_paths(&mut value, &workspace_prefixes);
    snapshot_json(name, &value);
}

fn workspace_prefixes(workspace: &Path) -> Vec<String> {
    let mut paths = vec![workspace.to_path_buf()];
    if let Ok(canonical) = dunce::canonicalize(workspace)
        && canonical != workspace
    {
        paths.push(canonical);
    }
    let mut prefixes = paths
        .into_iter()
        .flat_map(|path| {
            let native = path.to_string_lossy().trim_end_matches(['/', '\\']).to_string();
            let slash = native.replace('\\', "/");
            let uri = Url::from_directory_path(&path)
                .expect("invariant violated: workspace path is not absolute")
                .to_string()
                .trim_end_matches('/')
                .to_string();
            [native, slash, uri]
        })
        .collect::<Vec<_>>();
    prefixes
        .sort_unstable_by(|left, right| right.len().cmp(&left.len()).then_with(|| left.cmp(right)));
    prefixes.dedup();
    prefixes
}

fn normalize_workspace_paths(value: &mut Value, workspace_prefixes: &[String]) {
    match value {
        Value::Array(values) => {
            for value in values {
                normalize_workspace_paths(value, workspace_prefixes);
            }
        }
        Value::Object(fields) => {
            for value in fields.values_mut() {
                normalize_workspace_paths(value, workspace_prefixes);
            }
        }
        Value::String(string) => {
            for workspace_prefix in workspace_prefixes {
                *string = replace_path_prefix(string, workspace_prefix);
            }
            if string.contains("[WORKSPACE]") {
                *string = string.replace('\\', "/");
            }
        }
        Value::Bool(_) | Value::Null | Value::Number(_) => {}
    }
}

fn replace_path_prefix(value: &str, prefix: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut remaining = value;
    while let Some(index) = remaining.find(prefix) {
        let suffix = &remaining[index + prefix.len()..];
        let boundary = suffix.is_empty() || suffix.starts_with(['/', '\\']);
        result.push_str(&remaining[..index]);
        if boundary {
            result.push_str("[WORKSPACE]");
        } else {
            result.push_str(prefix);
        }
        remaining = suffix;
    }
    result.push_str(remaining);
    result
}

fn normalize_process_status(value: &mut Value) {
    static STATUS: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"exit (?:status|code): (-?\d+)").unwrap());

    match value {
        Value::Array(values) => values.iter_mut().for_each(normalize_process_status),
        Value::Object(fields) => fields.values_mut().for_each(normalize_process_status),
        Value::String(string) => {
            *string = STATUS.replace_all(string, "exit code: $1").into_owned();
        }
        Value::Bool(_) | Value::Null | Value::Number(_) => {}
    }
}

#[test]
fn snapshot_normalization_preserves_path_boundaries_and_platform_status_details() {
    let mut value = json!([
        r"C:\workspace\src\Main.purs",
        r"C:\workspace-other\src\Main.purs",
        "exit status: 7",
        "exit code: 9",
        "signal: 15",
    ]);
    normalize_workspace_paths(&mut value, &[r"C:\workspace".to_string()]);
    normalize_process_status(&mut value);

    assert_eq!(value[0], "[WORKSPACE]/src/Main.purs");
    assert_eq!(value[1], r"C:\workspace-other\src\Main.purs");
    assert_eq!(value[2], "exit code: 7");
    assert_eq!(value[3], "exit code: 9");
    assert_eq!(value[4], "signal: 15");
}

#[cfg(unix)]
#[test]
fn snapshot_normalization_includes_a_canonical_symlinked_temp_ancestor() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().unwrap();
    let real = directory.path().join("real workspace");
    let linked = directory.path().join("linked-workspace");
    fs::create_dir_all(real.join("project/src")).unwrap();
    symlink(&real, &linked).unwrap();
    let workspace = linked.join("project");
    let canonical = dunce::canonicalize(&workspace).unwrap();
    let mut value = json!([
        canonical.join("src/Main.purs").to_string_lossy(),
        Url::from_file_path(canonical.join("src/Main.purs")).unwrap(),
    ]);
    let prefixes = workspace_prefixes(&workspace);
    normalize_workspace_paths(&mut value, &prefixes);

    assert_eq!(value, json!(["[WORKSPACE]/src/Main.purs", "[WORKSPACE]/src/Main.purs"]));
}

struct ClientState {
    configuration: Mutex<Option<Value>>,
    configuration_requests: AtomicUsize,
    registrations: Mutex<Vec<Registration>>,
    progress_tokens: Mutex<Vec<ProgressToken>>,
    progress_creation: ProgressCreation,
    unexpected_requests: Mutex<Vec<String>>,
    notifications: mpsc::Sender<Value>,
}

#[derive(Clone)]
enum ProgressCreation {
    Immediate,
    Delayed(Arc<Semaphore>),
    Rejected,
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
        LanguageServer::start_with_progress_creation(
            workspace,
            directory,
            arguments,
            root,
            capabilities,
            configuration,
            ProgressCreation::Immediate,
        )
    }

    fn start_with_progress_creation(
        workspace: &TestWorkspace,
        directory: &str,
        arguments: &[&str],
        root: &Path,
        capabilities: Value,
        configuration: Option<Value>,
        progress_creation: ProgressCreation,
    ) -> LanguageServer {
        let mut arguments = arguments.to_vec();
        arguments.extend(["--lsp-log", "off"]);
        let (notifications, messages) = mpsc::channel();
        let client = Arc::new(ClientState {
            configuration: Mutex::new(configuration),
            configuration_requests: AtomicUsize::new(0),
            registrations: Mutex::new(vec![]),
            progress_tokens: Mutex::new(vec![]),
            progress_creation,
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
                    let creation = state.progress_creation.clone();
                    async move {
                        match creation {
                            ProgressCreation::Immediate => Ok(()),
                            ProgressCreation::Delayed(gate) => {
                                let permit = gate.acquire().await.unwrap();
                                permit.forget();
                                Ok(())
                            }
                            ProgressCreation::Rejected => Err(ResponseError::new(
                                ErrorCode::REQUEST_FAILED,
                                "progress creation rejected by test client",
                            )),
                        }
                    }
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
        snapshot_json("server_capabilities", &serde_json::to_value(result.capabilities).unwrap());
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

    fn request_async(&self, method: &str, parameters: Value) -> Receiver<Result<Value, Error>> {
        assert_eq!(method, "workspace/symbol");
        let parameters: WorkspaceSymbolParams = serde_json::from_value(parameters).unwrap();
        let server = self.server.clone();
        let (response, result) = mpsc::channel();
        self.runtime.spawn(async move {
            let request = server.request::<WorkspaceSymbolRequest>(parameters).await;
            let request = request.map(|value| serde_json::to_value(value).unwrap());
            let _ = response.send(request);
        });
        result
    }

    fn document_symbols_async(&self, uri: Url) -> Receiver<Result<Value, Error>> {
        let parameters = DocumentSymbolParams {
            text_document: TextDocumentIdentifier { uri },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };
        let mut server = self.server.clone();
        let request = server.document_symbol(parameters);
        let (response, result) = mpsc::channel();
        self.runtime.spawn(async move {
            let request = request.await.map(|value| serde_json::to_value(value).unwrap());
            let _ = response.send(request);
        });
        result
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

    fn wait_for_progress_tokens(&self, count: usize) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.client.progress_tokens.lock().unwrap().len() < count {
            assert!(Instant::now() < deadline, "timed out waiting for progress creation request");
            thread::sleep(Duration::from_millis(10));
        }
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
            snapshot_json("invalid_string_diagnostics", &Value::Array(diagnostics.clone()));
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

    let cases: &[&[&str]] = &[&["lsp"], &["lsp", "--stdio"]];
    for arguments in cases {
        let mut server = LanguageServer::start(&workspace, "", arguments, workspace.path());
        let symbols = server.request("workspace/symbol", json!({"query": "fromSpago"}));
        snapshot_workspace_json("empty_configuration_workspace_symbol", &symbols, workspace.path());
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

    snapshot_workspace_json("nested_package_workspace_symbol", &symbols, workspace.path());
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

    snapshot_workspace_json("prepared_workspace_symbol", &symbols, workspace.path());
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
    let token = end["params"]["token"].clone();
    let progress = server
        .notifications
        .iter()
        .filter(|notification| notification["method"] == "$/progress")
        .collect::<Vec<_>>();
    assert!(progress.iter().all(|notification| notification["params"]["token"] == token));
    let begin = progress
        .iter()
        .find(|notification| notification["params"]["value"]["kind"] == "begin")
        .expect("workspace progress did not begin");
    for report in
        progress.iter().filter(|notification| notification["params"]["value"]["kind"] == "report")
    {
        assert_eq!(report["params"]["value"]["cancellable"], true);
        let percentage = report["params"]["value"]["percentage"].as_u64().unwrap();
        assert!(percentage <= 100);
    }
    snapshot_json(
        "workspace_preparation_progress",
        &json!([begin["params"]["value"], end["params"]["value"]]),
    );
    let progress_tokens = server.client.progress_tokens.lock().unwrap();
    assert_eq!(progress_tokens.as_slice(), &[serde_json::from_value(token.clone()).unwrap()]);
    drop(progress_tokens);
    server
        .server
        .work_done_progress_cancel(WorkDoneProgressCancelParams {
            token: serde_json::from_value(token).unwrap(),
        })
        .unwrap();
    let symbols = server.request("workspace/symbol", json!({"query": "value"}));
    snapshot_workspace_json("workspace_symbol_after_progress", &symbols, workspace.path());
    server.shutdown();
}

#[test]
fn balances_progress_when_creation_is_accepted_after_preparation_finishes() {
    let workspace = TestWorkspace::empty();
    workspace
        .write("spago.yaml", "package:\n  name: application\n  dependencies: []\nworkspace: {}\n");
    workspace.write("src/Library.purs", "module Library where\nvalue = 42\n");
    let gate = Arc::new(Semaphore::new(0));
    let mut server = LanguageServer::start_with_progress_creation(
        &workspace,
        "",
        &["lsp"],
        workspace.path(),
        json!({"window": {"workDoneProgress": true}}),
        None,
        ProgressCreation::Delayed(Arc::clone(&gate)),
    );

    let symbols = server.request("workspace/symbol", json!({"query": "value"}));
    assert!(symbols.as_array().unwrap().iter().any(|symbol| symbol["name"] == "value"));
    server.wait_for_progress_tokens(1);
    server.collect_notifications();
    assert!(server.notifications.iter().all(|notification| notification["method"] != "$/progress"));

    gate.add_permits(1);
    let end = server.wait_for_notification_matching("$/progress", |notification| {
        notification["params"]["value"]["kind"] == "end"
    });
    let token = end["params"]["token"].clone();
    let progress = server
        .notifications
        .iter()
        .filter(|notification| {
            notification["method"] == "$/progress" && notification["params"]["token"] == token
        })
        .map(|notification| notification["params"]["value"].clone())
        .collect::<Vec<_>>();
    assert_eq!(progress.first().and_then(|value| value["kind"].as_str()), Some("begin"));
    assert_eq!(end["params"]["value"]["kind"], "end");
    snapshot_json(
        "late_workspace_progress_creation",
        &json!([progress.first().unwrap(), end["params"]["value"]]),
    );
    server.shutdown();
}

#[test]
fn rejected_progress_creation_does_not_block_preparation_or_emit_progress() {
    let workspace = TestWorkspace::empty();
    workspace
        .write("spago.yaml", "package:\n  name: application\n  dependencies: []\nworkspace: {}\n");
    workspace.write("src/Library.purs", "module Library where\nvalue = 42\n");
    let mut server = LanguageServer::start_with_progress_creation(
        &workspace,
        "",
        &["lsp"],
        workspace.path(),
        json!({"window": {"workDoneProgress": true}}),
        None,
        ProgressCreation::Rejected,
    );

    let symbols = server.request("workspace/symbol", json!({"query": "value"}));
    assert!(symbols.as_array().unwrap().iter().any(|symbol| symbol["name"] == "value"));
    server.wait_for_progress_tokens(1);
    server.collect_notifications();
    assert!(server.notifications.iter().all(|notification| notification["method"] != "$/progress"));
    server.shutdown();
}

#[test]
fn cancels_workspace_preparation_for_the_active_progress_token() {
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

    let begin = server.wait_for_notification_matching("$/progress", |notification| {
        notification["params"]["value"]["kind"] == "begin"
    });
    let token: ProgressToken = serde_json::from_value(begin["params"]["token"].clone()).unwrap();
    wait_for_path(&started, "Spago fetch to start");
    wait_for_path(&pid_file, "Spago pid to be recorded");
    wait_for_path(&descendant_pid_file, "Spago descendant to start");
    let pid: i32 = fs::read_to_string(&pid_file).unwrap().trim().parse().unwrap();
    let descendant_pid: i32 =
        fs::read_to_string(&descendant_pid_file).unwrap().trim().parse().unwrap();

    for token in [ProgressToken::Number(99), ProgressToken::String("unknown".to_string())] {
        server.server.work_done_progress_cancel(WorkDoneProgressCancelParams { token }).unwrap();
    }
    let request = server.request_async("workspace/symbol", json!({"query": "fromDisk"}));
    assert!(matches!(
        request.recv_timeout(Duration::from_millis(200)),
        Err(RecvTimeoutError::Timeout)
    ));
    #[cfg(unix)]
    {
        assert!(process_is_running(pid));
        assert!(process_is_running(descendant_pid));
    }

    server
        .server
        .work_done_progress_cancel(WorkDoneProgressCancelParams { token: token.clone() })
        .unwrap();
    server.server.work_done_progress_cancel(WorkDoneProgressCancelParams { token }).unwrap();
    let end = server.wait_for_notification_matching("$/progress", |notification| {
        notification["params"]["value"]["kind"] == "end"
    });
    snapshot_json(
        "cancelled_workspace_preparation_progress",
        &json!([begin["params"]["value"], end["params"]["value"]]),
    );
    let error = request
        .recv_timeout(Duration::from_secs(10))
        .expect("timed out waiting for the cancelled preparation request")
        .expect_err("cancelled preparation unexpectedly answered its waiting request");
    let Error::Response(response) = error else {
        panic!("expected a response error after cancellation, got {error:?}");
    };
    assert_eq!(response.code, ErrorCode::REQUEST_FAILED);
    snapshot_json("workspace_request_while_preparing", &serde_json::to_value(response).unwrap());

    #[cfg(unix)]
    {
        wait_for_process_exit(pid, "Spago process");
        wait_for_process_exit(descendant_pid, "Spago descendant");
    }
    server.collect_notifications();
    assert!(!server.notifications.iter().any(|notification| {
        notification["method"] == "$/progress" && notification["params"]["value"]["kind"] == "end"
    }));
    assert!(!release.exists());
    assert!(!descendant_release.exists());
    server.shutdown();
}

#[cfg(unix)]
#[test]
fn cancellation_retires_a_pipe_holding_descendant_after_the_spago_leader_exits() {
    let workspace = TestWorkspace::empty();
    workspace
        .write("spago.yaml", "package:\n  name: application\n  dependencies: []\nworkspace: {}\n");
    let pid_file = workspace.path().join("spago-pid");
    let descendant_pid_file = workspace.path().join("spago-descendant-pid");
    let descendant_release = workspace.path().join("spago-descendant-release");
    workspace.set_env("IRIS_E2E_SPAGO_PID", pid_file.to_str().unwrap());
    workspace.set_env("IRIS_E2E_SPAGO_DESCENDANT_PID", descendant_pid_file.to_str().unwrap());
    workspace.set_env("IRIS_E2E_SPAGO_DESCENDANT_RELEASE", descendant_release.to_str().unwrap());
    workspace.set_env("IRIS_E2E_SPAGO_EXIT_AFTER_DESCENDANT", "1");
    let mut server = LanguageServer::start_with_capabilities(
        &workspace,
        "",
        &["lsp"],
        workspace.path(),
        json!({"window": {"workDoneProgress": true}}),
        None,
    );

    let begin = server.wait_for_notification_matching("$/progress", |notification| {
        notification["params"]["value"]["kind"] == "begin"
    });
    wait_for_path(&pid_file, "Spago pid to be recorded");
    wait_for_path(&descendant_pid_file, "Spago descendant to start");
    let pid: i32 = fs::read_to_string(pid_file).unwrap().trim().parse().unwrap();
    let descendant_pid: i32 =
        fs::read_to_string(descendant_pid_file).unwrap().trim().parse().unwrap();
    wait_for_process_exit(pid, "Spago leader");
    assert!(process_is_running(descendant_pid));

    server
        .server
        .work_done_progress_cancel(WorkDoneProgressCancelParams {
            token: serde_json::from_value(begin["params"]["token"].clone()).unwrap(),
        })
        .unwrap();
    server.wait_for_notification_matching("$/progress", |notification| {
        notification["params"]["value"]["kind"] == "end"
    });
    wait_for_process_exit(descendant_pid, "Spago descendant");
    assert!(!descendant_release.exists());
    server.shutdown();
}

#[test]
fn analysis_request_restarts_cancelled_preparation_and_waits_for_the_workspace() {
    let workspace = TestWorkspace::empty();
    workspace
        .write("spago.yaml", "package:\n  name: application\n  dependencies: []\nworkspace: {}\n");
    workspace.write("src/Library.purs", "module Library where\nfromDisk = 0\n");
    let attempts = workspace.path().join("spago-attempts");
    workspace.set_env("IRIS_E2E_SPAGO_GATE_DIRECTORY", attempts.to_str().unwrap());
    let mut server = LanguageServer::start_with_capabilities(
        &workspace,
        "",
        &["lsp"],
        workspace.path(),
        json!({"window": {"workDoneProgress": true}}),
        None,
    );

    let first_begin = server.wait_for_notification_matching("$/progress", |notification| {
        notification["params"]["value"]["kind"] == "begin"
    });
    let first_token: ProgressToken =
        serde_json::from_value(first_begin["params"]["token"].clone()).unwrap();
    let first_started = attempts.join("1.started");
    wait_for_path(&first_started, "first Spago fetch to start");
    let _first_pid: i32 = fs::read_to_string(&first_started).unwrap().trim().parse().unwrap();

    let uri = Url::from_file_path(workspace.path().join("src/Library.purs")).unwrap();
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
    server
        .server
        .work_done_progress_cancel(WorkDoneProgressCancelParams { token: first_token })
        .unwrap();
    let first_end = server.wait_for_notification_matching("$/progress", |notification| {
        notification["params"]["value"]["kind"] == "end"
    });

    let request_count = std::thread::available_parallelism().map_or(2, |count| count.get() + 1);
    let mut responses = (0..request_count)
        .map(|index| {
            let query = if index == 0 { "fromBuffer" } else { "noSuchSymbol" };
            server.request_async("workspace/symbol", json!({"query": query}))
        })
        .collect::<Vec<_>>();
    assert!(matches!(
        responses[0].recv_timeout(Duration::from_millis(200)),
        Err(RecvTimeoutError::Timeout)
    ));

    let second_begin = server.wait_for_notification_matching("$/progress", |notification| {
        notification["params"]["value"]["kind"] == "begin"
    });
    assert_ne!(first_begin["params"]["token"], second_begin["params"]["token"]);
    wait_for_path(&attempts.join("2.started"), "second Spago fetch to start");
    #[cfg(unix)]
    assert!(!process_is_running(_first_pid), "first Spago fetch overlapped its retry");

    fs::write(attempts.join("2.release"), "release\n").unwrap();
    let symbols = responses
        .remove(0)
        .recv_timeout(Duration::from_secs(30))
        .expect("timed out waiting for the original workspace request")
        .expect("original workspace request failed after retry");
    for response in responses {
        let symbols = response
            .recv_timeout(Duration::from_secs(30))
            .expect("timed out waiting for a coalesced workspace request")
            .expect("coalesced workspace request failed after retry");
        assert!(symbols.as_array().unwrap().is_empty());
    }
    assert!(symbols.as_array().unwrap().iter().any(|symbol| symbol["name"] == "fromBuffer"));
    snapshot_workspace_json("workspace_symbol_after_preparation_retry", &symbols, workspace.path());
    let second_end = server.wait_for_notification_matching("$/progress", |notification| {
        notification["params"]["value"]["kind"] == "end"
    });
    snapshot_json(
        "workspace_preparation_retry_progress",
        &json!([
            first_begin["params"]["value"],
            first_end["params"]["value"],
            second_begin["params"]["value"],
            second_end["params"]["value"],
        ]),
    );
    workspace.assert_spago_calls(
        "",
        &[&["fetch", "-p", "application"], &["fetch", "-p", "application"]],
    );
    server.shutdown();
}

#[test]
fn cancelling_a_retry_rejects_its_waiter_and_a_later_request_starts_again() {
    let workspace = TestWorkspace::empty();
    workspace
        .write("spago.yaml", "package:\n  name: application\n  dependencies: []\nworkspace: {}\n");
    workspace.write("src/Library.purs", "module Library where\nrecovered = 42\n");
    let attempts = workspace.path().join("spago-attempts");
    workspace.set_env("IRIS_E2E_SPAGO_GATE_DIRECTORY", attempts.to_str().unwrap());
    let mut server = LanguageServer::start_with_capabilities(
        &workspace,
        "",
        &["lsp"],
        workspace.path(),
        json!({"window": {"workDoneProgress": true}}),
        None,
    );

    let first_begin = server.wait_for_notification_matching("$/progress", |notification| {
        notification["params"]["value"]["kind"] == "begin"
    });
    wait_for_path(&attempts.join("1.started"), "first Spago fetch to start");
    server
        .server
        .work_done_progress_cancel(WorkDoneProgressCancelParams {
            token: serde_json::from_value(first_begin["params"]["token"].clone()).unwrap(),
        })
        .unwrap();
    server.wait_for_notification_matching("$/progress", |notification| {
        notification["params"]["value"]["kind"] == "end"
    });

    let first_request = server.request_async("workspace/symbol", json!({"query": "recovered"}));
    let second_begin = server.wait_for_notification_matching("$/progress", |notification| {
        notification["params"]["value"]["kind"] == "begin"
    });
    wait_for_path(&attempts.join("2.started"), "second Spago fetch to start");
    server
        .server
        .work_done_progress_cancel(WorkDoneProgressCancelParams {
            token: serde_json::from_value(second_begin["params"]["token"].clone()).unwrap(),
        })
        .unwrap();
    server.wait_for_notification_matching("$/progress", |notification| {
        notification["params"]["value"]["kind"] == "end"
    });
    let error = first_request
        .recv_timeout(Duration::from_secs(10))
        .expect("timed out waiting for cancelled retry request")
        .expect_err("cancelled retry unexpectedly answered its waiting request");
    let Error::Response(response) = error else {
        panic!("expected a response error after retry cancellation, got {error:?}");
    };
    assert_eq!(response.code, ErrorCode::REQUEST_FAILED);
    snapshot_json(
        "workspace_request_after_retry_cancellation",
        &serde_json::to_value(response).unwrap(),
    );

    let second_request = server.request_async("workspace/symbol", json!({"query": "recovered"}));
    server.wait_for_notification_matching("$/progress", |notification| {
        notification["params"]["value"]["kind"] == "begin"
    });
    wait_for_path(&attempts.join("3.started"), "third Spago fetch to start");
    fs::write(attempts.join("3.release"), "release\n").unwrap();
    let symbols = second_request
        .recv_timeout(Duration::from_secs(30))
        .expect("timed out waiting for request after repeated cancellation")
        .expect("request after repeated cancellation failed");
    assert!(symbols.as_array().unwrap().iter().any(|symbol| symbol["name"] == "recovered"));
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

#[cfg(unix)]
fn wait_for_process_exit(pid: i32, description: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while process_is_running(pid) {
        assert!(Instant::now() < deadline, "timed out waiting for {description} to exit");
        thread::sleep(Duration::from_millis(10));
    }
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

    let uri = Url::from_file_path(root.join("src/Library.purs")).unwrap();
    server.notify(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": uri,
                "languageId": "purescript",
                "version": 1,
                "text": "module Library where\nfromOpenBuffer = 1\n"
            }
        }),
    );
    let document_symbols = server.document_symbols_async(Url::clone(&uri));
    server.notify(
        "textDocument/didChange",
        json!({
            "textDocument": {"uri": uri, "version": 2},
            "contentChanges": [{"text": "module Library where\nfromChangedBuffer = 2\n"}]
        }),
    );
    server.notify("textDocument/didSave", json!({"textDocument": {"uri": uri}}));
    let workspace_symbols =
        server.request_async("workspace/symbol", json!({"query": "fromChangedBuffer"}));
    assert!(matches!(
        document_symbols.recv_timeout(Duration::from_millis(200)),
        Err(RecvTimeoutError::Timeout)
    ));
    assert!(matches!(
        workspace_symbols.recv_timeout(Duration::from_millis(200)),
        Err(RecvTimeoutError::Timeout)
    ));

    fs::write(&release, "release\n").unwrap();
    let symbols = document_symbols
        .recv_timeout(Duration::from_secs(30))
        .expect("timed out waiting for deferred document symbols")
        .expect("deferred document-symbol request failed");
    let symbols = symbols.as_array().expect("document-symbol response was not an array");
    assert!(symbols.iter().any(|symbol| symbol["name"] == "fromChangedBuffer"));
    assert!(!symbols.iter().any(|symbol| symbol["name"] == "fromOpenBuffer"));
    assert!(!symbols.iter().any(|symbol| symbol["name"] == "fromDisk"));
    let symbols = workspace_symbols
        .recv_timeout(Duration::from_secs(30))
        .expect("timed out waiting for deferred workspace symbols")
        .expect("deferred workspace-symbol request failed");
    assert!(symbols.as_array().unwrap().iter().any(|symbol| symbol["name"] == "fromChangedBuffer"));
    server.wait_for_symbol("fromChangedBuffer", true);
    server.wait_for_symbol("fromOpenBuffer", false);
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
    snapshot_json("invalid_configuration_while_preparing", &message["params"]);

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
    let uri = Url::from_file_path(workspace.path().join("src/Library.purs")).unwrap();
    let document_symbols = server.document_symbols_async(uri);
    assert!(matches!(
        document_symbols.recv_timeout(Duration::from_millis(200)),
        Err(RecvTimeoutError::Timeout)
    ));

    server.request_shutdown();
    let error = document_symbols
        .recv_timeout(Duration::from_secs(10))
        .expect("timed out waiting for document symbols to terminate during shutdown")
        .expect_err("document symbols unexpectedly succeeded during shutdown");
    let Error::Response(response) = error else {
        panic!("expected a response error during shutdown, got {error:?}");
    };
    assert_eq!(response.code, ErrorCode::CONTENT_MODIFIED);
    let end = server.wait_for_notification_matching("$/progress", |notification| {
        notification["params"]["value"]["kind"] == "end"
    });
    snapshot_json("shutdown_progress", &end["params"]["value"]);

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
fn shutdown_balances_progress_when_creation_is_still_pending() {
    let workspace = TestWorkspace::empty();
    workspace
        .write("spago.yaml", "package:\n  name: application\n  dependencies: []\nworkspace: {}\n");
    let (started, _release) = gate_preparation(&workspace);
    let gate = Arc::new(Semaphore::new(0));
    let mut server = LanguageServer::start_with_progress_creation(
        &workspace,
        "",
        &["lsp"],
        workspace.path(),
        json!({"window": {"workDoneProgress": true}}),
        None,
        ProgressCreation::Delayed(Arc::clone(&gate)),
    );
    server.wait_for_progress_tokens(1);
    wait_for_path(&started, "Spago fetch to start");

    server.request_shutdown();
    gate.add_permits(1);
    let end = server.wait_for_notification_matching("$/progress", |notification| {
        notification["params"]["value"]["kind"] == "end"
    });
    let token = end["params"]["token"].clone();
    let begin = server
        .notifications
        .iter()
        .find(|notification| {
            notification["method"] == "$/progress"
                && notification["params"]["token"] == token
                && notification["params"]["value"]["kind"] == "begin"
        })
        .expect("late progress acceptance did not receive Begin");
    snapshot_json(
        "shutdown_with_pending_progress_creation",
        &json!([begin["params"]["value"], end["params"]["value"]]),
    );
    server.finish_shutdown();
}

#[test]
fn preparation_failure_is_reported_and_rejects_analysis() {
    let workspace = TestWorkspace::empty();
    workspace
        .write("spago.yaml", "package:\n  name: application\n  dependencies: []\nworkspace: {}\n");
    workspace.write("src/Library.purs", "module Library where\nfromDisk = 0\n");
    let (started, release) = gate_preparation(&workspace);
    workspace.set_env("IRIS_E2E_SPAGO_FAIL", "simulated spago failure\n");

    let mut server = LanguageServer::start_with_capabilities(
        &workspace,
        "",
        &["lsp"],
        workspace.path(),
        json!({"window": {"workDoneProgress": true}}),
        None,
    );
    wait_for_path(&started, "Spago fetch to start");
    let uri = Url::from_file_path(workspace.path().join("src/Library.purs")).unwrap();
    let document_symbols = server.document_symbols_async(uri);
    assert!(matches!(
        document_symbols.recv_timeout(Duration::from_millis(200)),
        Err(RecvTimeoutError::Timeout)
    ));
    fs::write(release, "release\n").unwrap();

    let message = server.wait_for_notification("window/showMessage");
    let end = server.wait_for_notification_matching("$/progress", |notification| {
        notification["params"]["value"]["kind"] == "end"
    });
    snapshot_workspace_json(
        "workspace_preparation_failure",
        &json!({
            "message": message["params"],
            "progress": end["params"]["value"],
        }),
        workspace.path(),
    );
    let error = document_symbols
        .recv_timeout(Duration::from_secs(10))
        .expect("timed out waiting for deferred request after preparation failure")
        .expect_err("deferred request unexpectedly succeeded after preparation failure");
    let Error::Response(response) = error else {
        panic!("expected a response error after failure, got {error:?}");
    };
    assert_eq!(response.code, ErrorCode::REQUEST_FAILED);

    let error = server
        .request_once("workspace/symbol", json!({"query": "fromDisk"}))
        .expect_err("invariant violated: failed workspace answered a request");
    let Error::Response(response) = error else {
        panic!("expected a response error after failure, got {error:?}");
    };
    snapshot_json(
        "workspace_request_after_preparation_failure",
        &serde_json::to_value(response).unwrap(),
    );
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

    snapshot_workspace_json("symlinked_workspace_symbol", &symbols, workspace.path());
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
    let runtime = json!({
        "diagnostics": {"onOpen": false, "onSave": false, "onChange": true}
    });
    let root = dunce::canonicalize(workspace.path().join("project")).unwrap();
    let mut server = LanguageServer::start_with_capabilities(
        &workspace,
        "launcher",
        &["lsp"],
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
    assert_diagnostic_triggers_for(&mut server, &root, "AfterUpdate.purs", true, true, false);
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
    snapshot_json("invalid_runtime_diagnostics_configuration", &message["params"]);
    server.wait_for_symbol("stillLoaded", true);

    server.set_configuration(json!({
        "sources": {
            "kind": "command",
            "program": "node",
            "arguments": ["slow failure.mjs"]
        }
    }));
    let message = server.wait_for_notification("window/showMessage");
    snapshot_json("invalid_runtime_sources_configuration", &message["params"]);
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
fn clients_without_workspace_configuration_use_default_settings() {
    let workspace = TestWorkspace::empty();
    workspace
        .write("spago.yaml", "package:\n  name: application\n  dependencies: []\nworkspace: {}\n");
    workspace.write("src/Library.purs", "module Library where\ndefaultOnly = 42\n");
    let mut server = LanguageServer::start(&workspace, "", &["lsp"], workspace.path());
    server.notify(
        "workspace/didChangeConfiguration",
        json!({"settings": {"diagnostics": {"onOpen": false}}}),
    );
    let symbols = server.request("workspace/symbol", json!({"query": "defaultOnly"}));
    snapshot_workspace_json("default_configuration_workspace_symbol", &symbols, workspace.path());
    assert_eq!(server.client.configuration_requests.load(Ordering::Relaxed), 0);
    server.shutdown();
}
