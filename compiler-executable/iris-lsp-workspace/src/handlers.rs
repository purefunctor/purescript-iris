//! Analysis and document handlers.

use std::sync::Arc;

use building::QueryError;
use building::lifecycle::{
    DiskObservation, DocumentKey, DocumentKind, ForeignEvent, LifecycleEvent, SourceEvent,
    SourceUnitKey,
};
use iris_analysis::AnalyzerError;
use iris_analysis::position::PositionEncoding;
use iris_lsp_server::{Answer, Rejection};
use lsp_types::notification::{
    DidChangeTextDocument, DidChangeWatchedFiles, DidCloseTextDocument, DidOpenTextDocument,
    DidSaveTextDocument, Notification,
};
use lsp_types::request::{
    CodeActionRequest, Completion, DocumentHighlightRequest, DocumentSymbolRequest, GotoDefinition,
    HoverRequest, PrepareRenameRequest, References, Rename, Request, ResolveCompletionItem,
    SemanticTokensFullRequest, WorkspaceSymbolRequest,
};
use lsp_types::*;
use rustc_hash::FxHashSet;
use serde::Serialize;
use serde_json::Value;
use thiserror::Error;

use crate::analysis::{CONTENT_MODIFIED, ChangeSignal, Snapshot};
use crate::state::{
    DiagnosticTrigger, ReadyWorkspace, WorkspaceEffects, document_kind, observe_disk,
    source_unit_from_document_uri, source_unit_from_foreign_uri, source_unit_from_source_uri,
};

#[derive(Error, Debug)]
pub(crate) enum DocumentError {
    #[error("QueryError: {0}")]
    QueryError(#[from] QueryError),
    #[error("Expected a file URI, received {0}")]
    InvalidFileUri(Url),
    #[error("Expected a PureScript or JavaScript document URI, received {0}")]
    UnsupportedDocumentUri(Url),
    #[error("Invalid content change for document {0}")]
    InvalidContentChange(Url),
    #[error("UrlParseError: {0}")]
    UrlParseError(#[from] url::ParseError),
}

/// A document notification the workspace actor handles.
pub(crate) enum DocumentNotification {
    Open(DidOpenTextDocumentParams),
    Change(DidChangeTextDocumentParams),
    Close(DidCloseTextDocumentParams),
    Save(DidSaveTextDocumentParams),
    ChangeWatchedFiles(DidChangeWatchedFilesParams),
}

/// What document handlers need besides the workspace.
pub(crate) struct DocumentContext {
    pub(crate) root: Option<std::path::PathBuf>,
    pub(crate) position_encoding: PositionEncoding,
    pub(crate) change_signal: ChangeSignal,
}

impl DocumentNotification {
    /// Decodes a notification, or returns `None` for a method the workspace actor does not
    /// handle.
    pub(crate) fn decode(
        method: &str,
        params: Value,
    ) -> Result<Option<DocumentNotification>, serde_json::Error> {
        let notification = match method {
            DidOpenTextDocument::METHOD => {
                DocumentNotification::Open(serde_json::from_value(params)?)
            }
            DidChangeTextDocument::METHOD => {
                DocumentNotification::Change(serde_json::from_value(params)?)
            }
            DidCloseTextDocument::METHOD => {
                DocumentNotification::Close(serde_json::from_value(params)?)
            }
            DidSaveTextDocument::METHOD => {
                DocumentNotification::Save(serde_json::from_value(params)?)
            }
            DidChangeWatchedFiles::METHOD => {
                DocumentNotification::ChangeWatchedFiles(serde_json::from_value(params)?)
            }
            _ => return Ok(None),
        };
        Ok(Some(notification))
    }
}

/// Applies a document notification. Runs on a blocking thread: a change to engine inputs waits
/// until running analysis notices the cancelled flag, and `didSave` takes the suggestions cache
/// lock that a running completion request holds. Settings and `didSave` do not change engine
/// inputs, so they cancel nothing.
pub(crate) fn apply_document(
    workspace: &mut ReadyWorkspace,
    context: &DocumentContext,
    notification: DocumentNotification,
) -> WorkspaceEffects {
    let result = match notification {
        DocumentNotification::Open(parameters) => did_open(workspace, context, parameters),
        DocumentNotification::Change(parameters) => did_change(workspace, context, parameters),
        DocumentNotification::Close(parameters) => did_close(workspace, context, parameters),
        DocumentNotification::Save(parameters) => did_save(workspace, parameters),
        DocumentNotification::ChangeWatchedFiles(parameters) => {
            did_change_watched_files(workspace, context, parameters)
        }
    };
    result.unwrap_or_else(|error| {
        tracing::error!("{error}");
        WorkspaceEffects::default()
    })
}

fn did_open(
    workspace: &mut ReadyWorkspace,
    context: &DocumentContext,
    parameters: DidOpenTextDocumentParams,
) -> Result<WorkspaceEffects, DocumentError> {
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
            let metadata = workspace.source_metadata(context.root.as_deref(), &unit, uri);
            events.push(LifecycleEvent::Source {
                unit: SourceUnitKey::clone(&unit),
                event: SourceEvent::Opened {
                    text: Arc::from(parameters.text_document.text.as_str()),
                    version: parameters.text_document.version,
                    metadata,
                },
            });
            events.extend(workspace.observe_sibling_foreign(&unit)?);
        }
    }
    let trigger = if workspace.configuration.diagnostics.on_open {
        DiagnosticTrigger::AssociatedSource(parameters.text_document.uri)
    } else {
        DiagnosticTrigger::None
    };
    Ok(workspace.apply_lifecycle_events(events, trigger, &context.change_signal))
}

