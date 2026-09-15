use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{fs, io};

use building::{
    DiskObservation, ForeignEvent, LifecycleEvent, QueryCancellation, SourceEvent, SourceUnitKey,
};
use configuration::{Configuration, SourceDiscovery};
use files::ForeignSourceKind;
use iris_build::compilation::{CompilationState, MaterializedPrim};
use iris_build::compile::{InitialBuildConfig, PackageExecution, build_initial};
use iris_build::events::SilentBuildEvents;
use iris_build::plan::PackageInput;
use itertools::Itertools;
use lsp_types::Url;
use path_absolutize::Absolutize;
use rustc_hash::{FxHashMap, FxHashSet};
use smol_str::SmolStr;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use super::error::LspError;
use super::workspace::{
    PreparedInitialWorkspace, PreparedSourceReconfiguration, SourceRoot, filesystem_identity,
};
use super::{SourceMetadata, observe_disk, source_unit_from_source_uri, source_uri};
use crate::walk;

#[derive(Clone, Copy)]
pub(super) enum ConfigurationOrigin {
    Startup,
    Client,
}

pub(super) struct Preparation {
    pub(super) generation: u64,
    pub(super) configuration: Arc<Configuration>,
    pub(super) origin: ConfigurationOrigin,
    pub(super) dirty: bool,
    pub(super) fallback: bool,
    pub(super) queries: QueryCancellation,
    pub(super) process: CancellationToken,
}

impl Preparation {
    pub(super) fn cancel(&self) {
        self.queries.cancel();
        self.process.cancel();
    }
}

impl Drop for Preparation {
    fn drop(&mut self) {
        self.cancel();
    }
}

pub(super) struct ReconfigurationInput {
    pub(super) selected_sources: FxHashSet<Arc<str>>,
    pub(super) excluded_sources: FxHashSet<Arc<str>>,
    pub(super) source_identities: FxHashMap<Arc<str>, PathBuf>,
    pub(super) tracked: Vec<SourceUnitKey>,
}

pub(super) enum PreparedWorkspace {
    Initial(PreparedInitialWorkspace),
    Reconfigured(PreparedSourceReconfiguration),
}

pub(super) struct PreparationFinished {
    pub(super) generation: u64,
    pub(super) result: Result<PreparedWorkspace, LspError>,
}

pub(super) async fn prepare(
    root: PathBuf,
    configuration: Arc<Configuration>,
    previous: Option<ReconfigurationInput>,
    queries: QueryCancellation,
    cancellation: CancellationToken,
    permit: Arc<Semaphore>,
) -> Result<PreparedWorkspace, LspError> {
    let permit = tokio::select! {
        _ = cancellation.cancelled() => return Err(building::QueryError::Cancelled.into()),
        permit = permit.acquire_owned() => permit.expect("preparation permit is never closed"),
    };
    queries.check()?;
    let output = match &configuration.sources {
        SourceDiscovery::Spago {} => None,
        SourceDiscovery::Command { program, arguments } => {
            let output = super::process::output(&root, program, arguments, &cancellation).await;
            queries.check()?;
            let output = output?;
            if !output.status.success() {
                return Err(LspError::SourceCommandFailed {
                    status: output.status,
                    stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                });
            }
            Some(output.stdout)
        }
    };

    // The permit belongs to the closure, not its awaiter. Supersession cannot admit
    // another expensive preparation while this blocking operation is still retiring.
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        queries.check()?;
        let discovered = if let Some(output) = output {
            discover_manual(&root, str::from_utf8(&output)?, &queries)?
        } else {
            discover_spago(&root)?
        };
        queries.check()?;
        let prepared = if let Some(previous) = previous {
            PreparedWorkspace::Reconfigured(prepare_reconfiguration(
                configuration,
                discovered,
                previous,
                &queries,
            )?)
        } else {
            let selected_sources = discovered.source_globs.iter().map(source_uri);
            let selected_sources = selected_sources.collect::<Result<FxHashSet<_>, _>>()?;
            let source_identities = selected_sources.iter().filter_map(|source| {
                filesystem_identity(source).map(|identity| (Arc::clone(source), identity))
            });
            let source_identities = source_identities.collect();
            let initial = build_initial::<i32, SourceMetadata, _>(InitialBuildConfig {
                root: &root,
                source_globs: &discovered.source_globs,
                excluded: &[],
                packages: discovered.packages,
                prim_metadata: SourceMetadata::Builtin,
                source_metadata: |path: &Path| {
                    discovered
                        .metadata
                        .get(path)
                        .cloned()
                        .expect("discovered source has no metadata")
                },
                execution: PackageExecution::Parallel,
                cancellation: Some(QueryCancellation::clone(&queries)),
                events: &SilentBuildEvents,
            })?;
            PreparedWorkspace::Initial(PreparedInitialWorkspace {
                configuration,
                compilation: initial.into_compilation(),
                source_roots: discovered.source_roots,
                selected_sources,
                source_identities,
            })
        };
        queries.check()?;
        Ok(prepared)
    })
    .await?
}

pub(super) fn fallback(configuration: Arc<Configuration>) -> Result<PreparedWorkspace, LspError> {
    let prim = MaterializedPrim::new()?;
    let compilation = CompilationState::new(prim, SourceMetadata::Builtin);
    Ok(PreparedWorkspace::Initial(PreparedInitialWorkspace {
        configuration,
        compilation,
        source_roots: vec![],
        selected_sources: FxHashSet::default(),
        source_identities: FxHashMap::default(),
    }))
}

pub(super) struct DiscoveredWorkspace {
    pub(super) source_globs: Vec<PathBuf>,
    pub(super) packages: Vec<PackageInput>,
    pub(super) metadata: BTreeMap<PathBuf, SourceMetadata>,
    pub(super) source_roots: Vec<SourceRoot>,
}

