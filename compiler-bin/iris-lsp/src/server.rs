pub mod capabilities;
pub mod error;
pub mod event;
pub mod extension;

#[cfg(test)]
mod tests;

use std::borrow::BorrowMut;
use std::collections::BTreeMap;
use std::ops::ControlFlow;
use std::path::PathBuf;
use std::sync::Arc;
use std::{env, fs, io, mem, process};

use analyzer::completion::SuggestionsCache;
use analyzer::position::PositionEncoding;
use analyzer::symbols::WorkspaceSymbolsCache;
use analyzer::{AnalyzerCapabilities, AnalyzerContext, AnalyzerHost};
use async_lsp::client_monitor::ClientProcessMonitorLayer;
use async_lsp::concurrency::ConcurrencyLayer;
use async_lsp::panic::CatchUnwindLayer;
use async_lsp::router::Router;
use async_lsp::server::LifecycleLayer;
use async_lsp::{ClientSocket, LanguageClient, ResponseError};
use building::QueryEngine;
use building::lifecycle::{
    AnalysisInvalidation, DiskObservation, DocumentKey, DocumentKind, FileLifecycle, ForeignEvent,
    LifecycleChange, LifecycleEvent, ReloadFailure, SourceEvent, SourceUnitKey,
};
use configuration::{Configuration, ConfigurationSettings, SourceDiscovery};
use files::{FileId, ForeignSourceKind};
use iris_build::compilation::{CompilationParts, CompilationState, MaterializedPrim};
use iris_build::compile::{InitialBuildConfig, PackageExecution, build_initial};
use iris_build::events::SilentBuildEvents;
use iris_build::plan::PackageInput;
use itertools::Itertools;
use lsp_types::notification::Notification;
use lsp_types::request::Request;
use lsp_types::*;
use parking_lot::{RwLock, RwLockReadGuard};
use path_absolutize::Absolutize;
use rustc_hash::FxHashSet;
use tokio::task;
use tower::ServiceBuilder;

use crate::server::capabilities::{
    ConfigurationCapabilities, negotiate_analyzer_capabilities,
    negotiate_configuration_capabilities, negotiate_position_encoding,
};
use crate::server::error::{AnalyzerResultExt, LspError};
use crate::{ServerConfig, ServerError, walk};

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

struct LspWorkspace {
    engine: QueryEngine,
    files: Arc<RwLock<FileLifecycle<i32, SourceMetadata>>>,
    source_roots: Vec<SourceRoot>,
    selected_sources: FxHashSet<Arc<str>>,
    excluded_sources: FxHashSet<Arc<str>>,
    _prim: MaterializedPrim,
}

struct SourceRoot {
    path: PathBuf,
    metadata: SourceMetadata,
}

enum WorkspaceState {
    WaitingForConfiguration { pending: Vec<PendingNotification> },
    Ready { workspace: LspWorkspace },
}

enum PendingNotification {
    DidOpen(DidOpenTextDocumentParams),
    DidSave(DidSaveTextDocumentParams),
    DidClose(DidCloseTextDocumentParams),
    DidChange(DidChangeTextDocumentParams),
    DidChangeWatchedFiles(DidChangeWatchedFilesParams),
}

pub struct State {
    pub startup_config: Arc<Configuration>,
    pub config: Arc<Configuration>,
    pub client: ClientSocket,

    workspace: WorkspaceState,
    pub diagnostics: event::DiagnosticScheduler,

    pub workspace_symbols_cache: Arc<RwLock<WorkspaceSymbolsCache>>,
    pub suggestions_cache: Arc<RwLock<SuggestionsCache>>,

    pub root: Option<PathBuf>,
    pub configuration_scope: Option<Url>,
    pub configuration_capabilities: ConfigurationCapabilities,
    pub configuration_generation: u64,
    pub position_encoding: PositionEncoding,
    pub analyzer_capabilities: AnalyzerCapabilities,
    pub watched_files_dynamic_registration: bool,
    pub name: String,
    pub version: String,
}

