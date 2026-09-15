pub mod capabilities;
pub mod error;
pub mod event;
pub mod extension;

mod analysis;
mod diagnostics;
mod document;
mod preparation;
mod process;
mod workspace;

#[cfg(test)]
mod tests;

use std::borrow::BorrowMut;
use std::ops::ControlFlow;
use std::path::PathBuf;
use std::sync::Arc;
use std::{env, fs, io};

use analyzer::AnalyzerCapabilities;
use analyzer::position::PositionEncoding;
use async_lsp::client_monitor::ClientProcessMonitorLayer;
use async_lsp::concurrency::ConcurrencyLayer;
use async_lsp::panic::CatchUnwindLayer;
use async_lsp::router::Router;
use async_lsp::server::LifecycleLayer;
use async_lsp::{ClientSocket, LanguageClient, ResponseError};
use building::QueryCancellation;
use building::lifecycle::{
    DiskObservation, DocumentKey, DocumentKind, ForeignEvent, LifecycleEvent, ReloadFailure,
    SourceEvent, SourceUnitKey,
};
use configuration::{Configuration, ConfigurationSettings};
use files::ForeignSourceKind;
use lsp_types::notification::Notification;
use lsp_types::request::Request;
use lsp_types::*;
use rustc_hash::FxHashSet;
use tokio::sync::Semaphore;
use tokio::task;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tower::ServiceBuilder;

use crate::server::analysis::AnalysisSnapshot as StateSnapshot;
use crate::server::capabilities::{
    ConfigurationCapabilities, negotiate_analyzer_capabilities,
    negotiate_configuration_capabilities, negotiate_position_encoding,
};
use crate::server::document::DocumentPath;
use crate::server::error::{AnalyzerResultExt, LspError};
use crate::server::preparation::{
    ConfigurationOrigin, Preparation, PreparationFinished, PreparedWorkspace,
};
use crate::server::workspace::{
    DiagnosticTrigger, ReadyWorkspace, WorkspaceContext, WorkspaceEffects, WorkspaceNotification,
    WorkspaceRuntime,
};
use crate::{ServerConfig, ServerError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum SourceMetadata {
    Builtin,
    Package { editable: bool },
    Unmanaged { editable: bool },
}

impl SourceMetadata {
    fn editable(&self) -> bool {
        match self {
            SourceMetadata::Builtin => false,
            SourceMetadata::Package { editable } | SourceMetadata::Unmanaged { editable } => {
                *editable
            }
        }
    }
}

struct ServerIdentity {
    name: String,
    version: String,
}

struct ProtocolSession {
    startup_configuration: Arc<Configuration>,
    root: Option<PathBuf>,
    configuration_scope: Option<Url>,
    configuration_capabilities: ConfigurationCapabilities,
    configuration_generation: u64,
    position_encoding: PositionEncoding,
    analyzer_capabilities: AnalyzerCapabilities,
    watched_files_dynamic_registration: bool,
}

pub struct State {
    client: ClientSocket,
    identity: ServerIdentity,
    protocol: ProtocolSession,
    workspace: WorkspaceRuntime,
    diagnostics: Option<diagnostics::DiagnosticWorker>,
    stopped: bool,
    preparation: Option<Preparation>,
    preparation_generation: u64,
    preparation_permit: Arc<Semaphore>,
    tasks: TaskTracker,
    cancellation: CancellationToken,
}

impl State {
    fn new(
        config: Arc<Configuration>,
        client: ClientSocket,
        name: String,
        version: String,
    ) -> State {
        State {
            client,
            identity: ServerIdentity { name, version },
            protocol: ProtocolSession {
                startup_configuration: config,
                root: None,
                configuration_scope: None,
                configuration_capabilities: ConfigurationCapabilities::default(),
                configuration_generation: 0,
                position_encoding: PositionEncoding::Utf16,
                analyzer_capabilities: AnalyzerCapabilities::default(),
                watched_files_dynamic_registration: false,
            },
            workspace: WorkspaceRuntime::new(),
            diagnostics: None,
            stopped: false,
            preparation: None,
            preparation_generation: 0,
            preparation_permit: Arc::new(Semaphore::new(1)),
            tasks: TaskTracker::new(),
            cancellation: CancellationToken::new(),
        }
    }

    fn spawn<T>(
        &self,
        action: impl FnOnce(StateSnapshot) -> T + Send + 'static,
    ) -> Result<task::JoinHandle<T>, LspError>
    where
        T: Send + 'static,
    {
        if self.stopped {
            return Err(building::QueryError::Cancelled.into());
        }
        let snapshot = self
            .workspace
            .snapshot(self.protocol.position_encoding, self.protocol.analyzer_capabilities)?;
        Ok(self.tasks.spawn_blocking(move || action(snapshot)))
    }

    fn stop(&mut self) {
        if self.stopped {
            return;
        }
        self.stopped = true;
        self.preparation.take();
        self.cancellation.cancel();
        self.diagnostics.take();
        if let Ok(workspace) = self.workspace.ready() {
            let analysis = Arc::clone(&workspace.analysis);
            if tokio::runtime::Handle::try_current().is_ok() {
                self.tasks.spawn_blocking(move || analysis.shutdown());
            } else {
                analysis.shutdown();
            }
        }
        self.tasks.close();
    }
}

impl Drop for State {
    fn drop(&mut self) {
        self.stop();
    }
}

fn initialize(
    state: &mut State,
    parameters: extension::CustomInitializeParams,
) -> impl Future<Output = Result<InitializeResult, ResponseError>> + use<> {
    let position_encoding = negotiate_position_encoding(&parameters.initialize_params);
    state.protocol.position_encoding = position_encoding;
    state.protocol.analyzer_capabilities =
        negotiate_analyzer_capabilities(&parameters.initialize_params);
    state.protocol.configuration_capabilities =
        negotiate_configuration_capabilities(&parameters.initialize_params);
    state.protocol.watched_files_dynamic_registration =
        watched_files_dynamic_registration(&parameters.initialize_params.capabilities);

    state.protocol.configuration_scope = parameters
        .initialize_params
        .workspace_folders
        .and_then(|folders| folders.first().map(|folder| Url::clone(&folder.uri)));
    state.protocol.root = state
        .protocol
        .configuration_scope
        .as_ref()
        .and_then(|uri| uri.to_file_path().ok())
        .or_else(|| env::current_dir().ok());
    let server_info = ServerInfo {
        name: String::clone(&state.identity.name),
        version: Some(String::clone(&state.identity.version)),
    };
    async move {
        Ok(InitializeResult {
            server_info: Some(server_info),
            capabilities: ServerCapabilities {
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(true),
                    trigger_characters: Some(vec![".".to_string()]),
                    all_commit_characters: None,
                    work_done_progress_options: WorkDoneProgressOptions {
                        work_done_progress: None,
                    },
                    completion_item: Some(CompletionOptionsCompletionItem {
                        label_details_support: Some(true),
                    }),
                }),
                code_action_provider: Some(CodeActionProviderCapability::Options(
                    CodeActionOptions {
                        code_action_kinds: Some(vec![CodeActionKind::QUICKFIX]),
                        ..CodeActionOptions::default()
                    },
                )),
                definition_provider: Some(OneOf::Left(true)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                references_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: WorkDoneProgressOptions {
                        work_done_progress: None,
                    },
                })),
                document_highlight_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            work_done_progress_options: WorkDoneProgressOptions {
                                work_done_progress: None,
                            },
                            legend: SemanticTokensLegend {
                                token_types: analyzer::semantic_tokens::TOKEN_TYPES.to_vec(),
                                token_modifiers: analyzer::semantic_tokens::TOKEN_MODIFIERS
                                    .to_vec(),
                            },
                            range: Some(false),
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                        },
                    ),
                ),
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::INCREMENTAL),
                        save: Some(TextDocumentSyncSaveOptions::Supported(true)),
                        ..TextDocumentSyncOptions::default()
                    },
                )),
                position_encoding: Some(PositionEncodingKind::from(position_encoding)),
                ..ServerCapabilities::default()
            },
        })
    }
}

