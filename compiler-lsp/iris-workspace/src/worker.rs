use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::{Arc, mpsc};
use std::{fs, io, panic};

use analyzer::AnalyzerContext;
use building::{
    DiskObservation, FileLifecycle, ForeignEvent, LifecycleEvent, QueryEngine, SourceEvent,
    SourceUnitKey,
};
use files::ForeignSourceKind;
use iris_build::analysis::{self, AnalysisConfig, AnalysisOverlay, AnalysisSelection};
use iris_build::compilation::{CompilationParts, MaterializedPrim};
use iris_build::events::{BuildEvent, BuildEventSink};
use lsp_types::{Diagnostic, Url};

use crate::controller::Message;
use crate::documents::{OpenDocument, document_path};
use crate::language_server::{Analysis, Host, failure};
use crate::{
    AnalysisStamp, Cancellation, ConfigurationInput, Generation, LanguageServer, Options,
    RequestFailure,
};

pub(crate) enum Work {
    Prepare {
        configuration: ConfigurationInput,
        documents: BTreeMap<Url, OpenDocument>,
        generation: Generation,
        stamp: AnalysisStamp,
        cancellation: Cancellation,
    },
    Reconcile {
        documents: BTreeMap<Url, OpenDocument>,
        dirty: BTreeSet<Url>,
        stamp: AnalysisStamp,
    },
    Analyze(LanguageServer),
    Diagnostics {
        uri: Url,
        stamp: AnalysisStamp,
        cancellation: Cancellation,
    },
    Stop,
}

pub(crate) enum Completed {
    Reconciled {
        generation: Generation,
        stamp: AnalysisStamp,
        sources: Vec<Url>,
    },
    Analyzed,
    Diagnostics {
        uri: Url,
        stamp: AnalysisStamp,
        version: Option<i32>,
        result: Result<Vec<Diagnostic>, RequestFailure>,
    },
    Failed {
        generation: Generation,
        failure: RequestFailure,
    },
    Stopped,
}

struct Events {
    sender: mpsc::Sender<Message>,
    generation: Generation,
}

impl BuildEventSink for Events {
    fn send(&self, event: BuildEvent) {
        let _ = self.sender.send(Message::Progress { generation: self.generation, event });
    }
}

struct Compilation {
    engine: QueryEngine,
    files: FileLifecycle<i32, bool>,
    selection: AnalysisSelection,
    documents: BTreeMap<Url, OpenDocument>,
    analysis: Analysis,
    generation: Generation,
    stamp: AnalysisStamp,
}

impl Compilation {
    fn source_uris(&self) -> Vec<Url> {
        self.files
            .source_ids()
            .filter_map(|file_id| {
                if self.files.source_metadata(file_id) != Some(&true) {
                    return None;
                }
                self.files.source_path(file_id).and_then(|uri| Url::parse(&uri).ok())
            })
            .collect()
    }

