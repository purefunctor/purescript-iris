//! Snapshots, the `AnalyzerHost` implementation, and analysis tasks and their cancellation.
//!
//! Analysis requests and diagnostic collection read a snapshot and run in parallel, each limited
//! by its own semaphore. Before a change to the query engine's inputs is applied, every task still
//! waiting for a permit drops its snapshot; applying the change calls
//! `QueryEngine::request_cancel`, whose cancelled flag stops running tasks at their next query.
//! A change therefore waits only until running tasks notice the flag.

use std::any::Any;
use std::future::Future;
use std::mem;
use std::panic::{AssertUnwindSafe, catch_unwind};
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
use iris_lsp_server::{Answer, Rejection};
use lsp_types::Url;
use parking_lot::{RwLock, RwLockReadGuard};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, oneshot, watch};
use tokio::task;
use tokio_util::task::TaskTracker;

use crate::diagnostics::DiagnosticScheduler;
use crate::handlers::{AnalysisJob, DocumentError};
use crate::state::SourceMetadata;

/// The message for analysis cancelled by a change to engine inputs.
pub(crate) const CONTENT_MODIFIED: &str = "Content modified";

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

    /// Applies lifecycle events to the query engine.
    ///
    /// `QueryEngine::request_cancel` waits until every snapshot is dropped, so this must run on
    /// a blocking thread, after tasks waiting for permits were told to drop theirs.
    pub(crate) fn apply(
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

    pub(crate) fn invalidate_suggestions_cache(&self) {
        let mut cache = self.suggestions_cache.write();
        mem::take(&mut *cache);
    }

    pub(crate) fn document_content(
        &self,
        document: DocumentKind,
        uri: &Url,
    ) -> Result<Arc<str>, DocumentError> {
        let files = self.files.read();
        match document {
            DocumentKind::Source => {
                let file_id = files
                    .source_id(uri.as_str())
                    .ok_or_else(|| DocumentError::InvalidContentChange(Url::clone(uri)))?;
                self.engine.content(file_id).map_err(DocumentError::from)
            }
            DocumentKind::Foreign(_) => {
                let file_id = files
                    .foreign_id(uri.as_str())
                    .ok_or_else(|| DocumentError::InvalidContentChange(Url::clone(uri)))?;
                self.engine
                    .foreign_content(file_id)
                    .ok_or_else(|| DocumentError::InvalidContentChange(Url::clone(uri)))
            }
        }
    }
}

impl Snapshot {
    #[cfg(test)]
    pub(crate) fn first_source(&self) -> FileId {
        self.files.read().source_ids().next().expect("invariant violated: no sources")
    }

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

/// Analysis and diagnostic tasks, the permits that limit them, and the signal that makes tasks
/// still waiting for a permit drop their snapshots.
pub(crate) struct Workers {
    analysis_permits: Arc<Semaphore>,
    diagnostic_permits: Arc<Semaphore>,
    changes: Arc<watch::Sender<u64>>,
    tracker: TaskTracker,
}

/// Tells tasks waiting for a permit that a change to engine inputs is about to be applied.
#[derive(Clone)]
pub(crate) struct ChangeSignal(Arc<watch::Sender<u64>>);

impl ChangeSignal {
    pub(crate) fn send(&self) {
        self.0.send_modify(|changes| *changes = changes.wrapping_add(1));
    }
}

impl Workers {
    pub(crate) fn new(analysis_permits: usize, diagnostic_permits: usize) -> Workers {
        let (changes, _) = watch::channel(0);
        Workers {
            analysis_permits: Arc::new(Semaphore::new(analysis_permits.max(1))),
            diagnostic_permits: Arc::new(Semaphore::new(diagnostic_permits.max(1))),
            changes: Arc::new(changes),
            tracker: TaskTracker::new(),
        }
    }

    pub(crate) fn change_signal(&self) -> ChangeSignal {
        ChangeSignal(Arc::clone(&self.changes))
    }

    pub(crate) fn changes(&self) -> watch::Receiver<u64> {
        self.changes.subscribe()
    }

    pub(crate) fn diagnostic_permits(&self) -> Arc<Semaphore> {
        Arc::clone(&self.diagnostic_permits)
    }

    pub(crate) fn spawn(&self, task: impl Future<Output = ()> + Send + 'static) {
        self.tracker.spawn(task);
    }

    /// Waits for a permit, or returns `None` if a change to engine inputs arrives first.
    ///
    /// `changes` must be subscribed before the snapshot the caller holds was taken, so that a
    /// change applied after the snapshot is always observed.
    pub(crate) async fn wait_for_permit(
        permits: &Arc<Semaphore>,
        changes: &mut watch::Receiver<u64>,
    ) -> Option<OwnedSemaphorePermit> {
        let permit = tokio::select! {
            biased;
            _ = changes.changed() => return None,
            permit = Arc::clone(permits).acquire_owned() => {
                permit.expect("invariant violated: a worker semaphore was closed")
            }
        };
        // A change that arrived together with the permit still wins.
        if changes.has_changed().unwrap_or(true) {
            return None;
        }
        Some(permit)
    }

    /// Starts an analysis task. It answers `ContentModified` if a change to engine inputs arrives
    /// while it waits for a permit or cancels its queries, and it stops without answering if the
    /// request is cancelled first. Either way the snapshot is dropped.
    pub(crate) fn spawn_analysis(
        &self,
        method: String,
        snapshot: Snapshot,
        job: AnalysisJob,
        mut reply: oneshot::Sender<Answer>,
        mut changes: watch::Receiver<u64>,
    ) {
        let permits = Arc::clone(&self.analysis_permits);
        self.tracker.spawn(async move {
            let permit = tokio::select! {
                biased;
                () = reply.closed() => return,
                permit = Workers::wait_for_permit(&permits, &mut changes) => permit,
            };
            let Some(permit) = permit else {
                drop(snapshot);
                let _ = reply.send(Err(Rejection::ContentModified(CONTENT_MODIFIED.to_string())));
                return;
            };
            if reply.is_closed() {
                return;
            }
            let answer = task::spawn_blocking(move || run_job(&method, snapshot, job)).await;
            drop(permit);
            let answer = answer.unwrap_or_else(|error| Err(Rejection::Internal(error.to_string())));
            let _ = reply.send(answer);
        });
    }

    /// Waits for every task after telling the waiting ones to drop their snapshots.
    pub(crate) async fn close(&self) {
        self.change_signal().send();
        self.tracker.close();
        self.tracker.wait().await;
    }
}

/// Runs an analysis job, turning a panic into an internal error that names the method, so that
/// one failing handler answers its own request instead of stopping the server.
fn run_job(method: &str, snapshot: Snapshot, job: AnalysisJob) -> Answer {
    let answer = catch_unwind(AssertUnwindSafe(|| job(&snapshot)));
    drop(snapshot);
    answer.unwrap_or_else(|payload| {
        let message = panic_message(payload);
        Err(Rejection::Internal(format!("Request handler of {method} panicked: {message}")))
    })
}

fn panic_message(payload: Box<dyn Any + Send>) -> String {
    match payload.downcast::<String>() {
        Ok(message) => *message,
        Err(payload) => match payload.downcast::<&'static str>() {
            Ok(message) => (*message).to_string(),
            Err(_) => "unknown".to_string(),
        },
    }
}
