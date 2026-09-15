use std::sync::Arc;

use analyzer::completion::SuggestionsCache;
use analyzer::position::PositionEncoding;
use analyzer::symbols::WorkspaceSymbolsCache;
use analyzer::{AnalyzerCapabilities, AnalyzerContext, AnalyzerHost};
use building::QueryEngine;
use building::lifecycle::{AnalysisInvalidation, FileLifecycle, LifecycleChange, LifecycleEvent};
use files::{FileId, ForeignFileId};
use lsp_types::*;
use parking_lot::{RwLock, RwLockReadGuard};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum SourceMetadata {
    Builtin,
    Package { editable: bool },
    Unmanaged { editable: bool },
}

impl SourceMetadata {
    pub(super) fn editable(&self) -> bool {
        match self {
            SourceMetadata::Builtin => false,
            SourceMetadata::Package { editable } | SourceMetadata::Unmanaged { editable } => {
                *editable
            }
        }
    }
}

/// Mutable compiler state with one write barrier and independently executable read snapshots.
pub(super) struct Analysis {
    engine: QueryEngine,
    files: Arc<RwLock<FileLifecycle<i32, SourceMetadata>>>,
    workspace_symbols_cache: Arc<RwLock<WorkspaceSymbolsCache>>,
    suggestions_cache: Arc<RwLock<SuggestionsCache>>,
}

impl Analysis {
    pub(super) fn new(engine: QueryEngine, files: FileLifecycle<i32, SourceMetadata>) -> Analysis {
        Analysis {
            engine,
            files: Arc::new(RwLock::new(files)),
            workspace_symbols_cache: Arc::new(RwLock::new(WorkspaceSymbolsCache::default())),
            suggestions_cache: Arc::new(RwLock::new(SuggestionsCache::default())),
        }
    }

    pub(super) fn snapshot(
        &self,
        position_encoding: PositionEncoding,
        analyzer_capabilities: AnalyzerCapabilities,
    ) -> AnalysisSnapshot {
        AnalysisSnapshot {
            engine: self.engine.snapshot(),
            files: Arc::clone(&self.files),
            workspace_symbols_cache: Arc::clone(&self.workspace_symbols_cache),
            suggestions_cache: Arc::clone(&self.suggestions_cache),
            position_encoding,
            analyzer_capabilities,
        }
    }

    /// Cancel and retire every admitted snapshot before changing compiler or file state.
    pub(super) fn apply(
        &mut self,
        events: impl IntoIterator<Item = LifecycleEvent<i32, SourceMetadata>>,
    ) -> LifecycleChange {
        self.engine.request_cancel();
        let mut change = LifecycleChange::default();
        {
            let mut files = self.files.write();
            for event in events {
                change.combine(files.apply(&self.engine, event));
            }
        }
        if !matches!(change.analysis(), AnalysisInvalidation::None) {
            *self.workspace_symbols_cache.write() = WorkspaceSymbolsCache::default();
            *self.suggestions_cache.write() = SuggestionsCache::default();
        }
        change
    }

    pub(super) fn cancel(&mut self) {
        self.engine.request_cancel();
    }

    pub(super) fn content(&self, file_id: FileId) -> building::QueryResult<Arc<str>> {
        self.engine.content(file_id)
    }

    pub(super) fn foreign_content(&self, file_id: ForeignFileId) -> Option<Arc<str>> {
        self.engine.foreign_content(file_id)
    }

    #[cfg(test)]
    pub(super) fn foreign_file(&self, file_id: FileId) -> Option<ForeignFileId> {
        self.engine.foreign_file(file_id)
    }

    pub(super) fn invalidate_suggestions(&self) {
        *self.suggestions_cache.write() = SuggestionsCache::default();
    }

    pub(super) fn with_files<T>(
        &self,
        action: impl FnOnce(&FileLifecycle<i32, SourceMetadata>) -> T,
    ) -> T {
        action(&self.files.read())
    }
}

pub(super) struct AnalysisSnapshot {
    engine: QueryEngine,
    files: Arc<RwLock<FileLifecycle<i32, SourceMetadata>>>,
    workspace_symbols_cache: Arc<RwLock<WorkspaceSymbolsCache>>,
    suggestions_cache: Arc<RwLock<SuggestionsCache>>,
    position_encoding: PositionEncoding,
    analyzer_capabilities: AnalyzerCapabilities,
}

