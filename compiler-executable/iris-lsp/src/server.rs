mod analysis;
mod cancellation;
mod preparation;
mod process;
mod workspace;

pub mod capabilities;
pub mod error;
pub mod event;

#[cfg(test)]
mod tests;

use std::borrow::BorrowMut;
use std::collections::BTreeMap;
use std::ops::ControlFlow;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::available_parallelism;
use std::{env, fs, io};

use async_lsp::client_monitor::ClientProcessMonitorLayer;
use async_lsp::panic::CatchUnwindLayer;
use async_lsp::router::Router;
use async_lsp::server::LifecycleLayer;
use async_lsp::{ClientSocket, LanguageClient, ResponseError};
use building::lifecycle::{
    DiskObservation, DocumentKey, DocumentKind, ForeignEvent, LifecycleEvent, ReloadFailure,
    SourceEvent, SourceUnitKey,
};
use files::ForeignSourceKind;
use iris_analysis::AnalyzerCapabilities;
use iris_analysis::position::PositionEncoding;
use iris_build::compile::{InitialBuildConfig, PackageExecution, build_initial};
use iris_build::events::BuildEventSink;
use iris_build::plan::PackageInput;
use iris_configuration::{Configuration, ConfigurationSettings};
use itertools::Itertools;
use lsp_types::notification::Notification;
use lsp_types::request::Request;
use lsp_types::*;
use path_absolutize::Absolutize;
use rustc_hash::FxHashSet;
use smol_str::SmolStr;
use tokio::sync::{Semaphore, oneshot};
use tokio::task;
use tower::ServiceBuilder;

use crate::server::analysis::StateSnapshot;
use crate::server::cancellation::CancellationLayer;
use crate::server::capabilities::{
    ConfigurationCapabilities, negotiate_analyzer_capabilities,
    negotiate_configuration_capabilities, negotiate_position_encoding,
};
use crate::server::error::{AnalyzerResultExt, LspError};
use crate::server::preparation::{
    CompletionDisposition, Preparation, PreparationFinished, PreparationTicket,
};
use crate::server::workspace::{
    ConfigurationApplyError, DiagnosticTrigger, PreparedInitialWorkspace, ReadyWorkspace,
    SourceRoot, WorkspaceContext, WorkspaceEffects, WorkspaceNotification, WorkspaceRuntime,
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
    default_configuration: Arc<Configuration>,
    root: Option<PathBuf>,
    configuration_scope: Option<Url>,
    configuration_capabilities: ConfigurationCapabilities,
    configuration_generation: u64,
    configuration_initialized: bool,
    position_encoding: PositionEncoding,
    analyzer_capabilities: AnalyzerCapabilities,
    watched_files_dynamic_registration: bool,
    work_done_progress: bool,
    shutting_down: bool,
}

pub struct State {
    client: ClientSocket,
    identity: ServerIdentity,
    protocol: ProtocolSession,
    workspace: WorkspaceRuntime,
    preparation: Arc<Preparation>,
    analysis_requests: Arc<Semaphore>,
}

enum SnapshotReadiness {
    Ready(StateSnapshot),
    Waiting(PreparationTicket),
}

struct AcquireSnapshot {
    response: oneshot::Sender<Result<StateSnapshot, LspError>>,
}

impl State {
    fn new(
        config: Arc<Configuration>,
        client: ClientSocket,
        name: String,
        version: String,
        preparation: Arc<Preparation>,
    ) -> State {
        State {
            client,
            identity: ServerIdentity { name, version },
            protocol: ProtocolSession {
                default_configuration: Arc::clone(&config),
                root: None,
                configuration_scope: None,
                configuration_capabilities: ConfigurationCapabilities::default(),
                configuration_generation: 0,
                configuration_initialized: false,
                position_encoding: PositionEncoding::Utf16,
                analyzer_capabilities: AnalyzerCapabilities::default(),
                watched_files_dynamic_registration: false,
                work_done_progress: false,
                shutting_down: false,
            },
            workspace: WorkspaceRuntime::new(config),
            preparation,
            analysis_requests: Arc::new(Semaphore::new(
                available_parallelism().map_or(1, std::num::NonZero::get),
            )),
        }
    }

