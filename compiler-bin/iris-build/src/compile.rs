use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use std::{fs, io};

use building::{DiskObservation, QueryError, SourceUnitKey};
use diagnostics::Severity;
use files::{FileId, ForeignSourceKind};
use itertools::Itertools;
use rayon::prelude::*;
use thiserror::Error;
use url::Url;

use super::compilation::{CompilationState, MaterializedPrim};
use super::events::{BuildEvent, BuildEventSink, BuildOutcome};
use super::plan::{BuildPlan, BuildPlanError, PackageInput, SelectedSource};
use super::{executor, walk};

pub(crate) struct BuildConfig<'a> {
    pub root: PathBuf,
    pub output: PathBuf,
    pub source_globs: Vec<PathBuf>,
    pub packages: Vec<PackageInput>,
    pub color: bool,
    pub diagnostics: bool,
    pub resilient: bool,
    pub events: &'a dyn BuildEventSink,
}

#[derive(Debug, Error)]
pub(crate) enum CompileError {
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
    Query(#[from] QueryError),
    #[error(transparent)]
    Walk(#[from] walk::Error),
}

pub(crate) struct RebuildResult {
    pub outcome: BuildOutcome,
    pub outputs: BTreeSet<PathBuf>,
}

struct ModuleWrite {
    outputs: Vec<PathBuf>,
    result: Result<(), CompileError>,
}

pub(crate) fn build(config: BuildConfig<'_>) -> Result<(), CompileError> {
    let BuildConfig { root, output, source_globs, packages, color, diagnostics, resilient, events } =
        config;
    let started = Instant::now();
    let selected_paths = walk::walk(&root, &source_globs)?.files;
    let selected_sources = selected_paths.into_iter().map(|path| {
        let identity = dunce::canonicalize(&path)?;
        Ok(SelectedSource { path, identity })
    });
    let selected_sources = selected_sources.collect::<Result<Vec<_>, io::Error>>()?;
    let package_inputs = packages.into_iter().map(|package| {
        let source_identities = package.source_identities.into_iter().map(dunce::canonicalize);
        let source_identities = source_identities.collect::<Result<Vec<_>, io::Error>>()?;
        Ok(PackageInput { source_identities, ..package })
    });
    let package_inputs = package_inputs.collect::<Result<Vec<_>, io::Error>>()?;
    let plan = BuildPlan::new(selected_sources, package_inputs)?;
    if plan.is_empty() {
        return Err(CompileError::NoInputs);
    }
    events.send(BuildEvent::PlanReady { package_count: plan.package_count() });

    let prim = MaterializedPrim::new()?;
    let mut compilation = CompilationState::new(prim, ());
    let source_paths = plan.packages().flat_map(|package| package.source_paths.iter()).cloned();
    let source_paths = source_paths.collect::<BTreeSet<_>>();
    let mut sources = HashMap::new();
    for path in source_paths {
        sources.insert(PathBuf::clone(&path), load_source(&mut compilation, &path)?);
    }

    executor::execute_parallel(&plan, events, &|package| {
        let package_sources = package.source_paths.iter().map(|path| sources[path]);
        let package_sources = package_sources.collect_vec();
        query_package(&compilation, &package_sources)
    })?;

    let duration = started.elapsed();
    events.send(BuildEvent::Finalizing { duration });
    let all_sources = sources.into_values().collect_vec();
    let (diagnostic_collections, has_errors) = collect_diagnostics(&compilation, &all_sources)?;
    if !has_errors || resilient {
        let modules = collect_modules(&compilation, &all_sources)?;
        write_modules(&compilation, &modules, &output, &mut BTreeSet::new())?;
    }
    let outcome = if has_errors { BuildOutcome::Diagnostics } else { BuildOutcome::Succeeded };
    events.send(BuildEvent::Finished { duration: started.elapsed(), outcome });
    if diagnostics {
        report_diagnostics(&compilation, diagnostic_collections, &root, color);
    }
    if has_errors {
        return Err(CompileError::Diagnostics { reported: diagnostics });
    }
    Ok(())
}

pub(crate) fn rebuild(
    compilation: &CompilationState,
    root: &Path,
    output: &Path,
    color: bool,
    diagnostics: bool,
    owned_outputs: &mut BTreeSet<PathBuf>,
) -> Result<RebuildResult, CompileError> {
    let sources = compilation.source_ids().collect_vec();
    query_package(compilation, &sources)?;
    let (diagnostic_collections, has_errors) = collect_diagnostics(compilation, &sources)?;
    if diagnostics {
        report_diagnostics(compilation, diagnostic_collections, root, color);
    }
    let outputs = if has_errors {
        BTreeSet::new()
    } else {
        let modules = collect_modules(compilation, &sources)?;
        write_modules(compilation, &modules, output, owned_outputs)?
    };
    let outcome = if has_errors { BuildOutcome::Diagnostics } else { BuildOutcome::Succeeded };
    Ok(RebuildResult { outcome, outputs })
}

fn load_source(compilation: &mut CompilationState, path: &Path) -> Result<FileId, CompileError> {
    let source_url =
        Url::from_file_path(path).map_err(|_| CompileError::InvalidPath(path.to_path_buf()))?;
    let foreign_path = path.with_extension("js");
    let foreign_url = Url::from_file_path(&foreign_path)
        .map_err(|_| CompileError::InvalidPath(PathBuf::clone(&foreign_path)))?;
    let unit = SourceUnitKey::new(source_url.as_str(), foreign_url.as_str());
    let content = fs::read_to_string(path)?;
    let change = compilation.observe_source(
        SourceUnitKey::clone(&unit),
        DiskObservation::Found(content.into()),
        (),
    );
    let file_id = change
        .changed_sources()
        .next()
        .expect("invariant violated: newly loaded source did not change its lifecycle");
    for kind in ForeignSourceKind::ALL {
        let foreign_path = path.with_extension(kind.extension());
        let disk = match fs::read_to_string(&foreign_path) {
            Ok(content) => DiskObservation::Found(content.into()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => DiskObservation::NotFound,
            Err(error) => return Err(error.into()),
        };
        compilation.observe_foreign(SourceUnitKey::clone(&unit), kind, disk);
    }
    Ok(file_id)
}

fn query_package(compilation: &CompilationState, sources: &[FileId]) -> Result<(), CompileError> {
    sources.par_iter().try_for_each(|&file_id| {
        let engine = compilation.snapshot();
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

fn collect_diagnostics(
    compilation: &CompilationState,
    sources: &[FileId],
) -> Result<(Vec<diagnostics::DiagnosticCollection>, bool), CompileError> {
    let engine = compilation.snapshot();
    let diagnostics = diagnostics::collect_diagnostics(&engine, sources)?;
    let has_errors = diagnostics
        .iter()
        .flat_map(diagnostics::DiagnosticCollection::diagnostics)
        .any(|diagnostic| diagnostic.severity == Severity::Error);
    Ok((diagnostics, has_errors))
}

fn report_diagnostics(
    compilation: &CompilationState,
    diagnostics: Vec<diagnostics::DiagnosticCollection>,
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
            diagnostics::format_rich_with_path(
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
    compilation: &CompilationState,
    sources: &[FileId],
) -> Result<Vec<Arc<javascript::Module>>, CompileError> {
    let mut pending = sources.to_vec();
    let mut visited = HashSet::new();
    let mut modules = vec![];
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

fn write_modules(
    compilation: &CompilationState,
    modules: &[Arc<javascript::Module>],
    output: &Path,
    owned_outputs: &mut BTreeSet<PathBuf>,
) -> Result<BTreeSet<PathBuf>, CompileError> {
    let mut outputs = BTreeSet::new();
    if modules.iter().any(|module| module.requires_runtime()) {
        fs::create_dir_all(output)?;
        let runtime = output.join(javascript::runtime_filename());
        write_if_changed(&runtime, javascript::runtime_source().as_bytes())?;
        owned_outputs.insert(PathBuf::clone(&runtime));
        outputs.insert(runtime);
    }
    let module_writes = modules.par_iter().map(|module| write_module(compilation, module, output));
    let module_writes = module_writes.collect::<Vec<_>>();
    let mut failure = None;
    for write in module_writes {
        owned_outputs.extend(write.outputs.iter().cloned());
        outputs.extend(write.outputs);
        if let Err(error) = write.result
            && failure.is_none()
        {
            failure = Some(error);
        }
    }
    if let Some(error) = failure {
        return Err(error);
    }
    Ok(outputs)
}

fn write_module(
    compilation: &CompilationState,
    module: &javascript::Module,
    output: &Path,
) -> ModuleWrite {
    let output_path = output.join(module.filename());
    let result = fs::create_dir_all(
        output_path.parent().expect("invariant violated: module filename has no parent"),
    )
    .and_then(|_| write_if_changed(&output_path, module.source().as_bytes()));
    if let Err(error) = result {
        return ModuleWrite { outputs: vec![], result: Err(error.into()) };
    }
    let mut outputs = vec![output_path];
    if let Some(kind) = module.foreign_kind() {
        let output_path = output.join(javascript::foreign_module_filename(module.name(), kind));
        let foreign = compilation
            .source_foreign_content(module.file_id())
            .expect("invariant violated: generated module requires missing foreign content");
        if let Err(error) = write_if_changed(&output_path, foreign.as_bytes()) {
            return ModuleWrite { outputs, result: Err(error.into()) };
        }
        outputs.push(output_path);
    }
    ModuleWrite { outputs, result: Ok(()) }
}

fn write_if_changed(path: &Path, content: &[u8]) -> io::Result<()> {
    match fs::read(path) {
        Ok(previous) if previous == content => Ok(()),
        Ok(_) => fs::write(path, content),
        Err(error) if error.kind() == io::ErrorKind::NotFound => fs::write(path, content),
        Err(error) => Err(error),
    }
}
