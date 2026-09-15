use analyzer::diagnostics::CollectedDiagnostics;
use async_lsp::{ClientSocket, LanguageClient};
use building::lifecycle::{AnalysisInvalidation, FileLifecycle, LifecycleChange};
use files::FileId;
use itertools::Itertools;
use lsp_types::PublishDiagnosticsParams;
use rustc_hash::FxHashMap;

use crate::server::SourceMetadata;
use crate::server::error::LspError;

/// Protocol-owned diagnostic validity. Execution and cancellation belong to
/// `DiagnosticActor`; this object stays beside the mutable workspace so the
/// final version check and publication are serialized with protocol events.
#[derive(Default)]
pub(super) struct DiagnosticProtocol {
    generations: FxHashMap<FileId, u64>,
    next_job_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct DiagnosticTicket {
    pub(super) file_id: FileId,
    pub(super) generation: u64,
    pub(super) version: Option<i32>,
    pub(super) job_id: u64,
}

impl DiagnosticProtocol {
    pub(super) fn invalidate(
        &mut self,
        change: &LifecycleChange,
        files: &FileLifecycle<i32, SourceMetadata>,
    ) -> Vec<FileId> {
        let mut invalidated = match change.analysis() {
            AnalysisInvalidation::None => vec![],
            AnalysisInvalidation::Sources(sources) => sources.iter().copied().collect_vec(),
            AnalysisInvalidation::Workspace => files.source_ids().collect_vec(),
        };
        invalidated.extend(change.removed_sources().iter().map(|removed| removed.file_id));
        invalidated.sort_unstable();
        invalidated.dedup();
        for file_id in &invalidated {
            self.invalidate_source(*file_id);
        }
        invalidated
    }

    fn invalidate_source(&mut self, file_id: FileId) {
        let generation = self.generations.entry(file_id).or_default();
        *generation = generation
            .checked_add(1)
            .expect("invariant violated: diagnostic generation overflowed");
    }

    pub(super) fn ticket(&mut self, file_id: FileId, version: Option<i32>) -> DiagnosticTicket {
        let generation = self.generations.get(&file_id).copied().unwrap_or_default();
        let job_id = self.next_job_id;
        self.next_job_id = self
            .next_job_id
            .checked_add(1)
            .expect("invariant violated: diagnostic job identity overflowed");
        DiagnosticTicket { file_id, generation, version, job_id }
    }

    pub(super) fn is_current(
        &self,
        ticket: DiagnosticTicket,
        files: &FileLifecycle<i32, SourceMetadata>,
    ) -> bool {
        self.generations.get(&ticket.file_id).copied().unwrap_or_default() == ticket.generation
            && files.contains_source(ticket.file_id)
            && files.source_version(ticket.file_id) == ticket.version
    }

    pub(super) fn publish(
        &self,
        client: &ClientSocket,
        files: &FileLifecycle<i32, SourceMetadata>,
        ticket: DiagnosticTicket,
        collected: Option<CollectedDiagnostics>,
    ) -> Result<(), LspError> {
        if !self.is_current(ticket, files) {
            return Ok(());
        }
        let Some(collected) = collected else {
            return Ok(());
        };
        let mut client = ClientSocket::clone(client);
        client
            .publish_diagnostics(PublishDiagnosticsParams {
                uri: collected.uri,
                diagnostics: collected.diagnostics,
                version: ticket.version,
            })
            .map_err(LspError::from)
    }
}

pub struct CollectDiagnostics(pub(super) FileId);

pub struct DiagnosticsFinished {
    pub(super) ticket: DiagnosticTicket,
    pub(super) collected: Option<CollectedDiagnostics>,
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use building::QueryEngine;
    use building::lifecycle::{
        DiskObservation, FileLifecycle, LifecycleEvent, SourceEvent, SourceUnitKey,
    };
    use files::Files;

    use super::{DiagnosticProtocol, SourceMetadata};

    fn file_id() -> files::FileId {
        let mut files = Files::default();
        files.insert("file:///src/Main.purs", "module Main where\n")
    }

    #[test]
    fn invalidation_advances_the_lifecycle_generation() {
        let file_id = file_id();
        let mut protocol = DiagnosticProtocol::default();
        let ticket = protocol.ticket(file_id, Some(1));
        protocol.invalidate_source(file_id);
        assert_ne!(protocol.ticket(file_id, Some(1)).generation, ticket.generation);
    }

    #[test]
    fn repeated_logical_tickets_have_distinct_job_identities() {
        let file_id = file_id();
        let mut protocol = DiagnosticProtocol::default();
        let first = protocol.ticket(file_id, Some(1));
        let second = protocol.ticket(file_id, Some(1));
        assert_ne!(first.job_id, second.job_id);
    }

    #[test]
    fn workspace_change_invalidates_unrelated_diagnostics() {
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
        let mut protocol = DiagnosticProtocol::default();
        let ticket = protocol.ticket(first_id, None);
        assert!(protocol.is_current(ticket, &lifecycle));
        let wrong_version = super::DiagnosticTicket { version: Some(7), ..ticket };
        assert!(!protocol.is_current(wrong_version, &lifecycle));
        let second_unit = SourceUnitKey::new("file:///src/Library.purs", "file:///src/Library.js");
        let event = LifecycleEvent::Source {
            unit: second_unit,
            event: SourceEvent::DiskObserved {
                disk: DiskObservation::Found(Arc::from("module Library where\n")),
                metadata: SourceMetadata::Unmanaged { editable: true },
            },
        };
        let change = lifecycle.apply(&engine, event);
        protocol.invalidate(&change, &lifecycle);
        assert!(!protocol.is_current(ticket, &lifecycle));
    }
}
