use std::mem;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use analyzer::position::PositionEncoding;
use async_lsp::{ClientSocket, LanguageClient};
use building::lifecycle::{AnalysisInvalidation, LifecycleEvent};
use configuration::Configuration;
use files::FileId;
use iris_build::compilation::{CompilationParts, CompilationState, MaterializedPrim};
use itertools::Itertools;
use lsp_types::{
    DidChangeTextDocumentParams, DidChangeWatchedFilesParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams, PublishDiagnosticsParams, Url,
};
use rustc_hash::FxHashSet;

use super::analysis::Analysis;
pub(super) use super::analysis::AnalysisSnapshot as StateSnapshot;

use super::diagnostics::{DiagnosticActor, DiagnosticWorkerEvent};
use super::error::LspError;
use super::event::{CollectDiagnostics, DiagnosticProtocol, DiagnosticTicket};
use super::preparation::ReconfigurationBaseline;
use super::{
    SourceMetadata, did_change, did_change_watched_files, did_close, did_open, did_save,
    source_unit_from_document_uri,
};

pub(super) struct SourceRoot {
    pub(super) path: PathBuf,
    pub(super) metadata: SourceMetadata,
}

pub(super) enum WorkspaceNotification {
    DidOpen(DidOpenTextDocumentParams),
    DidSave(DidSaveTextDocumentParams),
    DidClose(DidCloseTextDocumentParams),
    DidChange(DidChangeTextDocumentParams),
    DidChangeWatchedFiles(DidChangeWatchedFilesParams),
}

pub(super) struct WorkspaceContext<'a> {
    pub(super) root: Option<&'a Path>,
    pub(super) position_encoding: PositionEncoding,
}

pub(super) struct WorkspaceRuntime {
    state: WorkspaceState,
}

enum WorkspaceState {
    WaitingForConfiguration { pending: Vec<WorkspaceNotification> },
    Ready { workspace: ReadyWorkspace },
}

pub(super) struct ReadyWorkspace {
    pub(super) configuration: Arc<Configuration>,
    pub(super) analysis: Arc<Analysis>,
    pub(super) source_roots: Vec<SourceRoot>,
    pub(super) selected_sources: FxHashSet<Arc<str>>,
    pub(super) excluded_sources: FxHashSet<Arc<str>>,
    pub(super) diagnostics: DiagnosticProtocol,
    pub(super) diagnostic_actor: Option<DiagnosticActor>,
    _prim: MaterializedPrim,
}

pub(super) struct PreparedInitialWorkspace {
    pub(super) configuration: Arc<Configuration>,
    pub(super) compilation: CompilationState<i32, SourceMetadata>,
    pub(super) source_roots: Vec<SourceRoot>,
    pub(super) selected_sources: FxHashSet<Arc<str>>,
}

pub(super) struct PreparedSourceReconfiguration {
    pub(super) configuration: Arc<Configuration>,
    pub(super) source_roots: Vec<SourceRoot>,
    pub(super) selected_sources: FxHashSet<Arc<str>>,
    pub(super) excluded_sources: FxHashSet<Arc<str>>,
    pub(super) events: Vec<LifecycleEvent<i32, SourceMetadata>>,
}

pub(super) enum DiagnosticTrigger {
    None,
    AssociatedSource(Url),
    AnalysisChange,
}

#[must_use]
pub(super) struct WorkspaceEffects {
    clear_diagnostics: Vec<PublishDiagnosticsParams>,
    collect_diagnostics: Vec<FileId>,
}

impl WorkspaceEffects {
    pub(super) fn none() -> WorkspaceEffects {
        WorkspaceEffects { clear_diagnostics: vec![], collect_diagnostics: vec![] }
    }

    pub(super) fn associated(
        workspace: &ReadyWorkspace,
        uri: Url,
    ) -> Result<WorkspaceEffects, LspError> {
        let (_, unit) = source_unit_from_document_uri(&uri)?;
        let files = workspace.analysis.files.read();
        let collect_diagnostics = files.source_id(unit.source()).into_iter().collect_vec();
        Ok(WorkspaceEffects { clear_diagnostics: vec![], collect_diagnostics })
    }

