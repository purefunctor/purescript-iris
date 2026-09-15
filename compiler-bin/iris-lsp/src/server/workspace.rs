use std::mem;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use analyzer::AnalyzerCapabilities;
use analyzer::position::PositionEncoding;
use async_lsp::ClientSocket;
use building::QueryCancellation;
use building::lifecycle::{AnalysisInvalidation, DocumentKey, LifecycleEvent, SourceUnitKey};
use configuration::Configuration;
use iris_build::compilation::{CompilationParts, CompilationState, MaterializedPrim};
use itertools::Itertools;
use lsp_types::{
    DidChangeTextDocumentParams, DidChangeWatchedFilesParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams, PublishDiagnosticsParams, Url,
};
use rustc_hash::{FxHashMap, FxHashSet};

use super::analysis::{Analysis, AnalysisSnapshot};
use super::error::LspError;
use super::event::{DiagnosticTicket, DiagnosticValidity};
use super::preparation::ReconfigurationInput;
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
    source_identities: FxHashMap<Arc<str>, PathBuf>,
    pub(super) diagnostics: DiagnosticValidity,
    _prim: MaterializedPrim,
}

pub(super) struct PreparedInitialWorkspace {
    pub(super) configuration: Arc<Configuration>,
    pub(super) compilation: CompilationState<i32, SourceMetadata>,
    pub(super) source_roots: Vec<SourceRoot>,
    pub(super) selected_sources: FxHashSet<Arc<str>>,
    pub(super) source_identities: FxHashMap<Arc<str>, PathBuf>,
}

pub(super) struct PreparedSourceReconfiguration {
    pub(super) configuration: Arc<Configuration>,
    pub(super) source_roots: Vec<SourceRoot>,
    pub(super) selected_sources: FxHashSet<Arc<str>>,
    pub(super) excluded_sources: FxHashSet<Arc<str>>,
    pub(super) source_identities: FxHashMap<Arc<str>, PathBuf>,
    pub(super) events: Vec<LifecycleEvent<i32, SourceMetadata>>,
}

pub(super) enum DiagnosticTrigger {
    None,
    AssociatedSource(Url),
    AnalysisChange,
}

#[must_use]
pub(super) struct WorkspaceEffects {
    pub(super) clear_diagnostics: Vec<PublishDiagnosticsParams>,
    pub(super) collect_diagnostics: Vec<DiagnosticTicket>,
}

impl WorkspaceEffects {
    pub(super) fn none() -> WorkspaceEffects {
        WorkspaceEffects { clear_diagnostics: vec![], collect_diagnostics: vec![] }
    }

    pub(super) fn associated(
        workspace: &mut ReadyWorkspace,
        uri: Url,
    ) -> Result<WorkspaceEffects, LspError> {
        let (_, unit) = source_unit_from_document_uri(&uri)?;
        let files = workspace.analysis.files.read();
        let collect_diagnostics = files
            .source_id(unit.source())
            .map(|file_id| workspace.diagnostics.schedule(file_id, files.source_version(file_id)));
        let collect_diagnostics = collect_diagnostics.into_iter().collect_vec();
        Ok(WorkspaceEffects { clear_diagnostics: vec![], collect_diagnostics })
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

    pub(super) fn snapshot(
        &self,
        position_encoding: PositionEncoding,
        analyzer_capabilities: AnalyzerCapabilities,
        cancellation: QueryCancellation,
    ) -> Result<AnalysisSnapshot, LspError> {
        let workspace = self.ready()?;
        workspace
            .analysis
            .snapshot(position_encoding, analyzer_capabilities, cancellation)
            .map_err(LspError::from)
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
            source_identities: prepared.source_identities,
            diagnostics: DiagnosticValidity::default(),
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

    pub(super) fn reconfiguration_input(&self) -> Option<ReconfigurationInput> {
        let workspace = self.ready().ok()?;
        Some(ReconfigurationInput {
            selected_sources: FxHashSet::clone(&workspace.selected_sources),
            excluded_sources: FxHashSet::clone(&workspace.excluded_sources),
            source_identities: FxHashMap::clone(&workspace.source_identities),
            tracked: workspace.analysis.files.read().unit_keys().cloned().collect_vec(),
        })
    }

    pub(super) fn finish_diagnostics(
        &mut self,
        ticket: DiagnosticTicket,
    ) -> Result<bool, LspError> {
        let workspace = self.ready_mut()?;
        let current = workspace.diagnostics.complete(ticket) && {
            let files = workspace.analysis.files.read();
            files.contains_source(ticket.file_id)
                && files.source_version(ticket.file_id) == ticket.version
        };
        Ok(current)
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
    pub(super) fn remember_source_identity(&mut self, unit: &SourceUnitKey) {
        if let Some(identity) = filesystem_identity(unit.source()) {
            self.source_identities.insert(Arc::from(unit.source()), identity);
        }
    }

    pub(super) fn source_excluded(&self, unit: &SourceUnitKey) -> bool {
        if self.excluded_sources.contains(unit.source()) {
            return true;
        }
        let identity = filesystem_identity(unit.source());
        let identity = identity.as_ref().or_else(|| self.source_identities.get(unit.source()));
        identity.is_some_and(|identity| {
            self.excluded_sources
                .iter()
                .any(|source| self.source_identities.get(source) == Some(identity))
        })
    }

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
            self.diagnostics.invalidate(&change, &files);
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
        let files = self.analysis.files.read();
        let collect_diagnostics = collect_diagnostics.into_iter().filter_map(|file_id| {
            files
                .contains_source(file_id)
                .then(|| self.diagnostics.schedule(file_id, files.source_version(file_id)))
        });
        let collect_diagnostics = collect_diagnostics.collect_vec();
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
            source_identities,
            mut events,
        } = prepared;
        {
            let files = self.analysis.files.read();
            events.retain(|event| {
                let document = match event {
                    LifecycleEvent::Source { unit, .. } => {
                        DocumentKey::Source(SourceUnitKey::clone(unit))
                    }
                    LifecycleEvent::Foreign { unit, kind, .. } => {
                        DocumentKey::Foreign(SourceUnitKey::clone(unit), *kind)
                    }
                };
                !files.is_open(&document)
            });
        }
        let effects = self.apply_lifecycle_events(events, DiagnosticTrigger::AnalysisChange);
        self.configuration = configuration;
        self.source_roots = source_roots;
        self.selected_sources = selected_sources;
        self.excluded_sources = excluded_sources;
        self.source_identities = source_identities;
        effects
    }
}

pub(super) fn filesystem_identity(source: &str) -> Option<PathBuf> {
    let path = Url::parse(source).ok()?.to_file_path().ok()?;
    dunce::canonicalize(path).ok()
}
