pub mod capabilities;
pub mod error;
pub mod event;
pub mod extension;

mod document;
mod workspace;

#[cfg(test)]
mod tests;

use std::borrow::BorrowMut;
use std::collections::BTreeMap;
use std::ops::ControlFlow;
use std::path::PathBuf;
use std::sync::Arc;
use std::{env, fs, io, process};

use analyzer::AnalyzerCapabilities;
use analyzer::position::PositionEncoding;
use async_lsp::client_monitor::ClientProcessMonitorLayer;
use async_lsp::concurrency::ConcurrencyLayer;
use async_lsp::panic::CatchUnwindLayer;
use async_lsp::router::Router;
use async_lsp::server::LifecycleLayer;
use async_lsp::{ClientSocket, LanguageClient, ResponseError};
use building::lifecycle::{
    DiskObservation, DocumentKey, DocumentKind, ForeignEvent, LifecycleEvent, ReloadFailure,
    SourceEvent, SourceUnitKey,
};
use configuration::{Configuration, ConfigurationSettings, SourceDiscovery};
use files::ForeignSourceKind;
use iris_build::compile::{InitialBuildConfig, PackageExecution, build_initial};
use iris_build::events::SilentBuildEvents;
use iris_build::plan::PackageInput;
use itertools::Itertools;
use lsp_types::notification::Notification;
use lsp_types::request::Request;
use lsp_types::*;
use path_absolutize::Absolutize;
use rustc_hash::FxHashSet;
use smol_str::SmolStr;
use tokio::task;
use tower::ServiceBuilder;

use crate::server::capabilities::{
    ConfigurationCapabilities, negotiate_analyzer_capabilities,
    negotiate_configuration_capabilities, negotiate_position_encoding,
};
use crate::server::document::DocumentPath;
use crate::server::error::{AnalyzerResultExt, LspError};
use crate::server::workspace::{
    ConfigurationApplyError, DiagnosticTrigger, PreparedInitialWorkspace, ReadyWorkspace,
    SourceRoot, StateSnapshot, WorkspaceContext, WorkspaceEffects, WorkspaceNotification,
    WorkspaceRuntime,
};
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
        }
    }

    fn spawn<T>(
        &self,
        action: impl FnOnce(StateSnapshot) -> T + Send + 'static,
    ) -> Result<task::JoinHandle<T>, LspError>
    where
        T: Send + 'static,
    {
        let snapshot = self
            .workspace
            .snapshot(self.protocol.position_encoding, self.protocol.analyzer_capabilities)?;
        Ok(task::spawn_blocking(move || action(snapshot)))
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

fn shutdown(_state: &mut State, (): ()) -> impl Future<Output = Result<(), ResponseError>> + use<> {
    async { Ok(()) }
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
                    settings.unwrap_or_default().apply_to(&state.protocol.startup_configuration)
                })
                .map_err(|error| format!("Invalid Iris settings: {error}"))
        });

    match configuration {
        Ok(configuration) => {
            if let Err(error) = apply_configuration_inner(state, Arc::new(configuration)) {
                match error {
                    ConfigurationApplyError::Preparation(error) => {
                        let error = format!("Failed to apply Iris settings: {error}");
                        report_configuration_error(state, &error);
                        if !state.workspace.is_ready() {
                            apply_configuration(
                                state,
                                Arc::clone(&state.protocol.startup_configuration),
                            )?;
                        }
                    }
                    ConfigurationApplyError::Delivery(error) => {
                        report_configuration_delivery_error(state, &error);
                    }
                }
            }
            Ok(())
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
        name: SmolStr::new("unmanaged"),
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

    let package_inputs = packages.iter().map(|(name, package)| PackageInput {
        name: SmolStr::clone(name),
        source_identities: Vec::clone(&package.sources),
        dependencies: package.dependencies.iter().cloned().collect_vec(),
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
    apply_configuration_inner(state, configuration).map_err(|error| match error {
        ConfigurationApplyError::Preparation(error) | ConfigurationApplyError::Delivery(error) => {
            error
        }
    })
}

fn apply_configuration_inner(
    state: &mut State,
    configuration: Arc<Configuration>,
) -> Result<(), ConfigurationApplyError> {
    let root = state
        .protocol
        .root
        .as_deref()
        .ok_or(LspError::MissingRoot)
        .map_err(ConfigurationApplyError::Preparation)?;
    if state.workspace.update_configuration_if_sources_equal(Arc::clone(&configuration)) {
        return Ok(());
    }

    let discovered = match &configuration.sources {
        SourceDiscovery::Spago {} => discover_spago(root),
        SourceDiscovery::Command { program, arguments } => {
            discover_manual(root, program, arguments)
        }
    }
    .map_err(ConfigurationApplyError::Preparation)?;

    if state.workspace.is_ready() {
        let prepared = state
            .workspace
            .prepare_reconfiguration(configuration, discovered)
            .map_err(ConfigurationApplyError::Preparation)?;
        let effects = state
            .workspace
            .commit_reconfiguration(prepared)
            .map_err(ConfigurationApplyError::Preparation)?;
        effects.deliver(&state.client).map_err(ConfigurationApplyError::Delivery)?;
        return Ok(());
    }

    let selected_sources = discovered.source_globs.iter().map(source_uri);
    let selected_sources = selected_sources
        .collect::<Result<FxHashSet<_>, _>>()
        .map_err(ConfigurationApplyError::Preparation)?;

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
    })
    .map_err(LspError::from)
    .map_err(ConfigurationApplyError::Preparation)?;

    let prepared = PreparedInitialWorkspace {
        configuration,
        compilation: initial.into_compilation(),
        source_roots: discovered.source_roots,
        selected_sources,
    };
    let pending =
        state.workspace.install(prepared).map_err(ConfigurationApplyError::Preparation)?;

    for notification in pending {
        let context = WorkspaceContext {
            root: state.protocol.root.as_deref(),
            position_encoding: state.protocol.position_encoding,
        };
        let result = state.workspace.dispatch(notification, context, &state.client);
        if let Err(error) = result {
            error.emit_trace();
        }
    }
    tracing::info!("Loaded {} files.", discovered.source_globs.len());
    Ok(())
}

fn source_uri(path: &PathBuf) -> Result<Arc<str>, LspError> {
    let uri = DocumentPath::new(path)?.uri()?;
    Ok(Arc::from(uri.as_str()))
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
    let files = workspace.files.read();
    match document {
        DocumentKind::Source => {
            let file_id = files
                .source_id(uri.as_str())
                .ok_or_else(|| LspError::InvalidContentChange(Url::clone(&uri)))?;
            workspace.engine.content(file_id).map_err(LspError::from)
        }
        DocumentKind::Foreign(_) => {
            let file_id = files
                .foreign_id(uri.as_str())
                .ok_or_else(|| LspError::InvalidContentChange(Url::clone(&uri)))?;
            workspace
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
            let was_open = workspace.files.read().is_open(&document);
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
        if workspace.files.read().is_open(&document) {
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
        if workspace.files.read().is_open(&document) {
            continue;
        }
        let source_uri = Url::parse(unit.source())?;
        if !source_editable(workspace, context.root, &unit, &source_uri) {
            continue;
        }
        let tracked = {
            let files = workspace.files.read();
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
        if workspace.files.read().is_open(&document) {
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
        let files = workspace.files.read();
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
