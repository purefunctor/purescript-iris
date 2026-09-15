use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use building::Cancellation;
use building::lifecycle::{
    DiskObservation, ForeignEvent, LifecycleEvent, SourceEvent, SourceUnitKey,
};
use configuration::{Configuration, SourceDiscovery};
use files::ForeignSourceKind;
use iris_build::compilation::MaterializedPrim;
use iris_build::compile::{InitialBuildConfig, PackageExecution, build_initial};
use iris_build::events::SilentBuildEvents;
use iris_build::plan::PackageInput;
use itertools::Itertools;
use path_absolutize::Absolutize;
use rustc_hash::FxHashSet;
use smol_str::SmolStr;
use tokio::sync::Semaphore;
use tokio::task;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use url::Url;

use super::error::LspError;
use super::workspace::{PreparedInitialWorkspace, PreparedSourceReconfiguration, SourceRoot};
use super::{SourceMetadata, observe_disk, process, source_unit_from_source_uri, source_uri};
use crate::walk;

pub(super) struct DiscoveredWorkspace {
    pub(super) source_globs: Vec<PathBuf>,
    pub(super) packages: Vec<PackageInput>,
    pub(super) metadata: BTreeMap<PathBuf, SourceMetadata>,
    pub(super) source_roots: Vec<SourceRoot>,
}

#[derive(Clone)]
pub(super) struct ReconfigurationBaseline {
    pub(super) selected_sources: FxHashSet<Arc<str>>,
    pub(super) excluded_sources: FxHashSet<Arc<str>>,
    pub(super) tracked_sources: Vec<Arc<str>>,
}

pub(super) enum PreparedWorkspace {
    Initial(PreparedInitialWorkspace),
    Reconfiguration(PreparedSourceReconfiguration),
    Fallback { prepared: PreparedInitialWorkspace, error: LspError },
}

pub(super) struct PreparationFinished {
    pub(super) generation: u64,
    pub(super) result: Result<PreparedWorkspace, LspError>,
}

struct Attempt {
    generation: u64,
    query_cancellation: Cancellation,
    command_cancellation: CancellationToken,
    dirty: bool,
}

impl Drop for Attempt {
    fn drop(&mut self) {
        self.query_cancellation.cancel();
        self.command_cancellation.cancel();
    }
}

pub(super) struct PreparationRuntime {
    generation: u64,
    permit: Arc<Semaphore>,
    active: Option<Attempt>,
    tasks: TaskTracker,
}

impl PreparationRuntime {
    pub(super) fn new() -> PreparationRuntime {
        PreparationRuntime {
            generation: 0,
            permit: Arc::new(Semaphore::new(1)),
            active: None,
            tasks: TaskTracker::new(),
        }
    }

    pub(super) fn admit(
        &mut self,
        root: PathBuf,
        configuration: Arc<Configuration>,
        baseline: Option<ReconfigurationBaseline>,
        startup_configuration: Option<Arc<Configuration>>,
        client: async_lsp::ClientSocket,
    ) {
        self.cancel_active();
        self.generation =
            self.generation.checked_add(1).expect("preparation generation overflowed");
        let generation = self.generation;
        let query_cancellation = Cancellation::new();
        let command_cancellation = CancellationToken::new();
        let permit = Arc::clone(&self.permit);
        let task_query = Cancellation::clone(&query_cancellation);
        let task_command = CancellationToken::clone(&command_cancellation);
        self.tasks.spawn(async move {
            let initial = baseline.is_none();
            let result = prepare(
                root,
                Arc::clone(&configuration),
                baseline,
                permit,
                Cancellation::clone(&task_query),
                task_command,
            )
            .await;
            let result = match (result, startup_configuration) {
                (Err(error), Some(configuration)) if initial && task_query.check().is_ok() => {
                    task::spawn_blocking(move || {
                        task_query.check()?;
                        let prepared = minimal_workspace(configuration)?;
                        task_query.check()?;
                        Ok(PreparedWorkspace::Fallback { prepared, error })
                    })
                    .await
                    .map_err(LspError::from)
                    .and_then(|result| result)
                }
                (result, _) => result,
            };
            if let Err(error) = client.emit(PreparationFinished { generation, result }) {
                tracing::error!("Failed to deliver workspace preparation: {error}");
            }
        });
        self.active =
            Some(Attempt { generation, query_cancellation, command_cancellation, dirty: false });
    }