fn did_change(
    workspace: &mut ReadyWorkspace,
    context: &DocumentContext,
    parameters: DidChangeTextDocumentParams,
) -> Result<WorkspaceEffects, DocumentError> {
    let uri = &parameters.text_document.uri;
    if parameters.content_changes.is_empty() {
        return Ok(WorkspaceEffects::default());
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
    Ok(workspace.apply_lifecycle_events([event], trigger, &context.change_signal))
}

fn did_close(
    workspace: &mut ReadyWorkspace,
    context: &DocumentContext,
    parameters: DidCloseTextDocumentParams,
) -> Result<WorkspaceEffects, DocumentError> {
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
                events.extend(workspace.observe_sibling_foreign(&unit)?);
            }
        }
    }
    let trigger = DiagnosticTrigger::AnalysisChange;
    Ok(workspace.apply_lifecycle_events(events, trigger, &context.change_signal))
}

fn did_save(
    workspace: &mut ReadyWorkspace,
    parameters: DidSaveTextDocumentParams,
) -> Result<WorkspaceEffects, DocumentError> {
    workspace.analysis.invalidate_suggestions_cache();

    if workspace.configuration.diagnostics.on_save {
        workspace.associated_effects(&parameters.text_document.uri)
    } else {
        Ok(WorkspaceEffects::default())
    }
}

fn did_change_watched_files(
    workspace: &mut ReadyWorkspace,
    context: &DocumentContext,
    parameters: DidChangeWatchedFilesParams,
) -> Result<WorkspaceEffects, DocumentError> {
    let root = context.root.as_deref();
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
        if !workspace.source_editable(root, &unit, &uri) {
            continue;
        }
        let disk = observe_disk(&uri);
        let source_found = matches!(disk, DiskObservation::Found(_));
        let metadata = workspace.source_metadata(root, &unit, &uri);
        let event = LifecycleEvent::Source {
            unit: SourceUnitKey::clone(&unit),
            event: SourceEvent::DiskObserved { disk, metadata },
        };
        events.push(event);
        if source_found {
            events.extend(workspace.observe_sibling_foreign(&unit)?);
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
        if !workspace.source_editable(root, &unit, &source_uri) {
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

    let trigger = DiagnosticTrigger::AnalysisChange;
    Ok(workspace.apply_lifecycle_events(events, trigger, &context.change_signal))
}

pub(crate) fn apply_content_changes(
    uri: &Url,
    content: &str,
    content_changes: &[TextDocumentContentChangeEvent],
    position_encoding: PositionEncoding,
) -> Result<Arc<str>, DocumentError> {
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
            return Err(DocumentError::InvalidContentChange(Url::clone(uri)));
        };

        let start = usize::from(start);
        let end = usize::from(end);

        if start > end {
            return Err(DocumentError::InvalidContentChange(Url::clone(uri)));
        }

        content.replace_range(start..end, &content_change.text);
    }
    Ok(Arc::from(content))
}

/// An analysis request with decoded parameters, ready to run on a snapshot.
pub(crate) type AnalysisJob = Box<dyn FnOnce(&Snapshot) -> Answer + Send>;