fn watched_files_dynamic_registration(capabilities: &ClientCapabilities) -> bool {
    capabilities
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.did_change_watched_files.as_ref())
        .and_then(|watched_files| watched_files.dynamic_registration)
        .unwrap_or(false)
}

fn shutdown(state: &mut State, (): ()) -> impl Future<Output = Result<(), ResponseError>> + use<> {
    state.stop();
    let tasks = TaskTracker::clone(&state.tasks);
    async move {
        tasks.wait().await;
        Ok(())
    }
}

fn initialized(state: &mut State, _: InitializedParams) -> Result<(), LspError> {
    let _span = tracing::info_span!("initialization").entered();
    register_file_watcher(state);
    register_configuration_changes(state);

    if state.protocol.configuration_capabilities.workspace_configuration {
        request_workspace_configuration(state);
        Ok(())
    } else {
        apply_configuration(state, Arc::clone(&state.protocol.startup_configuration))
    }
}

fn register_configuration_changes(state: &State) {
    if !state.protocol.configuration_capabilities.dynamic_registration {
        return;
    }

    let registration = Registration {
        id: "iris-workspace-configuration".to_string(),
        method: notification::DidChangeConfiguration::METHOD.to_string(),
        register_options: None,
    };
    let parameters = RegistrationParams { registrations: vec![registration] };
    let mut client = ClientSocket::clone(&state.client);
    let cancellation = CancellationToken::clone(&state.cancellation);
    state.tasks.spawn(async move {
        tokio::select! {
            _ = cancellation.cancelled() => {},
            result = client.register_capability(parameters) => if let Err(error) = result {
                tracing::warn!("Failed to register workspace configuration changes: {error}");
            },
        }
    });
}

struct ConfigurationReceived {
    generation: u64,
    result: Result<Vec<serde_json::Value>, String>,
}

fn request_workspace_configuration(state: &mut State) {
    if state.stopped {
        return;
    }
    state.preparation.take();
    state.protocol.configuration_generation =
        state.protocol.configuration_generation.wrapping_add(1);
    let generation = state.protocol.configuration_generation;
    let parameters = ConfigurationParams {
        items: vec![ConfigurationItem {
            scope_uri: state.protocol.configuration_scope.clone(),
            section: Some("iris.server".to_string()),
        }],
    };
    let mut client = ClientSocket::clone(&state.client);
    let cancellation = CancellationToken::clone(&state.cancellation);
    state.tasks.spawn(async move {
        let result = tokio::select! {
            _ = cancellation.cancelled() => return,
            result = tokio::time::timeout(std::time::Duration::from_secs(10), client.configuration(parameters)) => result,
        }
        .map_err(|_| "workspace/configuration request timed out".to_string())
        .and_then(|result| result.map_err(|error| error.to_string()));
        if let Err(error) = client.emit(ConfigurationReceived { generation, result }) {
            tracing::error!("Failed to deliver workspace configuration: {error}");
        }
    });
}