impl State {
    fn new(
        config: Arc<Configuration>,
        client: ClientSocket,
        name: String,
        version: String,
    ) -> State {
        let diagnostics = event::DiagnosticScheduler::default();

        let workspace_symbols_cache = WorkspaceSymbolsCache::default();
        let workspace_symbols_cache = Arc::new(RwLock::new(workspace_symbols_cache));

        let suggestions_cache = SuggestionsCache::default();
        let suggestions_cache = Arc::new(RwLock::new(suggestions_cache));

        let root = None;
        let configuration_scope = None;
        let configuration_capabilities = ConfigurationCapabilities::default();
        let configuration_generation = 0;
        let position_encoding = PositionEncoding::Utf16;
        let analyzer_capabilities = AnalyzerCapabilities::default();
        let watched_files_dynamic_registration = false;

        State {
            startup_config: Arc::clone(&config),
            config,
            client,
            workspace: WorkspaceState::WaitingForConfiguration { pending: vec![] },
            diagnostics,
            workspace_symbols_cache,
            suggestions_cache,
            root,
            configuration_scope,
            configuration_capabilities,
            configuration_generation,
            position_encoding,
            analyzer_capabilities,
            watched_files_dynamic_registration,
            name,
            version,
        }
    }

    fn workspace(&self) -> Result<&LspWorkspace, LspError> {
        match &self.workspace {
            WorkspaceState::WaitingForConfiguration { .. } => Err(LspError::WorkspaceNotReady),
            WorkspaceState::Ready { workspace } => Ok(workspace),
        }
    }

    fn workspace_loaded(&self) -> bool {
        matches!(self.workspace, WorkspaceState::Ready { .. })
    }

    fn install_compilation(
        &mut self,
        compilation: CompilationState<i32, SourceMetadata>,
        source_roots: Vec<SourceRoot>,
        selected_sources: FxHashSet<Arc<str>>,
    ) -> Vec<PendingNotification> {
        let CompilationParts { engine, files, prim } = compilation.into_parts();
        let workspace = LspWorkspace {
            engine,
            files: Arc::new(RwLock::new(files)),
            source_roots,
            selected_sources,
            excluded_sources: FxHashSet::default(),
            _prim: prim,
        };
        let previous = mem::replace(&mut self.workspace, WorkspaceState::Ready { workspace });
        match previous {
            WorkspaceState::WaitingForConfiguration { pending } => pending,
            WorkspaceState::Ready { .. } => vec![],
        }
    }

    fn engine(&self) -> &QueryEngine {
        &self
            .workspace()
            .expect("invariant violated: LSP operation requires a ready workspace")
            .engine
    }

    fn files(&self) -> &Arc<RwLock<FileLifecycle<i32, SourceMetadata>>> {
        &self
            .workspace()
            .expect("invariant violated: LSP operation requires a ready workspace")
            .files
    }

    fn spawn<T>(
        &self,
        action: impl FnOnce(StateSnapshot) -> T + Send + 'static,
    ) -> Result<task::JoinHandle<T>, LspError>
    where
        T: Send + 'static,
    {
        let workspace = self.workspace()?;
        let snapshot = StateSnapshot {
            engine: workspace.engine.snapshot(),
            files: Arc::clone(&workspace.files),
            workspace_symbols_cache: Arc::clone(&self.workspace_symbols_cache),
            suggestions_cache: Arc::clone(&self.suggestions_cache),
            position_encoding: self.position_encoding,
            analyzer_capabilities: self.analyzer_capabilities,
        };
        Ok(task::spawn_blocking(move || action(snapshot)))
    }

    fn invalidate_workspace_symbols(&self) {
        let mut cache = self.workspace_symbols_cache.write();
        mem::take(&mut *cache);
    }

    fn invalidate_suggestions_cache(&self) {
        let mut cache = self.suggestions_cache.write();
        mem::take(&mut *cache);
    }
}

struct StateSnapshot {
    engine: QueryEngine,
    files: Arc<RwLock<FileLifecycle<i32, SourceMetadata>>>,
    workspace_symbols_cache: Arc<RwLock<WorkspaceSymbolsCache>>,
    suggestions_cache: Arc<RwLock<SuggestionsCache>>,
    position_encoding: PositionEncoding,
    analyzer_capabilities: AnalyzerCapabilities,
}

impl StateSnapshot {
    fn with_analyzer_context<T>(
        &self,
        action: impl FnOnce(&AnalyzerContext<LspAnalyzerHost<'_>>) -> T,
    ) -> T {
        let files = self.files.read();
        let host = LspAnalyzerHost { queries: &self.engine, files };
        let context =
            AnalyzerContext::new(&host, self.position_encoding, self.analyzer_capabilities);
        action(&context)
    }
}

struct LspAnalyzerHost<'a> {
    queries: &'a QueryEngine,
    files: RwLockReadGuard<'a, FileLifecycle<i32, SourceMetadata>>,
}