/// Decodes an analysis request. Unknown methods and undecodable parameters are rejected before
/// the request waits for the workspace.
pub(crate) fn analysis_job(method: &str, params: Value) -> Result<AnalysisJob, Rejection> {
    match method {
        GotoDefinition::METHOD => job::<GotoDefinition>(params, definition),
        HoverRequest::METHOD => job::<HoverRequest>(params, hover),
        CodeActionRequest::METHOD => job::<CodeActionRequest>(params, code_action),
        Completion::METHOD => job::<Completion>(params, completion),
        ResolveCompletionItem::METHOD => job::<ResolveCompletionItem>(params, resolve_completion),
        References::METHOD => job::<References>(params, references),
        PrepareRenameRequest::METHOD => job::<PrepareRenameRequest>(params, prepare_rename),
        Rename::METHOD => job::<Rename>(params, rename),
        DocumentHighlightRequest::METHOD => {
            job::<DocumentHighlightRequest>(params, document_highlight)
        }
        WorkspaceSymbolRequest::METHOD => job::<WorkspaceSymbolRequest>(params, workspace_symbols),
        DocumentSymbolRequest::METHOD => job::<DocumentSymbolRequest>(params, document_symbols),
        SemanticTokensFullRequest::METHOD => {
            job::<SemanticTokensFullRequest>(params, semantic_tokens)
        }
        #[cfg(test)]
        crate::tests::GATED_METHOD => crate::tests::gated_job(params),
        _ => Err(Rejection::MethodNotFound),
    }
}

fn job<R>(
    params: Value,
    handler: fn(&Snapshot, R::Params) -> Result<R::Result, AnalyzerError>,
) -> Result<AnalysisJob, Rejection>
where
    R: Request,
    R::Params: Send + 'static,
    R::Result: Serialize,
{
    let params = serde_json::from_value::<R::Params>(params).map_err(|error| {
        Rejection::InvalidParams(format!("Failed to deserialize parameters: {error}"))
    })?;
    Ok(Box::new(move |snapshot| {
        let result = handler(snapshot, params).map_err(rejection)?;
        Ok(serde_json::to_value(result).expect("invariant violated: results must serialize"))
    }))
}

/// Maps an analysis error to the answer the editor receives.
fn rejection(error: AnalyzerError) -> Rejection {
    match error {
        AnalyzerError::QueryError(QueryError::Cancelled) => {
            tracing::warn!("AnalyzerError: {error}");
            Rejection::ContentModified(CONTENT_MODIFIED.to_string())
        }
        AnalyzerError::RenameRejected(message) => {
            tracing::warn!("AnalyzerError: Rename rejected: {message}");
            Rejection::InvalidParams(message)
        }
        error => {
            tracing::error!("AnalyzerError: {error}");
            Rejection::RequestFailed("Request failed".to_string())
        }
    }
}

/// Turns [`AnalyzerError::NonFatal`] into `item`.
fn on_non_fatal<T>(result: Result<T, AnalyzerError>, item: T) -> Result<T, AnalyzerError> {
    match result {
        Err(AnalyzerError::NonFatal) => Ok(item),
        result => result,
    }
}

fn definition(
    snapshot: &Snapshot,
    parameters: GotoDefinitionParams,
) -> Result<Option<GotoDefinitionResponse>, AnalyzerError> {
    let _span = tracing::info_span!("definition").entered();
    let uri = parameters.text_document_position_params.text_document.uri;
    let position = parameters.text_document_position_params.position;
    let result = snapshot.with_analyzer_context(|context| {
        iris_analysis::definition::implementation(context, uri, position)
    });
    on_non_fatal(result, None)
}

fn hover(snapshot: &Snapshot, parameters: HoverParams) -> Result<Option<Hover>, AnalyzerError> {
    let _span = tracing::info_span!("hover").entered();
    let uri = parameters.text_document_position_params.text_document.uri;
    let position = parameters.text_document_position_params.position;
    let result = snapshot.with_analyzer_context(|context| {
        iris_analysis::hover::implementation(context, uri, position)
    });
    on_non_fatal(result, None)
}

fn code_action(
    snapshot: &Snapshot,
    parameters: CodeActionParams,
) -> Result<Option<CodeActionResponse>, AnalyzerError> {
    let _span = tracing::info_span!("code_action").entered();
    let uri = parameters.text_document.uri;
    let range = parameters.range;
    let action_context = parameters.context;
    let result = snapshot.with_analyzer_context(|context| {
        iris_analysis::code_action::implementation(context, uri, range, action_context)
    });
    on_non_fatal(result, None)
}