fn finish_workspace_configuration(
    state: &mut State,
    ConfigurationReceived { generation, result }: ConfigurationReceived,
) -> Result<(), LspError> {
    if state.stopped || generation != state.protocol.configuration_generation {
        return Ok(());
    }

    let configuration = result
        .map_err(|error| format!("Failed to retrieve Iris settings: {error}"))
        .and_then(|mut values| {
            if values.len() != 1 {
                return Err(format!(
                    "Invalid workspace/configuration response: expected one item, received {}",
                    values.len()
                ));
            }
            let value = values.pop().expect("invariant violated: expected one configuration item");
            serde_json::from_value::<Option<ConfigurationSettings>>(value)
                .map(|settings| {
                    settings.unwrap_or_default().apply_to(&state.protocol.startup_configuration)
                })
                .map_err(|error| format!("Invalid Iris settings: {error}"))
        });

    match configuration {
        Ok(configuration) => {
            start_preparation(state, Arc::new(configuration), ConfigurationOrigin::Client, false)
        }
        Err(error) => {
            report_configuration_error(state, &error);
            if state.workspace.is_ready() {
                Ok(())
            } else {
                apply_configuration(state, Arc::clone(&state.protocol.startup_configuration))
            }
        }
    }
}

fn did_change_configuration(
    state: &mut State,
    _: DidChangeConfigurationParams,
) -> Result<(), LspError> {
    if state.protocol.configuration_capabilities.workspace_configuration {
        request_workspace_configuration(state);
    }
    Ok(())
}

fn report_configuration_error(state: &mut State, error: &str) {
    tracing::error!("{error}");
    let message = if state.workspace.is_ready() {
        format!("{error}. The previous Iris settings remain active.")
    } else {
        format!("{error}. Iris will use its startup settings.")
    };
    if let Err(error) =
        state.client.show_message(ShowMessageParams { typ: MessageType::ERROR, message })
    {
        tracing::warn!("Failed to report configuration error: {error}");
    }
}

fn report_configuration_delivery_error(state: &mut State, error: &LspError) {
    tracing::error!("Failed to deliver Iris settings effects: {error}");
    let message = format!(
        "Iris applied the new settings, but could not deliver all resulting client updates: {error}"
    );
    if let Err(error) =
        state.client.show_message(ShowMessageParams { typ: MessageType::ERROR, message })
    {
        tracing::warn!("Failed to report configuration delivery error: {error}");
    }
}

fn register_file_watcher(state: &State) {
    if !state.protocol.watched_files_dynamic_registration {
        return;
    }

    let parameters = file_watcher_registration();
    let mut client = ClientSocket::clone(&state.client);
    let cancellation = CancellationToken::clone(&state.cancellation);
    state.tasks.spawn(async move {
        tokio::select! {
            _ = cancellation.cancelled() => {},
            result = client.register_capability(parameters) => if let Err(error) = result {
                tracing::warn!("Failed to register source file watcher: {error}");
            },
        }
    });
}

fn file_watcher_registration() -> RegistrationParams {
    let options = DidChangeWatchedFilesRegistrationOptions {
        watchers: vec![
            FileSystemWatcher {
                glob_pattern: GlobPattern::String("**/*.purs".to_string()),
                kind: None,
            },
            FileSystemWatcher {
                glob_pattern: GlobPattern::String("**/*.js".to_string()),
                kind: None,
            },
            FileSystemWatcher {
                glob_pattern: GlobPattern::String("**/*.jsx".to_string()),
                kind: None,
            },
        ],
    };
    let register_options = serde_json::to_value(options)
        .expect("invariant violated: watched file registration options must serialize");
    let registration = Registration {
        id: "purescript-source-files".to_string(),
        method: notification::DidChangeWatchedFiles::METHOD.to_string(),
        register_options: Some(register_options),
    };
    RegistrationParams { registrations: vec![registration] }
}

fn exit(state: &mut State, (): ()) -> Result<(), LspError> {
    state.stop();
    Ok(())
}

fn apply_configuration(
    state: &mut State,
    configuration: Arc<Configuration>,
) -> Result<(), LspError> {
    start_preparation(state, configuration, ConfigurationOrigin::Startup, false)
}

fn start_preparation(
    state: &mut State,
    configuration: Arc<Configuration>,
    origin: ConfigurationOrigin,
    fallback: bool,
) -> Result<(), LspError> {
    if state.stopped {
        return Ok(());
    }
    state.preparation.take();
    if state.workspace.update_configuration_if_sources_equal(Arc::clone(&configuration)) {
        return Ok(());
    }
    let root = state.protocol.root.as_ref().ok_or(LspError::MissingRoot)?;
    let root = PathBuf::clone(root);
    let previous = state.workspace.reconfiguration_input();
    state.preparation_generation =
        state.preparation_generation.checked_add(1).expect("preparation generation overflowed");
    let generation = state.preparation_generation;
    let queries = QueryCancellation::default();
    let cancellation = state.cancellation.child_token();
    state.preparation = Some(Preparation {
        generation,
        configuration: Arc::clone(&configuration),
        origin,
        dirty: false,
        fallback,
        queries: QueryCancellation::clone(&queries),
        process: CancellationToken::clone(&cancellation),
    });
    let permit = Arc::clone(&state.preparation_permit);
    let client = ClientSocket::clone(&state.client);
    state.tasks.spawn(async move {
        let result = if fallback {
            task::spawn_blocking(move || preparation::fallback(configuration))
                .await
                .map_err(LspError::from)
                .flatten()
        } else {
            preparation::prepare(root, configuration, previous, queries, cancellation, permit).await
        };
        let _ = client.emit(PreparationFinished { generation, result });
    });
    Ok(())
}