impl AnalyzerHost for LspAnalyzerHost<'_> {
    type Queries = QueryEngine;

    fn queries(&self) -> &QueryEngine {
        self.queries
    }

    fn file_id(&self, uri: &str) -> Option<FileId> {
        self.files.source_id(uri)
    }

    fn file_uri(&self, file_id: FileId) -> Result<Option<Url>, url::ParseError> {
        let Some(uri) = self.files.source_path(file_id) else {
            return Ok(None);
        };
        Url::parse(&uri).map(Some)
    }

    fn active_files(&self) -> impl Iterator<Item = FileId> {
        self.files.source_ids()
    }

    fn is_editable(&self, file_id: FileId) -> bool {
        self.files.source_metadata(file_id).is_some_and(SourceMetadata::editable)
    }
}

fn initialize(
    state: &mut State,
    parameters: extension::CustomInitializeParams,
) -> impl Future<Output = Result<InitializeResult, ResponseError>> + use<> {
    let position_encoding = negotiate_position_encoding(&parameters.initialize_params);
    state.position_encoding = position_encoding;
    state.analyzer_capabilities = negotiate_analyzer_capabilities(&parameters.initialize_params);
    state.configuration_capabilities =
        negotiate_configuration_capabilities(&parameters.initialize_params);
    state.watched_files_dynamic_registration =
        watched_files_dynamic_registration(&parameters.initialize_params.capabilities);

    state.configuration_scope = parameters
        .initialize_params
        .workspace_folders
        .and_then(|folders| folders.first().map(|folder| Url::clone(&folder.uri)));
    state.root = state
        .configuration_scope
        .as_ref()
        .and_then(|uri| uri.to_file_path().ok())
        .or_else(|| env::current_dir().ok());
    let server_info = ServerInfo {
        name: String::clone(&state.name),
        version: Some(String::clone(&state.version)),
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

fn shutdown(_state: &mut State, (): ()) -> impl Future<Output = Result<(), ResponseError>> + use<> {
    async { Ok(()) }
}

fn initialized(state: &mut State, _: InitializedParams) -> Result<(), LspError> {
    let _span = tracing::info_span!("initialization").entered();
    register_file_watcher(state);
    register_configuration_changes(state);

    if state.configuration_capabilities.workspace_configuration {
        request_workspace_configuration(state);
        Ok(())
    } else {
        apply_configuration(state, Arc::clone(&state.startup_config))
    }
}

fn register_configuration_changes(state: &State) {
    if !state.configuration_capabilities.dynamic_registration {
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
    state.configuration_generation = state.configuration_generation.wrapping_add(1);
    let generation = state.configuration_generation;
    let parameters = ConfigurationParams {
        items: vec![ConfigurationItem {
            scope_uri: state.configuration_scope.clone(),
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
    if generation != state.configuration_generation {
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
                .map(|settings| settings.unwrap_or_default().apply_to(&state.startup_config))
                .map_err(|error| format!("Invalid Iris settings: {error}"))
        });

    match configuration {
        Ok(configuration) => {
            if let Err(error) = apply_configuration(state, Arc::new(configuration)) {
                let error = format!("Failed to apply Iris settings: {error}");
                report_configuration_error(state, &error);
                if !state.workspace_loaded() {
                    apply_configuration(state, Arc::clone(&state.startup_config))?;
                }
            }
            Ok(())
        }
        Err(error) => {
            report_configuration_error(state, &error);
            if state.workspace_loaded() {
                Ok(())
            } else {
                apply_configuration(state, Arc::clone(&state.startup_config))
            }
        }
    }
}

fn did_change_configuration(
    state: &mut State,
    _: DidChangeConfigurationParams,
) -> Result<(), LspError> {
    if state.configuration_capabilities.workspace_configuration {
        request_workspace_configuration(state);
    }
    Ok(())
}

fn report_configuration_error(state: &mut State, error: &str) {
    tracing::error!("{error}");
    let message = if state.workspace_loaded() {
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

fn register_file_watcher(state: &State) {
    if !state.watched_files_dynamic_registration {
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
    source_globs: Vec<PathBuf>,
    packages: Vec<PackageInput>,
    metadata: BTreeMap<PathBuf, SourceMetadata>,
    source_roots: Vec<SourceRoot>,
}

fn discover_manual(
    root: &std::path::Path,
    program: &str,
    arguments: &[String],
) -> Result<DiscoveredWorkspace, LspError> {
    tracing::info!("Using '{}'", program);

    let mut command = process::Command::new(program);
    command.args(arguments);

    let output = command.output()?;
    if !output.status.success() {
        return Err(LspError::SourceCommandFailed(output.status));
    }
    let output = str::from_utf8(&output.stdout)?;

    let walk::Walk { files, .. } = walk::walk(root, output.lines())?;

    let metadata = files.iter().map(|file| {
        let editable = file.starts_with(root);
        (PathBuf::clone(file), SourceMetadata::Unmanaged { editable })
    });
    let metadata = metadata.collect();

    let package = PackageInput {
        name: "unmanaged".to_string(),
        source_identities: Vec::clone(&files),
        dependencies: vec![],
    };

    let source_roots = vec![SourceRoot {
        path: root.to_path_buf(),
        metadata: SourceMetadata::Unmanaged { editable: true },
    }];

    Ok(DiscoveredWorkspace { source_globs: files, packages: vec![package], metadata, source_roots })
}

fn discover_spago(root: &std::path::Path) -> Result<DiscoveredWorkspace, LspError> {
    tracing::info!("Using 'spago.lock'");

    let packages = spago::source_files_by_package(root).map_err(LspError::SpagoLock)?;

    let package_inputs = packages.iter().map(|(name, package)| {
        let dependencies = package.dependencies.iter().map(ToString::to_string).collect_vec();
        PackageInput {
            name: name.to_string(),
            source_identities: Vec::clone(&package.sources),
            dependencies,
        }
    });
    let package_inputs = package_inputs.collect_vec();

    let metadata = packages.values().flat_map(|package| {
        let editable = matches!(
            package.reference,
            spago::PackageReference::Workspace | spago::PackageReference::Local
        );
        package
            .sources
            .iter()
            .map(move |file| (PathBuf::clone(file), SourceMetadata::Package { editable }))
    });
    let metadata = metadata.collect::<BTreeMap<_, _>>();

    let source_roots = packages.values().map(|package| package_source_roots(root, package));
    let source_root_groups =
        source_roots.process_results(|source_roots| source_roots.collect_vec())?;
    let mut source_roots = source_root_groups.into_iter().flatten().collect_vec();
    source_roots
        .sort_by_key(|source_root| std::cmp::Reverse(source_root.path.components().count()));

    let source_globs = metadata.keys().cloned().collect_vec();
    Ok(DiscoveredWorkspace { source_globs, packages: package_inputs, metadata, source_roots })
}

fn package_source_roots(
    workspace_root: &std::path::Path,
    package: &spago::PackageSources,
) -> io::Result<Vec<SourceRoot>> {
    let editable = matches!(
        package.reference,
        spago::PackageReference::Workspace | spago::PackageReference::Local
    );
    let metadata = SourceMetadata::Package { editable };

    let mut roots = vec![];
    for root in &package.roots {
        let root = workspace_root.join(root).absolutize()?.to_path_buf();
        roots.push(SourceRoot {
            path: PathBuf::clone(&root),
            metadata: SourceMetadata::clone(&metadata),
        });

        if let Ok(canonical) = dunce::canonicalize(&root)
            && canonical != root
        {
            roots.push(SourceRoot { path: canonical, metadata: SourceMetadata::clone(&metadata) });
        }
    }

    Ok(roots)
}

fn apply_configuration(
    state: &mut State,
    configuration: Arc<Configuration>,
) -> Result<(), LspError> {
    if state.workspace_loaded() && configuration.sources == state.config.sources {
        state.config = configuration;
        return Ok(());
    }

    let root = state.root.as_ref().ok_or(LspError::MissingRoot)?;
    let discovered = match &configuration.sources {
        SourceDiscovery::Spago {} => discover_spago(root)?,
        SourceDiscovery::Command { program, arguments } => {
            discover_manual(root, program, arguments)?
        }
    };

    if state.workspace_loaded() {
        let mut files = BTreeMap::new();
        for path in &discovered.source_globs {
            let content = Arc::from(fs::read_to_string(path)?);
            let metadata = discovered
                .metadata
                .get(path)
                .cloned()
                .expect("invariant violated: discovered source has no LSP metadata");
            files.insert(PathBuf::clone(path), (content, metadata));
        }

        reconcile_files(state, &files)?;
        let workspace = match &mut state.workspace {
            WorkspaceState::WaitingForConfiguration { .. } => {
                return Err(LspError::WorkspaceNotReady);
            }
            WorkspaceState::Ready { workspace } => workspace,
        };
        workspace.source_roots = discovered.source_roots;
        state.config = configuration;
        return Ok(());
    }

    let selected_sources = discovered.source_globs.iter().map(source_uri);
    let selected_sources = selected_sources.collect::<Result<FxHashSet<_>, _>>()?;

    let initial = build_initial::<i32, SourceMetadata, _>(InitialBuildConfig {
        root,
        source_globs: &discovered.source_globs,
        excluded: &[],
        packages: discovered.packages,
        prim_metadata: SourceMetadata::Builtin,
        source_metadata: |path: &std::path::Path| {
            discovered
                .metadata
                .get(path)
                .cloned()
                .expect("invariant violated: discovered source has no LSP metadata")
        },
        execution: PackageExecution::Parallel,
        events: &SilentBuildEvents,
    })?;

    let pending = state.install_compilation(
        initial.into_compilation(),
        discovered.source_roots,
        selected_sources,
    );
    state.config = configuration;

    replay_pending_notifications(state, pending);
    tracing::info!("Loaded {} files.", discovered.source_globs.len());
    Ok(())
}

fn source_uri(path: &PathBuf) -> Result<Arc<str>, LspError> {
    let uri =
        Url::from_file_path(path).map_err(|_| LspError::PathParseFail(PathBuf::clone(path)))?;
    Ok(Arc::from(uri.as_str()))
}

fn reconcile_files(
    state: &mut State,
    files: &BTreeMap<PathBuf, (Arc<str>, SourceMetadata)>,
) -> Result<(), LspError> {
    tracing::info!("Loading {} files.", files.len());

    let selected_sources = files.keys().map(source_uri);
    let selected_sources = selected_sources.collect::<Result<FxHashSet<_>, _>>()?;
    let previous_sources = FxHashSet::clone(&state.workspace()?.selected_sources);
    let removed_sources = previous_sources.difference(&selected_sources).cloned().collect_vec();
    let mut lifecycle_change = LifecycleChange::default();
    for source in removed_sources {
        let uri = Url::parse(&source)?;
        let unit = source_unit_from_source_uri(&uri)?;
        let event = LifecycleEvent::Source {
            unit: SourceUnitKey::clone(&unit),
            event: SourceEvent::DiskObserved {
                disk: DiskObservation::NotFound,
                metadata: SourceMetadata::Unmanaged { editable: false },
            },
        };
        lifecycle_change.combine(apply_lifecycle_event(state, event));
        for kind in ForeignSourceKind::ALL {
            let event = LifecycleEvent::Foreign {
                unit: SourceUnitKey::clone(&unit),
                kind,
                event: ForeignEvent::DiskObserved { disk: DiskObservation::NotFound },
            };
            lifecycle_change.combine(apply_lifecycle_event(state, event));
        }
    }
    for (file, (content, metadata)) in files {
        let uri =
            Url::from_file_path(file).map_err(|_| LspError::PathParseFail(PathBuf::clone(file)))?;
        let unit = source_unit_from_source_uri(&uri)?;
        let event = LifecycleEvent::Source {
            unit: SourceUnitKey::clone(&unit),
            event: SourceEvent::DiskObserved {
                disk: DiskObservation::Found(Arc::clone(content)),
                metadata: SourceMetadata::clone(metadata),
            },
        };
        lifecycle_change.combine(apply_lifecycle_event(state, event));
        lifecycle_change.combine(observe_sibling_foreign(state, &unit)?);
    }
    finish_lifecycle_change(state, &lifecycle_change)?;
    emit_diagnostics_for_change(state, &lifecycle_change)?;

    let workspace = match &mut state.workspace {
        WorkspaceState::WaitingForConfiguration { .. } => {
            return Err(LspError::WorkspaceNotReady);
        }
        WorkspaceState::Ready { workspace } => workspace,
    };
    workspace.excluded_sources.extend(previous_sources.difference(&selected_sources).cloned());
    for selected in &selected_sources {
        workspace.excluded_sources.remove(selected);
    }
    workspace.selected_sources = selected_sources;
    tracing::info!("Loaded {} files.", files.len());
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
    state: &State,
    document: DocumentKind,
    uri: &Url,
) -> Result<Arc<str>, LspError> {
    let files = state.files().read();
    match document {
        DocumentKind::Source => {
            let file_id = files
                .source_id(uri.as_str())
                .ok_or_else(|| LspError::InvalidContentChange(Url::clone(uri)))?;
            state.engine().content(file_id).map_err(LspError::from)
        }
        DocumentKind::Foreign(_) => {
            let file_id = files
                .foreign_id(uri.as_str())
                .ok_or_else(|| LspError::InvalidContentChange(Url::clone(uri)))?;
            state
                .engine()
                .foreign_content(file_id)
                .ok_or_else(|| LspError::InvalidContentChange(Url::clone(uri)))
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

fn did_change(state: &mut State, parameters: DidChangeTextDocumentParams) -> Result<(), LspError> {
    let uri = &parameters.text_document.uri;
    if parameters.content_changes.is_empty() {
        return Ok(());
    }
    let (document, unit) = source_unit_from_document_uri(uri)?;
    let content = document_content(state, document, uri)?;
    let content =
        apply_content_changes(uri, &content, &parameters.content_changes, state.position_encoding)?;
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
    let change = apply_lifecycle_event(state, event);
    finish_lifecycle_change(state, &change)?;

    if state.config.diagnostics.on_change {
        emit_associated_diagnostics(state, Url::clone(&parameters.text_document.uri))?;
    }

    Ok(())
}

fn did_open(state: &mut State, parameters: DidOpenTextDocumentParams) -> Result<(), LspError> {
    let uri = &parameters.text_document.uri;
    let (document, unit) = source_unit_from_document_uri(uri)?;

    let change = match document {
        DocumentKind::Foreign(kind) => {
            let event = LifecycleEvent::Foreign {
                unit,
                kind,
                event: ForeignEvent::Opened {
                    text: Arc::from(parameters.text_document.text.as_str()),
                    version: parameters.text_document.version,
                },
            };
            apply_lifecycle_event(state, event)
        }
        DocumentKind::Source => {
            let metadata = source_metadata(state, &unit, uri);
            let event = LifecycleEvent::Source {
                unit: SourceUnitKey::clone(&unit),
                event: SourceEvent::Opened {
                    text: Arc::from(parameters.text_document.text.as_str()),
                    version: parameters.text_document.version,
                    metadata,
                },
            };
            let mut change = apply_lifecycle_event(state, event);
            change.combine(observe_sibling_foreign(state, &unit)?);
            change
        }
    };
    finish_lifecycle_change(state, &change)?;

    if state.config.diagnostics.on_open {
        emit_associated_diagnostics(state, parameters.text_document.uri)?;
    }

    Ok(())
}

fn did_close(state: &mut State, parameters: DidCloseTextDocumentParams) -> Result<(), LspError> {
    let uri = parameters.text_document.uri;
    let (document, unit) = source_unit_from_document_uri(&uri)?;
    let source_uri = Arc::<str>::from(unit.source());
    let excluded = state.workspace()?.excluded_sources.contains(&source_uri);
    let disk = if excluded { DiskObservation::NotFound } else { observe_disk(&uri) };
    let change = match document {
        DocumentKind::Foreign(kind) => {
            let event =
                LifecycleEvent::Foreign { unit, kind, event: ForeignEvent::Closed { disk } };
            apply_lifecycle_event(state, event)
        }
        DocumentKind::Source => {
            let document = DocumentKey::Source(SourceUnitKey::clone(&unit));
            let was_open = state.files().read().is_open(&document);
            let event = LifecycleEvent::Source {
                unit: SourceUnitKey::clone(&unit),
                event: SourceEvent::Closed { disk },
            };
            let mut change = apply_lifecycle_event(state, event);
            if was_open {
                change.combine(observe_sibling_foreign(state, &unit)?);
            }
            change
        }
    };
    finish_lifecycle_change(state, &change)?;
    emit_diagnostics_for_change(state, &change)?;
    Ok(())
}

fn did_save(state: &mut State, parameters: DidSaveTextDocumentParams) -> Result<(), LspError> {
    state.invalidate_suggestions_cache();

    if state.config.diagnostics.on_save {
        emit_associated_diagnostics(state, parameters.text_document.uri)?;
    }
    Ok(())
}

fn did_change_watched_files(
    state: &mut State,
    parameters: DidChangeWatchedFilesParams,
) -> Result<(), LspError> {
    let mut source_units = FxHashSet::default();
    let mut foreign_units = FxHashSet::default();
    for change in parameters.changes {
        match document_kind(&change.uri) {
            Some(DocumentKind::Foreign(kind)) => {
                let unit = source_unit_from_foreign_uri(&change.uri)?;
                if state.workspace()?.excluded_sources.contains(unit.source()) {
                    continue;
                }
                foreign_units.insert((unit, kind));
            }
            Some(DocumentKind::Source) => {
                let unit = source_unit_from_source_uri(&change.uri)?;
                if state.workspace()?.excluded_sources.contains(unit.source()) {
                    continue;
                }
                source_units.insert(unit);
            }
            None => {}
        }
    }

    let mut lifecycle_change = LifecycleChange::default();
    let mut observed_foreign = FxHashSet::default();
    for unit in source_units {
        let document = DocumentKey::Source(SourceUnitKey::clone(&unit));
        if state.files().read().is_open(&document) {
            continue;
        }
        let uri = Url::parse(unit.source())?;
        if !source_editable(state, &unit, &uri) {
            continue;
        }
        let disk = observe_disk(&uri);
        let source_found = matches!(disk, DiskObservation::Found(_));
        let metadata = source_metadata(state, &unit, &uri);
        let event = LifecycleEvent::Source {
            unit: SourceUnitKey::clone(&unit),
            event: SourceEvent::DiskObserved { disk, metadata },
        };
        lifecycle_change.combine(apply_lifecycle_event(state, event));
        if source_found {
            lifecycle_change.combine(observe_sibling_foreign(state, &unit)?);
            observed_foreign.insert(unit);
        }
    }

    for (unit, kind) in foreign_units {
        if observed_foreign.contains(&unit) {
            continue;
        }
        let document = DocumentKey::Foreign(SourceUnitKey::clone(&unit), kind);
        if state.files().read().is_open(&document) {
            continue;
        }
        let source_uri = Url::parse(unit.source())?;
        if !source_editable(state, &unit, &source_uri) {
            continue;
        }
        let tracked = {
            let files = state.files().read();
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
        lifecycle_change.combine(apply_lifecycle_event(state, event));
    }

    finish_lifecycle_change(state, &lifecycle_change)?;
    emit_diagnostics_for_change(state, &lifecycle_change)?;
    Ok(())
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

fn emit_associated_diagnostics(state: &mut State, uri: Url) -> Result<(), LspError> {
    let (_, unit) = source_unit_from_document_uri(&uri)?;
    event::emit_collect_diagnostics(state, Url::parse(unit.source())?)
}

fn apply_lifecycle_event(
    state: &mut State,
    event: LifecycleEvent<i32, SourceMetadata>,
) -> LifecycleChange {
    // Cancel in-flight queries so that threads holding a read lock over the
    // lifecycle finish before this write waits for expensive LSP requests.
    state.engine().request_cancel();
    state.files().write().apply(state.engine(), event)
}

fn finish_lifecycle_change(state: &mut State, change: &LifecycleChange) -> Result<(), LspError> {
    let files = Arc::clone(state.files());
    state.diagnostics.invalidate(change, &files.read());
    if !matches!(change.analysis(), AnalysisInvalidation::None) {
        state.invalidate_workspace_symbols();
        state.invalidate_suggestions_cache();
    }
    for warning in change.warnings() {
        tracing::warn!("{warning}");
    }
    for removed in change.removed_sources() {
        state.client.publish_diagnostics(PublishDiagnosticsParams {
            uri: Url::parse(&removed.locator)?,
            diagnostics: vec![],
            version: None,
        })?;
    }
    Ok(())
}

fn observe_sibling_foreign(
    state: &mut State,
    unit: &SourceUnitKey,
) -> Result<LifecycleChange, LspError> {
    let mut change = LifecycleChange::default();
    for kind in ForeignSourceKind::ALL {
        let document = DocumentKey::Foreign(SourceUnitKey::clone(unit), kind);
        if state.files().read().is_open(&document) {
            continue;
        }
        let uri = Url::parse(unit.foreign_for(kind))?;
        let event = LifecycleEvent::Foreign {
            unit: SourceUnitKey::clone(unit),
            kind,
            event: ForeignEvent::DiskObserved { disk: observe_disk(&uri) },
        };
        change.combine(apply_lifecycle_event(state, event));
    }
    Ok(change)
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

fn source_metadata(state: &State, unit: &SourceUnitKey, uri: &Url) -> SourceMetadata {
    let previous = {
        let files = state.files().read();
        let file_id = files.source_id(unit.source());
        file_id.and_then(|file_id| files.source_metadata(file_id)).cloned()
    };
    previous.unwrap_or_else(|| {
        let path = uri.to_file_path().ok();
        let package_metadata = path.as_ref().and_then(|path| {
            state
                .workspace()
                .ok()?
                .source_roots
                .iter()
                .find(|source_root| path.starts_with(&source_root.path))
                .map(|source_root| SourceMetadata::clone(&source_root.metadata))
        });
        package_metadata.unwrap_or_else(|| match (&state.root, path) {
            (Some(root), Some(path)) => {
                SourceMetadata::Unmanaged { editable: path.starts_with(root) }
            }
            (Some(_), None) => SourceMetadata::Unmanaged { editable: false },
            (None, _) => SourceMetadata::Unmanaged { editable: true },
        })
    })
}

fn source_editable(state: &State, unit: &SourceUnitKey, uri: &Url) -> bool {
    source_metadata(state, unit, uri).editable()
}

fn emit_diagnostics_for_change(
    state: &mut State,
    change: &LifecycleChange,
) -> Result<(), LspError> {
    match change.analysis() {
        AnalysisInvalidation::None => Ok(()),
        AnalysisInvalidation::Sources(sources) => {
            for file_id in sources {
                event::emit_collect_diagnostics_id(state, *file_id)?;
            }
            Ok(())
        }
        AnalysisInvalidation::Workspace => event::emit_collect_all_diagnostics(state),
    }
}

fn replay_pending_notifications(state: &mut State, pending: Vec<PendingNotification>) {
    for notification in pending {
        let result = match notification {
            PendingNotification::DidOpen(parameters) => did_open(state, parameters),
            PendingNotification::DidSave(parameters) => did_save(state, parameters),
            PendingNotification::DidClose(parameters) => did_close(state, parameters),
            PendingNotification::DidChange(parameters) => did_change(state, parameters),
            PendingNotification::DidChangeWatchedFiles(parameters) => {
                did_change_watched_files(state, parameters)
            }
        };
        if let Err(error) = result {
            error.emit_trace();
        }
    }
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
        pending: fn(N::Params) -> PendingNotification,
        action: impl Fn(&mut State, N::Params) -> Result<(), LspError> + Send + Copy + 'static,
    ) -> &mut Self {
        let this: &mut Router<State> = self.borrow_mut();
        this.notification::<N>(move |state, parameters| {
            let result = match &mut state.workspace {
                WorkspaceState::WaitingForConfiguration { pending: notifications } => {
                    notifications.push(pending(parameters));
                    Ok(())
                }
                WorkspaceState::Ready { .. } => action(state, parameters),
            };
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
    let (server, _) = async_lsp::MainLoop::new_server(move |client| {
        let client_socket = ClientSocket::clone(&client);
        let mut router: Router<State, ResponseError> = Router::new(State::new(
            Arc::clone(&config),
            client_socket,
            String::clone(&name),
            String::clone(&version),
        ));

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
                PendingNotification::DidOpen,
                did_open,
            )
            .workspace_notification::<notification::DidSaveTextDocument>(
                PendingNotification::DidSave,
                did_save,
            )
            .workspace_notification::<notification::DidCloseTextDocument>(
                PendingNotification::DidClose,
                did_close,
            )
            .notification_ext::<notification::DidChangeConfiguration>(did_change_configuration)
            .workspace_notification::<notification::DidChangeTextDocument>(
                PendingNotification::DidChange,
                did_change,
            )
            .workspace_notification::<notification::DidChangeWatchedFiles>(
                PendingNotification::DidChangeWatchedFiles,
                did_change_watched_files,
            )
            .event_ext::<event::CollectDiagnostics>(event::collect_diagnostics)
            .event_ext::<event::DiagnosticsFinished>(event::finish_diagnostics)
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

    server.run_buffered(stdin, stdout).await.map_err(ServerError::new)
}