    pub(super) fn mark_dirty(&mut self) {
        if let Some(active) = &mut self.active {
            active.dirty = true;
            active.query_cancellation.cancel();
            active.command_cancellation.cancel();
        }
    }

    pub(super) fn take_current(&mut self, generation: u64) -> Option<bool> {
        if self.active.as_ref().is_none_or(|attempt| attempt.generation != generation) {
            return None;
        }
        self.active.take().map(|attempt| attempt.dirty)
    }

    /// Requests cancellation without claiming that process or blocking work has retired.
    pub(super) fn cancel_active(&mut self) {
        if let Some(active) = &self.active {
            active.query_cancellation.cancel();
            active.command_cancellation.cancel();
        }
    }

    pub(super) fn supersede_active(&mut self) {
        self.cancel_active();
        self.active = None;
    }

    pub(super) fn shutdown(&mut self) -> TaskTracker {
        self.supersede_active();
        self.tasks.close();
        TaskTracker::clone(&self.tasks)
    }
}

async fn prepare(
    root: PathBuf,
    configuration: Arc<Configuration>,
    baseline: Option<ReconfigurationBaseline>,
    permit: Arc<Semaphore>,
    query_cancellation: Cancellation,
    command_cancellation: CancellationToken,
) -> Result<PreparedWorkspace, LspError> {
    let manual_output = match &configuration.sources {
        SourceDiscovery::Command { program, arguments } => {
            let output = process::run(&root, program, arguments, &command_cancellation).await?;
            if !output.status.success() {
                return Err(LspError::SourceCommandFailed {
                    status: output.status,
                    stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                });
            }
            Some(String::from_utf8(output.stdout).map_err(|error| error.utf8_error())?)
        }
        SourceDiscovery::Spago {} => None,
    };
    let permit = permit.acquire_owned().await.expect("preparation permit closed");
    task::spawn_blocking(move || {
        // The permit is owned by this closure so cancellation never allows a second
        // compiler preparation to enter before the first blocking attempt retires.
        let _permit = permit;
        query_cancellation.check()?;
        let discovered = match manual_output {
            Some(output) => discover_manual(&root, &output)?,
            None => discover_spago(&root)?,
        };
        let result = match baseline {
            Some(baseline) => prepare_reconfiguration(
                configuration,
                discovered,
                baseline,
                Cancellation::clone(&query_cancellation),
            )
            .map(PreparedWorkspace::Reconfiguration),
            None => prepare_initial(
                &root,
                configuration,
                discovered,
                Cancellation::clone(&query_cancellation),
            )
            .map(PreparedWorkspace::Initial),
        };
        query_cancellation.check()?;
        result
    })
    .await?
}

fn prepare_initial(
    root: &Path,
    configuration: Arc<Configuration>,
    discovered: DiscoveredWorkspace,
    cancellation: Cancellation,
) -> Result<PreparedInitialWorkspace, LspError> {
    let selected_sources = discovered.source_globs.iter().map(|path| source_uri(path));
    let selected_sources = selected_sources.collect::<Result<FxHashSet<_>, _>>()?;
    let initial = build_initial::<i32, SourceMetadata, _>(InitialBuildConfig {
        root,
        source_globs: &discovered.source_globs,
        excluded: &[],
        packages: discovered.packages,
        prim_metadata: SourceMetadata::Builtin,
        source_metadata: |path: &Path| {
            discovered.metadata.get(path).cloned().expect("source metadata missing")
        },
        execution: PackageExecution::Parallel,
        events: &SilentBuildEvents,
        cancellation: Some(cancellation),
    })?;
    Ok(PreparedInitialWorkspace {
        configuration,
        compilation: initial.into_compilation(),
        source_roots: discovered.source_roots,
        selected_sources,
    })
}

