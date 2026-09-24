//! Workspace states (loading, ready, failed) and the document lifecycle.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{fs, io};

use building::lifecycle::{
    AnalysisInvalidation, DiskObservation, DocumentKind, LifecycleEvent, ReloadFailure,
    SourceUnitKey,
};
use files::{FileId, ForeignSourceKind};
use iris_build::compilation::{CompilationParts, CompilationState, MaterializedPrim};
use iris_configuration::Configuration;
use itertools::Itertools;
use lsp_types::Uri;

use crate::analysis::{Analysis, ChangeSignal};
use crate::diagnostics::DiagnosticScheduler;
use crate::handlers::{DocumentError, DocumentNotification};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SourceMetadata {
    Builtin,
    Package { editable: bool },
    Unmanaged { editable: bool },
}

impl SourceMetadata {
    pub(crate) fn editable(&self) -> bool {
        match self {
            SourceMetadata::Builtin => false,
            SourceMetadata::Package { editable } | SourceMetadata::Unmanaged { editable } => {
                *editable
            }
        }
    }
}

pub(crate) struct SourceRoot {
    pub(crate) path: PathBuf,
    pub(crate) metadata: SourceMetadata,
}

/// The result of a successful preparation attempt.
pub(crate) struct PreparedWorkspace {
    pub(crate) compilation: CompilationState<i32, SourceMetadata>,
    pub(crate) source_roots: Vec<SourceRoot>,
}

pub(crate) enum WorkspaceState {
    /// Preparation has not finished. Document notifications are buffered in order and replayed
    /// once it finishes; settings replace the configuration to install.
    Loading {
        pending: Vec<DocumentNotification>,
        configuration: Arc<Configuration>,
    },
    Ready(Box<ReadyWorkspace>),
    /// Preparation failed; analysis is rejected for the rest of the session.
    Failed,
}

pub(crate) struct ReadyWorkspace {
    pub(crate) configuration: Arc<Configuration>,
    pub(crate) analysis: Analysis,
    pub(crate) source_roots: Vec<SourceRoot>,
    pub(crate) diagnostics: DiagnosticScheduler,
    _prim: MaterializedPrim,
}

/// Which sources a document change collects diagnostics for.
pub(crate) enum DiagnosticTrigger {
    None,
    AssociatedSource(Uri),
    AnalysisChange,
}

/// What the workspace actor does after a document notification was applied.
#[derive(Default)]
#[must_use]
pub(crate) struct WorkspaceEffects {
    /// Removed sources whose published diagnostics are cleared.
    pub(crate) clear_diagnostics: Vec<Uri>,
    pub(crate) collect_diagnostics: Vec<FileId>,
}

impl ReadyWorkspace {
    pub(crate) fn new(
        prepared: PreparedWorkspace,
        configuration: Arc<Configuration>,
    ) -> ReadyWorkspace {
        let CompilationParts { engine, files, prim } = prepared.compilation.into_parts();
        ReadyWorkspace {
            configuration,
            analysis: Analysis::new(engine, files),
            source_roots: prepared.source_roots,
            diagnostics: DiagnosticScheduler::default(),
            _prim: prim,
        }
    }