impl AnalysisSnapshot {
    fn with_analyzer_context<T>(
        &self,
        action: impl FnOnce(&AnalyzerContext<SnapshotHost<'_>>) -> T,
    ) -> T {
        let files = self.files.read();
        let host = SnapshotHost { queries: &self.engine, files };
        let context =
            AnalyzerContext::new(&host, self.position_encoding, self.analyzer_capabilities);
        action(&context)
    }

    pub(super) fn definition(
        &self,
        uri: Url,
        position: Position,
    ) -> Result<Option<GotoDefinitionResponse>, analyzer::AnalyzerError> {
        self.with_analyzer_context(|context| {
            analyzer::definition::implementation(context, uri, position)
        })
    }

    pub(super) fn hover(
        &self,
        uri: Url,
        position: Position,
    ) -> Result<Option<Hover>, analyzer::AnalyzerError> {
        self.with_analyzer_context(|context| {
            analyzer::hover::implementation(context, uri, position)
        })
    }

    pub(super) fn code_action(
        &self,
        uri: Url,
        range: Range,
        context: CodeActionContext,
    ) -> Result<Option<CodeActionResponse>, analyzer::AnalyzerError> {
        self.with_analyzer_context(|language| {
            analyzer::code_action::implementation(language, uri, range, context)
        })
    }

    pub(super) fn completion(
        &self,
        uri: Url,
        position: Position,
    ) -> Result<Option<CompletionResponse>, analyzer::AnalyzerError> {
        let mut cache = self.suggestions_cache.write();
        self.with_analyzer_context(|context| {
            analyzer::completion::implementation(context, &mut cache, uri, position)
        })
    }

    pub(super) fn resolve_completion(
        &self,
        item: CompletionItem,
    ) -> Result<CompletionItem, (analyzer::AnalyzerError, CompletionItem)> {
        analyzer::completion::resolve::implementation(&self.engine, item)
    }

    pub(super) fn references(
        &self,
        uri: Url,
        position: Position,
    ) -> Result<Option<Vec<Location>>, analyzer::AnalyzerError> {
        self.with_analyzer_context(|context| {
            analyzer::references::implementation(context, uri, position)
        })
    }

    pub(super) fn rename(
        &self,
        uri: Url,
        position: Position,
        new_name: String,
    ) -> Result<Option<WorkspaceEdit>, analyzer::AnalyzerError> {
        self.with_analyzer_context(|context| {
            analyzer::rename::implementation(context, uri, position, new_name)
        })
    }

    pub(super) fn prepare_rename(
        &self,
        uri: Url,
        position: Position,
    ) -> Result<Option<PrepareRenameResponse>, analyzer::AnalyzerError> {
        self.with_analyzer_context(|context| analyzer::rename::prepare(context, uri, position))
    }

    pub(super) fn document_highlight(
        &self,
        uri: Url,
        position: Position,
    ) -> Result<Option<Vec<DocumentHighlight>>, analyzer::AnalyzerError> {
        self.with_analyzer_context(|context| {
            analyzer::document_highlight::implementation(context, uri, position)
        })
    }

    pub(super) fn workspace_symbols(
        &self,
        query: &str,
    ) -> Result<Option<WorkspaceSymbolResponse>, analyzer::AnalyzerError> {
        let mut cache = self.workspace_symbols_cache.write();
        self.with_analyzer_context(|context| {
            analyzer::symbols::workspace(context, &mut cache, query)
        })
    }

    pub(super) fn document_symbols(
        &self,
        uri: Url,
    ) -> Result<Option<DocumentSymbolResponse>, analyzer::AnalyzerError> {
        self.with_analyzer_context(|context| analyzer::symbols::document(context, uri))
    }

    pub(super) fn semantic_tokens(
        &self,
        uri: Url,
    ) -> Result<Option<SemanticTokens>, analyzer::AnalyzerError> {
        self.with_analyzer_context(|context| {
            analyzer::semantic_tokens::implementation(context, uri)
        })
    }

    pub(super) fn diagnostics(
        &self,
        file_id: FileId,
    ) -> Result<analyzer::diagnostics::CollectedDiagnostics, analyzer::AnalyzerError> {
        self.with_analyzer_context(|context| {
            analyzer::diagnostics::implementation(context, file_id)
        })
    }
}

struct SnapshotHost<'a> {
    queries: &'a QueryEngine,
    files: RwLockReadGuard<'a, FileLifecycle<i32, SourceMetadata>>,
}

impl AnalyzerHost for SnapshotHost<'_> {
    type Queries = QueryEngine;

    fn queries(&self) -> &QueryEngine {
        self.queries
    }

    fn file_id(&self, uri: &str) -> Option<FileId> {
        self.files.source_id(uri)
    }

    fn file_uri(&self, file_id: FileId) -> Result<Option<lsp_types::Url>, url::ParseError> {
        self.files.source_path(file_id).map(|uri| lsp_types::Url::parse(&uri)).transpose()
    }

    fn active_files(&self) -> impl Iterator<Item = FileId> {
        self.files.source_ids()
    }

    fn is_editable(&self, file_id: FileId) -> bool {
        self.files.source_metadata(file_id).is_some_and(SourceMetadata::editable)
    }
}