fn finish_preparation(state: &mut State, completion: PreparationFinished) -> Result<(), LspError> {
    if state.stopped
        || state
            .preparation
            .as_ref()
            .is_none_or(|active| active.generation != completion.generation)
    {
        return Ok(());
    }
    let active = state.preparation.take().unwrap();
    if active.dirty {
        return start_preparation(
            state,
            Arc::clone(&active.configuration),
            active.origin,
            active.fallback,
        );
    }
    let prepared = match completion.result {
        Ok(prepared) => prepared,
        Err(error) => {
            report_configuration_error(state, &format!("Failed to apply Iris settings: {error}"));
            if !state.workspace.is_ready() && !active.fallback {
                let fallback = matches!(active.origin, ConfigurationOrigin::Startup)
                    || active.configuration.sources == state.protocol.startup_configuration.sources;
                start_preparation(
                    state,
                    Arc::clone(&state.protocol.startup_configuration),
                    ConfigurationOrigin::Startup,
                    fallback,
                )?;
            }
            return Ok(());
        }
    };
    match prepared {
        PreparedWorkspace::Initial(prepared) => {
            let pending = state.workspace.install(prepared)?;
            for notification in pending {
                let context = WorkspaceContext {
                    root: state.protocol.root.as_deref(),
                    position_encoding: state.protocol.position_encoding,
                };
                if let Err(error) = state.workspace.dispatch(notification, context, &state.client) {
                    error.emit_trace();
                }
            }
        }
        PreparedWorkspace::Reconfigured(prepared) => {
            let effects = state.workspace.commit_reconfiguration(prepared)?;
            if let Err(error) = effects.deliver(&state.client) {
                report_configuration_delivery_error(state, &error);
            }
        }
    }
    Ok(())
}

fn source_uri(path: &PathBuf) -> Result<Arc<str>, LspError> {
    let uri = DocumentPath::new(path)?.uri()?;
    Ok(Arc::from(uri.as_str()))
}

fn collect_diagnostics(
    state: &mut State,
    event::CollectDiagnostics { ticket }: event::CollectDiagnostics,
) -> Result<(), LspError> {
    if state.stopped {
        return Ok(());
    }
    let workspace = state.workspace.ready_mut()?;
    if !workspace.diagnostics.is_current(ticket) {
        return Ok(());
    }
    let worker = state.diagnostics.get_or_insert_with(|| {
        diagnostics::DiagnosticWorker::start(
            Arc::clone(&workspace.analysis),
            state.protocol.position_encoding,
            state.protocol.analyzer_capabilities,
            ClientSocket::clone(&state.client),
            &state.tasks,
        )
    });
    workspace.diagnostics.worker = Some(tokio::sync::mpsc::UnboundedSender::clone(&worker.sender));
    let _ = worker.sender.send(diagnostics::DiagnosticEvent::Schedule { ticket });
    Ok(())
}

fn finish_diagnostics(
    state: &mut State,
    event::DiagnosticsFinished { ticket, collected }: event::DiagnosticsFinished,
) -> Result<(), LspError> {
    if state.stopped {
        return Ok(());
    }
    if state.workspace.finish_diagnostics(ticket)?
        && let Some(collected) = collected
    {
        state.client.publish_diagnostics(PublishDiagnosticsParams {
            uri: collected.uri,
            diagnostics: collected.diagnostics,
            version: ticket.version,
        })?;
    }
    Ok(())
}

fn definition(
    snapshot: StateSnapshot,
    parameters: GotoDefinitionParams,
) -> Result<Option<GotoDefinitionResponse>, LspError> {
    let _span = tracing::info_span!("definition").entered();
    let uri = parameters.text_document_position_params.text_document.uri;
    let position = parameters.text_document_position_params.position;

    let result = snapshot.with_analyzer_context(|context| {
        analyzer::definition::implementation(context, uri, position)
    });

    result.on_non_fatal(None)
}

fn hover(snapshot: StateSnapshot, parameters: HoverParams) -> Result<Option<Hover>, LspError> {
    let _span = tracing::info_span!("hover").entered();
    let uri = parameters.text_document_position_params.text_document.uri;
    let position = parameters.text_document_position_params.position;

    let result = snapshot
        .with_analyzer_context(|context| analyzer::hover::implementation(context, uri, position));

    result.on_non_fatal(None)
}

fn code_action(
    snapshot: StateSnapshot,
    parameters: CodeActionParams,
) -> Result<Option<CodeActionResponse>, LspError> {
    let _span = tracing::info_span!("code_action").entered();
    let uri = parameters.text_document.uri;
    let range = parameters.range;
    let action_context = parameters.context;

    let result = snapshot.with_analyzer_context(|context| {
        analyzer::code_action::implementation(context, uri, range, action_context)
    });

    result.on_non_fatal(None)
}

fn completion(
    snapshot: StateSnapshot,
    parameters: CompletionParams,
) -> Result<Option<CompletionResponse>, LspError> {
    let _span = tracing::info_span!("completion").entered();
    let uri = parameters.text_document_position.text_document.uri;
    let position = parameters.text_document_position.position;

    let mut cache = snapshot.suggestions_cache.write();

    let result = snapshot.with_analyzer_context(|context| {
        analyzer::completion::implementation(context, &mut cache, uri, position)
    });

    result.on_non_fatal(None)
}

fn resolve_completion_item(
    snapshot: StateSnapshot,
    item: CompletionItem,
) -> Result<CompletionItem, LspError> {
    let _span = tracing::info_span!("resolve_completion_item").entered();
    analyzer::completion::resolve::implementation(&snapshot.engine, item)
        .or_else(|(error, item)| Err(error).on_non_fatal(item))
}

fn references(
    snapshot: StateSnapshot,
    parameters: ReferenceParams,
) -> Result<Option<Vec<Location>>, LspError> {
    let _span = tracing::info_span!("references").entered();
    let uri = parameters.text_document_position.text_document.uri;
    let position = parameters.text_document_position.position;

    let result = snapshot.with_analyzer_context(|context| {
        analyzer::references::implementation(context, uri, position)
    });

    result.on_non_fatal(None)
}

