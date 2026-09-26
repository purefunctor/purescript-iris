//! The workspace actor loop, the channel for completed background work, and dispatch by method.
//!
//! The actor handles `OrderedMessage`s one at a time. Work that can wait for snapshots to be
//! dropped runs on a blocking thread and is awaited, so it never runs on the actor's own task. A
//! separate task handles `ControlMessage`s, so they can cancel preparation while the actor is busy.
//! Background work reports its completion on a channel the actor reads, including while a request
//! waits for preparation.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::{env, mem, panic};

use files::FileId;
use iris_analysis::AnalyzerCapabilities;
use iris_analysis::diagnostics::CollectedDiagnostics;
use iris_analysis::position::PositionEncoding;
use iris_configuration::Configuration;
use iris_lsp_server::{
    Answer, ControlMessage, OrderedMessage, Rejection, SettingsResponse, WorkspaceEvent,
    WorkspaceEventSender, WorkspaceFailure, WorkspaceReceivers,
};
use lsp_types::{InitializeParams, Uri, WorkspaceFolders};
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};
use tokio::task;

use crate::WorkspaceConfig;
use crate::analysis::Workers;
use crate::capabilities::{
    initialize_result, negotiate_analyzer_capabilities, negotiate_position_encoding,
};
use crate::diagnostics::{self, DiagnosticTicket};
use crate::handlers::{DocumentContext, DocumentNotification, analysis_job, apply_document};
use crate::preparation::{self, Disposition, Outcome, Preparation, PreparationError};
use crate::settings::{Settings, SettingsUpdate};
use crate::state::{PreparedWorkspace, ReadyWorkspace, WorkspaceEffects, WorkspaceState};

/// Completed background work, reported to the workspace actor.
pub(crate) enum Background {
    PreparationFinished {
        generation: u64,
        result: Result<Box<PreparedWorkspace>, PreparationError>,
    },
    DiagnosticsFinished {
        ticket: DiagnosticTicket,
        collected: Option<CollectedDiagnostics>,
    },
    /// The control task handled a `ControlMessage`; a request waiting for preparation checks
    /// whether its attempt was cancelled.
    Control,
}

pub(crate) struct Actor {
    config: WorkspaceConfig,
    events: WorkspaceEventSender,
    session: Session,
    settings: Settings,
    state: WorkspaceState,
    preparation: Arc<Preparation>,
    background: mpsc::UnboundedReceiver<Background>,
    background_sender: mpsc::UnboundedSender<Background>,
    shutdown: Arc<AtomicBool>,
    workers: Workers,
}

/// What the workspace actor negotiated at `initialize`.
struct Session {
    root: Option<PathBuf>,
    position_encoding: PositionEncoding,
    analyzer_capabilities: AnalyzerCapabilities,
}

const WORKSPACE_LOADING: &str = "Workspace is loading";
const WORKSPACE_FAILED: &str = "Workspace preparation failed";
const WORKSPACE_CANCELLED: &str = "Workspace preparation cancelled";

impl Actor {
    pub(crate) fn new(config: WorkspaceConfig, events: WorkspaceEventSender) -> Actor {
        let (background_sender, background) = mpsc::unbounded_channel();
        let preparation = Preparation::new(
            WorkspaceEventSender::clone(&events),
            mpsc::UnboundedSender::clone(&background_sender),
        );
        let settings = Settings::new();
        let state = WorkspaceState::Loading {
            pending: Vec::new(),
            configuration: Arc::clone(&settings.default),
        };
        Actor::with_state(
            config,
            events,
            settings,
            state,
            preparation,
            background,
            background_sender,
        )
    }