    pub(super) fn deliver(self, client: &ClientSocket) -> Result<(), LspError> {
        let mut client = ClientSocket::clone(client);
        for parameters in self.clear_diagnostics {
            client.publish_diagnostics(parameters)?;
        }
        for file_id in self.collect_diagnostics {
            client.emit(CollectDiagnostics(file_id))?;
        }
        Ok(())
    }
}

impl WorkspaceRuntime {
    pub(super) fn new() -> WorkspaceRuntime {
        WorkspaceRuntime { state: WorkspaceState::WaitingForConfiguration { pending: vec![] } }
    }

    pub(super) fn is_ready(&self) -> bool {
        matches!(self.state, WorkspaceState::Ready { .. })
    }

    pub(super) fn ready(&self) -> Result<&ReadyWorkspace, LspError> {
        match &self.state {
            WorkspaceState::WaitingForConfiguration { .. } => Err(LspError::WorkspaceNotReady),
            WorkspaceState::Ready { workspace } => Ok(workspace),
        }
    }

    pub(super) fn ready_mut(&mut self) -> Result<&mut ReadyWorkspace, LspError> {
        match &mut self.state {
            WorkspaceState::WaitingForConfiguration { .. } => Err(LspError::WorkspaceNotReady),
            WorkspaceState::Ready { workspace } => Ok(workspace),
        }
    }

    pub(super) fn install(
        &mut self,
        prepared: PreparedInitialWorkspace,
    ) -> Result<Vec<WorkspaceNotification>, LspError> {
        let WorkspaceState::WaitingForConfiguration { pending } = &mut self.state else {
            return Err(LspError::WorkspaceAlreadyReady);
        };
        let pending = mem::take(pending);
        let CompilationParts { engine, files, prim } = prepared.compilation.into_parts();
        let workspace = ReadyWorkspace {
            configuration: prepared.configuration,
            analysis: Arc::new(Analysis::new(engine, files)),
            source_roots: prepared.source_roots,
            selected_sources: prepared.selected_sources,
            excluded_sources: FxHashSet::default(),
            diagnostics: DiagnosticProtocol::default(),
            diagnostic_actor: None,
            _prim: prim,
        };
        self.state = WorkspaceState::Ready { workspace };
        Ok(pending)
    }

    pub(super) fn dispatch(
        &mut self,
        notification: WorkspaceNotification,
        context: WorkspaceContext<'_>,
        client: &ClientSocket,
    ) -> Result<(), LspError> {
        match &mut self.state {
            WorkspaceState::WaitingForConfiguration { pending } => {
                pending.push(notification);
                Ok(())
            }
            WorkspaceState::Ready { workspace } => {
                workspace.dispatch(notification, context, client)
            }
        }
    }

    pub(super) fn commit_reconfiguration(
        &mut self,
        prepared: PreparedSourceReconfiguration,
    ) -> Result<WorkspaceEffects, LspError> {
        self.ready_mut().map(|workspace| workspace.commit_reconfiguration(prepared))
    }

    pub(super) fn update_configuration_if_sources_equal(
        &mut self,
        configuration: Arc<Configuration>,
    ) -> bool {
        let Ok(workspace) = self.ready_mut() else {
            return false;
        };
        if configuration.sources != workspace.configuration.sources {
            return false;
        }
        workspace.configuration = configuration;
        true
    }

    pub(super) fn reconfiguration_baseline(&self) -> Option<ReconfigurationBaseline> {
        self.ready().ok().map(|workspace| ReconfigurationBaseline {
            selected_sources: FxHashSet::clone(&workspace.selected_sources),
            excluded_sources: FxHashSet::clone(&workspace.excluded_sources),
        })
    }

    pub(super) fn schedule_diagnostics(
        &mut self,
        file_id: FileId,
    ) -> Result<Option<DiagnosticTicket>, LspError> {
        let workspace = self.ready_mut()?;
        let version = {
            let files = workspace.analysis.files.read();
            if !files.contains_source(file_id) {
                return Ok(None);
            }
            files.source_version(file_id)
        };
        Ok(Some(workspace.diagnostics.ticket(file_id, version)))
    }

    pub(super) async fn shutdown(mut self) {
        if let WorkspaceState::Ready { workspace } = &mut self.state
            && let Some(actor) = workspace.diagnostic_actor.take()
        {
            actor.shutdown().await;
        }
        // Retain Prim and lifecycle ownership until every admitted reader retires.
        let retirement = tokio::task::spawn_blocking(move || {
            if let WorkspaceState::Ready { workspace } = &self.state {
                workspace.analysis.engine.request_cancel();
            }
        });
        let _ = retirement.await;
    }