fn rename(
    snapshot: StateSnapshot,
    parameters: RenameParams,
) -> Result<Option<WorkspaceEdit>, LspError> {
    let _span = tracing::info_span!("rename").entered();
    let uri = parameters.text_document_position.text_document.uri;
    let position = parameters.text_document_position.position;
    let new_name = parameters.new_name;

    let result = snapshot.with_analyzer_context(|context| {
        analyzer::rename::implementation(context, uri, position, new_name)
    });

    result.on_non_fatal(None)
}

fn prepare_rename(
    snapshot: StateSnapshot,
    parameters: TextDocumentPositionParams,
) -> Result<Option<PrepareRenameResponse>, LspError> {
    let _span = tracing::info_span!("prepare_rename").entered();
    let uri = parameters.text_document.uri;
    let position = parameters.position;

    let result =
        snapshot.with_analyzer_context(|context| analyzer::rename::prepare(context, uri, position));

    result.on_non_fatal(None)
}

fn document_highlight(
    snapshot: StateSnapshot,
    parameters: DocumentHighlightParams,
) -> Result<Option<Vec<DocumentHighlight>>, LspError> {
    let _span = tracing::info_span!("document_highlight").entered();
    let uri = parameters.text_document_position_params.text_document.uri;
    let position = parameters.text_document_position_params.position;
    let result = snapshot.with_analyzer_context(|context| {
        analyzer::document_highlight::implementation(context, uri, position)
    });

    result.on_non_fatal(None)
}

fn workspace_symbols(
    snapshot: StateSnapshot,
    parameters: WorkspaceSymbolParams,
) -> Result<Option<WorkspaceSymbolResponse>, LspError> {
    let _span = tracing::info_span!("workspace_symbols").entered();

    let mut cache = snapshot.workspace_symbols_cache.write();

    let result = snapshot.with_analyzer_context(|context| {
        analyzer::symbols::workspace(context, &mut cache, &parameters.query)
    });

    result.on_non_fatal(None)
}

fn document_symbols(
    snapshot: StateSnapshot,
    parameters: DocumentSymbolParams,
) -> Result<Option<DocumentSymbolResponse>, LspError> {
    let _span = tracing::info_span!("document_symbols").entered();
    let uri = parameters.text_document.uri;
    let result =
        snapshot.with_analyzer_context(|context| analyzer::symbols::document(context, uri));

    result.on_non_fatal(None)
}

fn semantic_tokens(
    snapshot: StateSnapshot,
    parameters: SemanticTokensParams,
) -> Result<Option<SemanticTokensResult>, LspError> {
    let _span = tracing::info_span!("semantic_tokens").entered();
    let uri = parameters.text_document.uri;
    let result = snapshot.with_analyzer_context(|context| {
        analyzer::semantic_tokens::implementation(context, uri)
            .map(|tokens| tokens.map(SemanticTokensResult::Tokens))
    });

    result.on_non_fatal(None)
}

fn document_content(
    workspace: &ReadyWorkspace,
    document: DocumentKind,
    uri: &Url,
) -> Result<Arc<str>, LspError> {
    let uri = DocumentPath::from_uri(uri)?.uri()?;
    let files = workspace.analysis.files.read();
    match document {
        DocumentKind::Source => {
            let file_id = files
                .source_id(uri.as_str())
                .ok_or_else(|| LspError::InvalidContentChange(Url::clone(&uri)))?;
            workspace.analysis.engine.content(file_id).map_err(LspError::from)
        }
        DocumentKind::Foreign(_) => {
            let file_id = files
                .foreign_id(uri.as_str())
                .ok_or_else(|| LspError::InvalidContentChange(Url::clone(&uri)))?;
            workspace
                .analysis
                .engine
                .foreign_content(file_id)
                .ok_or_else(|| LspError::InvalidContentChange(Url::clone(&uri)))
        }
    }
}

fn apply_content_changes(
    uri: &Url,
    content: &str,
    content_changes: &[TextDocumentContentChangeEvent],
    position_encoding: PositionEncoding,
) -> Result<Arc<str>, LspError> {
    let mut content = content.to_string();
    for content_change in content_changes {
        let Some(range) = content_change.range else {
            content = String::clone(&content_change.text);
            continue;
        };

        let positions = analyzer::position::PositionConverter::new(&content, position_encoding);
        let start = positions
            .protocol_position_to_utf8(range.start)
            .and_then(|position| positions.utf8_position_to_offset(position));

        let end = positions
            .protocol_position_to_utf8(range.end)
            .and_then(|position| positions.utf8_position_to_offset(position));

        let (Some(start), Some(end)) = (start, end) else {
            return Err(LspError::InvalidContentChange(Url::clone(uri)));
        };

        let start = usize::from(start);
        let end = usize::from(end);

        if start > end {
            return Err(LspError::InvalidContentChange(Url::clone(uri)));
        }

        content.replace_range(start..end, &content_change.text);
    }
    Ok(Arc::from(content))
}

fn did_change(
    workspace: &mut ReadyWorkspace,
    context: WorkspaceContext<'_>,
    client: &ClientSocket,
    parameters: DidChangeTextDocumentParams,
) -> Result<(), LspError> {
    let uri = &parameters.text_document.uri;
    if parameters.content_changes.is_empty() {
        return Ok(());
    }
    let (document, unit) = source_unit_from_document_uri(uri)?;
    let content = document_content(workspace, document, uri)?;
    let content = apply_content_changes(
        uri,
        &content,
        &parameters.content_changes,
        context.position_encoding,
    )?;
    let event = match document {
        DocumentKind::Foreign(kind) => LifecycleEvent::Foreign {
            unit,
            kind,
            event: ForeignEvent::Changed {
                text: content,
                version: parameters.text_document.version,
            },
        },
        DocumentKind::Source => LifecycleEvent::Source {
            unit,
            event: SourceEvent::Changed {
                text: content,
                version: parameters.text_document.version,
            },
        },
    };
    let trigger = if workspace.configuration.diagnostics.on_change {
        DiagnosticTrigger::AssociatedSource(Url::clone(&parameters.text_document.uri))
    } else {
        DiagnosticTrigger::None
    };
    workspace.apply_lifecycle_events([event], trigger).deliver(client)
}

