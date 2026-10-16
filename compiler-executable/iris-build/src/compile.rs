use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use std::{fs, io, iter};

use building::{
    DiskObservation, ForeignEvent, LifecycleEvent, QueryError, SourceEvent, SourceUnitKey,
};
use files::{FileId, ForeignSourceKind};
use iris_diagnostics::Severity;
use itertools::Itertools;
use rayon::prelude::*;
use smol_str::SmolStr;
use thiserror::Error;
use url::Url;

use super::compilation::{CompilationState, MaterializedPrim};
use super::events::{BuildEvent, BuildEventSink, BuildOutcome};
use super::output::{BuildOutput, OutputError};
use super::ownership::SourcePackage;
use super::packages::DiscoveredPackage;
use super::plan::{BuildPlan, BuildPlanError, PackageInput, SelectedSource};
use super::{executor, walk};

pub(crate) struct BuildConfig<'a> {
    pub root: PathBuf,
    pub output: BuildOutput,
    pub source_globs: Vec<PathBuf>,
    pub packages: Vec<DiscoveredPackage>,
    pub color: bool,
    pub diagnostics: bool,
    pub events: &'a dyn BuildEventSink,
}

#[derive(Debug, Error)]
pub enum CompileError {
    #[error("compilation failed")]
    Diagnostics { reported: bool },
    #[error("failed to convert path to a file URL: {0}")]
    InvalidPath(PathBuf),
    #[error("no input files found")]
    NoInputs,
    #[error(transparent)]
    BuildPlan(#[from] BuildPlanError),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Output(#[from] OutputError),
    #[error(transparent)]
    Query(#[from] QueryError),
    #[error(transparent)]
    Walk(#[from] walk::Error),
}

pub(crate) type ProjectCompilation = CompilationState<(), Option<SourcePackage>>;

pub(crate) enum Compiled {
    NoInputs,
    Diagnostics,
    Succeeded(Vec<Arc<javascript::Module>>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackageExecution {
    Serial,
    Parallel,
}

pub struct InitialBuildConfig<'a, Metadata, SourceMetadata> {
    pub root: &'a Path,
    pub source_globs: &'a [PathBuf],
    pub excluded: &'a [PathBuf],
    pub packages: Vec<PackageInput>,
    pub prim_metadata: Metadata,
    pub source_metadata: SourceMetadata,
    pub execution: PackageExecution,
    pub events: &'a dyn BuildEventSink,
}

pub struct InitialBuild<Version, Metadata> {
    compilation: CompilationState<Version, Metadata>,
    source_paths: BTreeSet<PathBuf>,
    report: InitialBuildReport,
}

pub(crate) struct InitialBuildReport {
    sources: Vec<FileId>,
    diagnostics: Vec<iris_diagnostics::DiagnosticCollection>,
    has_errors: bool,
    no_inputs: bool,
    failure: Option<CompileError>,
    duration: std::time::Duration,
}

pub(crate) struct InitialBuildParts<Version, Metadata> {
    pub compilation: CompilationState<Version, Metadata>,
    pub source_paths: BTreeSet<PathBuf>,
    pub report: InitialBuildReport,
}

impl<Version, Metadata> InitialBuild<Version, Metadata> {
    pub fn compilation(&self) -> &CompilationState<Version, Metadata> {
        &self.compilation
    }

    pub fn sources(&self) -> &[FileId] {
        &self.report.sources
    }

    pub fn has_errors(&self) -> bool {
        self.report.has_errors
    }

    pub fn duration(&self) -> std::time::Duration {
        self.report.duration
    }

    pub fn into_compilation(self) -> CompilationState<Version, Metadata> {
        self.compilation
    }

    pub(crate) fn into_parts(self) -> InitialBuildParts<Version, Metadata> {
        InitialBuildParts {
            compilation: self.compilation,
            source_paths: self.source_paths,
            report: self.report,
        }
    }
}

pub fn build_initial<Version, Metadata, SourceMetadata>(
    config: InitialBuildConfig<'_, Metadata, SourceMetadata>,
) -> Result<InitialBuild<Version, Metadata>, CompileError>
where
    Version: Clone + Ord,
    Metadata: Clone,
    SourceMetadata: for<'path> Fn(&'path Path) -> Metadata,
{
    let InitialBuildConfig {
        root,
        source_globs,
        excluded,
        packages,
        prim_metadata,
        source_metadata,
        execution,
        events,
    } = config;
    let started = Instant::now();
    let selected_paths = walk::walk_filtered(root, source_globs, excluded)?.files;
    let selected_sources = selected_paths.into_iter().map(|path| {
        let identity = dunce::canonicalize(&path)?;
        Ok::<_, io::Error>(SelectedSource { path, identity })
    });
    let selected_sources = selected_sources.process_results(|sources| sources.collect_vec())?;
    let package_inputs = packages.into_iter().map(|package| {
        let source_identities = package.source_identities.into_iter().map(dunce::canonicalize);
        let source_identities =
            source_identities.process_results(|sources| sources.collect_vec())?;
        Ok::<_, io::Error>(PackageInput { source_identities, ..package })
    });
    let package_inputs = package_inputs.process_results(|packages| packages.collect_vec())?;
    let plan = BuildPlan::new(selected_sources, package_inputs)?;
    events.send(BuildEvent::PlanReady { package_count: plan.package_count() });

    let prim = MaterializedPrim::new()?;
    let mut compilation = CompilationState::new(prim, prim_metadata);
    let planned_source_paths =
        plan.packages().flat_map(|package| package.source_paths.iter()).cloned();
    let source_paths = planned_source_paths.collect::<BTreeSet<_>>();
    let source_units = source_paths.par_iter().map(|path| read_source_unit(path));
    let source_units = source_units.collect::<Result<Vec<_>, _>>()?;
    let mut observations = Vec::new();
    for (path, source_unit) in source_paths.iter().zip(&source_units) {
        let metadata = source_metadata(path);
        observations.extend(source_unit.observations(metadata));
    }
    compilation.observe_all(observations);
    let sources = source_paths.iter().zip(&source_units).map(|(path, source_unit)| {
        let file_id = compilation
            .source_id(source_unit.unit.source())
            .expect("invariant violated: loaded source has no lifecycle identity");
        (PathBuf::clone(path), file_id)
    });
    let sources = sources.collect::<HashMap<_, _>>();

    let engine = compilation.query_engine();
    let execute = |package: &super::plan::PlannedPackage| {
        let package_sources = package.source_paths.iter().map(|path| sources[path]);
        let package_sources = package_sources.collect_vec();
        query_package(engine, &package_sources)
    };
    let execution = match execution {
        PackageExecution::Serial => executor::execute_serial(&plan, events, &execute),
        PackageExecution::Parallel => executor::execute_parallel(&plan, events, &execute),
    };

    let duration = started.elapsed();
    events.send(BuildEvent::Finalizing { duration });
    let sources = sources.into_values().collect_vec();
    let no_inputs = sources.is_empty();
    let (diagnostics, has_errors, failure) = match execution {
        Ok(()) => match collect_diagnostics(&compilation, &sources) {
            Ok((diagnostics, has_errors)) => (diagnostics, has_errors, None),
            Err(error) => (Vec::new(), true, Some(error)),
        },
        Err(error) => (Vec::new(), true, Some(error)),
    };
    let report =
        InitialBuildReport { sources, diagnostics, has_errors, no_inputs, failure, duration };
    Ok(InitialBuild { compilation, source_paths, report })
}

pub(crate) fn build_initial_packages(
    root: &Path,
    source_globs: &[PathBuf],
    output: &Path,
    packages: &[DiscoveredPackage],
    events: &dyn BuildEventSink,
) -> Result<InitialBuild<(), Option<SourcePackage>>, CompileError> {
    let owners = packages
        .iter()
        .flat_map(|package| package.files.iter().map(move |file| (file.as_path(), package)))
        .collect::<HashMap<_, _>>();
    let inputs = packages.iter().map(|package| PackageInput {
        name: SmolStr::clone(&package.name),
        source_identities: Vec::clone(&package.files),
        dependencies: Vec::clone(&package.dependencies),
    });
    let excluded = [output.to_path_buf()];
    build_initial(InitialBuildConfig {
        root,
        source_globs,
        excluded: &excluded,
        packages: inputs.collect(),
        prim_metadata: None,
        source_metadata: |path: &Path| {
            let package = owners.get(path).map(|package| SourcePackage::new(package));
            Some(package.expect("invariant violated: a planned source was not discovered"))
        },
        execution: PackageExecution::Parallel,
        events,
    })
}

pub(crate) fn build(config: BuildConfig<'_>) -> Result<BTreeSet<String>, CompileError> {
    let started = Instant::now();
    let BuildConfig { root, output, source_globs, packages, color, diagnostics, events } = config;
    let initial =
        build_initial_packages(&root, &source_globs, output.directory(), &packages, events)?;
    let InitialBuildParts { compilation, source_paths: _, mut report } = initial.into_parts();
    if report.no_inputs {
        return Err(CompileError::NoInputs);
    }
    if let Some(error) = report.failure.take() {
        return Err(error);
    }
    let has_errors = report.has_errors;
    let compiled = if has_errors {
        None
    } else {
        let modules = collect_modules(&compilation, &report.sources)?;
        output.write(&compilation, &modules)?;
        Some(modules.iter().map(|module| module.name().to_owned()).collect())
    };
    let outcome = if has_errors { BuildOutcome::Diagnostics } else { BuildOutcome::Succeeded };
    events.send(BuildEvent::Finished { duration: started.elapsed(), outcome });
    if diagnostics {
        report_diagnostics(&compilation, std::mem::take(&mut report.diagnostics), &root, color);
    }
    compiled.ok_or(CompileError::Diagnostics { reported: diagnostics })
}

pub(crate) fn rebuild(
    compilation: &ProjectCompilation,
    root: &Path,
    color: bool,
    diagnostics: bool,
) -> Result<Compiled, CompileError> {
    let sources = compilation.source_ids().collect_vec();
    let engine = compilation.query_engine();
    query_package(engine, &sources)?;
    let (diagnostic_collections, has_errors) = collect_diagnostics(compilation, &sources)?;
    if diagnostics {
        report_diagnostics(compilation, diagnostic_collections, root, color);
    }
    if has_errors {
        return Ok(Compiled::Diagnostics);
    }
    Ok(Compiled::Succeeded(collect_modules(compilation, &sources)?))
}

pub(crate) fn finish_initial(
    compilation: &ProjectCompilation,
    report: &mut InitialBuildReport,
    root: &Path,
    color: bool,
    diagnostics: bool,
) -> Result<Compiled, CompileError> {
    if report.no_inputs {
        return Ok(Compiled::NoInputs);
    }
    if let Some(error) = report.failure.take() {
        return Err(error);
    }
    if diagnostics {
        let diagnostics = std::mem::take(&mut report.diagnostics);
        report_diagnostics(compilation, diagnostics, root, color);
    }
    if report.has_errors {
        return Ok(Compiled::Diagnostics);
    }
    Ok(Compiled::Succeeded(collect_modules(compilation, &report.sources)?))
}

struct SourceUnitRead {
    unit: SourceUnitKey,
    source: Arc<str>,
    foreign: Vec<(ForeignSourceKind, DiskObservation)>,
}

impl SourceUnitRead {
    fn observations<Version, Metadata>(
        &self,
        metadata: Metadata,
    ) -> Vec<LifecycleEvent<Version, Metadata>> {
        let disk = DiskObservation::Found(Arc::clone(&self.source));
        let source = LifecycleEvent::Source {
            unit: SourceUnitKey::clone(&self.unit),
            event: SourceEvent::DiskObserved { disk, metadata },
        };
        let foreign = self.foreign.iter().map(|(kind, disk)| LifecycleEvent::Foreign {
            unit: SourceUnitKey::clone(&self.unit),
            kind: *kind,
            event: ForeignEvent::DiskObserved { disk: DiskObservation::clone(disk) },
        });
        iter::once(source).chain(foreign).collect()
    }
}

fn read_source_unit(path: &Path) -> Result<SourceUnitRead, CompileError> {
    let source_url =
        Url::from_file_path(path).map_err(|_| CompileError::InvalidPath(path.to_path_buf()))?;
    let foreign_path = path.with_extension("js");
    let foreign_url = Url::from_file_path(&foreign_path)
        .map_err(|_| CompileError::InvalidPath(PathBuf::clone(&foreign_path)))?;
    let unit = SourceUnitKey::new(source_url.as_str(), foreign_url.as_str());
    let source = fs::read_to_string(path)?.into();
    let foreign = ForeignSourceKind::ALL.into_iter().map(|kind| {
        let foreign_path = path.with_extension(kind.extension());
        let disk = match fs::read_to_string(&foreign_path) {
            Ok(content) => DiskObservation::Found(content.into()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => DiskObservation::NotFound,
            Err(error) => return Err(error),
        };
        Ok((kind, disk))
    });
    let foreign = foreign.collect::<Result<Vec<_>, _>>()?;
    Ok(SourceUnitRead { unit, source, foreign })
}

fn query_package(engine: &building::QueryEngine, sources: &[FileId]) -> Result<(), CompileError> {
    sources.par_iter().try_for_each(|&file_id| {
        let engine = engine.snapshot();
        engine.stabilized(file_id)?;
        engine.indexed(file_id)?;
        engine.resolved(file_id)?;
        engine.lowered(file_id)?;
        engine.checked(file_id)?;
        engine.foreign_validation(file_id)?;
        let _ = engine.javascript(file_id)?;
        Ok::<_, CompileError>(())
    })
}

fn collect_diagnostics<Version, Metadata>(
    compilation: &CompilationState<Version, Metadata>,
    sources: &[FileId],
) -> Result<(Vec<iris_diagnostics::DiagnosticCollection>, bool), CompileError>
where
    Version: Clone + Ord,
    Metadata: Clone,
{
    let engine = compilation.snapshot();
    let diagnostics = iris_diagnostics::collect_diagnostics(&engine, sources)?;
    let has_errors = diagnostics
        .iter()
        .flat_map(iris_diagnostics::DiagnosticCollection::diagnostics)
        .any(|diagnostic| diagnostic.severity == Severity::Error);
    Ok((diagnostics, has_errors))
}

fn report_diagnostics(
    compilation: &ProjectCompilation,
    diagnostics: Vec<iris_diagnostics::DiagnosticCollection>,
    root: &Path,
    color: bool,
) {
    for collected in diagnostics {
        if collected.diagnostics().is_empty() {
            continue;
        }
        let source_path = compilation
            .source_path(collected.file_id)
            .expect("invariant violated: input source has no lifecycle path");
        let display_path = display_source_path(&source_path, root);
        let line_index = line_index::LineIndex::new(&collected.content);
        eprint!(
            "{}",
            iris_diagnostics::format_rich_with_path(
                collected.diagnostics(),
                &collected.content,
                &line_index,
                &display_path,
                color,
            )
        );
    }
}

fn display_source_path(source_path: &str, root: &Path) -> String {
    if let Some(file_path) = Url::parse(source_path).ok().and_then(|url| url.to_file_path().ok()) {
        file_path.strip_prefix(root).unwrap_or(&file_path).to_string_lossy().replace('\\', "/")
    } else {
        source_path.to_owned()
    }
}

fn collect_modules(
    compilation: &ProjectCompilation,
    sources: &[FileId],
) -> Result<Vec<Arc<javascript::Module>>, CompileError> {
    let mut pending = sources.to_vec();
    let mut visited = HashSet::new();
    let mut modules = Vec::new();
    while !pending.is_empty() {
        let frontier = pending.drain(..).filter(|file_id| visited.insert(*file_id));
        let frontier = frontier.collect_vec();
        let generated = frontier.par_iter().map(|&file_id| {
            let module = compilation.snapshot().javascript(file_id)?.ok();
            Ok::<_, CompileError>(module)
        });
        let generated = generated.collect::<Result<Vec<_>, _>>()?;
        for module in generated.into_iter().flatten() {
            pending.extend(module.dependencies().iter().copied());
            modules.push(module);
        }
    }
    Ok(modules)
}
