use std::mem;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use analyzer::AnalyzerCapabilities;
use analyzer::position::PositionEncoding;
use async_lsp::{ClientSocket, LanguageClient};
use building::lifecycle::{AnalysisInvalidation, LifecycleChange, LifecycleEvent};
use configuration::Configuration;
use files::FileId;
use iris_build::compilation::{CompilationParts, CompilationState, MaterializedPrim};
use itertools::Itertools;
use lsp_types::{
    DidChangeTextDocumentParams, DidChangeWatchedFilesParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams, PublishDiagnosticsParams, Url,
};

use super::analysis::{Analysis, StateSnapshot};
use super::error::LspError;
use super::event::{CollectDiagnostics, DiagnosticScheduler, DiagnosticTicket};
use super::{
    SourceMetadata, did_change, did_change_watched_files, did_close, did_open, did_save,
    source_unit_from_document_uri,
};

pub(super) struct SourceRoot {
    pub(super) path: PathBuf,
    pub(super) metadata: SourceMetadata,
}

pub(super) enum WorkspaceNotification {
    Open(DidOpenTextDocumentParams),
    Save(DidSaveTextDocumentParams),
    Close(DidCloseTextDocumentParams),
    Change(DidChangeTextDocumentParams),
    ChangeWatchedFiles(DidChangeWatchedFilesParams),
}

pub(super) struct WorkspaceContext<'a> {
    pub(super) root: Option<&'a Path>,
    pub(super) position_encoding: PositionEncoding,
}

pub(super) struct WorkspaceRuntime {
    state: WorkspaceState,
}

enum WorkspaceState {
    Loading { pending: Vec<WorkspaceNotification>, configuration: Arc<Configuration> },
    Ready { workspace: ReadyWorkspace },
    Failed,
}

pub(super) struct ReadyWorkspace {
    pub(super) configuration: Arc<Configuration>,
    pub(super) analysis: Analysis,
    pub(super) source_roots: Vec<SourceRoot>,
    pub(super) diagnostics: DiagnosticScheduler,
    _prim: MaterializedPrim,
}

pub(super) struct PreparedInitialWorkspace {
    pub(super) compilation: CompilationState<i32, SourceMetadata>,
    pub(super) source_roots: Vec<SourceRoot>,
}

pub(super) enum ConfigurationApplyError {
    Apply(LspError),
}

impl std::fmt::Display for ConfigurationApplyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigurationApplyError::Apply(error) => error.fmt(formatter),
        }
    }
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
    pub(super) fn new(configuration: Arc<Configuration>) -> WorkspaceRuntime {
        WorkspaceRuntime { state: WorkspaceState::Loading { pending: vec![], configuration } }
    }

    #[cfg(test)]
    pub(super) fn is_ready(&self) -> bool {
        matches!(self.state, WorkspaceState::Ready { .. })
    }

    fn ready(&self) -> Result<&ReadyWorkspace, LspError> {
        match &self.state {
            WorkspaceState::Ready { workspace } => Ok(workspace),
            WorkspaceState::Loading { .. } => Err(LspError::WorkspaceNotReady),
            WorkspaceState::Failed => Err(LspError::WorkspaceFailed),
        }
    }

    fn ready_mut(&mut self) -> Result<&mut ReadyWorkspace, LspError> {
        match &mut self.state {
            WorkspaceState::Ready { workspace } => Ok(workspace),
            WorkspaceState::Loading { .. } => Err(LspError::WorkspaceNotReady),
            WorkspaceState::Failed => Err(LspError::WorkspaceFailed),
        }
    }

    pub(super) fn snapshot(
        &self,
        position_encoding: PositionEncoding,
        analyzer_capabilities: AnalyzerCapabilities,
    ) -> Result<StateSnapshot, LspError> {
        let workspace = self.ready()?;
        Ok(workspace.analysis.snapshot(position_encoding, analyzer_capabilities))
    }

    pub(super) fn install(
        &mut self,
        prepared: PreparedInitialWorkspace,
    ) -> Result<Vec<WorkspaceNotification>, LspError> {
        let WorkspaceState::Loading { pending, configuration } = &mut self.state else {
            return Err(LspError::WorkspaceAlreadyReady);
        };
        let pending = mem::take(pending);
        let configuration = Arc::clone(configuration);
        let CompilationParts { engine, files, prim } = prepared.compilation.into_parts();
        let workspace = ReadyWorkspace {
            configuration,
            analysis: Analysis::new(engine, files),
            source_roots: prepared.source_roots,
            diagnostics: DiagnosticScheduler::default(),
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
            WorkspaceState::Loading { pending, .. } => {
                pending.push(notification);
                Ok(())
            }
            WorkspaceState::Ready { workspace } => {
                workspace.dispatch(notification, context, client)
            }
            WorkspaceState::Failed => Ok(()),
        }
    }

    pub(super) fn update_configuration(&mut self, configuration: Arc<Configuration>) -> bool {
        let Ok(workspace) = self.ready_mut() else {
            return false;
        };
        workspace.configuration = configuration;
        true
    }

    /// Stores the configuration to install when preparation finishes.
    ///
    /// A configuration received while preparation is in flight only replaces
    /// the staged value; it never starts a second preparation.
    pub(super) fn stage_configuration(&mut self, configuration: Arc<Configuration>) {
        if let WorkspaceState::Loading { configuration: staged, .. } = &mut self.state {
            *staged = configuration;
        }
    }

    /// Marks preparation as terminally failed for this session.
    pub(super) fn fail(&mut self) {
        if matches!(self.state, WorkspaceState::Loading { .. }) {
            self.state = WorkspaceState::Failed;
        }
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
        Ok(workspace.diagnostics.schedule(file_id, version))
    }

    pub(super) fn finish_diagnostics(
        &mut self,
        ticket: DiagnosticTicket,
    ) -> Result<(bool, Option<DiagnosticTicket>), LspError> {
        let workspace = self.ready_mut()?;
        let running = workspace.diagnostics.is_running(ticket);
        let current = running && workspace.diagnostics.is_current(ticket) && {
            let files = workspace.analysis.files.read();
            files.contains_source(ticket.file_id)
                && files.source_version(ticket.file_id) == ticket.version
        };
        let next = workspace.diagnostics.complete(ticket);
        Ok((current, next))
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
            WorkspaceState::Loading { pending, .. } => pending.len(),
            WorkspaceState::Ready { .. } | WorkspaceState::Failed => 0,
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
            WorkspaceNotification::Open(parameters) => did_open(self, context, client, parameters),
            WorkspaceNotification::Save(parameters) => did_save(self, client, parameters),
            WorkspaceNotification::Close(parameters) => {
                did_close(self, context, client, parameters)
            }
            WorkspaceNotification::Change(parameters) => {
                did_change(self, context, client, parameters)
            }
            WorkspaceNotification::ChangeWatchedFiles(parameters) => {
                did_change_watched_files(self, context, client, parameters)
            }
        }
    }

    pub(super) fn apply_lifecycle_events(
        &mut self,
        events: impl IntoIterator<Item = LifecycleEvent<i32, SourceMetadata>>,
        trigger: DiagnosticTrigger,
    ) -> WorkspaceEffects {
        let change: LifecycleChange = self.analysis.apply(events, &mut self.diagnostics);
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
}
