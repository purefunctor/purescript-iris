use std::collections::BTreeMap;

use analyzer::completion::SuggestionsCache;
use analyzer::symbols::WorkspaceSymbolsCache;
use analyzer::{AnalyzerContext, AnalyzerError, AnalyzerHost};
use building::{FileLifecycle, QueryCancellation, QueryEngine, QueryError};
use files::FileId;
use lsp_types::*;

use crate::transport::Shared;
use crate::{AnalysisStamp, Cancellation, Options, Reply, RequestFailure};

#[derive(Clone, Debug, thiserror::Error)]
pub enum LanguageServerFailure {
    #[error("rename rejected: {0}")]
    RenameRejected(String),
    #[error("analysis failed: {0}")]
    Analysis(String),
}

macro_rules! language_requests {
    ($($name:ident { $($field:ident: $input:ty),* } => $output:ty),* $(,)?) => {
        pub enum LanguageServer {
            $($name { $($field: $input,)* reply: Reply<$output> }),*
        }

        impl LanguageServer {
            pub(crate) fn admit(&mut self, shared: Shared, stamp: AnalysisStamp) {
                match self { $(LanguageServer::$name { reply, .. } => reply.admit(shared, stamp)),* }
            }

            pub(crate) fn reject(&mut self, failure: RequestFailure) {
                match self { $(LanguageServer::$name { reply, .. } => reply.reject(failure)),* }
            }

            pub(crate) fn cancellation(&self) -> Cancellation {
                match self { $(LanguageServer::$name { reply, .. } => Cancellation::clone(&reply.cancellation)),* }
            }

            pub(crate) fn stamp(&self) -> AnalysisStamp {
                match self { $(LanguageServer::$name { reply, .. } => reply.stamp()),* }
            }
        }
    };
}

language_requests! {
    Hover { uri: Url, position: Position } => Option<Hover>,
    Definition { uri: Url, position: Position } => Option<GotoDefinitionResponse>,
    References { uri: Url, position: Position } => Option<Vec<Location>>,
    Completion { uri: Url, position: Position } => Option<CompletionResponse>,
    ResolveCompletion { item: CompletionItem } => CompletionItem,
    Rename { uri: Url, position: Position, new_name: String } => Option<WorkspaceEdit>,
    PrepareRename { uri: Url, position: Position } => Option<PrepareRenameResponse>,
    DocumentHighlight { uri: Url, position: Position } => Option<Vec<DocumentHighlight>>,
    DocumentSymbols { uri: Url } => Option<DocumentSymbolResponse>,
    WorkspaceSymbols { query: String } => Option<WorkspaceSymbolResponse>,
    SemanticTokens { uri: Url } => Option<SemanticTokens>,
    CodeAction { uri: Url, range: Range, context: CodeActionContext } => Option<CodeActionResponse>,
}

pub(crate) struct Host<'a> {
    pub(crate) engine: &'a QueryEngine,
    pub(crate) files: &'a FileLifecycle<i32, bool>,
}

impl AnalyzerHost for Host<'_> {
    type Queries = QueryEngine;

    fn queries(&self) -> &QueryEngine {
        self.engine
    }

    fn file_id(&self, uri: &str) -> Option<FileId> {
        self.files.source_id(uri)
    }

    fn file_uri(&self, file_id: FileId) -> Result<Option<Url>, url::ParseError> {
        self.files.source_path(file_id).map(|uri| Url::parse(&uri)).transpose()
    }

    fn active_files(&self) -> impl Iterator<Item = FileId> {
        self.files.source_ids()
    }

    fn is_editable(&self, file_id: FileId) -> bool {
        self.files.source_metadata(file_id).copied().unwrap_or(false)
    }
}

pub(crate) fn failure(error: AnalyzerError) -> RequestFailure {
    match error {
        AnalyzerError::QueryError(QueryError::Cancelled) => RequestFailure::Cancelled,
        AnalyzerError::RenameRejected(message) => {
            LanguageServerFailure::RenameRejected(message).into()
        }
        error => LanguageServerFailure::Analysis(error.to_string()).into(),
    }
}

fn optional<T>(result: Result<Option<T>, AnalyzerError>) -> Result<Option<T>, RequestFailure> {
    match result {
        Err(AnalyzerError::NonFatal) => Ok(None),
        result => result.map_err(failure),
    }
}

#[derive(Default)]
pub(crate) struct Analysis {
    suggestions: SuggestionsCache,
    symbols: WorkspaceSymbolsCache,
    resolve: BTreeMap<String, serde_json::Value>,
    next_resolve: u64,
    session: String,
}

impl Analysis {
    pub(crate) fn new(session: String) -> Analysis {
        Analysis { session, ..Analysis::default() }
    }