fn discover_manual(
    root: &Path,
    output: &str,
    cancellation: &QueryCancellation,
) -> Result<DiscoveredWorkspace, LspError> {
    let walk::Walk { files } = walk::walk_cancellable(root, output.lines(), cancellation)?;
    let mut source_roots = vec![SourceRoot {
        path: root.to_path_buf(),
        metadata: SourceMetadata::Unmanaged { editable: true },
    }];
    if let Ok(canonical) = dunce::canonicalize(root)
        && canonical != root
    {
        source_roots.push(SourceRoot {
            path: canonical,
            metadata: SourceMetadata::Unmanaged { editable: true },
        });
    }

    let metadata = files.iter().map(|file| {
        let editable = source_roots.iter().any(|root| file.starts_with(&root.path));
        (PathBuf::clone(file), SourceMetadata::Unmanaged { editable })
    });
    let metadata = metadata.collect();
    let package = PackageInput {
        name: SmolStr::new("unmanaged"),
        source_identities: Vec::clone(&files),
        dependencies: vec![],
    };
    Ok(DiscoveredWorkspace { source_globs: files, packages: vec![package], metadata, source_roots })
}

fn discover_spago(root: &Path) -> Result<DiscoveredWorkspace, LspError> {
    let packages = spago::source_files_by_package(root).map_err(LspError::SpagoLock)?;
    let package_inputs = packages.iter().map(|(name, package)| PackageInput {
        name: SmolStr::clone(name),
        source_identities: Vec::clone(&package.sources),
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
    let source_roots = packages.values().map(|package| package_source_roots(root, package));
    let source_root_groups = source_roots.process_results(|roots| roots.collect_vec())?;
    let mut source_roots = source_root_groups.into_iter().flatten().collect_vec();
    source_roots.sort_by_key(|root| std::cmp::Reverse(root.path.components().count()));
    let source_globs = metadata.keys().cloned().collect_vec();
    Ok(DiscoveredWorkspace { source_globs, packages: package_inputs, metadata, source_roots })
}

pub(super) fn package_source_roots(
    workspace_root: &Path,
    package: &spago::PackageSources,
) -> io::Result<Vec<SourceRoot>> {
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

pub(super) fn prepare_reconfiguration(
    configuration: Arc<Configuration>,
    discovered: DiscoveredWorkspace,
    previous: ReconfigurationInput,
    cancellation: &QueryCancellation,
) -> Result<PreparedSourceReconfiguration, LspError> {
    let selected_sources = discovered.source_globs.iter().map(source_uri);
    let selected_sources = selected_sources.collect::<Result<FxHashSet<_>, _>>()?;
    let mut source_identities = previous.source_identities;
    let tracked = previous.tracked.iter().map(|unit| Arc::<str>::from(unit.source()));
    let tracked = tracked.collect::<FxHashSet<_>>();
    for source in selected_sources.iter().chain(&previous.selected_sources).chain(&tracked) {
        cancellation.check()?;
        if let Some(identity) = filesystem_identity(source) {
            source_identities.insert(Arc::clone(source), identity);
        }
    }
    let selected_identities =
        selected_sources.iter().filter_map(|source| source_identities.get(source)).cloned();
    let selected_identities = selected_identities.collect::<FxHashSet<_>>();
    let removed_sources = previous
        .selected_sources
        .difference(&selected_sources)
        .filter(|source| {
            source_identities
                .get(*source)
                .is_none_or(|identity| !selected_identities.contains(identity))
        })
        .cloned();
    let mut removed_sources = removed_sources.collect::<FxHashSet<_>>();
    let removed_identities =
        removed_sources.iter().filter_map(|source| source_identities.get(source)).cloned();
    let removed_identities = removed_identities.collect::<FxHashSet<_>>();
    for source in &tracked {
        if source_identities
            .get(source)
            .is_some_and(|identity| removed_identities.contains(identity))
        {
            removed_sources.insert(Arc::clone(source));
        }
    }
    let mut excluded_sources = previous.excluded_sources;
    excluded_sources.extend(removed_sources.iter().cloned());
    excluded_sources.retain(|source| {
        !selected_sources.contains(source)
            && source_identities
                .get(source)
                .is_none_or(|identity| !selected_identities.contains(identity))
    });
    source_identities.retain(|source, _| {
        selected_sources.contains(source)
            || excluded_sources.contains(source)
            || tracked.contains(source)
    });
    let mut events = vec![];
    for source in &removed_sources {
        cancellation.check()?;
        let unit = source_unit_from_source_uri(&Url::parse(source)?)?;
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
    for path in &discovered.source_globs {
        cancellation.check()?;
        let content = Arc::from(fs::read_to_string(path)?);
        let source = source_uri(path)?;
        excluded_sources.remove(&source);
        let unit = source_unit_from_source_uri(&Url::parse(&source)?)?;
        let metadata =
            discovered.metadata.get(path).cloned().expect("discovered source has no metadata");
        events.push(LifecycleEvent::Source {
            unit: SourceUnitKey::clone(&unit),
            event: SourceEvent::DiskObserved { disk: DiskObservation::Found(content), metadata },
        });
        for kind in ForeignSourceKind::ALL {
            let uri = Url::parse(unit.foreign_for(kind))?;
            events.push(LifecycleEvent::Foreign {
                unit: SourceUnitKey::clone(&unit),
                kind,
                event: ForeignEvent::DiskObserved { disk: observe_disk(&uri) },
            });
        }
    }
    cancellation.check()?;
    Ok(PreparedSourceReconfiguration {
        configuration,
        source_roots: discovered.source_roots,
        selected_sources,
        excluded_sources,
        source_identities,
        events,
    })
}
