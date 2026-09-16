use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{fs, mem};

use analyzer::AnalyzerCapabilities;
use analyzer::position::PositionEncoding;
use async_lsp::{ClientSocket, LanguageClient};
use building::lifecycle::{
    AnalysisInvalidation, DiskObservation, ForeignEvent, LifecycleChange, LifecycleEvent,
    SourceEvent, SourceUnitKey,
};
use configuration::Configuration;
use files::{FileId, ForeignSourceKind};
use iris_build::compilation::{CompilationParts, CompilationState, MaterializedPrim};
use itertools::Itertools;
use lsp_types::{
    DidChangeTextDocumentParams, DidChangeWatchedFilesParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams, PublishDiagnosticsParams, Url,
};
use rustc_hash::FxHashSet;

use super::analysis::{Analysis, StateSnapshot};
use super::error::LspError;
use super::event::{CollectDiagnostics, DiagnosticScheduler, DiagnosticTicket};
use super::{
    DiscoveredWorkspace, SourceMetadata, did_change, did_change_watched_files, did_close, did_open,
    did_save, observe_sibling_foreign, source_unit_from_document_uri, source_unit_from_source_uri,
    source_uri,
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
    pub(super) analysis: Analysis,
    pub(super) source_roots: Vec<SourceRoot>,
    pub(super) selected_sources: FxHashSet<Arc<str>>,
    pub(super) excluded_sources: FxHashSet<Arc<str>>,
    pub(super) diagnostics: DiagnosticScheduler,
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

pub(super) enum ConfigurationApplyError {
    Preparation(LspError),
    Delivery(LspError),
}

impl std::fmt::Display for ConfigurationApplyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigurationApplyError::Preparation(error)
            | ConfigurationApplyError::Delivery(error) => error.fmt(formatter),
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
    pub(super) fn new() -> WorkspaceRuntime {
        WorkspaceRuntime { state: WorkspaceState::WaitingForConfiguration { pending: vec![] } }
    }

    pub(super) fn is_ready(&self) -> bool {
        matches!(self.state, WorkspaceState::Ready { .. })
    }

    fn ready(&self) -> Result<&ReadyWorkspace, LspError> {
        match &self.state {
            WorkspaceState::WaitingForConfiguration { .. } => Err(LspError::WorkspaceNotReady),
            WorkspaceState::Ready { workspace } => Ok(workspace),
        }
    }

    fn ready_mut(&mut self) -> Result<&mut ReadyWorkspace, LspError> {
        match &mut self.state {
            WorkspaceState::WaitingForConfiguration { .. } => Err(LspError::WorkspaceNotReady),
            WorkspaceState::Ready { workspace } => Ok(workspace),
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
        let WorkspaceState::WaitingForConfiguration { pending } = &mut self.state else {
            return Err(LspError::WorkspaceAlreadyReady);
        };
        let pending = mem::take(pending);
        let CompilationParts { engine, files, prim } = prepared.compilation.into_parts();
        let workspace = ReadyWorkspace {
            configuration: prepared.configuration,
            analysis: Analysis::new(engine, files),
            source_roots: prepared.source_roots,
            selected_sources: prepared.selected_sources,
            excluded_sources: FxHashSet::default(),
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

    pub(super) fn prepare_reconfiguration(
        &self,
        configuration: Arc<Configuration>,
        discovered: DiscoveredWorkspace,
    ) -> Result<PreparedSourceReconfiguration, LspError> {
        let workspace = self.ready()?;
        let mut files = BTreeMap::new();
        for path in &discovered.source_globs {
            let content = Arc::from(fs::read_to_string(path)?);
            let metadata = discovered
                .metadata
                .get(path)
                .cloned()
                .expect("invariant violated: discovered source has no LSP metadata");
            files.insert(PathBuf::clone(path), (content, metadata));
        }
        tracing::info!("Loading {} files.", files.len());

        let selected_sources = files.keys().map(source_uri);
        let selected_sources = selected_sources.collect::<Result<FxHashSet<_>, _>>()?;
        let previous_sources = FxHashSet::clone(&workspace.selected_sources);
        let removed_sources = previous_sources.difference(&selected_sources).cloned().collect_vec();
        let mut events = vec![];
        for source in removed_sources {
            let uri = Url::parse(&source)?;
            let unit = source_unit_from_source_uri(&uri)?;
            events.push(LifecycleEvent::Source {
                unit: SourceUnitKey::clone(&unit),
                event: SourceEvent::DiskObserved {
                    disk: DiskObservation::NotFound,
                    metadata: SourceMetadata::Unmanaged { editable: false },
                },
            });
            for kind in ForeignSourceKind::ALL {
                events.push(LifecycleEvent::Foreign {
                    unit: SourceUnitKey::clone(&unit),
                    kind,
                    event: ForeignEvent::DiskObserved { disk: DiskObservation::NotFound },
                });
            }
        }
        for (file, (content, metadata)) in &files {
            let uri = Url::from_file_path(file)
                .map_err(|_| LspError::PathParseFail(PathBuf::clone(file)))?;
            let unit = source_unit_from_source_uri(&uri)?;
            events.push(LifecycleEvent::Source {
                unit: SourceUnitKey::clone(&unit),
                event: SourceEvent::DiskObserved {
                    disk: DiskObservation::Found(Arc::clone(content)),
                    metadata: SourceMetadata::clone(metadata),
                },
            });
            events.extend(observe_sibling_foreign(workspace, &unit)?);
        }

        let mut excluded_sources = FxHashSet::clone(&workspace.excluded_sources);
        excluded_sources.extend(previous_sources.difference(&selected_sources).cloned());
        for selected in &selected_sources {
            excluded_sources.remove(selected);
        }
        tracing::info!("Loaded {} files.", files.len());
        Ok(PreparedSourceReconfiguration {
            configuration,
            source_roots: discovered.source_roots,
            selected_sources,
            excluded_sources,
            events,
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