pub(super) fn minimal_workspace(
    configuration: Arc<Configuration>,
) -> Result<PreparedInitialWorkspace, LspError> {
    let prim = MaterializedPrim::new()?;
    Ok(PreparedInitialWorkspace {
        configuration,
        compilation: iris_build::compilation::CompilationState::new(prim, SourceMetadata::Builtin),
        source_roots: vec![],
        selected_sources: FxHashSet::default(),
    })
}

pub(super) fn prepare_reconfiguration(
    configuration: Arc<Configuration>,
    discovered: DiscoveredWorkspace,
    baseline: ReconfigurationBaseline,
    cancellation: Cancellation,
) -> Result<PreparedSourceReconfiguration, LspError> {
    let mut files = BTreeMap::new();
    for path in &discovered.source_globs {
        cancellation.check()?;
        let content = Arc::from(std::fs::read_to_string(path)?);
        let metadata = discovered.metadata.get(path).cloned().expect("source metadata missing");
        files.insert(PathBuf::clone(path), (content, metadata));
    }
    let selected_sources = files.keys().map(|path| source_uri(path));
    let mut selected_sources = selected_sources.collect::<Result<FxHashSet<_>, _>>()?;
    let selected_physical =
        files.keys().filter_map(|path| std::fs::canonicalize(path).ok()).collect::<FxHashSet<_>>();
    let canonical_source = |locator: &Arc<str>| {
        Url::parse(locator)
            .ok()
            .and_then(|uri| uri.to_file_path().ok())
            .and_then(|path| std::fs::canonicalize(path).ok())
    };
    let previous_physical =
        baseline.selected_sources.iter().filter_map(canonical_source).collect::<FxHashSet<_>>();
    let mut known_sources = FxHashSet::clone(&baseline.selected_sources);
    known_sources.extend(baseline.excluded_sources.iter().map(Arc::clone));
    for source in &baseline.tracked_sources {
        if canonical_source(source).is_some_and(|path| previous_physical.contains(&path)) {
            known_sources.insert(Arc::clone(source));
        }
    }
    let selected = |locator: &Arc<str>| {
        selected_sources.contains(locator)
            || canonical_source(locator).is_some_and(|path| selected_physical.contains(&path))
    };
    let selected_aliases =
        known_sources.iter().filter(|source| selected(source)).map(Arc::clone).collect_vec();
    let mut events = vec![];
    for source in &baseline.tracked_sources {
        cancellation.check()?;
        if selected(source) || !known_sources.contains(source) {
            continue;
        }
        let unit = source_unit_from_source_uri(&url::Url::parse(source)?)?;
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
        let uri = url::Url::from_file_path(file)
            .map_err(|_| LspError::PathParseFail(PathBuf::clone(file)))?;
        let unit = source_unit_from_source_uri(&uri)?;
        events.push(LifecycleEvent::Source {
            unit: SourceUnitKey::clone(&unit),
            event: SourceEvent::DiskObserved {
                disk: DiskObservation::Found(Arc::clone(content)),
                metadata: SourceMetadata::clone(metadata),
            },
        });
        for kind in ForeignSourceKind::ALL {
            let uri = url::Url::parse(unit.foreign_for(kind))?;
            events.push(LifecycleEvent::Foreign {
                unit: SourceUnitKey::clone(&unit),
                kind,
                event: ForeignEvent::DiskObserved { disk: observe_disk(&uri) },
            });
        }
    }
    for source in &baseline.tracked_sources {
        cancellation.check()?;
        if !known_sources.contains(source) || !selected(source) || selected_sources.contains(source)
        {
            continue;
        }
        let uri = Url::parse(source)?;
        let path = uri.to_file_path().map_err(|_| LspError::InvalidFileUri(Url::clone(&uri)))?;
        let metadata = canonical_source(source)
            .and_then(|alias| {
                files.iter().find_map(|(file, (_, metadata))| {
                    std::fs::canonicalize(file)
                        .ok()
                        .filter(|selected| *selected == alias)
                        .map(|_| SourceMetadata::clone(metadata))
                })
            })
            .unwrap_or(SourceMetadata::Unmanaged { editable: false });
        let unit = source_unit_from_source_uri(&uri)?;
        events.push(LifecycleEvent::Source {
            unit: SourceUnitKey::clone(&unit),
            event: SourceEvent::DiskObserved { disk: observe_disk(&uri), metadata },
        });
        for kind in ForeignSourceKind::ALL {
            let foreign_path = match kind {
                ForeignSourceKind::JavaScript => path.with_extension("js"),
                ForeignSourceKind::Jsx => path.with_extension("jsx"),
            };
            let foreign_uri = Url::from_file_path(&foreign_path)
                .map_err(|_| LspError::PathParseFail(foreign_path))?;
            events.push(LifecycleEvent::Foreign {
                unit: SourceUnitKey::clone(&unit),
                kind,
                event: ForeignEvent::DiskObserved { disk: observe_disk(&foreign_uri) },
            });
        }
    }
    let mut excluded_sources = FxHashSet::default();
    for source in known_sources {
        if !selected(&source) {
            excluded_sources.insert(source);
        }
    }
    // Retain exact alias membership so later failed canonicalization cannot
    // turn a previously selected or excluded representation into an unknown one.
    selected_sources.extend(selected_aliases);
    Ok(PreparedSourceReconfiguration {
        configuration,
        source_roots: discovered.source_roots,
        selected_sources,
        excluded_sources,
        events,
    })
}