    pub(crate) fn invalidate(&mut self) {
        self.suggestions = SuggestionsCache::default();
        self.symbols = WorkspaceSymbolsCache::default();
        self.resolve.clear();
    }

    fn protect_completions(&mut self, response: &mut CompletionResponse) {
        let items = match response {
            CompletionResponse::Array(items) => items,
            CompletionResponse::List(list) => &mut list.items,
        };
        for item in items {
            if let Some(data) = item.data.take() {
                self.next_resolve =
                    self.next_resolve.checked_add(1).expect("completion token overflow");
                let token = format!("{}:{}", self.session, self.next_resolve);
                if self.resolve.len() >= 4096 {
                    self.resolve.pop_first();
                }
                self.resolve.insert(String::clone(&token), data);
                item.data = Some(serde_json::Value::String(token));
            }
        }
    }

    pub(crate) fn execute(
        &mut self,
        command: LanguageServer,
        engine: &QueryEngine,
        files: &FileLifecycle<i32, bool>,
        options: Options,
    ) {
        let cancellation = command.cancellation();
        let snapshot =
            engine.snapshot_with_cancellation(QueryCancellation::clone(&cancellation.query));
        let host = Host { engine: &snapshot, files };
        let context = AnalyzerContext::new(&host, options.position_encoding, options.capabilities);
        match command {
            LanguageServer::Hover { uri, position, reply } => {
                reply.finish(optional(analyzer::hover::implementation(&context, uri, position)));
            }
            LanguageServer::Definition { uri, position, reply } => {
                reply.finish(optional(analyzer::definition::implementation(
                    &context, uri, position,
                )));
            }
            LanguageServer::References { uri, position, reply } => {
                reply.finish(optional(analyzer::references::implementation(
                    &context, uri, position,
                )));
            }
            LanguageServer::Completion { uri, position, reply } => {
                let result = optional(analyzer::completion::implementation(
                    &context,
                    &mut self.suggestions,
                    uri,
                    position,
                ));
                reply.finish(result.map(|response| {
                    response.map(|mut response| {
                        self.protect_completions(&mut response);
                        response
                    })
                }));
            }
            LanguageServer::ResolveCompletion { mut item, reply } => {
                let target = item.data.take().and_then(|data| {
                    data.as_str().and_then(|token| self.resolve.get(token)).cloned()
                });
                item.data = target;
                let result = analyzer::completion::resolve::implementation(&snapshot, item);
                reply.finish(match result {
                    Ok(item) | Err((AnalyzerError::NonFatal, item)) => Ok(item),
                    Err((error, _)) => Err(failure(error)),
                });
            }
            LanguageServer::Rename { uri, position, new_name, reply } => {
                let result =
                    optional(analyzer::rename::implementation(&context, uri, position, new_name));
                reply.finish(result.map(|edit| {
                    edit.map(|mut edit| {
                        version_edits(&mut edit, files);
                        edit
                    })
                }));
            }
            LanguageServer::PrepareRename { uri, position, reply } => {
                reply.finish(optional(analyzer::rename::prepare(&context, uri, position)));
            }
            LanguageServer::DocumentHighlight { uri, position, reply } => {
                reply.finish(optional(analyzer::document_highlight::implementation(
                    &context, uri, position,
                )));
            }
            LanguageServer::DocumentSymbols { uri, reply } => {
                reply.finish(optional(analyzer::symbols::document(&context, uri)));
            }
            LanguageServer::WorkspaceSymbols { query, reply } => {
                reply.finish(optional(analyzer::symbols::workspace(
                    &context,
                    &mut self.symbols,
                    &query,
                )));
            }
            LanguageServer::SemanticTokens { uri, reply } => {
                reply.finish(optional(analyzer::semantic_tokens::implementation(&context, uri)));
            }
            LanguageServer::CodeAction { uri, range, context: action_context, reply } => {
                reply.finish(optional(analyzer::code_action::implementation(
                    &context,
                    uri,
                    range,
                    action_context,
                )));
            }
        }
    }
}

fn version_edits(edit: &mut WorkspaceEdit, files: &FileLifecycle<i32, bool>) {
    let update = |edit: &mut TextDocumentEdit| {
        edit.text_document.version = files
            .source_id(edit.text_document.uri.as_str())
            .and_then(|file_id| files.source_version(file_id));
    };
    match &mut edit.document_changes {
        Some(DocumentChanges::Edits(edits)) => edits.iter_mut().for_each(update),
        Some(DocumentChanges::Operations(operations)) => {
            for operation in operations {
                if let DocumentChangeOperation::Edit(edit) = operation {
                    update(edit);
                }
            }
        }
        None => {}
    }
}
