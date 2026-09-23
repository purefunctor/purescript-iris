//! Workspace actor tests. They talk to the actor through its channels, without JSON-RPC.
//!
//! Two stand-ins make timing deterministic. [`GATED_METHOD`] is an analysis request that runs
//! until the test releases it; unless it is stubborn, it stops like real analysis when a change
//! sets the query engine's cancelled flag. [`gated_prepare`] is a preparation attempt that
//! succeeds or fails when the test releases it, and stops when it is cancelled.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use building::QueryError;
use building::lifecycle::{
    ContentAuthority, DiskObservation, DocumentKind, ForeignEvent, LifecycleEvent, SourceEvent,
    SourceUnitKey,
};
use files::ForeignSourceKind;
use iris_analysis::position::PositionEncoding;
use iris_build::compilation::{CompilationState, MaterializedPrim};
use iris_lsp_server::{
    Answer, ControlMessage, OrderedMessage, Rejection, SettingsResponse, WorkspaceEvent,
    WorkspaceEventSender, WorkspaceFailure, WorkspaceSenders,
};
use lsp_types::{Position, Range, TextDocumentContentChangeEvent, Url};
use parking_lot::Mutex;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::sync::{Notify, Semaphore, mpsc, oneshot, watch};
use tokio::task::JoinHandle;

use crate::WorkspaceConfig;
use crate::analysis::{CONTENT_MODIFIED, Snapshot};
use crate::discovery::package_source_roots;
use crate::handlers::{
    AnalysisJob, DocumentContext, DocumentNotification, apply_content_changes, apply_document,
};
use crate::preparation::{PreparationError, ProgressSink};
use crate::service::Actor;
use crate::state::{
    PreparedWorkspace, ReadyWorkspace, SourceMetadata, document_kind, observe_disk,
    source_unit_from_document_uri, source_unit_from_foreign_uri, source_unit_from_source_uri,
};

const PATIENCE: Duration = Duration::from_secs(10);

pub(crate) const GATED_METHOD: &str = "iris/test/gated";

/// A gated analysis request, named by the `gate` parameter.
#[derive(Default)]
struct Gate {
    started: Notify,
    released: AtomicBool,
}

static GATES: LazyLock<Mutex<HashMap<String, Arc<Gate>>>> = LazyLock::new(Default::default);

fn gate(name: &str) -> Arc<Gate> {
    Arc::clone(GATES.lock().entry(name.to_string()).or_default())
}

