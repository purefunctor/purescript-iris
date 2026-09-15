use std::mem;
use std::sync::Arc;

use analyzer::completion::SuggestionsCache;
use analyzer::position::PositionEncoding;
use analyzer::symbols::WorkspaceSymbolsCache;
use analyzer::{AnalyzerCapabilities, AnalyzerContext, AnalyzerHost};
use building::lifecycle::{AnalysisInvalidation, FileLifecycle, LifecycleChange, LifecycleEvent};
use building::{QueryCancellation, QueryEngine, QueryError};
use files::FileId;
use lsp_types::Url;
use parking_lot::{Mutex, RwLock, RwLockReadGuard};

use super::SourceMetadata;
use super::document::DocumentPath;

pub(super) struct Analysis {
    admission: Mutex<bool>,
    pub(super) engine: QueryEngine,
    pub(super) files: Arc<RwLock<FileLifecycle<i32, SourceMetadata>>>,
    pub(super) workspace_symbols_cache: Arc<RwLock<WorkspaceSymbolsCache>>,
    pub(super) suggestions_cache: Arc<RwLock<SuggestionsCache>>,
    #[cfg(test)]
    retiring: Option<std::sync::mpsc::Sender<()>>,
}

impl Analysis {
    pub(super) fn new(engine: QueryEngine, files: FileLifecycle<i32, SourceMetadata>) -> Analysis {
        Analysis {
            admission: Mutex::new(true),
            engine,
            files: Arc::new(RwLock::new(files)),
            workspace_symbols_cache: Arc::new(RwLock::new(WorkspaceSymbolsCache::default())),
            suggestions_cache: Arc::new(RwLock::new(SuggestionsCache::default())),
            #[cfg(test)]
            retiring: None,
        }
    }

    pub(super) fn snapshot(
        &self,
        position_encoding: PositionEncoding,
        analyzer_capabilities: AnalyzerCapabilities,
        cancellation: QueryCancellation,
    ) -> Result<AnalysisSnapshot, QueryError> {
        let admission = self.admission.lock();
        if !*admission {
            return Err(QueryError::Cancelled);
        }
        cancellation.check()?;
        Ok(AnalysisSnapshot {
            engine: self.engine.scoped_snapshot(cancellation),
            files: Arc::clone(&self.files),
            workspace_symbols_cache: Arc::clone(&self.workspace_symbols_cache),
            suggestions_cache: Arc::clone(&self.suggestions_cache),
            position_encoding,
            analyzer_capabilities,
        })
    }

    pub(super) fn apply(
        &self,
        events: impl IntoIterator<Item = LifecycleEvent<i32, SourceMetadata>>,
    ) -> LifecycleChange {
        let admission = self.admission.lock();
        if !*admission {
            return LifecycleChange::default();
        }
        self.retire();

        let mut change = LifecycleChange::default();
        {
            let mut files = self.files.write();
            for event in events {
                change.combine(files.apply(&self.engine, event));
            }
        }
        if !matches!(change.analysis(), AnalysisInvalidation::None) {
            mem::take(&mut *self.workspace_symbols_cache.write());
            mem::take(&mut *self.suggestions_cache.write());
        }
        change
    }

    pub(super) fn invalidate_suggestions(&self) {
        let _admission = self.admission.lock();
        self.retire();
        mem::take(&mut *self.suggestions_cache.write());
    }

    pub(super) fn shutdown(&self) {
        let mut admission = self.admission.lock();
        *admission = false;
        self.retire();
    }

    fn retire(&self) {
        #[cfg(test)]
        if let Some(retiring) = &self.retiring {
            retiring.send(()).unwrap();
        }
        self.engine.request_cancel();
    }
}

pub(super) struct AnalysisSnapshot {
    pub(super) engine: QueryEngine,
    files: Arc<RwLock<FileLifecycle<i32, SourceMetadata>>>,
    pub(super) workspace_symbols_cache: Arc<RwLock<WorkspaceSymbolsCache>>,
    pub(super) suggestions_cache: Arc<RwLock<SuggestionsCache>>,
    position_encoding: PositionEncoding,
    analyzer_capabilities: AnalyzerCapabilities,
}