    fn snapshot_readiness(&mut self) -> Result<SnapshotReadiness, LspError> {
        if self.protocol.shutting_down {
            return Err(LspError::WorkspaceNotReady);
        }
        match self.snapshot() {
            Ok(snapshot) => Ok(SnapshotReadiness::Ready(snapshot)),
            Err(LspError::WorkspaceNotReady) => self
                .preparation
                .initial_ticket()
                .or_else(|| self.preparation.demand_retry())
                .map(SnapshotReadiness::Waiting)
                .ok_or(LspError::WorkspaceNotReady),
            Err(error) => Err(error),
        }
    }

    fn snapshot(&self) -> Result<StateSnapshot, LspError> {
        if self.protocol.shutting_down {
            return Err(LspError::WorkspaceNotReady);
        }
        self.workspace
            .snapshot(self.protocol.position_encoding, self.protocol.analyzer_capabilities)
    }

    fn spawn<T>(
        &self,
        action: impl FnOnce(StateSnapshot) -> T + Send + 'static,
    ) -> Result<task::JoinHandle<T>, LspError>
    where
        T: Send + 'static,
    {
        let snapshot = self.snapshot()?;
        Ok(task::spawn_blocking(move || action(snapshot)))
    }
}

fn initialize(
    state: &mut State,
    parameters: InitializeParams,
) -> impl Future<Output = Result<InitializeResult, ResponseError>> + use<> {
    let position_encoding = negotiate_position_encoding(&parameters);
    state.protocol.position_encoding = position_encoding;
    state.protocol.analyzer_capabilities = negotiate_analyzer_capabilities(&parameters);
    state.protocol.configuration_capabilities = negotiate_configuration_capabilities(&parameters);
    state.protocol.watched_files_dynamic_registration =
        watched_files_dynamic_registration(&parameters.capabilities);
    state.protocol.work_done_progress = parameters
        .capabilities
        .window
        .as_ref()
        .and_then(|window| window.work_done_progress)
        .unwrap_or(false);

    state.protocol.configuration_scope = parameters
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
                                token_types: iris_analysis::semantic_tokens::TOKEN_TYPES.to_vec(),
                                token_modifiers: iris_analysis::semantic_tokens::TOKEN_MODIFIERS
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
    state.protocol.shutting_down = true;
    state.preparation.cancel();
    async { Ok(()) }
}

fn work_done_progress_cancel(
    state: &mut State,
    parameters: WorkDoneProgressCancelParams,
) -> Result<(), LspError> {
    state.preparation.cancel_progress(&parameters.token);
    Ok(())
}

