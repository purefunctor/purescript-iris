use std::mem;
use std::sync::Arc;

use analyzer::completion::SuggestionsCache;
use analyzer::position::PositionEncoding;
use analyzer::symbols::WorkspaceSymbolsCache;
use analyzer::{AnalyzerCapabilities, AnalyzerContext, AnalyzerHost};
use building::lifecycle::{AnalysisInvalidation, FileLifecycle, LifecycleChange, LifecycleEvent};
use building::{Cancellation, QueryEngine};
use files::FileId;
use iris_build::compilation::DocumentPath;
use lsp_types::Url;
use parking_lot::{Mutex, RwLock, RwLockReadGuard};

use super::SourceMetadata;

pub(super) struct Analysis {
    admission: Mutex<()>,
    pub(super) engine: QueryEngine,
    pub(super) files: Arc<RwLock<FileLifecycle<i32, SourceMetadata>>>,
    pub(super) workspace_symbols_cache: Arc<RwLock<WorkspaceSymbolsCache>>,
    pub(super) suggestions_cache: Arc<RwLock<SuggestionsCache>>,
    #[cfg(test)]
    write_admitted: Mutex<Option<std::sync::mpsc::Sender<()>>>,
}

impl Analysis {
    pub(super) fn new(engine: QueryEngine, files: FileLifecycle<i32, SourceMetadata>) -> Analysis {
        Analysis {
            admission: Mutex::new(()),
            engine,
            files: Arc::new(RwLock::new(files)),
            workspace_symbols_cache: Arc::default(),
            suggestions_cache: Arc::default(),
            #[cfg(test)]
            write_admitted: Mutex::default(),
        }
    }

    pub(super) fn snapshot(
        &self,
        position_encoding: PositionEncoding,
        analyzer_capabilities: AnalyzerCapabilities,
        cancellation: Cancellation,
    ) -> AnalysisSnapshot {
        let _admission = self.admission.lock();
        AnalysisSnapshot {
            engine: self.engine.scoped_snapshot(cancellation),
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
    ) -> LifecycleChange {
        // Admission spans the whole write, but readers do not retain it.
        // Retire snapshots before taking locks they may need to finish.
        let _admission = self.admission.lock();
        #[cfg(test)]
        if let Some(admitted) = self.write_admitted.lock().take() {
            admitted.send(()).unwrap();
        }
        self.engine.request_cancel();

        let mut change = LifecycleChange::default();
        {
            let mut files = self.files.write();
            for event in events {
                change.combine(files.apply(&self.engine, event));
            }
        }

        // No previous snapshot can repopulate these caches after retirement.
        // New snapshots remain excluded until both lifecycle and caches agree.
        if !matches!(change.analysis(), AnalysisInvalidation::None) {
            mem::take(&mut *self.workspace_symbols_cache.write());
            mem::take(&mut *self.suggestions_cache.write());
        }
        change
    }

    pub(super) fn invalidate_suggestions(&self) {
        let _admission = self.admission.lock();
        self.engine.request_cancel();
        mem::take(&mut *self.suggestions_cache.write());
    }

    #[cfg(test)]
    pub(super) fn notify_when_write_admitted(&self, sender: std::sync::mpsc::Sender<()>) {
        *self.write_admitted.lock() = Some(sender);
    }
}

pub(super) struct AnalysisSnapshot {
    pub(super) engine: QueryEngine,
    pub(super) files: Arc<RwLock<FileLifecycle<i32, SourceMetadata>>>,

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

    use building::lifecycle::{DiskObservation, SourceEvent, SourceUnitKey};

    use super::*;

    fn snapshot(analysis: &Analysis) -> AnalysisSnapshot {
        analysis.snapshot(
            PositionEncoding::Utf16,
            AnalyzerCapabilities::default(),
            Cancellation::new(),
        )
    }

    #[test]
    fn independent_analysis_reads_overlap() {
        let analysis = Analysis::new(QueryEngine::default(), FileLifecycle::default());
        let readers = Barrier::new(2);
        std::thread::scope(|scope| {
            for _ in 0..2 {
                scope.spawn(|| {
                    let snapshot = snapshot(&analysis);
                    snapshot.with_analyzer_context(|_| readers.wait());
                });
            }
        });
    }

    #[test]
    fn write_retires_readers_before_mutating_files_and_caches() {
        let directory = tempfile::tempdir().unwrap();
        let path = DocumentPath::new(&directory.path().join("Main.purs")).unwrap();
        let unit = path.source_unit().unwrap();
        let analysis = Analysis::new(QueryEngine::default(), FileLifecycle::default());
        let event = LifecycleEvent::Source {
            unit: SourceUnitKey::clone(&unit),
            event: SourceEvent::DiskObserved {
                disk: DiskObservation::Found(Arc::from("module Main where\nold = 1\n")),
                metadata: SourceMetadata::Unmanaged { editable: true },
            },
        };
        analysis.apply([event]);
        let previous = snapshot(&analysis);
        let file_id = analysis.files.read().source_id(unit.source()).unwrap();
        let (admitted, admission) = mpsc::channel();
        *analysis.write_admitted.lock() = Some(admitted);

        std::thread::scope(|scope| {
            let writer = scope.spawn(|| {
                let event = LifecycleEvent::Source {
                    unit: SourceUnitKey::clone(&unit),
                    event: SourceEvent::DiskObserved {
                        disk: DiskObservation::Found(Arc::from("module Main where\nnew = 2\n")),
                        metadata: SourceMetadata::Unmanaged { editable: true },
                    },
                };
                analysis.apply([event]);
            });
            admission.recv().unwrap();

            // A waiting writer must leave both reader locks available. Cache
            // writes made by this last reader must be cleared after retirement.
            previous.with_analyzer_context(|context| {
                assert_eq!(context.file_id(path.uri().unwrap().as_str()), Some(file_id));
                assert_eq!(
                    previous.engine.content(file_id).unwrap().as_ref(),
                    "module Main where\nold = 1\n"
                );
            });
            previous.workspace_symbols_cache.write().insert("old".to_string(), Arc::new(vec![]));
            drop(previous);
            writer.join().unwrap();
        });

        let current = snapshot(&analysis);
        assert_eq!(
            current.engine.content(file_id).unwrap().as_ref(),
            "module Main where\nnew = 2\n"
        );
        assert!(current.workspace_symbols_cache.read().get("old").is_none());
        current.with_analyzer_context(|context| {
            assert_eq!(context.file_id(path.uri().unwrap().as_str()), Some(file_id));
        });
    }
}