fn completion(
    snapshot: &Snapshot,
    parameters: CompletionParams,
) -> Result<Option<CompletionResponse>, AnalyzerError> {
    let _span = tracing::info_span!("completion").entered();
    let uri = parameters.text_document_position.text_document.uri;
    let position = parameters.text_document_position.position;
    let mut cache = snapshot.suggestions_cache.write();
    let result = snapshot.with_analyzer_context(|context| {
        iris_analysis::completion::implementation(context, &mut cache, uri, position)
    });
    on_non_fatal(result, None)
}

fn resolve_completion(
    snapshot: &Snapshot,
    item: CompletionItem,
) -> Result<CompletionItem, AnalyzerError> {
    let _span = tracing::info_span!("resolve_completion_item").entered();
    iris_analysis::completion::resolve::implementation(&snapshot.engine, item)
        .or_else(|(error, item)| on_non_fatal(Err(error), item))
}

fn references(
    snapshot: &Snapshot,
    parameters: ReferenceParams,
) -> Result<Option<Vec<Location>>, AnalyzerError> {
    let _span = tracing::info_span!("references").entered();
    let uri = parameters.text_document_position.text_document.uri;
    let position = parameters.text_document_position.position;
    let result = snapshot.with_analyzer_context(|context| {
        iris_analysis::references::implementation(context, uri, position)
    });
    on_non_fatal(result, None)
}

fn prepare_rename(
    snapshot: &Snapshot,
    parameters: TextDocumentPositionParams,
) -> Result<Option<PrepareRenameResponse>, AnalyzerError> {
    let _span = tracing::info_span!("prepare_rename").entered();
    let uri = parameters.text_document.uri;
    let position = parameters.position;
    let result = snapshot
        .with_analyzer_context(|context| iris_analysis::rename::prepare(context, uri, position));
    on_non_fatal(result, None)
}

fn rename(
    snapshot: &Snapshot,
    parameters: RenameParams,
) -> Result<Option<WorkspaceEdit>, AnalyzerError> {
    let _span = tracing::info_span!("rename").entered();
    let uri = parameters.text_document_position.text_document.uri;
    let position = parameters.text_document_position.position;
    let new_name = parameters.new_name;
    let result = snapshot.with_analyzer_context(|context| {
        iris_analysis::rename::implementation(context, uri, position, new_name)
    });
    on_non_fatal(result, None)
}

fn document_highlight(
    snapshot: &Snapshot,
    parameters: DocumentHighlightParams,
) -> Result<Option<Vec<DocumentHighlight>>, AnalyzerError> {
    let _span = tracing::info_span!("document_highlight").entered();
    let uri = parameters.text_document_position_params.text_document.uri;
    let position = parameters.text_document_position_params.position;
    let result = snapshot.with_analyzer_context(|context| {
        iris_analysis::document_highlight::implementation(context, uri, position)
    });
    on_non_fatal(result, None)
}

fn workspace_symbols(
    snapshot: &Snapshot,
    parameters: WorkspaceSymbolParams,
) -> Result<Option<WorkspaceSymbolResponse>, AnalyzerError> {
    let _span = tracing::info_span!("workspace_symbols").entered();
    let mut cache = snapshot.workspace_symbols_cache.write();
    let result = snapshot.with_analyzer_context(|context| {
        iris_analysis::symbols::workspace(context, &mut cache, &parameters.query)
    });
    on_non_fatal(result, None)
}

fn document_symbols(
    snapshot: &Snapshot,
    parameters: DocumentSymbolParams,
) -> Result<Option<DocumentSymbolResponse>, AnalyzerError> {
    let _span = tracing::info_span!("document_symbols").entered();
    let uri = parameters.text_document.uri;
    let result =
        snapshot.with_analyzer_context(|context| iris_analysis::symbols::document(context, uri));
    on_non_fatal(result, None)
}

fn semantic_tokens(
    snapshot: &Snapshot,
    parameters: SemanticTokensParams,
) -> Result<Option<SemanticTokensResult>, AnalyzerError> {
    let _span = tracing::info_span!("semantic_tokens").entered();
    let uri = parameters.text_document.uri;
    let result = snapshot.with_analyzer_context(|context| {
        iris_analysis::semantic_tokens::implementation(context, uri)
            .map(|tokens| tokens.map(SemanticTokensResult::Tokens))
    });
    on_non_fatal(result, None)
}