fn did_open(
    workspace: &mut ReadyWorkspace,
    context: WorkspaceContext<'_>,
    client: &ClientSocket,
    parameters: DidOpenTextDocumentParams,
) -> Result<(), LspError> {
    let uri = &parameters.text_document.uri;
    let (document, unit) = source_unit_from_document_uri(uri)?;

    let mut events = vec![];
    match document {
        DocumentKind::Foreign(kind) => {
            events.push(LifecycleEvent::Foreign {
                unit,
                kind,
                event: ForeignEvent::Opened {
                    text: Arc::from(parameters.text_document.text.as_str()),
                    version: parameters.text_document.version,
                },
            });
        }
        DocumentKind::Source => {
            let metadata = source_metadata(workspace, context.root, &unit, uri);
            events.push(LifecycleEvent::Source {
                unit: SourceUnitKey::clone(&unit),
                event: SourceEvent::Opened {
                    text: Arc::from(parameters.text_document.text.as_str()),
                    version: parameters.text_document.version,
                    metadata,
                },
            });
            events.extend(observe_sibling_foreign(workspace, &unit)?);
        }
    }
    let trigger = if workspace.configuration.diagnostics.on_open {
        DiagnosticTrigger::AssociatedSource(parameters.text_document.uri)
    } else {
        DiagnosticTrigger::None
    };
    workspace.apply_lifecycle_events(events, trigger).deliver(client)
}

fn did_close(
    workspace: &mut ReadyWorkspace,
    _context: WorkspaceContext<'_>,
    client: &ClientSocket,
    parameters: DidCloseTextDocumentParams,
) -> Result<(), LspError> {
    let uri = parameters.text_document.uri;
    let (document, unit) = source_unit_from_document_uri(&uri)?;
    let source_uri = Arc::<str>::from(unit.source());
    let excluded = workspace.excluded_sources.contains(&source_uri);
    let disk = if excluded { DiskObservation::NotFound } else { observe_disk(&uri) };
    let mut events = vec![];
    match document {
        DocumentKind::Foreign(kind) => {
            events.push(LifecycleEvent::Foreign {
                unit,
                kind,
                event: ForeignEvent::Closed { disk },
            });
        }
        DocumentKind::Source => {
            let document = DocumentKey::Source(SourceUnitKey::clone(&unit));
            let was_open = workspace.analysis.files.read().is_open(&document);
            events.push(LifecycleEvent::Source {
                unit: SourceUnitKey::clone(&unit),
                event: SourceEvent::Closed { disk },
            });
            if was_open {
                events.extend(observe_sibling_foreign(workspace, &unit)?);
            }
        }
    }
    workspace.apply_lifecycle_events(events, DiagnosticTrigger::AnalysisChange).deliver(client)
}

fn did_save(
    workspace: &mut ReadyWorkspace,
    client: &ClientSocket,
    parameters: DidSaveTextDocumentParams,
) -> Result<(), LspError> {
    workspace.invalidate_suggestions_cache();

    let effects = if workspace.configuration.diagnostics.on_save {
        WorkspaceEffects::associated(workspace, parameters.text_document.uri)?
    } else {
        WorkspaceEffects::none()
    };
    effects.deliver(client)
}

fn did_change_watched_files(
    workspace: &mut ReadyWorkspace,
    context: WorkspaceContext<'_>,
    client: &ClientSocket,
    parameters: DidChangeWatchedFilesParams,
) -> Result<(), LspError> {
    let mut source_units = FxHashSet::default();
    let mut foreign_units = FxHashSet::default();
    for change in parameters.changes {
        match document_kind(&change.uri) {
            Some(DocumentKind::Foreign(kind)) => {
                let unit = source_unit_from_foreign_uri(&change.uri)?;
                if workspace.excluded_sources.contains(unit.source()) {
                    continue;
                }
                foreign_units.insert((unit, kind));
            }
            Some(DocumentKind::Source) => {
                let unit = source_unit_from_source_uri(&change.uri)?;
                if workspace.excluded_sources.contains(unit.source()) {
                    continue;
                }
                source_units.insert(unit);
            }
            None => {}
        }
    }

    let mut events = vec![];
    let mut observed_foreign = FxHashSet::default();
    for unit in source_units {
        let document = DocumentKey::Source(SourceUnitKey::clone(&unit));
        if workspace.analysis.files.read().is_open(&document) {
            continue;
        }
        let uri = Url::parse(unit.source())?;
        if !source_editable(workspace, context.root, &unit, &uri) {
            continue;
        }
        let disk = observe_disk(&uri);
        let source_found = matches!(disk, DiskObservation::Found(_));
        let metadata = source_metadata(workspace, context.root, &unit, &uri);
        let event = LifecycleEvent::Source {
            unit: SourceUnitKey::clone(&unit),
            event: SourceEvent::DiskObserved { disk, metadata },
        };
        events.push(event);
        if source_found {
            events.extend(observe_sibling_foreign(workspace, &unit)?);
            observed_foreign.insert(unit);
        }
    }

    for (unit, kind) in foreign_units {
        if observed_foreign.contains(&unit) {
            continue;
        }
        let document = DocumentKey::Foreign(SourceUnitKey::clone(&unit), kind);
        if workspace.analysis.files.read().is_open(&document) {
            continue;
        }
        let source_uri = Url::parse(unit.source())?;
        if !source_editable(workspace, context.root, &unit, &source_uri) {
            continue;
        }
        let tracked = {
            let files = workspace.analysis.files.read();
            files.source_id(unit.source()).is_some()
                || files.foreign_id(unit.foreign_for(kind)).is_some()
        };
        if !tracked {
            continue;
        }
        let uri = Url::parse(unit.foreign_for(kind))?;
        let event = LifecycleEvent::Foreign {
            unit,
            kind,
            event: ForeignEvent::DiskObserved { disk: observe_disk(&uri) },
        };
        events.push(event);
    }

    workspace.apply_lifecycle_events(events, DiagnosticTrigger::AnalysisChange).deliver(client)
}

