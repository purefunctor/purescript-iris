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
use parking_lot::{RwLock, RwLockReadGuard};

use super::SourceMetadata;
use super::document::DocumentPath;

pub(super) struct Analysis {
    admission: RwLock<bool>,
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
            admission: RwLock::new(true),
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
        // A queued worker must not occupy a blocking-pool thread waiting for a
        // writer that is retiring snapshots held by other queued workers.
        let admission = self.admission.try_read().ok_or(QueryError::Cancelled)?;
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
        let admission = self.admission.write();
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
        let _admission = self.admission.write();
        self.retire();
        mem::take(&mut *self.suggestions_cache.write());
    }

    pub(super) fn shutdown(&self) {
        let mut admission = self.admission.write();
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
            assert!(analysis.admission.try_write().is_none());
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
    fn queued_diagnostics_cannot_block_retirement_of_queued_requests() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .max_blocking_threads(1)
            .enable_time()
            .build()
            .unwrap();
        let mut analysis = Analysis::new(QueryEngine::default(), FileLifecycle::default());
        let (retiring, retirement) = mpsc::channel();
        analysis.retiring = Some(retiring);
        let analysis = Arc::new(analysis);
        let first = snapshot(&analysis);
        let (entered, entry) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let first = runtime.spawn_blocking(move || {
            entered.send(()).unwrap();
            released.recv().unwrap();
            drop(first);
        });
        entry.recv_timeout(Duration::from_secs(10)).unwrap();

        let diagnostic_analysis = Arc::clone(&analysis);
        let diagnostic = runtime.spawn_blocking(move || {
            diagnostic_analysis.snapshot(
                PositionEncoding::Utf16,
                AnalyzerCapabilities::default(),
                QueryCancellation::default(),
            )
        });
        // Keep a cleanup handle so even a failed assertion can release the writer.
        let request_snapshot = Arc::new(parking_lot::Mutex::new(Some(snapshot(&analysis))));
        let queued_snapshot = Arc::clone(&request_snapshot);
        let request = runtime.spawn_blocking(move || drop(queued_snapshot.lock().take()));
        let writer_analysis = Arc::clone(&analysis);
        let writer = std::thread::spawn(move || writer_analysis.invalidate_suggestions());
        retirement.recv_timeout(Duration::from_secs(10)).unwrap();
        release.send(()).unwrap();

        let diagnostic = runtime
            .block_on(async { tokio::time::timeout(Duration::from_secs(10), diagnostic).await });
        drop(request_snapshot.lock().take());
        writer.join().unwrap();
        runtime.block_on(async {
            first.await.unwrap();
            request.await.unwrap();
        });
        assert!(matches!(diagnostic, Ok(Ok(Err(QueryError::Cancelled)))));
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