fn initialized(state: &mut State, _: InitializedParams) -> Result<(), LspError> {
    let _span = tracing::info_span!("initialization").entered();
    register_file_watcher(state);
    register_configuration_changes(state);

    if state.protocol.configuration_capabilities.workspace_configuration {
        request_workspace_configuration(state);
        Ok(())
    } else {
        apply_configuration(state, Arc::clone(&state.protocol.default_configuration))
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
    task::spawn(async move {
        if let Err(error) = client.register_capability(parameters).await {
            tracing::warn!("Failed to register workspace configuration changes: {error}");
        }
    });
}

struct ConfigurationReceived {
    generation: u64,
    result: Result<Vec<serde_json::Value>, String>,
}

fn request_workspace_configuration(state: &mut State) {
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
    task::spawn(async move {
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            client.configuration(parameters),
        )
        .await
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
    if generation != state.protocol.configuration_generation {
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
                    settings.unwrap_or_default().apply_to(&state.protocol.default_configuration)
                })
                .map_err(|error| format!("Invalid Iris settings: {error}"))
        });

    match configuration {
        Ok(configuration) => {
            if let Err(error) = apply_configuration_inner(state, Arc::new(configuration)) {
                let ConfigurationApplyError::Apply(error) = error;
                let error = format!("Failed to apply Iris settings: {error}");
                report_configuration_error(state, &error);
                if !state.protocol.configuration_initialized {
                    apply_configuration(state, Arc::clone(&state.protocol.default_configuration))?;
                    state.protocol.configuration_initialized = true;
                }
            } else {
                state.protocol.configuration_initialized = true;
            }
            Ok(())
        }
        Err(error) => {
            report_configuration_error(state, &error);
            if state.protocol.configuration_initialized {
                Ok(())
            } else {
                apply_configuration(state, Arc::clone(&state.protocol.default_configuration))?;
                state.protocol.configuration_initialized = true;
                Ok(())
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
    let message = if state.protocol.configuration_initialized {
        format!("{error}. The previous Iris settings remain active.")
    } else {
        format!("{error}. Iris will use its default settings.")
    };
    if let Err(error) =
        state.client.show_message(ShowMessageParams { typ: MessageType::ERROR, message })
    {
        tracing::warn!("Failed to report configuration error: {error}");
    }
}

fn register_file_watcher(state: &State) {
    if !state.protocol.watched_files_dynamic_registration {
        return;
    }

    let parameters = file_watcher_registration();
    let mut client = ClientSocket::clone(&state.client);
    task::spawn(async move {
        if let Err(error) = client.register_capability(parameters).await {
            tracing::warn!("Failed to register source file watcher: {error}");
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

fn exit(_state: &mut State, (): ()) -> Result<(), LspError> {
    Ok(())
}

struct DiscoveredWorkspace {
    root: PathBuf,
    source_globs: Vec<PathBuf>,
    packages: Vec<PackageInput>,
    metadata: BTreeMap<PathBuf, SourceMetadata>,
    source_roots: Vec<SourceRoot>,
}

fn discover_workspace(
    workspace: &iris_build::Workspace,
    client_root: &std::path::Path,
) -> Result<DiscoveredWorkspace, LspError> {
    let discovered = iris_build::discover_packages(workspace)?;

    let packages = discovered.packages.iter().map(|package| PackageInput {
        name: SmolStr::clone(&package.name),
        source_identities: Vec::clone(&package.files),
        dependencies: Vec::clone(&package.dependencies),
    });

    let packages = packages.collect_vec();

    let metadata = discovered.packages.iter().flat_map(|package| {
        let metadata = SourceMetadata::Package { editable: package.editable };
        package
            .files
            .iter()
            .map(move |file| (PathBuf::clone(file), SourceMetadata::clone(&metadata)))
    });

    let metadata = metadata.collect::<BTreeMap<_, _>>();

    let source_roots = discovered
        .packages
        .iter()
        .map(|package| package_source_roots(&workspace.root, client_root, package));
    let source_root_groups =
        source_roots.process_results(|source_roots| source_roots.collect_vec())?;
    let mut source_roots = source_root_groups.into_iter().flatten().collect_vec();
    source_roots
        .sort_by_key(|source_root| std::cmp::Reverse(source_root.path.components().count()));

    Ok(DiscoveredWorkspace {
        root: PathBuf::clone(&workspace.root),
        source_globs: discovered.source_globs,
        packages,
        metadata,
        source_roots,
    })
}

/// Builds the initial compilation for a discovered workspace.
///
/// This is the blocking half of startup preparation: it maps discovered
/// packages to the existing `iris-build` inputs and runs the same initial
/// build the server has always used. It must run on a blocking thread.
fn build_prepared_workspace(
    workspace: iris_build::Workspace,
    client_root: PathBuf,
    events: &dyn BuildEventSink,
) -> Result<PreparedInitialWorkspace, LspError> {
    let discovered = discover_workspace(&workspace, &client_root)?;
    let DiscoveredWorkspace { root, source_globs, packages, metadata, source_roots } = discovered;

    let initial = build_initial::<i32, SourceMetadata, _>(InitialBuildConfig {
        root: &root,
        source_globs: &source_globs,
        excluded: &[],
        packages,
        prim_metadata: SourceMetadata::Builtin,
        source_metadata: |path: &std::path::Path| {
            metadata
                .get(path)
                .cloned()
                .expect("invariant violated: discovered source has no LSP metadata")
        },
        execution: PackageExecution::Parallel,
        events,
    })?;

    tracing::info!("Loaded {} files.", metadata.len());
    Ok(PreparedInitialWorkspace { compilation: initial.into_compilation(), source_roots })
}

fn package_source_roots(
    workspace_root: &std::path::Path,
    client_root: &std::path::Path,
    package: &iris_build::DiscoveredPackage,
) -> io::Result<Vec<SourceRoot>> {
    let metadata = SourceMetadata::Package { editable: package.editable };
    let canonical_client_root = dunce::canonicalize(client_root).ok();

    let mut roots = vec![];
    for root in &package.roots {
        let root = workspace_root.join(root).absolutize()?.to_path_buf();
        roots.push(SourceRoot {
            path: PathBuf::clone(&root),
            metadata: SourceMetadata::clone(&metadata),
        });

        let canonical = dunce::canonicalize(&root).ok();
        if let Some(canonical) = &canonical
            && *canonical != root
        {
            roots.push(SourceRoot {
                path: PathBuf::clone(canonical),
                metadata: SourceMetadata::clone(&metadata),
            });
        }

        if let Some(client_root_canonical) = &canonical_client_root
            && let Some(canonical) = &canonical
            && let Ok(relative) = canonical.strip_prefix(client_root_canonical)
        {
            let alias = client_root.join(relative).absolutize()?.to_path_buf();
            if roots.iter().all(|root| root.path != alias) {
                roots.push(SourceRoot { path: alias, metadata: SourceMetadata::clone(&metadata) });
            }
        }
    }

    Ok(roots)
}

fn apply_configuration(
    state: &mut State,
    configuration: Arc<Configuration>,
) -> Result<(), LspError> {
    apply_configuration_inner(state, configuration).map_err(|error| {
        let ConfigurationApplyError::Apply(error) = error;
        error
    })
}

fn apply_configuration_inner(
    state: &mut State,
    configuration: Arc<Configuration>,
) -> Result<(), ConfigurationApplyError> {
    if state.workspace.update_configuration(Arc::clone(&configuration)) {
        return Ok(());
    }

    let root = state
        .protocol
        .root
        .as_deref()
        .ok_or(LspError::MissingRoot)
        .map_err(ConfigurationApplyError::Apply)?;

    state.workspace.stage_configuration(configuration);
    let started = state
        .preparation
        .start(
            root.to_path_buf(),
            ClientSocket::clone(&state.client),
            state.protocol.work_done_progress,
        )
        .is_some();
    if started {
        tracing::info!("Preparing the Spago workspace at {}.", root.display());
    }
    Ok(())
}

fn finish_workspace_preparation(
    state: &mut State,
    PreparationFinished { generation, result }: PreparationFinished,
) -> Result<(), LspError> {
    match state.preparation.finish_disposition(generation) {
        CompletionDisposition::Apply => {}
        CompletionDisposition::Discarded | CompletionDisposition::Stale => return Ok(()),
    }

    match result {
        Ok(prepared) => {
            let pending = match state.workspace.install(prepared) {
                Ok(pending) => pending,
                Err(error) => {
                    report_preparation_error(state, &error);
                    state.workspace.fail();
                    state.preparation.finish_failure(generation, "Workspace preparation failed");
                    return Ok(());
                }
            };
            for notification in pending {
                let context = WorkspaceContext {
                    root: state.protocol.root.as_deref(),
                    position_encoding: state.protocol.position_encoding,
                };
                if let Err(error) = state.workspace.dispatch(notification, context, &state.client) {
                    error.emit_trace();
                }
            }
            state.preparation.finish_success(generation, "Workspace preparation finished");
            Ok(())
        }
        Err(error) => {
            report_preparation_error(state, &error);
            state.workspace.fail();
            state.preparation.finish_failure(generation, "Workspace preparation failed");
            Ok(())
        }
    }
}

fn acquire_snapshot(state: &mut State, event: AcquireSnapshot) -> Result<(), LspError> {
    let _ = event.response.send(state.snapshot());
    Ok(())
}

fn report_preparation_error(state: &mut State, error: &LspError) {
    tracing::error!("Failed to prepare the Iris workspace: {error}");
    let root = state
        .protocol
        .root
        .as_deref()
        .map(|root| root.display().to_string())
        .unwrap_or_else(|| "the workspace root".to_string());
    let message = format!(
        "Iris could not prepare the Spago workspace at {root}: {error}. \
         Correct the project (for example, by running `spago fetch`) and restart Iris."
    );
    if let Err(error) =
        state.client.show_message(ShowMessageParams { typ: MessageType::ERROR, message })
    {
        tracing::warn!("Failed to report preparation error: {error}");
    }
}

fn definition(
    snapshot: StateSnapshot,
    parameters: GotoDefinitionParams,
) -> Result<Option<GotoDefinitionResponse>, LspError> {
    let _span = tracing::info_span!("definition").entered();
    let uri = parameters.text_document_position_params.text_document.uri;
    let position = parameters.text_document_position_params.position;

    let result = snapshot.with_analyzer_context(|context| {
        iris_analysis::definition::implementation(context, uri, position)
    });

    result.on_non_fatal(None)
}

fn hover(snapshot: StateSnapshot, parameters: HoverParams) -> Result<Option<Hover>, LspError> {
    let _span = tracing::info_span!("hover").entered();
    let uri = parameters.text_document_position_params.text_document.uri;
    let position = parameters.text_document_position_params.position;

    let result = snapshot.with_analyzer_context(|context| {
        iris_analysis::hover::implementation(context, uri, position)
    });

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
        iris_analysis::code_action::implementation(context, uri, range, action_context)
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
        iris_analysis::completion::implementation(context, &mut cache, uri, position)
    });

    result.on_non_fatal(None)
}

fn resolve_completion_item(
    snapshot: StateSnapshot,
    item: CompletionItem,
) -> Result<CompletionItem, LspError> {
    let _span = tracing::info_span!("resolve_completion_item").entered();
    iris_analysis::completion::resolve::implementation(&snapshot.engine, item)
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
        iris_analysis::references::implementation(context, uri, position)
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
        iris_analysis::rename::implementation(context, uri, position, new_name)
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

    let result = snapshot
        .with_analyzer_context(|context| iris_analysis::rename::prepare(context, uri, position));

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
        iris_analysis::document_highlight::implementation(context, uri, position)
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
        iris_analysis::symbols::workspace(context, &mut cache, &parameters.query)
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
        snapshot.with_analyzer_context(|context| iris_analysis::symbols::document(context, uri));

    result.on_non_fatal(None)
}

fn semantic_tokens(
    snapshot: StateSnapshot,
    parameters: SemanticTokensParams,
) -> Result<Option<SemanticTokensResult>, LspError> {
    let _span = tracing::info_span!("semantic_tokens").entered();
    let uri = parameters.text_document.uri;
    let result = snapshot.with_analyzer_context(|context| {
        iris_analysis::semantic_tokens::implementation(context, uri)
            .map(|tokens| tokens.map(SemanticTokensResult::Tokens))
    });

    result.on_non_fatal(None)
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

        let positions =
            iris_analysis::position::PositionConverter::new(&content, position_encoding);
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
    let content = workspace.analysis.document_content(document, uri)?;
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
    let disk = observe_disk(&uri);
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
    workspace.analysis.invalidate_suggestions_cache();

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
                foreign_units.insert((unit, kind));
            }
            Some(DocumentKind::Source) => {
                let unit = source_unit_from_source_uri(&change.uri)?;
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
    if uri.scheme() != "file" || uri.to_file_path().is_err() {
        return Err(LspError::InvalidFileUri(Url::clone(uri)));
    }
    let uri_path = uri.path();
    let file_name_start = uri_path.rfind('/').map_or(0, |index| index + 1);
    let extension_start = uri_path[file_name_start..]
        .rfind('.')
        .filter(|index| *index > 0)
        .map_or(uri_path.len(), |index| file_name_start + index);
    let mut sibling_path = String::from(&uri_path[..extension_start]);
    sibling_path.push('.');
    sibling_path.push_str(extension);

    let mut sibling_uri = Url::clone(uri);
    sibling_uri.set_path(&sibling_path);
    Ok(sibling_uri)
}

fn source_unit_from_source_uri(source_uri: &Url) -> Result<SourceUnitKey, LspError> {
    let javascript_uri = file_uri_with_extension(source_uri, "js")?;
    let jsx_uri = file_uri_with_extension(source_uri, "jsx")?;
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
            let readiness = state.snapshot_readiness();
            let requests = Arc::clone(&state.analysis_requests);
            let client = ClientSocket::clone(&state.client);
            async move {
                let snapshot = match readiness.map_err(response_error)? {
                    SnapshotReadiness::Ready(snapshot) => snapshot,
                    SnapshotReadiness::Waiting(ticket) => {
                        ticket.wait().await.map_err(response_error)?;
                        let (response, snapshot) = oneshot::channel();
                        client
                            .emit(AcquireSnapshot { response })
                            .map_err(LspError::from)
                            .map_err(response_error)?;
                        snapshot
                            .await
                            .map_err(|_| LspError::WorkspaceNotReady)
                            .map_err(response_error)?
                            .map_err(response_error)?
                    }
                };
                let _permit = requests
                    .acquire_owned()
                    .await
                    .expect("invariant violated: analysis request semaphore was closed");
                let task = task::spawn_blocking(move || action(snapshot, parameters));
                task.await.map_err(LspError::JoinError).flatten().map_err(response_error)
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
            let context = WorkspaceContext {
                root: state.protocol.root.as_deref(),
                position_encoding: state.protocol.position_encoding,
            };
            let result = state.workspace.dispatch(notification(parameters), context, &state.client);
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
    let ServerConfig { name, version } = config;
    let config = Arc::new(Configuration::default());
    let preparation = Arc::new(Preparation::new());
    let preparation_for_state = Arc::clone(&preparation);
    let (server, _) = async_lsp::MainLoop::new_server(move |client| {
        let client_socket = ClientSocket::clone(&client);
        let mut router: Router<State, ResponseError> = Router::new(State::new(
            Arc::clone(&config),
            client_socket,
            String::clone(&name),
            String::clone(&version),
            Arc::clone(&preparation_for_state),
        ));

        router
            .request::<request::Initialize, _>(initialize)
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
            .notification_ext::<notification::WorkDoneProgressCancel>(work_done_progress_cancel)
            .workspace_notification::<notification::DidOpenTextDocument>(
                WorkspaceNotification::Open,
            )
            .workspace_notification::<notification::DidSaveTextDocument>(
                WorkspaceNotification::Save,
            )
            .workspace_notification::<notification::DidCloseTextDocument>(
                WorkspaceNotification::Close,
            )
            .notification_ext::<notification::DidChangeConfiguration>(did_change_configuration)
            .workspace_notification::<notification::DidChangeTextDocument>(
                WorkspaceNotification::Change,
            )
            .workspace_notification::<notification::DidChangeWatchedFiles>(
                WorkspaceNotification::ChangeWatchedFiles,
            )
            .event_ext::<event::CollectDiagnostics>(event::collect_diagnostics)
            .event_ext::<event::DiagnosticsFinished>(event::finish_diagnostics)
            .event_ext::<ConfigurationReceived>(finish_workspace_configuration)
            .event_ext::<PreparationFinished>(finish_workspace_preparation)
            .event_ext::<AcquireSnapshot>(acquire_snapshot);

        ServiceBuilder::new()
            .layer(LifecycleLayer::default())
            .layer(CatchUnwindLayer::default())
            .layer(CancellationLayer)
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

    let result = server.run_buffered(stdin, stdout).await;
    preparation.shutdown().await;
    result.map_err(ServerError::new)
}