fn document_kind(uri: &Url) -> Option<DocumentKind> {
    let uri = DocumentPath::from_uri(uri).ok()?.uri().ok()?;
    if uri.path().ends_with(".js") {
        Some(DocumentKind::Foreign(ForeignSourceKind::JavaScript))
    } else if uri.path().ends_with(".jsx") {
        Some(DocumentKind::Foreign(ForeignSourceKind::Jsx))
    } else if uri.path().ends_with(".purs") {
        Some(DocumentKind::Source)
    } else {
        None
    }
}

fn source_unit_from_document_uri(uri: &Url) -> Result<(DocumentKind, SourceUnitKey), LspError> {
    let document =
        document_kind(uri).ok_or_else(|| LspError::UnsupportedDocumentUri(Url::clone(uri)))?;
    let unit = match document {
        DocumentKind::Source => source_unit_from_source_uri(uri)?,
        DocumentKind::Foreign(_) => source_unit_from_foreign_uri(uri)?,
    };
    Ok((document, unit))
}

fn file_uri_with_extension(uri: &Url, extension: &str) -> Result<Url, LspError> {
    DocumentPath::from_uri(uri)?.with_extension(extension).uri()
}

fn source_unit_from_source_uri(source_uri: &Url) -> Result<SourceUnitKey, LspError> {
    let source = DocumentPath::from_uri(source_uri)?;
    let source_uri = source.uri()?;
    let javascript_uri = source.with_extension("js").uri()?;
    let jsx_uri = source.with_extension("jsx").uri()?;
    Ok(SourceUnitKey::with_foreign_sources(
        source_uri.as_str(),
        javascript_uri.as_str(),
        jsx_uri.as_str(),
    ))
}

fn source_unit_from_foreign_uri(foreign_uri: &Url) -> Result<SourceUnitKey, LspError> {
    let source_uri = file_uri_with_extension(foreign_uri, "purs")?;
    source_unit_from_source_uri(&source_uri)
}

fn observe_sibling_foreign(
    workspace: &ReadyWorkspace,
    unit: &SourceUnitKey,
) -> Result<Vec<LifecycleEvent<i32, SourceMetadata>>, LspError> {
    let mut events = vec![];
    for kind in ForeignSourceKind::ALL {
        let document = DocumentKey::Foreign(SourceUnitKey::clone(unit), kind);
        if workspace.analysis.files.read().is_open(&document) {
            continue;
        }
        let uri = Url::parse(unit.foreign_for(kind))?;
        events.push(LifecycleEvent::Foreign {
            unit: SourceUnitKey::clone(unit),
            kind,
            event: ForeignEvent::DiskObserved { disk: observe_disk(&uri) },
        });
    }
    Ok(events)
}