    #[cfg(test)]
    pub(super) fn test_ready(&self) -> &ReadyWorkspace {
        self.ready().expect("invariant violated: test workspace is not ready")
    }

    #[cfg(test)]
    pub(super) fn test_ready_mut(&mut self) -> &mut ReadyWorkspace {
        self.ready_mut().expect("invariant violated: test workspace is not ready")
    }

    #[cfg(test)]
    pub(super) fn test_pending_len(&self) -> usize {
        match &self.state {
            WorkspaceState::WaitingForConfiguration { pending } => pending.len(),
            WorkspaceState::Ready { .. } => 0,
        }
    }
}

impl ReadyWorkspace {
    fn dispatch(
        &mut self,
        notification: WorkspaceNotification,
        context: WorkspaceContext<'_>,
        client: &ClientSocket,
    ) -> Result<(), LspError> {
        match notification {
            WorkspaceNotification::DidOpen(parameters) => {
                did_open(self, context, client, parameters)
            }
            WorkspaceNotification::DidSave(parameters) => did_save(self, client, parameters),
            WorkspaceNotification::DidClose(parameters) => {
                did_close(self, context, client, parameters)
            }
            WorkspaceNotification::DidChange(parameters) => {
                did_change(self, context, client, parameters)
            }
            WorkspaceNotification::DidChangeWatchedFiles(parameters) => {
                did_change_watched_files(self, context, client, parameters)
            }
        }
    }

    pub(super) fn invalidate_suggestions_cache(&self) {
        self.analysis.invalidate_suggestions();
    }

    pub(super) fn apply_lifecycle_events(
        &mut self,
        events: impl IntoIterator<Item = LifecycleEvent<i32, SourceMetadata>>,
        trigger: DiagnosticTrigger,
    ) -> WorkspaceEffects {
        let change = self.analysis.apply(events);

        {
            let files = self.analysis.files.read();
            let invalidated = self.diagnostics.invalidate(&change, &files);
            if let Some(actor) = &self.diagnostic_actor {
                for file_id in invalidated {
                    actor.send(DiagnosticWorkerEvent::Invalidate { file_id });
                }
            }
        }
        for warning in change.warnings() {
            tracing::warn!("{warning}");
        }

        let clear_diagnostics =
            change.removed_sources().iter().map(|removed| PublishDiagnosticsParams {
                uri: Url::parse(&removed.locator)
                    .expect("invariant violated: removed source has an invalid locator"),
                diagnostics: vec![],
                version: None,
            });
        let clear_diagnostics = clear_diagnostics.collect_vec();
        let collect_diagnostics = match trigger {
            DiagnosticTrigger::None => vec![],
            DiagnosticTrigger::AssociatedSource(uri) => {
                let (_, unit) = source_unit_from_document_uri(&uri)
                    .expect("invariant violated: diagnostic trigger has an invalid document URI");
                self.analysis.files.read().source_id(unit.source()).into_iter().collect_vec()
            }
            DiagnosticTrigger::AnalysisChange => match change.analysis() {
                AnalysisInvalidation::None => vec![],
                AnalysisInvalidation::Sources(sources) => sources.iter().copied().collect_vec(),
                AnalysisInvalidation::Workspace => {
                    let files = self.analysis.files.read();
                    let editable_sources = files.source_ids().filter(|file_id| {
                        files.source_metadata(*file_id).is_some_and(SourceMetadata::editable)
                    });
                    editable_sources.collect_vec()
                }
            },
        };
        WorkspaceEffects { clear_diagnostics, collect_diagnostics }
    }

    fn commit_reconfiguration(
        &mut self,
        prepared: PreparedSourceReconfiguration,
    ) -> WorkspaceEffects {
        let PreparedSourceReconfiguration {
            configuration,
            source_roots,
            selected_sources,
            excluded_sources,
            events,
        } = prepared;
        let effects = self.apply_lifecycle_events(events, DiagnosticTrigger::AnalysisChange);
        self.configuration = configuration;
        self.source_roots = source_roots;
        self.selected_sources = selected_sources;
        self.excluded_sources = excluded_sources;
        effects
    }
}
