//! Diagnostic scheduling.
//!
//! Each source has at most one running diagnostic task and one queued collection. A change that
//! invalidates a source advances its generation, so a running task's result is published only
//! if its generation, file, and document version still match when it finishes.

use std::collections::hash_map::Entry;

use building::QueryError;
use building::lifecycle::{AnalysisInvalidation, FileLifecycle, LifecycleChange};
use files::FileId;
use iris_analysis::AnalyzerError;
use iris_analysis::diagnostics::CollectedDiagnostics;
use itertools::Itertools;
use rustc_hash::FxHashMap;
use tokio::sync::mpsc;
use tokio::task;

use crate::analysis::{Snapshot, Workers};
use crate::service::Background;
use crate::state::SourceMetadata;

#[derive(Default)]
pub(crate) struct DiagnosticScheduler {
    generations: FxHashMap<FileId, u64>,
    jobs: FxHashMap<FileId, DiagnosticJob>,
}

struct DiagnosticJob {
    running: DiagnosticTicket,
    queued: Option<DiagnosticTicket>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DiagnosticTicket {
    pub(crate) file_id: FileId,
    generation: u64,
    pub(crate) version: Option<i32>,
}

impl DiagnosticScheduler {
    pub(crate) fn invalidate(
        &mut self,
        change: &LifecycleChange,
        files: &FileLifecycle<i32, SourceMetadata>,
    ) {
        match change.analysis() {
            AnalysisInvalidation::None => {}
            AnalysisInvalidation::Sources(sources) => {
                for file_id in sources {
                    self.invalidate_source(*file_id);
                }
            }
            AnalysisInvalidation::Workspace => {
                let source_ids = files.source_ids();
                let source_ids = source_ids.collect_vec();
                for file_id in source_ids {
                    self.invalidate_source(file_id);
                }
            }
        }
        for removed in change.removed_sources() {
            self.invalidate_source(removed.file_id);
        }
    }

    fn invalidate_source(&mut self, file_id: FileId) {
        let generation = self.generations.entry(file_id).or_default();
        *generation = generation
            .checked_add(1)
            .expect("invariant violated: diagnostic generation overflowed");
        if let Some(job) = self.jobs.get_mut(&file_id) {
            job.queued = None;
        }
    }

    /// Returns a ticket to start now, or `None` if a task for the file is already running; then
    /// the collection is queued behind it.
    pub(crate) fn schedule(
        &mut self,
        file_id: FileId,
        version: Option<i32>,
    ) -> Option<DiagnosticTicket> {
        let generation = self.generations.get(&file_id).copied().unwrap_or_default();
        let ticket = DiagnosticTicket { file_id, generation, version };
        match self.jobs.entry(file_id) {
            Entry::Vacant(entry) => {
                entry.insert(DiagnosticJob { running: ticket, queued: None });
                Some(ticket)
            }
            Entry::Occupied(mut entry) => {
                let job = entry.get_mut();
                if job.running != ticket && job.queued != Some(ticket) {
                    job.queued = Some(ticket);
                }
                None
            }
        }
    }

    pub(crate) fn is_running(&self, ticket: DiagnosticTicket) -> bool {
        self.jobs.get(&ticket.file_id).is_some_and(|job| job.running == ticket)
    }

    pub(crate) fn is_current(&self, ticket: DiagnosticTicket) -> bool {
        self.generations.get(&ticket.file_id).copied().unwrap_or_default() == ticket.generation
    }