pub(crate) fn gated_job(params: Value) -> Result<AnalysisJob, Rejection> {
    let gate = gate(params["gate"].as_str().expect("gated requests need a gate"));
    let stubborn = params["stubborn"] == true;
    let panics = params["panic"] == true;
    Ok(Box::new(move |snapshot: &Snapshot| {
        if panics {
            panic!("gated request panicked");
        }
        gate.started.notify_one();
        let file_id = snapshot.first_source();
        // A failing test may never release the gate; the runtime waits for blocking work.
        let deadline = std::time::Instant::now() + 2 * PATIENCE;
        loop {
            if gate.released.load(Ordering::SeqCst) {
                return Ok(json!("released"));
            }
            if std::time::Instant::now() > deadline {
                return Err(Rejection::Internal("the gate was never released".to_string()));
            }
            if !stubborn && let Err(QueryError::Cancelled) = snapshot.engine.parsed(file_id) {
                return Err(Rejection::ContentModified(CONTENT_MODIFIED.to_string()));
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }))
}

/// Controls the preparation attempts of workspaces rooted at one directory.
#[derive(Default)]
struct PreparationGate {
    release: Notify,
    fail: AtomicBool,
}

static PREPARATIONS: LazyLock<Mutex<HashMap<PathBuf, Arc<PreparationGate>>>> =
    LazyLock::new(Default::default);

fn preparation_gate(root: &Path) -> Arc<PreparationGate> {
    Arc::clone(PREPARATIONS.lock().entry(root.to_path_buf()).or_default())
}

fn gated_prepare(
    root: PathBuf,
    mut cancel: watch::Receiver<bool>,
    _: ProgressSink,
) -> Pin<Box<dyn Future<Output = Result<PreparedWorkspace, PreparationError>> + Send>> {
    Box::pin(async move {
        let gate = preparation_gate(&root);
        tokio::select! {
            () = gate.release.notified() => {}
            _ = cancel.wait_for(|cancelled| *cancelled) => return Err(PreparationError::Cancelled),
        }
        if gate.fail.load(Ordering::SeqCst) {
            return Err(PreparationError::IoError(std::io::Error::other("simulated failure")));
        }
        Ok(prim_workspace())
    })
}

fn prim_workspace() -> PreparedWorkspace {
    let prim = MaterializedPrim::new().unwrap();
    PreparedWorkspace {
        compilation: CompilationState::new(prim, SourceMetadata::Builtin),
        source_roots: vec![],
    }
}

struct WorkspaceHarness {
    senders: WorkspaceSenders,
    events: mpsc::UnboundedReceiver<WorkspaceEvent>,
    files:
        Option<Arc<parking_lot::RwLock<building::lifecycle::FileLifecycle<i32, SourceMetadata>>>>,
    diagnostic_permits: Arc<Semaphore>,
    task: JoinHandle<Result<(), WorkspaceFailure>>,
    root: TempDir,
}

fn config(analysis_permits: usize) -> WorkspaceConfig {
    WorkspaceConfig {
        name: "iris".to_string(),
        version: "test".to_string(),
        analysis_permits,
        diagnostic_permits: 1,
    }
}

impl WorkspaceHarness {
    /// A ready workspace holding the Prim modules.
    async fn builtin(analysis_permits: usize) -> WorkspaceHarness {
        WorkspaceHarness::start(|events| {
            Actor::with_builtin_workspace(config(analysis_permits), events)
        })
        .await
    }

    /// A loading workspace whose preparation attempts run [`gated_prepare`].
    async fn loading() -> WorkspaceHarness {
        WorkspaceHarness::start(|events| Actor::with_prepare(config(2), events, gated_prepare))
            .await
    }

    async fn start(actor: impl FnOnce(WorkspaceEventSender) -> Actor) -> WorkspaceHarness {
        let root = tempfile::tempdir().unwrap();
        let (events, event_receiver) = WorkspaceEventSender::channel();
        let (senders, receivers) = WorkspaceSenders::channel();
        let actor = actor(events);
        let files = actor.files();
        let diagnostic_permits = actor.diagnostic_permits();
        let task = tokio::spawn(actor.run(receivers));
        let harness = WorkspaceHarness {
            senders,
            events: event_receiver,
            files,
            diagnostic_permits,
            task,
            root,
        };
        let root_uri = Url::from_directory_path(harness.root.path()).unwrap();
        let initialize = harness.senders.initialize(json!({
            "capabilities": {},
            "workspaceFolders": [{"uri": root_uri, "name": "workspace"}]
        }));
        answer(initialize).await.unwrap();
        harness
    }

    /// The gate for this workspace's preparation attempts. Preparation receives the workspace
    /// folder's path as the editor sent it, not its canonical spelling.
    fn preparation_gate(&self) -> Arc<PreparationGate> {
        preparation_gate(self.root.path())
    }

    fn uri(&self, name: &str) -> Url {
        Url::from_file_path(self.root.path().join(name)).unwrap()
    }

    fn request(&self, method: &str, params: Value) -> oneshot::Receiver<Answer> {
        self.senders.request(method.to_string(), params)
    }

    fn gated(&self, gate: &str) -> oneshot::Receiver<Answer> {
        self.request(GATED_METHOD, json!({"gate": gate}))
    }

    fn document_symbols(&self, uri: &Url) -> oneshot::Receiver<Answer> {
        self.request("textDocument/documentSymbol", json!({"textDocument": {"uri": uri}}))
    }

    fn notify(&self, method: &str, params: Value) {
        self.senders.send(OrderedMessage::Notification { method: method.to_string(), params });
    }

    fn open(&self, uri: &Url, version: i32, text: &str) {
        self.notify(
            "textDocument/didOpen",
            json!({"textDocument": {"uri": uri, "languageId": "purescript", "version": version, "text": text}}),
        );
    }

    fn change(&self, uri: &Url, version: i32, text: &str) {
        self.notify(
            "textDocument/didChange",
            json!({"textDocument": {"uri": uri, "version": version}, "contentChanges": [{"text": text}]}),
        );
    }

    fn settings(&self, response: SettingsResponse) {
        self.senders.send(OrderedMessage::Settings(response));
    }

    fn diagnostic_settings(&self, on_open: bool, on_save: bool, on_change: bool) {
        self.settings(SettingsResponse::Received(json!([{
            "diagnostics": {"onOpen": on_open, "onSave": on_save, "onChange": on_change}
        }])));
    }

    async fn event(&mut self) -> WorkspaceEvent {
        tokio::time::timeout(PATIENCE, self.events.recv())
            .await
            .expect("timed out waiting for a workspace event")
            .expect("the workspace event channel closed")
    }

    async fn assert_no_event(&mut self, duration: Duration) {
        if let Ok(Some(event)) = tokio::time::timeout(duration, self.events.recv()).await {
            panic!("unexpected workspace event {event:?}");
        }
    }

    /// Waits for the number of snapshots alive to reach `expected`.
    async fn wait_for_snapshots(&self, expected: usize) {
        let files = self.files.as_ref().expect("the workspace was not ready at start");
        // The workspace and the harness each hold one reference besides the snapshots.
        let snapshots = || Arc::strong_count(files) - 2;
        let deadline = tokio::time::Instant::now() + PATIENCE;
        while snapshots() != expected {
            assert!(
                tokio::time::Instant::now() < deadline,
                "expected {expected} snapshots, found {}",
                snapshots()
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    async fn stop(self) -> Result<(), WorkspaceFailure> {
        drop(self.senders);
        tokio::time::timeout(PATIENCE, self.task)
            .await
            .expect("timed out waiting for the workspace actor to stop")
            .unwrap()
    }
}

async fn answer(receiver: oneshot::Receiver<Answer>) -> Answer {
    tokio::time::timeout(PATIENCE, receiver)
        .await
        .expect("timed out waiting for an answer")
        .expect("the reply channel closed without an answer")
}

async fn started(gate_name: &str) {
    tokio::time::timeout(PATIENCE, gate(gate_name).started.notified())
        .await
        .expect("timed out waiting for the gated request to start");
}

fn release(gate_name: &str) {
    gate(gate_name).released.store(true, Ordering::SeqCst);
}

fn content_modified() -> Answer {
    Err(Rejection::ContentModified(CONTENT_MODIFIED.to_string()))
}

fn symbol_names(answer: &Value) -> Vec<String> {
    let symbols = answer.as_array().expect("expected a list of symbols");
    symbols.iter().map(|symbol| symbol["name"].as_str().unwrap().to_string()).collect()
}

fn module(name: &str, value: &str) -> String {
    format!("module {name} where\n{value} = 1\n")
}

#[tokio::test]
async fn a_change_wins_over_running_and_waiting_analysis() {
    // With one permit: a long request holds the permit, a second request waits for it, then a
    // change arrives. Before the rebuild, this sequence could hang the server.
    let harness = WorkspaceHarness::builtin(1).await;
    let uri = harness.uri("Main.purs");
    harness.open(&uri, 1, &module("Main", "before"));

    let running = harness.gated("change-wins");
    started("change-wins").await;
    let waiting = harness.document_symbols(&uri);
    harness.wait_for_snapshots(2).await;

    harness.change(&uri, 2, &module("Main", "after"));
    let after = harness.document_symbols(&uri);

    assert_eq!(answer(running).await, content_modified());
    assert_eq!(answer(waiting).await, content_modified());
    assert_eq!(symbol_names(&answer(after).await.unwrap()), ["after"]);
    harness.stop().await.unwrap();
}

#[tokio::test]
async fn a_request_that_finished_before_a_change_answers_from_the_old_document() {
    let harness = WorkspaceHarness::builtin(2).await;
    let uri = harness.uri("Main.purs");
    harness.open(&uri, 1, &module("Main", "before"));
    let before = harness.document_symbols(&uri);
    assert_eq!(symbol_names(&answer(before).await.unwrap()), ["before"]);
    harness.change(&uri, 2, &module("Main", "after"));
    let after = harness.document_symbols(&uri);
    assert_eq!(symbol_names(&answer(after).await.unwrap()), ["after"]);
    harness.stop().await.unwrap();
}

#[tokio::test]
async fn requests_never_answer_from_state_sent_after_them() {
    let harness = WorkspaceHarness::builtin(2).await;
    let uri = harness.uri("Main.purs");
    harness.open(&uri, 0, &module("Main", "value0"));
    let mut requests = vec![];
    for version in 1..=20 {
        requests.push((version - 1, harness.document_symbols(&uri)));
        harness.change(&uri, version, &module("Main", &format!("value{version}")));
    }
    let last = harness.document_symbols(&uri);
    for (version, request) in requests {
        match answer(request).await {
            Ok(symbols) => assert_eq!(symbol_names(&symbols), [format!("value{version}")]),
            rejected => assert_eq!(rejected, content_modified()),
        }
    }
    assert_eq!(symbol_names(&answer(last).await.unwrap()), ["value20"]);
    harness.stop().await.unwrap();
}

#[tokio::test]
async fn a_request_waiting_for_a_permit_does_not_delay_later_messages() {
    let harness = WorkspaceHarness::builtin(1).await;
    let running = harness.gated("permit-does-not-delay");
    started("permit-does-not-delay").await;
    let waiting = harness.request("workspace/symbol", json!({"query": ""}));
    // Handled by the actor itself, so answered while the request above waits for the permit.
    let unknown = harness.request("custom/unknown", json!({}));
    assert_eq!(answer(unknown).await, Err(Rejection::MethodNotFound));
    release("permit-does-not-delay");
    assert_eq!(answer(running).await, Ok(json!("released")));
    assert!(answer(waiting).await.is_ok());
    harness.stop().await.unwrap();
}

#[tokio::test]
async fn a_diagnostic_task_waiting_for_a_permit_yields_to_a_change() {
    let mut harness = WorkspaceHarness::builtin(2).await;
    harness.diagnostic_settings(true, false, true);
    let permit = Arc::clone(&harness.diagnostic_permits).acquire_owned().await.unwrap();

    let uri = harness.uri("Main.purs");
    harness.open(&uri, 1, "module Main where\nvalue :: Int\nvalue = \"one\"\n");
    harness.wait_for_snapshots(1).await;
    // The change waits for no snapshot; if the waiting diagnostic task kept its snapshot,
    // `QueryEngine::request_cancel` would block until the permit above is released.
    harness.change(&uri, 2, "module Main where\nvalue :: Int\nvalue = \"two\"\n");
    let after = harness.document_symbols(&uri);
    assert_eq!(symbol_names(&answer(after).await.unwrap()), ["value"]);

    drop(permit);
    let WorkspaceEvent::Diagnostics { uri: published, version, diagnostics } =
        harness.event().await
    else {
        panic!("expected diagnostics");
    };
    assert_eq!(published, uri);
    assert_eq!(version, Some(2));
    assert_eq!(diagnostics.as_array().unwrap().len(), 1);
    harness.assert_no_event(Duration::from_millis(300)).await;
    harness.stop().await.unwrap();
}

#[tokio::test]
async fn settings_and_did_save_do_not_cancel_running_analysis() {
    let harness = WorkspaceHarness::builtin(2).await;
    let uri = harness.uri("Main.purs");
    harness.open(&uri, 1, &module("Main", "value"));
    let running = harness.gated("settings-and-save");
    started("settings-and-save").await;

    harness.diagnostic_settings(false, true, false);
    harness.notify("textDocument/didSave", json!({"textDocument": {"uri": uri}}));
    // Answered after the actor handled the settings and the save.
    let symbols = harness.document_symbols(&uri);
    assert_eq!(symbol_names(&answer(symbols).await.unwrap()), ["value"]);

    release("settings-and-save");
    assert_eq!(answer(running).await, Ok(json!("released")));
    harness.stop().await.unwrap();
}

#[tokio::test]
async fn dropping_a_queued_request_drops_its_snapshot() {
    let harness = WorkspaceHarness::builtin(1).await;
    let running = harness.gated("dropped-request");
    started("dropped-request").await;
    let waiting = harness.request("workspace/symbol", json!({"query": ""}));
    harness.wait_for_snapshots(2).await;

    drop(waiting);
    harness.wait_for_snapshots(1).await;

    release("dropped-request");
    assert_eq!(answer(running).await, Ok(json!("released")));
    harness.wait_for_snapshots(0).await;
    harness.stop().await.unwrap();
}

#[tokio::test]
async fn a_change_does_not_delay_control_messages() {
    let mut harness = WorkspaceHarness::builtin(2).await;
    harness.diagnostic_settings(false, false, true);
    let uri = harness.uri("Main.purs");
    harness.open(&uri, 1, &module("Main", "value"));
    // A request that ignores the cancelled flag keeps the change below waiting.
    let stubborn = harness.request(GATED_METHOD, json!({"gate": "control", "stubborn": true}));
    started("control").await;
    harness.change(&uri, 2, &module("Main", "changed"));
    harness.senders.control(ControlMessage::Shutdown);
    tokio::time::sleep(Duration::from_millis(100)).await;

    release("control");
    assert_eq!(answer(stubborn).await, Ok(json!("released")));
    // Shutdown took effect while the change waited: the change starts no diagnostics.
    let symbols = harness.document_symbols(&uri);
    assert_eq!(symbol_names(&answer(symbols).await.unwrap()), ["changed"]);
    harness.assert_no_event(Duration::from_millis(300)).await;
    harness.stop().await.unwrap();
}

#[tokio::test]
async fn a_request_waiting_for_preparation_is_answered_and_later_settings_apply() {
    let mut harness = WorkspaceHarness::loading().await;
    harness.settings(SettingsResponse::Unsupported);
    assert!(matches!(
        harness.event().await,
        WorkspaceEvent::PreparationStarted { generation: 1, .. }
    ));

    let uri = harness.uri("Main.purs");
    harness.open(&uri, 1, &module("Main", "opened"));
    let waiting = harness.document_symbols(&uri);
    harness.diagnostic_settings(false, false, true);
    harness.change(&uri, 2, "module Main where\nvalue :: Int\nvalue = \"changed\"\n");
    let after = harness.document_symbols(&uri);

    harness.preparation_gate().release.notify_one();
    match answer(waiting).await {
        Ok(symbols) => assert_eq!(symbol_names(&symbols), ["opened"]),
        rejected => assert_eq!(rejected, content_modified()),
    }
    assert_eq!(symbol_names(&answer(after).await.unwrap()), ["value"]);
    assert_eq!(
        harness.event().await,
        WorkspaceEvent::PreparationEnded {
            generation: 1,
            message: "Workspace preparation finished".to_string()
        }
    );
    // The settings sent after the waiting request enabled diagnostics on change.
    let WorkspaceEvent::Diagnostics { version, .. } = harness.event().await else {
        panic!("expected diagnostics");
    };
    assert_eq!(version, Some(2));
    harness.stop().await.unwrap();
}

#[tokio::test]
async fn cancelling_preparation_rejects_waiting_requests_and_the_next_request_retries() {
    let mut harness = WorkspaceHarness::loading().await;
    harness.settings(SettingsResponse::Unsupported);
    assert!(matches!(
        harness.event().await,
        WorkspaceEvent::PreparationStarted { generation: 1, .. }
    ));
    let waiting = harness.request("workspace/symbol", json!({"query": ""}));
    // Make sure the request is waiting before cancelling.
    tokio::time::sleep(Duration::from_millis(50)).await;

    harness.senders.control(ControlMessage::CancelPreparation { generation: 1 });
    assert_eq!(
        answer(waiting).await,
        Err(Rejection::RequestFailed("Workspace preparation cancelled".to_string()))
    );
    assert_eq!(
        harness.event().await,
        WorkspaceEvent::PreparationEnded {
            generation: 1,
            message: "Workspace preparation cancelled".to_string()
        }
    );
    // Cancelling an attempt that is not running does nothing.
    harness.senders.control(ControlMessage::CancelPreparation { generation: 1 });

    // Cancelling the retry rejects its waiter too.
    let retrying = harness.request("workspace/symbol", json!({"query": "Prim"}));
    assert!(matches!(
        harness.event().await,
        WorkspaceEvent::PreparationStarted { generation: 2, .. }
    ));
    harness.senders.control(ControlMessage::CancelPreparation { generation: 2 });
    assert_eq!(
        answer(retrying).await,
        Err(Rejection::RequestFailed("Workspace preparation cancelled".to_string()))
    );
    assert_eq!(
        harness.event().await,
        WorkspaceEvent::PreparationEnded {
            generation: 2,
            message: "Workspace preparation cancelled".to_string()
        }
    );

    let retrying = harness.request("workspace/symbol", json!({"query": "Prim"}));
    assert!(matches!(
        harness.event().await,
        WorkspaceEvent::PreparationStarted { generation: 3, .. }
    ));
    harness.preparation_gate().release.notify_one();
    assert!(answer(retrying).await.is_ok());
    assert_eq!(
        harness.event().await,
        WorkspaceEvent::PreparationEnded {
            generation: 3,
            message: "Workspace preparation finished".to_string()
        }
    );
    harness.stop().await.unwrap();
}

#[tokio::test]
async fn requests_before_preparation_starts_are_told_the_workspace_is_loading() {
    let harness = WorkspaceHarness::loading().await;
    let early = harness.request("workspace/symbol", json!({"query": ""}));
    assert_eq!(answer(early).await, Err(Rejection::ContentModified("Workspace is loading".into())));
    harness.stop().await.unwrap();
}

#[tokio::test]
async fn failed_preparation_is_reported_and_rejects_analysis() {
    let mut harness = WorkspaceHarness::loading().await;
    harness.settings(SettingsResponse::Unsupported);
    assert!(matches!(harness.event().await, WorkspaceEvent::PreparationStarted { .. }));
    let waiting = harness.request("workspace/symbol", json!({"query": ""}));
    let gate = harness.preparation_gate();
    gate.fail.store(true, Ordering::SeqCst);
    gate.release.notify_one();

    let failed = Err(Rejection::RequestFailed("Workspace preparation failed".to_string()));
    assert_eq!(answer(waiting).await, failed);
    let WorkspaceEvent::Error { message } = harness.event().await else {
        panic!("expected an error message");
    };
    // The root is the workspace folder's path, spelled with a trailing separator.
    let root = Url::from_directory_path(harness.root.path()).unwrap().to_file_path().unwrap();
    let root = root.display();
    assert_eq!(
        message,
        format!(
            "Iris could not prepare the Spago workspace at {root}: IoError: simulated failure. \
             Correct the project (for example, by running `spago fetch`) and restart Iris."
        )
    );
    assert_eq!(
        harness.event().await,
        WorkspaceEvent::PreparationEnded {
            generation: 1,
            message: "Workspace preparation failed".to_string()
        }
    );
    let later = harness.request("workspace/symbol", json!({"query": ""}));
    assert_eq!(answer(later).await, failed);
    harness.stop().await.unwrap();
}

#[tokio::test]
async fn shutdown_rejects_requests_waiting_for_preparation() {
    let mut harness = WorkspaceHarness::loading().await;
    harness.settings(SettingsResponse::Unsupported);
    assert!(matches!(harness.event().await, WorkspaceEvent::PreparationStarted { .. }));
    let waiting = harness.request("workspace/symbol", json!({"query": ""}));
    tokio::time::sleep(Duration::from_millis(50)).await;
    harness.senders.control(ControlMessage::Shutdown);
    assert_eq!(
        answer(waiting).await,
        Err(Rejection::ContentModified("Workspace is loading".into()))
    );
    assert_eq!(
        harness.event().await,
        WorkspaceEvent::PreparationEnded {
            generation: 1,
            message: "Workspace preparation cancelled".to_string()
        }
    );
    harness.stop().await.unwrap();
}

#[tokio::test]
async fn invalid_settings_report_an_error_and_fall_back() {
    let mut harness = WorkspaceHarness::loading().await;
    harness.settings(SettingsResponse::Received(json!([{"diagnostics": {"onOpen": "invalid"}}])));
    assert_eq!(
        harness.event().await,
        WorkspaceEvent::Error {
            message:
                "Invalid Iris settings: invalid type: string \"invalid\", expected a boolean. \
                      Iris will use its default settings."
                    .to_string()
        }
    );
    // The defaults start preparation.
    assert!(matches!(
        harness.event().await,
        WorkspaceEvent::PreparationStarted { generation: 1, .. }
    ));

    harness.settings(SettingsResponse::Failed("workspace/configuration request timed out".into()));
    assert_eq!(
        harness.event().await,
        WorkspaceEvent::Error {
            message:
                "Failed to retrieve Iris settings: workspace/configuration request timed out. \
                      The previous Iris settings remain active."
                    .to_string()
        }
    );
    harness.assert_no_event(Duration::from_millis(100)).await;
    harness.stop().await.unwrap();
}

#[tokio::test]
async fn invalid_settings_while_preparing_keep_the_staged_settings() {
    let mut harness = WorkspaceHarness::loading().await;
    harness.diagnostic_settings(false, false, true);
    assert!(matches!(harness.event().await, WorkspaceEvent::PreparationStarted { .. }));
    harness.settings(SettingsResponse::Received(json!([{"diagnostics": {"onChange": "invalid"}}])));
    assert_eq!(
        harness.event().await,
        WorkspaceEvent::Error {
            message:
                "Invalid Iris settings: invalid type: string \"invalid\", expected a boolean. \
                      The previous Iris settings remain active."
                    .to_string()
        }
    );

    harness.preparation_gate().release.notify_one();
    let uri = harness.uri("Main.purs");
    harness.open(&uri, 1, "module Main where\nvalue :: Int\nvalue = \"one\"\n");
    harness.change(&uri, 2, "module Main where\nvalue :: Int\nvalue = \"two\"\n");
    assert!(matches!(harness.event().await, WorkspaceEvent::PreparationEnded { .. }));
    // Diagnostics on open stay disabled, so the first diagnostics are for the change.
    let WorkspaceEvent::Diagnostics { version, .. } = harness.event().await else {
        panic!("expected diagnostics on change");
    };
    assert_eq!(version, Some(2));
    harness.notify("textDocument/didSave", json!({"textDocument": {"uri": uri}}));
    let symbols = harness.document_symbols(&uri);
    assert_eq!(symbol_names(&answer(symbols).await.unwrap()), ["value"]);
    harness.assert_no_event(Duration::from_millis(300)).await;
    harness.stop().await.unwrap();
}

#[tokio::test]
async fn unknown_methods_and_invalid_parameters_are_rejected() {
    let harness = WorkspaceHarness::builtin(1).await;
    let unknown = harness.request("textDocument/formatting", json!({}));
    assert_eq!(answer(unknown).await, Err(Rejection::MethodNotFound));
    let invalid = harness.request("textDocument/hover", json!({"textDocument": 42}));
    let Err(Rejection::InvalidParams(message)) = answer(invalid).await else {
        panic!("expected invalid parameters");
    };
    assert!(message.starts_with("Failed to deserialize parameters: "), "{message}");
    harness.stop().await.unwrap();
}

#[tokio::test]
async fn a_panicking_handler_is_answered_with_an_internal_error() {
    let harness = WorkspaceHarness::builtin(1).await;
    let panicking = harness.request(GATED_METHOD, json!({"gate": "panic", "panic": true}));
    assert_eq!(
        answer(panicking).await,
        Err(Rejection::Internal(format!(
            "Request handler of {GATED_METHOD} panicked: gated request panicked"
        )))
    );
    let after = harness.request("workspace/symbol", json!({"query": ""}));
    assert!(answer(after).await.is_ok());
    harness.stop().await.unwrap();
}

#[tokio::test]
async fn a_malformed_document_notification_stops_the_actor() {
    let harness = WorkspaceHarness::builtin(1).await;
    harness.notify("textDocument/didOpen", json!({"textDocument": 42}));
    let result = tokio::time::timeout(PATIENCE, harness.task).await.unwrap().unwrap();
    let failure = result.unwrap_err();
    assert!(failure.0.starts_with("invalid textDocument/didOpen notification: "), "{failure}");
}

#[tokio::test]
async fn unknown_notifications_are_ignored() {
    let harness = WorkspaceHarness::builtin(1).await;
    harness.notify("custom/notification", json!({"anything": true}));
    let after = harness.request("workspace/symbol", json!({"query": ""}));
    assert!(answer(after).await.is_ok());
    harness.stop().await.unwrap();
}

#[tokio::test]
async fn cleanup_stops_running_analysis() {
    let harness = WorkspaceHarness::builtin(1).await;
    let running = harness.gated("cleanup");
    started("cleanup").await;
    let waiting = harness.request("workspace/symbol", json!({"query": ""}));
    drop(running);
    drop(waiting);
    harness.stop().await.unwrap();
}

#[tokio::test]
async fn cleanup_cancels_a_running_preparation_attempt() {
    let mut harness = WorkspaceHarness::loading().await;
    harness.settings(SettingsResponse::Unsupported);
    assert!(matches!(harness.event().await, WorkspaceEvent::PreparationStarted { .. }));
    harness.stop().await.unwrap();
}

fn ready_workspace() -> ReadyWorkspace {
    let configuration = Arc::new(iris_configuration::Configuration {
        diagnostics: iris_configuration::Diagnostics {
            on_open: false,
            on_save: false,
            on_change: false,
        },
    });
    ReadyWorkspace::new(prim_workspace(), configuration)
}

fn document_context() -> DocumentContext {
    DocumentContext {
        root: None,
        position_encoding: PositionEncoding::Utf16,
        change_signal: crate::analysis::Workers::new(1, 1).change_signal(),
    }
}

fn apply_event(workspace: &mut ReadyWorkspace, event: LifecycleEvent<i32, SourceMetadata>) {
    let signal = document_context().change_signal;
    let _ =
        workspace.apply_lifecycle_events([event], crate::state::DiagnosticTrigger::None, &signal);
}

fn close(workspace: &mut ReadyWorkspace, uri: &Url) {
    let notification = DocumentNotification::decode(
        "textDocument/didClose",
        json!({"textDocument": {"uri": uri}}),
    )
    .unwrap()
    .unwrap();
    let _ = apply_document(workspace, &document_context(), notification);
}

fn assert_source_close_result(
    source_uri: Url,
    foreign_uri: Url,
    source_authority: Option<ContentAuthority>,
) {
    let unit = source_unit_from_source_uri(&source_uri).unwrap();
    let mut workspace = ready_workspace();
    let event = LifecycleEvent::Source {
        unit: SourceUnitKey::clone(&unit),
        event: SourceEvent::Opened {
            text: Arc::from("module Main where\n"),
            version: 1,
            metadata: SourceMetadata::Unmanaged { editable: true },
        },
    };
    apply_event(&mut workspace, event);
    let event = LifecycleEvent::Foreign {
        unit: SourceUnitKey::clone(&unit),
        kind: ForeignSourceKind::JavaScript,
        event: ForeignEvent::DiskObserved {
            disk: DiskObservation::Found(Arc::from("export const life = 42;\n")),
        },
    };
    apply_event(&mut workspace, event);

    close(&mut workspace, &source_uri);

    let files = workspace.analysis.files.read();
    assert_eq!(files.source_authority(&unit), source_authority);
    assert_eq!(files.foreign_id(foreign_uri.as_str()), None);
}

#[test]
fn source_and_foreign_uris_produce_the_same_unit_key() {
    let directory = tempfile::tempdir().unwrap();
    let source_path = directory.path().join("Source Files").join("Main.purs");
    let foreign_path = source_path.with_extension("js");
    let jsx_path = source_path.with_extension("jsx");
    let source_uri = Url::from_file_path(source_path).unwrap();
    let foreign_uri = Url::from_file_path(foreign_path).unwrap();
    let jsx_uri = Url::from_file_path(jsx_path).unwrap();

    let from_source = source_unit_from_source_uri(&source_uri).unwrap();
    let from_foreign = source_unit_from_foreign_uri(&foreign_uri).unwrap();
    let from_jsx = source_unit_from_foreign_uri(&jsx_uri).unwrap();

    assert_eq!(from_source, from_foreign);
    assert_eq!(from_source, from_jsx);
    assert_eq!(from_source.source(), source_uri.as_str());
    assert_eq!(from_source.foreign(), foreign_uri.as_str());
    assert_eq!(from_source.foreign_for(ForeignSourceKind::Jsx), jsx_uri.as_str());
}

#[test]
fn localhost_source_and_foreign_uris_keep_the_same_authority() {
    let source_uri =
        Url::parse("file://localhost/workspace/Source%20Files/Main.purs?view=1#selection").unwrap();
    let foreign_uri =
        Url::parse("file://localhost/workspace/Source%20Files/Main.js?view=1#selection").unwrap();

    let from_source = source_unit_from_source_uri(&source_uri).unwrap();
    let from_foreign = source_unit_from_foreign_uri(&foreign_uri).unwrap();

    assert_eq!(from_source, from_foreign);
    assert_eq!(from_source.source(), source_uri.as_str());
    assert_eq!(from_source.foreign(), foreign_uri.as_str());
}

#[test]
fn non_file_document_uris_are_rejected() {
    let source_uri = Url::parse("untitled:Main.purs").unwrap();
    assert!(source_unit_from_source_uri(&source_uri).is_err());
}

#[test]
fn document_kind_is_bounded_to_source_and_foreign_extensions() {
    let source_uri = Url::parse("file:///workspace/Main.purs").unwrap();
    let foreign_uri = Url::parse("file:///workspace/Main.js").unwrap();
    let jsx_uri = Url::parse("file:///workspace/Main.jsx").unwrap();
    let unsupported_uri = Url::parse("file:///workspace/Main.json").unwrap();

    assert_eq!(document_kind(&source_uri), Some(DocumentKind::Source));
    assert_eq!(
        document_kind(&foreign_uri),
        Some(DocumentKind::Foreign(ForeignSourceKind::JavaScript))
    );
    assert_eq!(document_kind(&jsx_uri), Some(DocumentKind::Foreign(ForeignSourceKind::Jsx)));
    assert_eq!(document_kind(&unsupported_uri), None);
    assert!(source_unit_from_document_uri(&unsupported_uri).is_err());
}

#[test]
fn closing_a_deleted_source_also_removes_its_deleted_disk_foreign() {
    let directory = tempfile::tempdir().unwrap();
    let source_path = directory.path().join("Main.purs");
    let foreign_path = source_path.with_extension("js");
    let source_uri = Url::from_file_path(source_path).unwrap();
    let foreign_uri = Url::from_file_path(foreign_path).unwrap();
    assert_source_close_result(source_uri, foreign_uri, None);
}

#[test]
fn failed_source_reload_still_removes_its_deleted_disk_foreign() {
    let directory = tempfile::tempdir().unwrap();
    let source_path = directory.path().join("Main.purs");
    let foreign_path = source_path.with_extension("js");
    fs::write(&source_path, [0xff]).unwrap();
    let source_uri = Url::from_file_path(source_path).unwrap();
    let foreign_uri = Url::from_file_path(foreign_path).unwrap();
    assert_source_close_result(source_uri, foreign_uri, Some(ContentAuthority::Retained));
}

#[test]
fn duplicate_source_close_does_not_reconcile_foreign() {
    let directory = tempfile::tempdir().unwrap();
    let source_path = directory.path().join("Main.purs");
    let foreign_path = source_path.with_extension("js");
    fs::write(&source_path, "module Main where\n").unwrap();
    fs::write(&foreign_path, "export const life = 42;\n").unwrap();
    let source_uri = Url::from_file_path(source_path).unwrap();
    let foreign_uri = Url::from_file_path(&foreign_path).unwrap();
    let unit = source_unit_from_source_uri(&source_uri).unwrap();
    let mut workspace = ready_workspace();
    let event = LifecycleEvent::Source {
        unit: SourceUnitKey::clone(&unit),
        event: SourceEvent::Opened {
            text: Arc::from("module Main where\n"),
            version: 1,
            metadata: SourceMetadata::Unmanaged { editable: true },
        },
    };
    apply_event(&mut workspace, event);
    let event = LifecycleEvent::Foreign {
        unit: SourceUnitKey::clone(&unit),
        kind: ForeignSourceKind::JavaScript,
        event: ForeignEvent::DiskObserved {
            disk: DiskObservation::Found(Arc::from("export const life = 42;\n")),
        },
    };
    apply_event(&mut workspace, event);

    close(&mut workspace, &source_uri);
    fs::remove_file(foreign_path).unwrap();

    let source_id = workspace.analysis.files.read().source_id(source_uri.as_str()).unwrap();
    let foreign_id = workspace.analysis.files.read().foreign_id(foreign_uri.as_str()).unwrap();
    close(&mut workspace, &source_uri);

    let files = workspace.analysis.files.read();
    assert_eq!(files.source_id(source_uri.as_str()), Some(source_id));
    assert_eq!(files.foreign_id(foreign_uri.as_str()), Some(foreign_id));
    assert_eq!(workspace.analysis.engine.foreign_file(source_id), Some(foreign_id));
    assert_eq!(
        workspace.analysis.engine.foreign_content(foreign_id).unwrap().as_ref(),
        "export const life = 42;\n",
    );
}

#[test]
fn disk_observation_distinguishes_content_and_absence() {
    let directory = tempfile::tempdir().unwrap();
    let source_path = directory.path().join("Main.purs");
    let source_uri = Url::from_file_path(&source_path).unwrap();

    fs::write(&source_path, "module Main where\n").unwrap();
    assert!(matches!(
        observe_disk(&source_uri),
        DiskObservation::Found(content) if content.as_ref() == "module Main where\n"
    ));

    fs::remove_file(source_path).unwrap();
    assert_eq!(observe_disk(&source_uri), DiskObservation::NotFound);
}

#[cfg(unix)]
#[test]
fn package_roots_include_canonical_symlink_aliases() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().unwrap();
    let package_directory = directory.path().join("package");
    let linked_directory = directory.path().join("linked-package");
    fs::create_dir(&package_directory).unwrap();
    symlink(&package_directory, &linked_directory).unwrap();
    let package = iris_build::DiscoveredPackage {
        name: "linked-package".into(),
        files: vec![],
        dependencies: vec![],
        editable: true,
        roots: vec![PathBuf::from("linked-package")],
    };

    let roots = package_source_roots(directory.path(), directory.path(), &package).unwrap();
    let canonical = dunce::canonicalize(package_directory).unwrap();
    assert!(roots.iter().any(|root| root.path == canonical));
}

#[test]
fn incremental_content_changes_apply_sequentially() {
    let uri = Url::parse("file:///workspace/Main.purs").unwrap();
    let changes = [
        TextDocumentContentChangeEvent {
            range: Some(Range::new(Position::new(1, 0), Position::new(1, 4))),
            range_length: Some(4),
            text: "answer".to_string(),
        },
        TextDocumentContentChangeEvent {
            range: Some(Range::new(Position::new(1, 9), Position::new(1, 10))),
            range_length: Some(1),
            text: "42".to_string(),
        },
    ];

    let content = apply_content_changes(
        &uri,
        "module Main where\nlife = 0\n",
        &changes,
        PositionEncoding::Utf16,
    )
    .unwrap();

    assert_eq!(content.as_ref(), "module Main where\nanswer = 42\n");
}

#[test]
fn incremental_content_changes_use_negotiated_position_encoding() {
    let uri = Url::parse("file:///workspace/Main.purs").unwrap();
    let changes = [TextDocumentContentChangeEvent {
        range: Some(Range::new(Position::new(0, 3), Position::new(0, 4))),
        range_length: Some(1),
        text: "c".to_string(),
    }];

    let content = apply_content_changes(&uri, "a😀b", &changes, PositionEncoding::Utf16).unwrap();

    assert_eq!(content.as_ref(), "a😀c");
}

#[test]
fn full_content_change_resets_incremental_change_base() {
    let uri = Url::parse("file:///workspace/Main.purs").unwrap();
    let changes = [
        TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: "life = 1".to_string(),
        },
        TextDocumentContentChangeEvent {
            range: Some(Range::new(Position::new(0, 7), Position::new(0, 8))),
            range_length: Some(1),
            text: "2".to_string(),
        },
    ];

    let content =
        apply_content_changes(&uri, "discarded", &changes, PositionEncoding::Utf16).unwrap();

    assert_eq!(content.as_ref(), "life = 2");
}
