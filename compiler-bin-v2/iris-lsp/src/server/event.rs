use std::collections::hash_map::Entry;

use analyzer::diagnostics::CollectedDiagnostics;
use async_lsp::{ClientSocket, LanguageClient};
use building::lifecycle::{AnalysisInvalidation, FileLifecycle, LifecycleChange};
use files::FileId;
use itertools::Itertools;
use lsp_types::{PublishDiagnosticsParams, Url};
use rustc_hash::FxHashMap;
use tokio::task;

use crate::server::error::LspError;
use crate::server::{State, StateSnapshot};

#[derive(Default)]
pub struct DiagnosticScheduler {
    generations: FxHashMap<FileId, u64>,
    jobs: FxHashMap<FileId, DiagnosticJob>,
}

struct DiagnosticJob {
    running: DiagnosticTicket,
    queued: Option<DiagnosticTicket>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DiagnosticTicket {
    file_id: FileId,
    generation: u64,
    version: Option<i32>,
}

impl DiagnosticScheduler {
    pub fn invalidate(&mut self, change: &LifecycleChange, files: &FileLifecycle<i32, bool>) {
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

    fn schedule(&mut self, file_id: FileId, version: Option<i32>) -> Option<DiagnosticTicket> {
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

    fn is_running(&self, ticket: DiagnosticTicket) -> bool {
        self.jobs.get(&ticket.file_id).is_some_and(|job| job.running == ticket)
    }

    fn is_current(&self, ticket: DiagnosticTicket) -> bool {
        self.generations.get(&ticket.file_id).copied().unwrap_or_default() == ticket.generation
    }

    fn complete(&mut self, ticket: DiagnosticTicket) -> Option<DiagnosticTicket> {
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

pub fn emit_collect_diagnostics(state: &mut State, uri: Url) -> Result<(), LspError> {
    let files = state.files.read();
    let uri = uri.as_str();

    if let Some(file_id) = files.source_id(uri) {
        state.client.emit(CollectDiagnostics(file_id))?;
    }

    Ok(())
}

pub fn emit_collect_diagnostics_id(state: &mut State, file_id: FileId) -> Result<(), LspError> {
    if state.files.read().contains_source(file_id) {
        state.client.emit(CollectDiagnostics(file_id))?;
    }
    Ok(())
}

pub fn emit_collect_all_diagnostics(state: &mut State) -> Result<(), LspError> {
    let files = state.files.read();
    let editable_files = files
        .source_ids()
        .filter(|file_id| files.source_metadata(*file_id).copied().unwrap_or(false));
    for file_id in editable_files {
        state.client.emit(CollectDiagnostics(file_id))?;
    }
    Ok(())
}

pub struct CollectDiagnostics(FileId);

pub fn collect_diagnostics(
    state: &mut State,
    CollectDiagnostics(file_id): CollectDiagnostics,
) -> Result<(), LspError> {
    let version = {
        let files = state.files.read();
        if !files.contains_source(file_id) {
            return Ok(());
        }
        files.source_version(file_id)
    };
    if let Some(ticket) = state.diagnostics.schedule(file_id, version) {
        start_diagnostics(state, ticket);
    }
    Ok(())
}

fn start_diagnostics(state: &State, ticket: DiagnosticTicket) {
    let worker = state.spawn(move |snapshot| {
        let _span = tracing::info_span!("collect_diagnostics").entered();
        collect_diagnostics_core(snapshot, ticket)
    });
    let client = ClientSocket::clone(&state.client);
    task::spawn(async move {
        let collected = await_diagnostics(worker).await;
        let event = DiagnosticsFinished { ticket, collected };
        if let Err(error) = client.emit(event) {
            LspError::from(error).emit_trace();
        }
    });
}

fn collect_diagnostics_core(
    snapshot: StateSnapshot,
    ticket: DiagnosticTicket,
) -> Option<CollectedDiagnostics> {
    let result = snapshot.with_analyzer_context(|context| {
        analyzer::diagnostics::implementation(context, ticket.file_id)
    });
    match result {
        Ok(collected) => Some(collected),
        Err(error) => {
            LspError::from(error).emit_trace();
            None
        }
    }
}

async fn await_diagnostics(
    worker: task::JoinHandle<Option<CollectedDiagnostics>>,
) -> Option<CollectedDiagnostics> {
    match worker.await {
        Ok(collected) => collected,
        Err(error) => {
            LspError::JoinError(error).emit_trace();
            None
        }
    }
}

pub struct DiagnosticsFinished {
    ticket: DiagnosticTicket,
    collected: Option<CollectedDiagnostics>,
}

pub fn finish_diagnostics(
    state: &mut State,
    DiagnosticsFinished { ticket, collected }: DiagnosticsFinished,
) -> Result<(), LspError> {
    let running = state.diagnostics.is_running(ticket);
    let current = running && state.diagnostics.is_current(ticket) && {
        let files = state.files.read();
        files.contains_source(ticket.file_id)
            && files.source_version(ticket.file_id) == ticket.version
    };
    let next = state.diagnostics.complete(ticket);

    let publish_result = if current && let Some(collected) = collected {
        state.client.publish_diagnostics(PublishDiagnosticsParams {
            uri: collected.uri,
            diagnostics: collected.diagnostics,
            version: ticket.version,
        })
    } else {
        Ok(())
    };
    if let Some(next) = next {
        start_diagnostics(state, next);
    }
    publish_result.map_err(LspError::from)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use building::QueryEngine;
    use building::lifecycle::{
        DiskObservation, FileLifecycle, LifecycleEvent, SourceEvent, SourceUnitKey,
    };
    use files::Files;

    use super::{DiagnosticScheduler, await_diagnostics};

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
                metadata: true,
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
                metadata: true,
            },
        };
        let change = lifecycle.apply(&engine, event);
        scheduler.invalidate(&change, &lifecycle);

        assert!(!scheduler.is_current(ticket));
    }

    #[tokio::test]
    async fn failed_diagnostics_worker_returns_no_result() {
        let worker = tokio::spawn(async {
            panic!("diagnostics worker failed");
            #[allow(unreachable_code)]
            None
        });

        assert!(await_diagnostics(worker).await.is_none());
    }
}
