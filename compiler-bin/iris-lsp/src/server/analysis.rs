use std::mem;
use std::sync::Arc;

use building::QueryEngine;
use building::lifecycle::{
    AnalysisInvalidation, DocumentKind, FileLifecycle, LifecycleChange, LifecycleEvent,
};
use files::FileId;
use iris_analysis::completion::SuggestionsCache;
use iris_analysis::position::PositionEncoding;
use iris_analysis::symbols::WorkspaceSymbolsCache;
use iris_analysis::{AnalyzerCapabilities, AnalyzerContext, AnalyzerHost};
use lsp_types::Url;
use parking_lot::{RwLock, RwLockReadGuard};

use super::SourceMetadata;
use super::error::LspError;
use super::event::DiagnosticScheduler;

pub(super) struct Analysis {
    pub(super) engine: QueryEngine,
    pub(super) files: Arc<RwLock<FileLifecycle<i32, SourceMetadata>>>,
    pub(super) workspace_symbols_cache: Arc<RwLock<WorkspaceSymbolsCache>>,
    pub(super) suggestions_cache: Arc<RwLock<SuggestionsCache>>,
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
    ) -> StateSnapshot {
        StateSnapshot {
            engine: self.engine.snapshot(),
            files: Arc::clone(&self.files),
            workspace_symbols_cache: Arc::clone(&self.workspace_symbols_cache),
            suggestions_cache: Arc::clone(&self.suggestions_cache),
            position_encoding,
            analyzer_capabilities,
        }
    }

    pub(super) fn apply(
        &self,
        events: impl IntoIterator<Item = LifecycleEvent<i32, SourceMetadata>>,
        diagnostics: &mut DiagnosticScheduler,
    ) -> LifecycleChange {
        self.engine.request_cancel();
        let mut change = LifecycleChange::default();
        {
            let mut files = self.files.write();
            for event in events {
                change.combine(files.apply(&self.engine, event));
            }
        }

        {
            let files = self.files.read();
            diagnostics.invalidate(&change, &files);
        }
        if !matches!(change.analysis(), AnalysisInvalidation::None) {
            {
                let mut symbols = self.workspace_symbols_cache.write();
                mem::take(&mut *symbols);
            }
            self.invalidate_suggestions_cache();
        }
        change
    }

    pub(super) fn invalidate_suggestions_cache(&self) {
        let mut cache = self.suggestions_cache.write();
        mem::take(&mut *cache);
    }

    pub(super) fn document_content(
        &self,
        document: DocumentKind,
        uri: &Url,
    ) -> Result<Arc<str>, LspError> {
        let files = self.files.read();
        match document {
            DocumentKind::Source => {
                let file_id = files
                    .source_id(uri.as_str())
                    .ok_or_else(|| LspError::InvalidContentChange(Url::clone(uri)))?;
                self.engine.content(file_id).map_err(LspError::from)
            }
            DocumentKind::Foreign(_) => {
                let file_id = files
                    .foreign_id(uri.as_str())
                    .ok_or_else(|| LspError::InvalidContentChange(Url::clone(uri)))?;
                self.engine
                    .foreign_content(file_id)
                    .ok_or_else(|| LspError::InvalidContentChange(Url::clone(uri)))
            }
        }
    }
}

pub(super) struct StateSnapshot {
    pub(super) engine: QueryEngine,
    files: Arc<RwLock<FileLifecycle<i32, SourceMetadata>>>,
    pub(super) workspace_symbols_cache: Arc<RwLock<WorkspaceSymbolsCache>>,
    pub(super) suggestions_cache: Arc<RwLock<SuggestionsCache>>,
    position_encoding: PositionEncoding,
    analyzer_capabilities: AnalyzerCapabilities,
}

impl StateSnapshot {
    pub(super) fn with_analyzer_context<T>(
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

pub(super) struct LspAnalyzerHost<'a> {
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