    /// Creates an actor whose workspace holds only the Prim modules and needs no preparation.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn with_builtin_workspace(
        config: WorkspaceConfig,
        events: WorkspaceEventSender,
    ) -> Actor {
        use iris_build::compilation::{CompilationState, MaterializedPrim};

        use crate::state::SourceMetadata;

        let (background_sender, background) = mpsc::unbounded_channel();
        let preparation = Preparation::finished(
            WorkspaceEventSender::clone(&events),
            mpsc::UnboundedSender::clone(&background_sender),
        );
        let settings = Settings::new();
        let prim = MaterializedPrim::new()
            .expect("invariant violated: failed to materialize the Prim modules");
        let compilation = CompilationState::new(prim, SourceMetadata::Builtin);
        let prepared = PreparedWorkspace { compilation, source_roots: Vec::new() };
        let workspace = ReadyWorkspace::new(prepared, Arc::clone(&settings.default));
        let state = WorkspaceState::Ready(Box::new(workspace));
        Actor::with_state(
            config,
            events,
            settings,
            state,
            preparation,
            background,
            background_sender,
        )
    }

    fn with_state(
        config: WorkspaceConfig,
        events: WorkspaceEventSender,
        settings: Settings,
        state: WorkspaceState,
        preparation: Preparation,
        background: mpsc::UnboundedReceiver<Background>,
        background_sender: mpsc::UnboundedSender<Background>,
    ) -> Actor {
        let workers = Workers::new(config.analysis_permits, config.diagnostic_permits);
        Actor {
            config,
            events,
            session: Session {
                root: None,
                position_encoding: PositionEncoding::Utf16,
                analyzer_capabilities: AnalyzerCapabilities::default(),
            },
            settings,
            state,
            preparation: Arc::new(preparation),
            background,
            background_sender,
            shutdown: Arc::new(AtomicBool::new(false)),
            workers,
        }
    }

    /// Creates a loading actor whose preparation attempts run `prepare`.
    #[cfg(test)]
    pub(crate) fn with_prepare(
        config: WorkspaceConfig,
        events: WorkspaceEventSender,
        prepare: crate::preparation::Prepare,
    ) -> Actor {
        let (background_sender, background) = mpsc::unbounded_channel();
        let preparation = Preparation::with_prepare(
            WorkspaceEventSender::clone(&events),
            mpsc::UnboundedSender::clone(&background_sender),
            prepare,
        );
        let settings = Settings::new();
        let state = WorkspaceState::Loading {
            pending: Vec::new(),
            configuration: Arc::clone(&settings.default),
        };
        Actor::with_state(
            config,
            events,
            settings,
            state,
            preparation,
            background,
            background_sender,
        )
    }

    #[cfg(test)]
    pub(crate) fn diagnostic_permits(&self) -> Arc<tokio::sync::Semaphore> {
        self.workers.diagnostic_permits()
    }

    #[cfg(test)]
    pub(crate) fn files(
        &self,
    ) -> Option<
        Arc<
            parking_lot::RwLock<
                building::lifecycle::FileLifecycle<i32, crate::state::SourceMetadata>,
            >,
        >,
    > {
        match &self.state {
            WorkspaceState::Ready(workspace) => Some(Arc::clone(&workspace.analysis.files)),
            WorkspaceState::Loading { .. } | WorkspaceState::Failed => None,
        }
    }

    pub(crate) async fn run(
        mut self,
        receivers: WorkspaceReceivers,
    ) -> Result<(), WorkspaceFailure> {
        let WorkspaceReceivers { mut ordered, control } = receivers;
        let control = tokio::spawn(control_loop(
            control,
            Arc::clone(&self.preparation),
            Arc::clone(&self.shutdown),
            mpsc::UnboundedSender::clone(&self.background_sender),
        ));
        let result = self.serve(&mut ordered).await;
        control.abort();
        self.cleanup().await;
        result
    }

    async fn serve(
        &mut self,
        ordered: &mut mpsc::UnboundedReceiver<OrderedMessage>,
    ) -> Result<(), WorkspaceFailure> {
        loop {
            tokio::select! {
                biased;
                Some(work) = self.background.recv() => self.finish(work).await,
                message = ordered.recv() => {
                    let Some(message) = message else { return Ok(()) };
                    // Once `iris-lsp-server` dropped its senders, the messages still queued
                    // belong to a session that ended.
                    if ordered.is_closed() {
                        return Ok(());
                    }
                    self.handle(message).await?;
                }
            }
        }
    }

    async fn handle(&mut self, message: OrderedMessage) -> Result<(), WorkspaceFailure> {
        match message {
            OrderedMessage::Initialize { params, reply } => {
                let _ = reply.send(self.initialize(params));
            }
            OrderedMessage::Initialized => {
                tracing::info!("The editor finished initialization");
            }
            OrderedMessage::Settings(response) => self.apply_settings(response),
            OrderedMessage::Notification { method, params } => {
                match DocumentNotification::decode(&method, params) {
                    Ok(Some(notification)) => self.document(notification).await,
                    Ok(None) => tracing::warn!("Ignored unhandled notification {method}"),
                    Err(error) => {
                        return Err(WorkspaceFailure(format!(
                            "invalid {method} notification: {error}"
                        )));
                    }
                }
            }
            OrderedMessage::Request { method, params, reply } => {
                self.request(method, params, reply).await;
            }
        }
        Ok(())
    }

    fn initialize(&mut self, params: Value) -> Answer {
        let parameters = serde_json::from_value::<InitializeParams>(params).map_err(|error| {
            Rejection::InvalidParams(format!("Failed to deserialize parameters: {error}"))
        })?;
        self.session.position_encoding = negotiate_position_encoding(&parameters);
        self.session.analyzer_capabilities = negotiate_analyzer_capabilities(&parameters);
        let scope = match parameters.workspace_folders_initialize_params.workspace_folders {
            Some(WorkspaceFolders::WorkspaceFolderList(folders)) => {
                folders.first().map(|folder| Uri::clone(&folder.uri))
            }
            Some(WorkspaceFolders::Null) | None => None,
        };
        self.session.root =
            scope.and_then(|uri| uri.to_file_path().ok()).or_else(|| env::current_dir().ok());
        let result = initialize_result(
            &self.config.name,
            &self.config.version,
            self.session.position_encoding,
        );
        Ok(serde_json::to_value(result)
            .expect("invariant violated: InitializeResult must serialize"))
    }

    fn apply_settings(&mut self, response: SettingsResponse) {
        match self.settings.update(response) {
            SettingsUpdate::Defaults => {
                if let Err(error) = self.apply_configuration(Arc::clone(&self.settings.default)) {
                    tracing::error!("{error}");
                }
            }
            SettingsUpdate::Resolved(Ok(configuration)) => {
                match self.apply_configuration(Arc::new(configuration)) {
                    Ok(()) => self.settings.initialized = true,
                    Err(error) => {
                        self.report_settings_error(&format!(
                            "Failed to apply Iris settings: {error}"
                        ));
                        self.fall_back_to_defaults();
                    }
                }
            }
            SettingsUpdate::Resolved(Err(error)) => {
                self.report_settings_error(&error);
                self.fall_back_to_defaults();
            }
        }
    }

    /// Applies the default settings if no settings were applied yet; otherwise the previous
    /// settings stay active.
    fn fall_back_to_defaults(&mut self) {
        if self.settings.initialized {
            return;
        }
        if let Err(error) = self.apply_configuration(Arc::clone(&self.settings.default)) {
            tracing::error!("{error}");
        }
        self.settings.initialized = true;
    }

    fn report_settings_error(&self, error: &str) {
        tracing::error!("{error}");
        let message = self.settings.error_message(error);
        self.events.send(WorkspaceEvent::Error { message });
    }

    /// Makes `configuration` active, or stages it and starts preparation while loading.
    fn apply_configuration(&mut self, configuration: Arc<Configuration>) -> Result<(), String> {
        if let WorkspaceState::Ready(workspace) = &mut self.state {
            workspace.configuration = configuration;
            return Ok(());
        }
        let root = self.session.root.clone().ok_or("Invalid or missing workspace root")?;
        // A configuration received while preparation runs replaces the staged value; it never
        // starts a second preparation.
        if let WorkspaceState::Loading { configuration: staged, .. } = &mut self.state {
            *staged = configuration;
        }
        self.preparation.start(root);
        Ok(())
    }

    async fn document(&mut self, notification: DocumentNotification) {
        match &mut self.state {
            WorkspaceState::Loading { pending, .. } => {
                pending.push(notification);
                return;
            }
            WorkspaceState::Failed => return,
            WorkspaceState::Ready(_) => {}
        }
        let WorkspaceState::Ready(mut workspace) =
            mem::replace(&mut self.state, WorkspaceState::Failed)
        else {
            unreachable!("invariant violated: the workspace is not ready")
        };
        let context = DocumentContext {
            root: self.session.root.clone(),
            position_encoding: self.session.position_encoding,
            change_signal: self.workers.change_signal(),
        };
        let (workspace, effects) = blocking(move || {
            let effects = apply_document(&mut workspace, &context, notification);
            (workspace, effects)
        })
        .await;
        self.state = WorkspaceState::Ready(workspace);
        self.deliver(effects);
    }

    fn deliver(&mut self, effects: WorkspaceEffects) {
        for uri in effects.clear_diagnostics {
            self.events.send(WorkspaceEvent::Diagnostics {
                uri,
                version: None,
                diagnostics: json!([]),
            });
        }
        for file_id in effects.collect_diagnostics {
            self.schedule_diagnostics(file_id);
        }
    }

    /// Waits until the workspace is ready, takes a snapshot, and starts an analysis task.
    ///
    /// The actor does not wait for the task, so a request waiting for a permit never delays
    /// later messages.
    async fn request(&mut self, method: String, params: Value, mut reply: oneshot::Sender<Answer>) {
        let job = match analysis_job(&method, params) {
            Ok(job) => job,
            Err(rejection) => {
                let _ = reply.send(Err(rejection));
                return;
            }
        };
        if let Err(rejection) = self.wait_until_ready(&mut reply).await {
            if let Some(rejection) = rejection {
                let _ = reply.send(Err(rejection));
            }
            return;
        }
        if reply.is_closed() {
            return;
        }
        let WorkspaceState::Ready(workspace) = &self.state else {
            unreachable!("invariant violated: the workspace is not ready")
        };
        let changes = self.workers.changes();
        let snapshot = workspace
            .analysis
            .snapshot(self.session.position_encoding, self.session.analyzer_capabilities);
        self.workers.spawn_analysis(method, snapshot, job, reply, changes);
    }

    /// Returns once the workspace is ready. Fails with the rejection to answer, or with `None`
    /// if the request was cancelled while it waited.
    async fn wait_until_ready(
        &mut self,
        reply: &mut oneshot::Sender<Answer>,
    ) -> Result<(), Option<Rejection>> {
        let mut generation = None;
        loop {
            match &self.state {
                WorkspaceState::Ready(_) => return Ok(()),
                WorkspaceState::Failed => {
                    return Err(Some(Rejection::RequestFailed(WORKSPACE_FAILED.to_string())));
                }
                WorkspaceState::Loading { .. } => {}
            }
            let waiting = match generation {
                Some(generation) => generation,
                None => {
                    let Some(demanded) = self.preparation.demand() else {
                        return Err(Some(Rejection::ContentModified(
                            WORKSPACE_LOADING.to_string(),
                        )));
                    };
                    generation = Some(demanded);
                    demanded
                }
            };
            match self.preparation.outcome(waiting) {
                Outcome::Pending => {}
                Outcome::Cancelled => {
                    return Err(Some(Rejection::RequestFailed(WORKSPACE_CANCELLED.to_string())));
                }
                Outcome::ShuttingDown => {
                    return Err(Some(Rejection::ContentModified(WORKSPACE_LOADING.to_string())));
                }
            }
            tokio::select! {
                biased;
                () = reply.closed() => return Err(None),
                Some(work) = self.background.recv() => self.finish(work).await,
            }
        }
    }

    async fn finish(&mut self, work: Background) {
        match work {
            Background::PreparationFinished { generation, result } => {
                self.finish_preparation(generation, result).await;
            }
            Background::DiagnosticsFinished { ticket, collected } => {
                self.finish_diagnostics(ticket, collected);
            }
            Background::Control => {}
        }
    }

    async fn finish_preparation(
        &mut self,
        generation: u64,
        result: Result<Box<PreparedWorkspace>, PreparationError>,
    ) {
        match self.preparation.finish(generation, result.is_ok()) {
            Some(Disposition::Apply) => {}
            Some(Disposition::Discard) | None => return,
        }
        let WorkspaceState::Loading { pending, configuration } =
            mem::replace(&mut self.state, WorkspaceState::Failed)
        else {
            unreachable!("invariant violated: preparation finished outside the loading state")
        };
        match result {
            Ok(prepared) => {
                let workspace = ReadyWorkspace::new(*prepared, configuration);
                self.state = WorkspaceState::Ready(Box::new(workspace));
                for notification in pending {
                    self.document(notification).await;
                }
                self.preparation.ended(generation, preparation::FINISHED);
            }
            Err(error) => {
                self.report_preparation_error(&error);
                self.preparation.ended(generation, preparation::FAILED);
            }
        }
    }

    fn report_preparation_error(&self, error: &PreparationError) {
        tracing::error!("Failed to prepare the Iris workspace: {error}");
        let root = self
            .session
            .root
            .as_deref()
            .map(|root| root.display().to_string())
            .unwrap_or_else(|| "the workspace root".to_string());
        let message = format!(
            "Iris could not prepare the Spago workspace at {root}: {error}. \
             Correct the project (for example, by running `spago fetch`) and restart Iris."
        );
        self.events.send(WorkspaceEvent::Error { message });
    }

    fn schedule_diagnostics(&mut self, file_id: FileId) {
        let WorkspaceState::Ready(workspace) = &mut self.state else { return };
        let version = {
            let files = workspace.analysis.files.read();
            if !files.contains_source(file_id) {
                return;
            }
            files.source_version(file_id)
        };
        if let Some(ticket) = workspace.diagnostics.schedule(file_id, version) {
            self.start_diagnostics(ticket);
        }
    }

    fn start_diagnostics(&self, ticket: DiagnosticTicket) {
        if self.shutdown.load(Ordering::SeqCst) {
            return;
        }
        let WorkspaceState::Ready(workspace) = &self.state else { return };
        let snapshot = workspace
            .analysis
            .snapshot(self.session.position_encoding, self.session.analyzer_capabilities);
        let background = mpsc::UnboundedSender::clone(&self.background_sender);
        diagnostics::spawn(&self.workers, snapshot, ticket, background);
    }

    fn finish_diagnostics(
        &mut self,
        ticket: DiagnosticTicket,
        collected: Option<CollectedDiagnostics>,
    ) {
        let WorkspaceState::Ready(workspace) = &mut self.state else { return };
        let running = workspace.diagnostics.is_running(ticket);
        let current = running && workspace.diagnostics.is_current(ticket) && {
            let files = workspace.analysis.files.read();
            files.contains_source(ticket.file_id)
                && files.source_version(ticket.file_id) == ticket.version
        };
        let next = workspace.diagnostics.complete(ticket);
        if current && let Some(collected) = collected {
            let diagnostics = serde_json::to_value(collected.diagnostics)
                .expect("invariant violated: diagnostics must serialize");
            self.events.send(WorkspaceEvent::Diagnostics {
                uri: collected.uri,
                version: ticket.version,
                diagnostics,
            });
        }
        if let Some(next) = next {
            self.start_diagnostics(next);
        }
    }

    /// Kills Spago process trees and waits for blocking preparation work, then stops running
    /// analysis at its next query and waits for every worker.
    async fn cleanup(&mut self) {
        let attempts = self.preparation.close();
        self.workers.change_signal().send();
        if let WorkspaceState::Ready(workspace) =
            mem::replace(&mut self.state, WorkspaceState::Failed)
        {
            blocking(move || workspace.analysis.engine.request_cancel()).await;
        }
        self.workers.close().await;
        for attempt in attempts {
            if let Err(error) = attempt.await {
                tracing::error!("Workspace preparation failed during cleanup: {error}");
            }
        }
    }
}

async fn control_loop(
    mut control: mpsc::UnboundedReceiver<ControlMessage>,
    preparation: Arc<Preparation>,
    shutdown: Arc<AtomicBool>,
    background: mpsc::UnboundedSender<Background>,
) {
    while let Some(message) = control.recv().await {
        match message {
            ControlMessage::CancelPreparation { generation } => {
                preparation.cancel(generation);
            }
            ControlMessage::Shutdown => {
                shutdown.store(true, Ordering::SeqCst);
                preparation.shut_down();
            }
        }
        let _ = background.send(Background::Control);
    }
}

/// Runs `work` on a blocking thread and propagates its panic to the actor, which stops it.
async fn blocking<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    match task::spawn_blocking(work).await {
        Ok(value) => value,
        Err(error) if error.is_panic() => panic::resume_unwind(error.into_panic()),
        Err(error) => panic!("invariant violated: blocking work was cancelled: {error}"),
    }
}
