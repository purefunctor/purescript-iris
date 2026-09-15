use analyzer::diagnostics::CollectedDiagnostics;
use building::lifecycle::{AnalysisInvalidation, FileLifecycle, LifecycleChange};
use files::FileId;
use rustc_hash::FxHashMap;
use tokio::sync::mpsc;

use super::SourceMetadata;
use super::diagnostics::DiagnosticEvent;

#[derive(Default)]
pub(super) struct DiagnosticValidity {
    generations: FxHashMap<FileId, u64>,
    pending: FxHashMap<FileId, DiagnosticTicket>,
    sequence: u64,
    pub(super) worker: Option<mpsc::UnboundedSender<DiagnosticEvent>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct DiagnosticTicket {
    pub(super) file_id: FileId,
    pub(super) generation: u64,
    pub(super) version: Option<i32>,
    pub(super) sequence: u64,
}

impl DiagnosticValidity {
    pub(super) fn invalidate(
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
                for file_id in files.source_ids() {
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
        *generation = generation.checked_add(1).expect("diagnostic generation overflowed");
        self.pending.remove(&file_id);
        if let Some(worker) = &self.worker {
            let _ = worker.send(DiagnosticEvent::Invalidate { file_id, generation: *generation });
        }
    }

    pub(super) fn schedule(&mut self, file_id: FileId, version: Option<i32>) -> DiagnosticTicket {
        self.sequence = self.sequence.checked_add(1).expect("diagnostic sequence overflowed");
        let generation = self.generations.get(&file_id).copied().unwrap_or_default();
        let ticket = DiagnosticTicket { file_id, generation, version, sequence: self.sequence };
        self.pending.insert(file_id, ticket);
        ticket
    }

    pub(super) fn is_current(&self, ticket: DiagnosticTicket) -> bool {
        self.pending.get(&ticket.file_id) == Some(&ticket)
            && self.generations.get(&ticket.file_id).copied().unwrap_or_default()
                == ticket.generation
    }

    pub(super) fn complete(&mut self, ticket: DiagnosticTicket) -> bool {
        if !self.is_current(ticket) {
            return false;
        }
        self.pending.remove(&ticket.file_id);
        true
    }
}

pub struct CollectDiagnostics {
    pub(super) ticket: DiagnosticTicket,
}

pub struct DiagnosticsFinished {
    pub(super) ticket: DiagnosticTicket,
    pub(super) collected: Option<CollectedDiagnostics>,
}

#[cfg(test)]
mod tests {
    use files::Files;

    use super::*;

    #[test]
    fn invalidation_and_duplicate_completion_cannot_publish() {
        let file_id = Files::default().insert("Main.purs", "");
        let mut validity = DiagnosticValidity::default();
        let first = validity.schedule(file_id, Some(7));
        validity.invalidate_source(file_id);
        let replacement = validity.schedule(file_id, Some(7));

        assert!(!validity.complete(first));
        assert!(validity.complete(replacement));
        assert!(!validity.complete(replacement));
        let next = validity.schedule(file_id, Some(7));
        assert_ne!(next, replacement);
        assert!(!validity.complete(replacement));
        assert!(validity.complete(next));
    }
}