impl AnalysisSnapshot {
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
        let uri = Url::parse(uri).ok()?;
        let uri = DocumentPath::from_uri(&uri).ok()?.uri().ok()?;
        self.files.source_id(uri.as_str())
    }

    fn file_uri(&self, file_id: FileId) -> Result<Option<Url>, url::ParseError> {
        self.files.source_path(file_id).map(|uri| Url::parse(&uri)).transpose()
    }

    fn active_files(&self) -> impl Iterator<Item = FileId> {
        self.files.source_ids()
    }

    fn is_editable(&self, file_id: FileId) -> bool {
        self.files.source_metadata(file_id).is_some_and(SourceMetadata::editable)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Barrier, mpsc};
    use std::time::Duration;

    use building::{ForeignEvent, SourceUnitKey};
    use files::ForeignSourceKind;

    use super::*;

    fn snapshot(analysis: &Analysis) -> AnalysisSnapshot {
        analysis
            .snapshot(
                PositionEncoding::Utf16,
                AnalyzerCapabilities::default(),
                QueryCancellation::default(),
            )
            .unwrap()
    }

    #[test]
    fn independent_contexts_execute_in_parallel() {
        let analysis = Analysis::new(QueryEngine::default(), FileLifecycle::default());
        let readers = Barrier::new(2);
        std::thread::scope(|scope| {
            for _ in 0..2 {
                scope.spawn(|| {
                    let snapshot = snapshot(&analysis);
                    snapshot.with_analyzer_context(|context| {
                        readers.wait();
                        assert_eq!(context.active_files().count(), 0);
                    });
                });
            }
        });
    }

    #[test]
    fn writer_retires_snapshots_before_locking_files() {
        let mut analysis = Analysis::new(QueryEngine::default(), FileLifecycle::default());
        let (retiring, retirement) = mpsc::channel();
        analysis.retiring = Some(retiring);
        let before = snapshot(&analysis);
        let unit = SourceUnitKey::new("Main.purs", "Main.js");
        std::thread::scope(|scope| {
            let writer = scope.spawn(|| {
                analysis.apply([LifecycleEvent::Foreign {
                    unit,
                    kind: ForeignSourceKind::JavaScript,
                    event: ForeignEvent::Opened { text: Arc::from("buffer"), version: 3 },
                }])
            });
            retirement.recv_timeout(Duration::from_secs(10)).unwrap();
            assert!(analysis.admission.try_lock().is_none());
            let files = before.files.try_read().expect("writer locked files before retirement");
            assert!(files.foreign_id("Main.js").is_none());
            drop(files);
            drop(before);
            writer.join().unwrap();
        });
        let after = snapshot(&analysis);
        let file_id = after.files.read().foreign_id("Main.js").unwrap();
        assert_eq!(after.engine.foreign_content(file_id).as_deref(), Some("buffer"));
    }

    #[test]
    fn invalidation_waits_for_late_cache_writes_and_shutdown_rejects_reads() {
        let mut analysis = Analysis::new(QueryEngine::default(), FileLifecycle::default());
        let (retiring, retirement) = mpsc::channel();
        analysis.retiring = Some(retiring);
        let before = snapshot(&analysis);
        std::thread::scope(|scope| {
            let writer = scope.spawn(|| analysis.invalidate_suggestions());
            retirement.recv_timeout(Duration::from_secs(10)).unwrap();
            before.suggestions_cache.write().insert("late".to_string(), Arc::default());
            drop(before);
            writer.join().unwrap();
        });
        assert!(analysis.suggestions_cache.read().get("late").is_none());
        analysis.shutdown();
        assert!(matches!(
            analysis.snapshot(
                PositionEncoding::Utf16,
                AnalyzerCapabilities::default(),
                QueryCancellation::default()
            ),
            Err(QueryError::Cancelled)
        ));
    }
}