    /// Records that the task for `ticket` finished and returns the queued collection to start.
    pub(crate) fn complete(&mut self, ticket: DiagnosticTicket) -> Option<DiagnosticTicket> {
        let job = self.jobs.get_mut(&ticket.file_id)?;
        if job.running != ticket {
            return None;
        }
        let next = job.queued.take();
        if let Some(next) = next {
            job.running = next;
        } else {
            self.jobs.remove(&ticket.file_id);
        }
        next
    }
}

/// Starts a diagnostic task for `ticket`. It reports `Background::DiagnosticsFinished` with no
/// diagnostics if a change to engine inputs arrives while it waits for a permit, or if the
/// change cancels its queries.
pub(crate) fn spawn(
    workers: &Workers,
    snapshot: Snapshot,
    ticket: DiagnosticTicket,
    background: mpsc::UnboundedSender<Background>,
) {
    let permits = workers.diagnostic_permits();
    let mut changes = workers.changes();
    workers.spawn(async move {
        let collected = match Workers::wait_for_permit(&permits, &mut changes).await {
            Some(permit) => {
                let collected = task::spawn_blocking(move || collect(snapshot, ticket)).await;
                drop(permit);
                collected.unwrap_or_else(|error| {
                    tracing::error!("Diagnostic collection failed: {error}");
                    None
                })
            }
            None => None,
        };
        let _ = background.send(Background::DiagnosticsFinished { ticket, collected });
    });
}

fn collect(snapshot: Snapshot, ticket: DiagnosticTicket) -> Option<CollectedDiagnostics> {
    let _span = tracing::info_span!("collect_diagnostics").entered();
    let result = snapshot.with_analyzer_context(|context| {
        iris_analysis::diagnostics::implementation(context, ticket.file_id)
    });
    match result {
        Ok(collected) => Some(collected),
        Err(error @ AnalyzerError::QueryError(QueryError::Cancelled)) => {
            tracing::warn!("AnalyzerError: {error}");
            None
        }
        Err(error) => {
            tracing::error!("AnalyzerError: {error}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use building::QueryEngine;
    use building::lifecycle::{
        DiskObservation, FileLifecycle, LifecycleEvent, SourceEvent, SourceUnitKey,
    };
    use files::Files;

    use super::{DiagnosticScheduler, SourceMetadata};

    fn file_id() -> files::FileId {
        let mut files = Files::default();
        files.insert("file:///src/Main.purs", "module Main where\n")
    }

    #[test]
    fn coalesces_requests_to_the_latest_generation() {
        let file_id = file_id();
        let mut scheduler = DiagnosticScheduler::default();
        let first = scheduler.schedule(file_id, Some(1)).unwrap();

        scheduler.invalidate_source(file_id);
        assert_eq!(scheduler.schedule(file_id, Some(2)), None);
        let queued = scheduler.jobs[&file_id].queued.unwrap();
        assert_eq!(queued.version, Some(2));

        assert_eq!(scheduler.complete(first), Some(queued));
        assert!(scheduler.is_running(queued));
    }

    #[test]
    fn invalidation_discards_a_queued_stale_request() {
        let file_id = file_id();
        let mut scheduler = DiagnosticScheduler::default();
        let first = scheduler.schedule(file_id, Some(1)).unwrap();
        scheduler.invalidate_source(file_id);
        scheduler.schedule(file_id, Some(2));

        scheduler.invalidate_source(file_id);
        assert_eq!(scheduler.jobs[&file_id].queued, None);
        assert_eq!(scheduler.complete(first), None);
        assert!(!scheduler.is_current(first));
    }

    #[test]
    fn workspace_change_invalidates_unrelated_running_diagnostics() {
        let engine = QueryEngine::default();
        let mut lifecycle = FileLifecycle::default();
        let first_unit = SourceUnitKey::new("file:///src/Main.purs", "file:///src/Main.js");
        let event = LifecycleEvent::Source {
            unit: first_unit,
            event: SourceEvent::DiskObserved {
                disk: DiskObservation::Found(Arc::from("module Main where\n")),
                metadata: SourceMetadata::Unmanaged { editable: true },
            },
        };
        lifecycle.apply(&engine, event);
        let first_id = lifecycle.source_id("file:///src/Main.purs").unwrap();

        let mut scheduler = DiagnosticScheduler::default();
        let ticket = scheduler.schedule(first_id, None).unwrap();
        let second_unit = SourceUnitKey::new("file:///src/Library.purs", "file:///src/Library.js");
        let event = LifecycleEvent::Source {
            unit: second_unit,
            event: SourceEvent::DiskObserved {
                disk: DiskObservation::Found(Arc::from("module Library where\n")),
                metadata: SourceMetadata::Unmanaged { editable: true },
            },
        };
        let change = lifecycle.apply(&engine, event);
        scheduler.invalidate(&change, &lifecycle);

        assert!(!scheduler.is_current(ticket));
    }
}