    fn reconcile(
        &mut self,
        documents: BTreeMap<Url, OpenDocument>,
        dirty: BTreeSet<Url>,
        stamp: AnalysisStamp,
    ) -> Result<(), RequestFailure> {
        self.analysis.invalidate();
        for uri in dirty {
            let path = document_path(&uri)?;
            let unit = unit(&path)?;
            let current = documents.get(&uri);
            let previous = self.documents.get(&uri);
            let metadata = self.selection.metadata(&uri).unwrap_or(false);
            let source = path.extension().is_some_and(|extension| extension == "purs");
            let event = if source {
                let event = match (previous, current) {
                    (_, Some(document))
                        if previous
                            .is_none_or(|previous| previous.lifetime != document.lifetime) =>
                    {
                        SourceEvent::Opened {
                            text: Arc::clone(&document.text),
                            version: document.version,
                            metadata,
                        }
                    }
                    (Some(previous), Some(document)) if previous.version != document.version => {
                        SourceEvent::Changed {
                            text: Arc::clone(&document.text),
                            version: document.version,
                        }
                    }
                    (_, Some(_)) => continue,
                    (Some(_), None) if !self.selection.selected_sources.contains(uri.as_str()) => {
                        SourceEvent::Closed { disk: DiskObservation::NotFound }
                    }
                    (Some(_), None) => SourceEvent::Closed { disk: observe(&path)? },
                    (None, None) if !self.selection.selected_sources.contains(uri.as_str()) => {
                        continue;
                    }
                    (None, None) => SourceEvent::DiskObserved { disk: observe(&path)?, metadata },
                };
                LifecycleEvent::Source { unit: SourceUnitKey::clone(&unit), event }
            } else {
                let kind = if path.extension().is_some_and(|extension| extension == "jsx") {
                    ForeignSourceKind::Jsx
                } else {
                    ForeignSourceKind::JavaScript
                };
                let event = match (previous, current) {
                    (_, Some(document))
                        if previous
                            .is_none_or(|previous| previous.lifetime != document.lifetime) =>
                    {
                        ForeignEvent::Opened {
                            text: Arc::clone(&document.text),
                            version: document.version,
                        }
                    }
                    (Some(previous), Some(document)) if previous.version != document.version => {
                        ForeignEvent::Changed {
                            text: Arc::clone(&document.text),
                            version: document.version,
                        }
                    }
                    (_, Some(_)) => continue,
                    (Some(_), None) => ForeignEvent::Closed { disk: observe(&path)? },
                    (None, None) => ForeignEvent::DiskObserved { disk: observe(&path)? },
                };
                LifecycleEvent::Foreign { unit: SourceUnitKey::clone(&unit), kind, event }
            };
            self.files.apply(&self.engine, event);
            if source && (previous.is_none() || current.is_none()) {
                for kind in ForeignSourceKind::ALL {
                    let foreign_path = path.with_extension(kind.extension());
                    let foreign_uri = Url::from_file_path(&foreign_path)
                        .map_err(|()| RequestFailure::Workspace("invalid foreign path".into()))?;
                    if let Some(document) = documents.get(&foreign_uri) {
                        self.files.apply(
                            &self.engine,
                            LifecycleEvent::Foreign {
                                unit: SourceUnitKey::clone(&unit),
                                kind,
                                event: ForeignEvent::Opened {
                                    text: Arc::clone(&document.text),
                                    version: document.version,
                                },
                            },
                        );
                    } else {
                        self.files.apply(
                            &self.engine,
                            LifecycleEvent::Foreign {
                                unit: SourceUnitKey::clone(&unit),
                                kind,
                                event: ForeignEvent::DiskObserved { disk: observe(&foreign_path)? },
                            },
                        );
                    }
                }
            }
        }
        self.documents = documents;
        self.stamp = stamp;
        Ok(())
    }
}

fn observe(path: &Path) -> Result<DiskObservation, RequestFailure> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(DiskObservation::Found(text.into())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(DiskObservation::NotFound),
        Err(error) => Err(RequestFailure::Workspace(format!("{}: {error}", path.display()).into())),
    }
}

fn unit(path: &Path) -> Result<SourceUnitKey, RequestFailure> {
    let source = Url::from_file_path(path.with_extension("purs"))
        .map_err(|()| RequestFailure::Workspace("invalid source path".into()))?;
    let foreign = Url::from_file_path(path.with_extension("js"))
        .map_err(|()| RequestFailure::Workspace("invalid foreign path".into()))?;
    Ok(SourceUnitKey::new(source.as_str(), foreign.as_str()))
}