fn observe_disk(uri: &Url) -> DiskObservation {
    let path = uri.to_file_path().expect("invariant violated: expected a valid file URI");
    match fs::read_to_string(path) {
        Ok(content) => DiskObservation::Found(Arc::from(content)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => DiskObservation::NotFound,
        Err(error) => {
            let kind = error.kind();
            DiskObservation::Failed(ReloadFailure::new(kind, error.to_string()))
        }
    }
}

fn source_metadata(
    workspace: &ReadyWorkspace,
    root: Option<&std::path::Path>,
    unit: &SourceUnitKey,
    uri: &Url,
) -> SourceMetadata {
    let previous = {
        let files = workspace.analysis.files.read();
        let file_id = files.source_id(unit.source());
        file_id.and_then(|file_id| files.source_metadata(file_id)).cloned()
    };
    previous.unwrap_or_else(|| {
        let path = uri.to_file_path().ok();
        let package_metadata = path.as_ref().and_then(|path| {
            workspace
                .source_roots
                .iter()
                .find(|source_root| path.starts_with(&source_root.path))
                .map(|source_root| SourceMetadata::clone(&source_root.metadata))
        });
        package_metadata.unwrap_or_else(|| match (root, path) {
            (Some(root), Some(path)) => {
                SourceMetadata::Unmanaged { editable: path.starts_with(root) }
            }
            (Some(_), None) => SourceMetadata::Unmanaged { editable: false },
            (None, _) => SourceMetadata::Unmanaged { editable: true },
        })
    })
}

fn source_editable(
    workspace: &ReadyWorkspace,
    root: Option<&std::path::Path>,
    unit: &SourceUnitKey,
    uri: &Url,
) -> bool {
    source_metadata(workspace, root, unit, uri).editable()
}

trait RequestExtension: BorrowMut<Router<State>> {
    fn request_snapshot<R: Request>(
        &mut self,
        action: impl Fn(StateSnapshot, R::Params) -> Result<R::Result, LspError> + Send + Copy + 'static,
    ) -> &mut Self {
        self.borrow_mut().request::<R, _>(move |state, parameters| {
            let task = state.spawn(move |snapshot| action(snapshot, parameters));
            async move {
                let task = task.map_err(response_error)?;
                task.await
                    .map_err(LspError::JoinError)
                    .flatten()
                    .map_err(|error| response_error(error))
            }
        });
        self
    }

    fn notification_ext<N: Notification>(
        &mut self,
        action: impl Fn(&mut State, N::Params) -> Result<(), LspError> + Send + Copy + 'static,
    ) -> &mut Self {
        let this: &mut Router<State> = self.borrow_mut();
        this.notification::<N>(move |state, parameters| {
            let _ = action(state, parameters).inspect_err(|error| error.emit_trace());
            ControlFlow::Continue(())
        });
        self
    }

    fn workspace_notification<N: Notification>(
        &mut self,
        notification: fn(N::Params) -> WorkspaceNotification,
    ) -> &mut Self {
        let this: &mut Router<State> = self.borrow_mut();
        this.notification::<N>(move |state, parameters| {
            if state.stopped {
                return ControlFlow::Continue(());
            }
            let notification = notification(parameters);
            if state.workspace.is_ready()
                && !matches!(notification, WorkspaceNotification::DidChange(_))
                && let Some(preparation) = &mut state.preparation
            {
                preparation.dirty = true;
                preparation.cancel();
            }
            let context = WorkspaceContext {
                root: state.protocol.root.as_deref(),
                position_encoding: state.protocol.position_encoding,
            };
            let result = state.workspace.dispatch(notification, context, &state.client);
            let _ = result.inspect_err(|error| error.emit_trace());
            ControlFlow::Continue(())
        });
        self
    }
    fn event_ext<E>(
        &mut self,
        action: impl Fn(&mut State, E) -> Result<(), LspError> + Send + Copy + 'static,
    ) -> &mut Self
    where
        E: Send + 'static,
    {
        let this: &mut Router<State> = self.borrow_mut();
        this.event::<E>(move |state, event| {
            let _ = action(state, event).inspect_err(|error| error.emit_trace());
            ControlFlow::Continue(())
        });
        self
    }
}

impl RequestExtension for Router<State> {}

fn response_error(error: LspError) -> ResponseError {
    error.emit_trace();
    ResponseError::new(error.code(), error.message())
}

pub(crate) async fn async_start(config: ServerConfig) -> Result<(), ServerError> {
    let ServerConfig { configuration, name, version } = config;
    let config = Arc::new(configuration);
    let tasks = TaskTracker::new();
    let owned_tasks = TaskTracker::clone(&tasks);
    let (server, _) = async_lsp::MainLoop::new_server(move |client| {
        let client_socket = ClientSocket::clone(&client);
        let mut state = State::new(
            Arc::clone(&config),
            client_socket,
            String::clone(&name),
            String::clone(&version),
        );
        state.tasks = owned_tasks;
        let mut router: Router<State, ResponseError> = Router::new(state);

        router
            .request::<extension::CustomInitialize, _>(initialize)
            .request::<request::Shutdown, _>(shutdown)
            .request_snapshot::<request::GotoDefinition>(definition)
            .request_snapshot::<request::HoverRequest>(hover)
            .request_snapshot::<request::CodeActionRequest>(code_action)
            .request_snapshot::<request::Completion>(completion)
            .request_snapshot::<request::ResolveCompletionItem>(resolve_completion_item)
            .request_snapshot::<request::References>(references)
            .request_snapshot::<request::PrepareRenameRequest>(prepare_rename)
            .request_snapshot::<request::Rename>(rename)
            .request_snapshot::<request::DocumentHighlightRequest>(document_highlight)
            .request_snapshot::<request::WorkspaceSymbolRequest>(workspace_symbols)
            .request_snapshot::<request::DocumentSymbolRequest>(document_symbols)
            .request_snapshot::<request::SemanticTokensFullRequest>(semantic_tokens)
            .notification_ext::<notification::Initialized>(initialized)
            .notification_ext::<notification::Exit>(exit)
            .workspace_notification::<notification::DidOpenTextDocument>(
                WorkspaceNotification::DidOpen,
            )
            .workspace_notification::<notification::DidSaveTextDocument>(
                WorkspaceNotification::DidSave,
            )
            .workspace_notification::<notification::DidCloseTextDocument>(
                WorkspaceNotification::DidClose,
            )
            .notification_ext::<notification::DidChangeConfiguration>(did_change_configuration)
            .workspace_notification::<notification::DidChangeTextDocument>(
                WorkspaceNotification::DidChange,
            )
            .workspace_notification::<notification::DidChangeWatchedFiles>(
                WorkspaceNotification::DidChangeWatchedFiles,
            )
            .event_ext::<event::CollectDiagnostics>(collect_diagnostics)
            .event_ext::<event::DiagnosticsFinished>(finish_diagnostics)
            .event_ext::<PreparationFinished>(finish_preparation)
            .event_ext::<ConfigurationReceived>(finish_workspace_configuration);

        ServiceBuilder::new()
            .layer(LifecycleLayer::default())
            .layer(CatchUnwindLayer::default())
            .layer(ConcurrencyLayer::default())
            .layer(ClientProcessMonitorLayer::new(client))
            .service(router)
    });

    #[cfg(unix)]
    let (stdin, stdout) = (
        async_lsp::stdio::PipeStdin::lock_tokio().map_err(ServerError::new)?,
        async_lsp::stdio::PipeStdout::lock_tokio().map_err(ServerError::new)?,
    );

    #[cfg(not(unix))]
    let (stdin, stdout) = (
        tokio_util::compat::TokioAsyncReadCompatExt::compat(tokio::io::stdin()),
        tokio_util::compat::TokioAsyncWriteCompatExt::compat_write(tokio::io::stdout()),
    );

    let result = server.run_buffered(stdin, stdout).await.map_err(ServerError::new);
    tasks.close();
    tasks.wait().await;
    result
}