fn discover_manual(root: &Path, output: &str) -> Result<DiscoveredWorkspace, LspError> {
    let walk::Walk { files, .. } = walk::walk(root, output.lines())?;
    let metadata = files.iter().map(|file| {
        (PathBuf::clone(file), SourceMetadata::Unmanaged { editable: file.starts_with(root) })
    });
    let metadata = metadata.collect();

    let package = PackageInput {
        name: SmolStr::new("unmanaged"),
        source_identities: files.clone(),
        dependencies: vec![],
    };
    Ok(DiscoveredWorkspace {
        source_globs: files,
        packages: vec![package],
        metadata,
        source_roots: vec![SourceRoot {
            path: root.to_path_buf(),
            metadata: SourceMetadata::Unmanaged { editable: true },
        }],
    })
}

fn discover_spago(root: &Path) -> Result<DiscoveredWorkspace, LspError> {
    let packages = spago::source_files_by_package(root)?;
    let package_inputs = packages.iter().map(|(name, package)| PackageInput {
        name: SmolStr::clone(name),
        source_identities: package.sources.clone(),
        dependencies: package.dependencies.iter().cloned().collect_vec(),
    });
    let package_inputs = package_inputs.collect_vec();

    let metadata = packages.values().flat_map(|package| {
        let editable = matches!(
            package.reference,
            spago::PackageReference::Workspace | spago::PackageReference::Local
        );
        package
            .sources
            .iter()
            .map(move |file| (PathBuf::clone(file), SourceMetadata::Package { editable }))
    });
    let metadata = metadata.collect::<BTreeMap<_, _>>();

    let groups = packages
        .values()
        .map(|package| package_source_roots(root, package))
        .process_results(|roots| roots.collect_vec())?;
    let mut source_roots = groups.into_iter().flatten().collect_vec();
    source_roots
        .sort_by_key(|source_root| std::cmp::Reverse(source_root.path.components().count()));
    Ok(DiscoveredWorkspace {
        source_globs: metadata.keys().cloned().collect(),
        packages: package_inputs,
        metadata,
        source_roots,
    })
}

pub(super) fn package_source_roots(
    workspace_root: &Path,
    package: &spago::PackageSources,
) -> std::io::Result<Vec<SourceRoot>> {
    let editable = matches!(
        package.reference,
        spago::PackageReference::Workspace | spago::PackageReference::Local
    );
    let metadata = SourceMetadata::Package { editable };
    let mut roots = vec![];
    for root in &package.roots {
        let root = workspace_root.join(root).absolutize()?.to_path_buf();
        roots.push(SourceRoot {
            path: PathBuf::clone(&root),
            metadata: SourceMetadata::clone(&metadata),
        });
        if let Ok(canonical) = dunce::canonicalize(&root)
            && canonical != root
        {
            roots.push(SourceRoot { path: canonical, metadata: SourceMetadata::clone(&metadata) });
        }
    }
    Ok(roots)
}