pub(crate) fn run(
    receiver: mpsc::Receiver<Work>,
    sender: mpsc::Sender<Message>,
    options: Options,
    hooks: crate::testing::Hooks,
) {
    let mut compilation: Option<Compilation> = None;
    let mut prim: Option<Arc<MaterializedPrim>> = None;
    let mut generation = Generation::default();
    while let Ok(work) = receiver.recv() {
        let result = panic::catch_unwind(panic::AssertUnwindSafe(
            || -> Result<Completed, RequestFailure> {
                match work {
                    Work::Prepare {
                        configuration,
                        documents,
                        generation: next_generation,
                        stamp,
                        cancellation,
                    } => {
                        generation = next_generation;
                        compilation = None;
                        hooks.reach(crate::testing::Point::BeforePreparation);
                        if cancellation.is_cancelled() {
                            return Err(RequestFailure::Cancelled);
                        }
                        let materialized = match &prim {
                            Some(prim) => Arc::clone(prim),
                            None => {
                                let materialized =
                                    Arc::new(MaterializedPrim::new().map_err(|error| {
                                        RequestFailure::Workspace(error.to_string().into())
                                    })?);
                                prim = Some(Arc::clone(&materialized));
                                materialized
                            }
                        };
                        let configuration = AnalysisConfig {
                            root: configuration.root,
                            sources: configuration.settings.sources,
                        };
                        let overlays = documents
                            .iter()
                            .map(|(uri, document)| AnalysisOverlay {
                                uri: Url::clone(uri),
                                text: Arc::clone(&document.text),
                                version: document.version,
                            })
                            .collect::<Vec<_>>();
                        let events = Events { sender: sender.clone(), generation };
                        let prepared = analysis::prepare(
                            &configuration,
                            &overlays,
                            materialized,
                            &cancellation.build,
                            &events,
                        )
                        .map_err(|error| {
                            if matches!(error, analysis::AnalysisError::Cancelled) {
                                RequestFailure::Cancelled
                            } else {
                                RequestFailure::Workspace(error.to_string().into())
                            }
                        })?;
                        let CompilationParts { engine, files, .. } =
                            prepared.compilation.into_parts();
                        let prim_id = engine.module_file("Prim").expect("Prim must be registered");
                        let prim_uri =
                            files.source_path(prim_id).expect("Prim must have a locator");
                        let analysis =
                            Analysis::new(format!("{prim_uri}:{}", stamp.incarnation.value));
                        compilation = Some(Compilation {
                            engine,
                            files,
                            selection: prepared.selection,
                            documents,
                            analysis,
                            generation,
                            stamp,
                        });
                        let sources = compilation.as_ref().unwrap().source_uris();
                        Ok(Completed::Reconciled { generation, stamp, sources })
                    }
                    Work::Reconcile { documents, dirty, stamp } => {
                        let compilation =
                            compilation.as_mut().ok_or(RequestFailure::Unavailable)?;
                        compilation.reconcile(documents, dirty, stamp)?;
                        Ok(Completed::Reconciled {
                            generation: compilation.generation,
                            stamp,
                            sources: compilation.source_uris(),
                        })
                    }
                    Work::Analyze(mut command) => {
                        hooks.reach(crate::testing::Point::BeforeAnalysis);
                        match compilation.as_mut() {
                            Some(compilation) if compilation.stamp == command.stamp() => {
                                if command.cancellation().is_cancelled() {
                                    command.reject(RequestFailure::Cancelled);
                                } else {
                                    compilation.analysis.execute(
                                        command,
                                        &compilation.engine,
                                        &compilation.files,
                                        options,
                                    );
                                }
                            }
                            _ => command.reject(RequestFailure::Stale),
                        }
                        Ok(Completed::Analyzed)
                    }
                    Work::Diagnostics { uri, stamp, cancellation } => {
                        hooks.reach(crate::testing::Point::BeforeDiagnostics);
                        let compilation =
                            compilation.as_ref().ok_or(RequestFailure::Unavailable)?;
                        let version =
                            compilation.documents.get(&uri).map(|document| document.version);
                        let result = if compilation.stamp != stamp || cancellation.is_cancelled() {
                            Err(RequestFailure::Cancelled)
                        } else if let Some(file_id) = compilation.files.source_id(uri.as_str()) {
                            let snapshot =
                                compilation.engine.snapshot_with_cancellation(cancellation.query);
                            let host = Host { engine: &snapshot, files: &compilation.files };
                            let context = AnalyzerContext::new(
                                &host,
                                options.position_encoding,
                                options.capabilities,
                            );
                            analyzer::diagnostics::implementation(&context, file_id)
                                .map(|collected| collected.diagnostics)
                                .map_err(failure)
                        } else {
                            Ok(vec![])
                        };
                        Ok(Completed::Diagnostics { uri, stamp, version, result })
                    }
                    Work::Stop => {
                        compilation = None;
                        prim = None;
                        Ok(Completed::Stopped)
                    }
                }
            },
        ));
        let completed = match result {
            Ok(Ok(completed)) => completed,
            Ok(Err(failure)) => {
                compilation = None;
                Completed::Failed { generation, failure }
            }
            Err(_) => {
                compilation = None;
                Completed::Failed {
                    generation,
                    failure: RequestFailure::Workspace("compilation worker panicked".into()),
                }
            }
        };
        if matches!(completed, Completed::Reconciled { .. }) {
            hooks.reach(crate::testing::Point::BeforeAcknowledgement);
        }
        let stopped = matches!(completed, Completed::Stopped);
        if sender.send(Message::Completed(completed)).is_err() || stopped {
            break;
        }
    }
}
