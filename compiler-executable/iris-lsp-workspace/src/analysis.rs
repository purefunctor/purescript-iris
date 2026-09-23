//! Snapshots and the `AnalyzerHost` implementation.

use std::sync::Arc;

use building::QueryEngine;
use building::lifecycle::FileLifecycle;
use files::FileId;
use iris_analysis::completion::SuggestionsCache;
use iris_analysis::position::PositionEncoding;
use iris_analysis::symbols::WorkspaceSymbolsCache;
use iris_analysis::{AnalyzerCapabilities, AnalyzerContext, AnalyzerHost};
use lsp_types::Url;
use parking_lot::{RwLock, RwLockReadGuard};

use crate::state::SourceMetadata;

/// The query engine and the state analysis reads beside it.
pub(crate) struct Analysis {
    pub(crate) engine: QueryEngine,
    pub(crate) files: Arc<RwLock<FileLifecycle<i32, SourceMetadata>>>,
    pub(crate) workspace_symbols_cache: Arc<RwLock<WorkspaceSymbolsCache>>,
    pub(crate) suggestions_cache: Arc<RwLock<SuggestionsCache>>,
}

/// A read-only view of [`Analysis`] for one analysis request or diagnostic collection.
///
/// A snapshot holds a read lock on the query engine until it is dropped, and a change to the
/// engine's inputs waits for every snapshot to be dropped.
pub(crate) struct Snapshot {
    pub(crate) engine: QueryEngine,
    files: Arc<RwLock<FileLifecycle<i32, SourceMetadata>>>,
    pub(crate) workspace_symbols_cache: Arc<RwLock<WorkspaceSymbolsCache>>,
    pub(crate) suggestions_cache: Arc<RwLock<SuggestionsCache>>,
    position_encoding: PositionEncoding,
    analyzer_capabilities: AnalyzerCapabilities,
}

impl Analysis {
    pub(crate) fn new(engine: QueryEngine, files: FileLifecycle<i32, SourceMetadata>) -> Analysis {
        Analysis {
            engine,
            files: Arc::new(RwLock::new(files)),
            workspace_symbols_cache: Arc::new(RwLock::new(WorkspaceSymbolsCache::default())),
            suggestions_cache: Arc::new(RwLock::new(SuggestionsCache::default())),
        }
    }

    pub(crate) fn snapshot(
        &self,
        position_encoding: PositionEncoding,
        analyzer_capabilities: AnalyzerCapabilities,
    ) -> Snapshot {
        Snapshot {
            engine: self.engine.snapshot(),
            files: Arc::clone(&self.files),
            workspace_symbols_cache: Arc::clone(&self.workspace_symbols_cache),
            suggestions_cache: Arc::clone(&self.suggestions_cache),
            position_encoding,
            analyzer_capabilities,
        }
    }
}

impl Snapshot {
    pub(crate) fn with_analyzer_context<T>(
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

pub(crate) struct LspAnalyzerHost<'a> {
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