    /// Applies lifecycle events, first telling tasks waiting for permits to drop their snapshots.
    pub(crate) fn apply_lifecycle_events(
        &mut self,
        events: impl IntoIterator<Item = LifecycleEvent<i32, SourceMetadata>>,
        trigger: DiagnosticTrigger,
        change_signal: &ChangeSignal,
    ) -> WorkspaceEffects {
        change_signal.send();
        let change = self.analysis.apply(events, &mut self.diagnostics);
        for warning in change.warnings() {
            tracing::warn!("{warning}");
        }

        let clear_diagnostics = change.removed_sources().iter().map(|removed| {
            Uri::parse(&removed.locator)
                .expect("invariant violated: removed source has an invalid locator")
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

    /// Collects diagnostics for the source associated with `uri`, without changing anything.
    pub(crate) fn associated_effects(&self, uri: &Uri) -> Result<WorkspaceEffects, DocumentError> {
        let (_, unit) = source_unit_from_document_uri(uri)?;
        let files = self.analysis.files.read();
        let collect_diagnostics = files.source_id(unit.source()).into_iter().collect_vec();
        Ok(WorkspaceEffects { clear_diagnostics: vec![], collect_diagnostics })
    }

    /// The metadata of a source: what it was loaded with, the package root containing it, or
    /// whether it lies under the workspace root.
    pub(crate) fn source_metadata(
        &self,
        root: Option<&Path>,
        unit: &SourceUnitKey,
        uri: &Uri,
    ) -> SourceMetadata {
        let previous = {
            let files = self.analysis.files.read();
            let file_id = files.source_id(unit.source());
            file_id.and_then(|file_id| files.source_metadata(file_id)).cloned()
        };
        previous.unwrap_or_else(|| {
            let path = uri.to_file_path().ok();
            let package_metadata = path.as_ref().and_then(|path| {
                self.source_roots
                    .iter()
                    .find(|source_root| path.starts_with(&source_root.path))
                    .map(|source_root| SourceMetadata::clone(&source_root.metadata))
            });
            package_metadata.unwrap_or_else(|| match (root, path) {
                (Some(root), Some(path)) => {
                    SourceMetadata::Unmanaged { editable: path.starts_with(root) }
                }
                (Some(_), None) => SourceMetadata::Unmanaged { editable: false },
                (None, _) => SourceMetadata::Unmanaged { editable: true },
            })
        })
    }

    pub(crate) fn source_editable(
        &self,
        root: Option<&Path>,
        unit: &SourceUnitKey,
        uri: &Uri,
    ) -> bool {
        self.source_metadata(root, unit, uri).editable()
    }

    /// Observes the foreign files beside a source on disk, except those open in the editor.
    pub(crate) fn observe_sibling_foreign(
        &self,
        unit: &SourceUnitKey,
    ) -> Result<Vec<LifecycleEvent<i32, SourceMetadata>>, DocumentError> {
        let mut events = vec![];
        for kind in ForeignSourceKind::ALL {
            let document =
                building::lifecycle::DocumentKey::Foreign(SourceUnitKey::clone(unit), kind);
            if self.analysis.files.read().is_open(&document) {
                continue;
            }
            let uri = Uri::parse(unit.foreign_for(kind))?;
            events.push(LifecycleEvent::Foreign {
                unit: SourceUnitKey::clone(unit),
                kind,
                event: building::lifecycle::ForeignEvent::DiskObserved { disk: observe_disk(&uri) },
            });
        }
        Ok(events)
    }
}

pub(crate) fn document_kind(uri: &Uri) -> Option<DocumentKind> {
    if uri.path().ends_with(".js") {
        Some(DocumentKind::Foreign(ForeignSourceKind::JavaScript))
    } else if uri.path().ends_with(".jsx") {
        Some(DocumentKind::Foreign(ForeignSourceKind::Jsx))
    } else if uri.path().ends_with(".purs") {
        Some(DocumentKind::Source)
    } else {
        None
    }
}

pub(crate) fn source_unit_from_document_uri(
    uri: &Uri,
) -> Result<(DocumentKind, SourceUnitKey), DocumentError> {
    let document =
        document_kind(uri).ok_or_else(|| DocumentError::UnsupportedDocumentUri(Uri::clone(uri)))?;
    let unit = match document {
        DocumentKind::Source => source_unit_from_source_uri(uri)?,
        DocumentKind::Foreign(_) => source_unit_from_foreign_uri(uri)?,
    };
    Ok((document, unit))
}

fn file_uri_with_extension(uri: &Uri, extension: &str) -> Result<Uri, DocumentError> {
    if uri.scheme() != "file" || uri.to_file_path().is_err() {
        return Err(DocumentError::InvalidFileUri(Uri::clone(uri)));
    }
    let uri_path = uri.path();
    let file_name_start = uri_path.rfind('/').map_or(0, |index| index + 1);
    let extension_start = uri_path[file_name_start..]
        .rfind('.')
        .filter(|index| *index > 0)
        .map_or(uri_path.len(), |index| file_name_start + index);
    let mut sibling_path = String::from(&uri_path[..extension_start]);
    sibling_path.push('.');
    sibling_path.push_str(extension);

    let mut sibling_uri = Uri::clone(uri);
    sibling_uri.set_path(&sibling_path);
    Ok(sibling_uri)
}

pub(crate) fn source_unit_from_source_uri(
    source_uri: &Uri,
) -> Result<SourceUnitKey, DocumentError> {
    let javascript_uri = file_uri_with_extension(source_uri, "js")?;
    let jsx_uri = file_uri_with_extension(source_uri, "jsx")?;
    Ok(SourceUnitKey::with_foreign_sources(
        source_uri.as_str(),
        javascript_uri.as_str(),
        jsx_uri.as_str(),
    ))
}

pub(crate) fn source_unit_from_foreign_uri(
    foreign_uri: &Uri,
) -> Result<SourceUnitKey, DocumentError> {
    let source_uri = file_uri_with_extension(foreign_uri, "purs")?;
    source_unit_from_source_uri(&source_uri)
}

pub(crate) fn observe_disk(uri: &Uri) -> DiskObservation {
    let path = uri.to_file_path().expect("invariant violated: expected a valid file URI");
    match fs::read_to_string(path) {
        Ok(content) => DiskObservation::Found(Arc::from(content)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => DiskObservation::NotFound,
        Err(error) => {
            let kind = error.kind();
            DiskObservation::Failed(ReloadFailure::new(kind, error.to_string()))
        }
    }
}
