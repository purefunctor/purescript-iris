use std::collections::{BTreeSet, HashMap, HashSet};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use std::{fs, io, process};

use building::{DiskObservation, QueryError, SourceUnitKey};
use diagnostics::Severity;
use files::{FileId, ForeignSourceKind};
use indicatif::MultiProgress;
use itertools::Itertools;
use petgraph::algo::tarjan_scc;
use petgraph::prelude::DiGraphMap;
use rayon::prelude::*;
use thiserror::Error;
use url::Url;

use crate::cli::ColorChoice;
use crate::compilation::CompilationState;
use crate::{package, progress, walk};

pub struct CompileConfig {
    pub output: PathBuf,
    pub inputs: Vec<PathBuf>,
    pub packages: Vec<PathBuf>,
    pub json_errors: bool,
    pub quiet: bool,
    pub color: ColorChoice,
}

pub(crate) struct BuildConfig<'a> {
    pub output: &'a Path,
    pub current_directory: &'a Path,
    pub color: bool,
    pub progress: bool,
    pub resilience: Resilience,
}

pub(crate) struct PackageInput {
    pub name: String,
    pub sources: Vec<PathBuf>,
    pub dependencies: Vec<String>,
}

struct PackageJob {
    name: String,
    source_ids: Vec<FileId>,
    dependencies: Vec<usize>,
}

struct PackageGroup {
    packages: Vec<usize>,
    dependencies: Vec<usize>,
    dependents: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resilience {
    Strict,
    Resilient,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum BuildOutcome {
    Succeeded(BTreeSet<PathBuf>),
    Diagnostics,
}

#[derive(Debug, Error)]
pub(crate) enum CompileError {
    #[error("compilation failed")]
    Diagnostics,
    #[error("failed to convert path to a file URL: {0}")]
    InvalidPath(PathBuf),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Package(#[from] package::PackageError),
    #[error("source {path} belongs to both Spago packages {first_package} and {second_package}")]
    ConflictingPackageSource { path: PathBuf, first_package: String, second_package: String },
    #[error("Spago selected source {0}, but the lockfile does not assign it to a package")]
    UnownedPackageSource(PathBuf),
    #[error(transparent)]
    Query(#[from] QueryError),
    #[error(transparent)]
    Walk(#[from] walk::Error),
}

pub fn start(config: CompileConfig) {
    let json_errors = config.json_errors;
    if let Err(error) = compile(config) {
        if !matches!(error, CompileError::Diagnostics) {
            eprintln!("Compilation exited: {error}");
        }
        tracing::error!(?error, "Compilation exited");
        if json_errors {
            println!(
                r#"{{"warnings":[],"errors":[{{"message":{message:?}}}]}}"#,
                message = error.to_string()
            );
        }
        process::exit(1);
    }

    if json_errors {
        println!(r#"{{"warnings":[],"errors":[]}}"#);
    }
}

fn compile(config: CompileConfig) -> Result<(), CompileError> {
    let started = Instant::now();
    let preparation_progress = progress::bar(1, "Preparing", !config.quiet);
    let current_directory = std::env::current_dir()?;
    let walked = walk::walk(&current_directory, &config.inputs)?;

    let mut source_paths = walked.files.into_iter().collect::<BTreeSet<_>>();
    for package in &config.packages {
        source_paths.extend(package::source_files(&current_directory, package)?);
    }

    let build_config = BuildConfig {
        output: &config.output,
        current_directory: &current_directory,
        color: use_color(config.color),
        progress: !config.quiet,
        resilience: Resilience::Strict,
    };
    compile_source_paths(&build_config, source_paths, started, preparation_progress)
}

pub(crate) fn compile_package_inputs(
    root: &Path,
    output: &Path,
    inputs: &[PathBuf],
    packages: Vec<PackageInput>,
    quiet: bool,
    color: ColorChoice,
    resilience: Resilience,
) -> Result<(), CompileError> {
    let started = Instant::now();
    let color = use_color(color);
    let package_progress = progress::packages(!quiet, color);
    let prepared = (|| {
        let walked = walk::walk(root, inputs)?;
        let source_paths = walked.files.into_iter().collect::<BTreeSet<_>>();
        let packages = select_package_sources(&source_paths, packages)?;
        Ok::<_, CompileError>((source_paths, packages))
    })();
    let (source_paths, packages) = match prepared {
        Ok(prepared) => prepared,
        Err(error) => {
            package_progress.clear();
            return Err(error);
        }
    };

    let build_config =
        BuildConfig { output, current_directory: root, color, progress: !quiet, resilience };
    compile_package_source_paths(&build_config, source_paths, packages, started, &package_progress)
}

fn compile_source_paths(
    config: &BuildConfig<'_>,
    source_paths: BTreeSet<PathBuf>,
    started: Instant,
    preparation_progress: indicatif::ProgressBar,
) -> Result<(), CompileError> {
    let mut compilation = CompilationState::new();

    for path in source_paths {
        load_source(&mut compilation, &path)?;
    }
    let source_ids = compilation.input_source_ids();
    preparation_progress.inc(1);
    progress::finish(&preparation_progress);

    if source_ids.is_empty() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "no input files found").into());
    }

    if matches!(build(&compilation, config)?, BuildOutcome::Diagnostics) {
        return Err(CompileError::Diagnostics);
    }

    if config.progress {
        progress::report_completion(started.elapsed());
    }

    Ok(())
}

fn select_package_sources(
    source_paths: &BTreeSet<PathBuf>,
    packages: Vec<PackageInput>,
) -> Result<Vec<PackageInput>, CompileError> {
    let mut owners: HashMap<PathBuf, String> = HashMap::new();
    for package in &packages {
        for source in &package.sources {
            let source = dunce::canonicalize(source)?;
            if let Some(first_package) = owners.get(&source) {
                if first_package != &package.name {
                    return Err(CompileError::ConflictingPackageSource {
                        path: source,
                        first_package: String::clone(first_package),
                        second_package: String::clone(&package.name),
                    });
                }
            } else {
                owners.insert(source, String::clone(&package.name));
            }
        }
    }

    let mut selected: HashMap<String, Vec<PathBuf>> = HashMap::new();
    for source in source_paths {
        let canonical = dunce::canonicalize(source)?;
        let Some(owner) = owners.get(&canonical) else {
            return Err(CompileError::UnownedPackageSource(source.to_path_buf()));
        };
        selected.entry(String::clone(owner)).or_default().push(source.to_path_buf());
    }

    let retained = selected.keys().map(String::as_str).collect::<HashSet<_>>();
    let packages_by_name =
        packages.iter().map(|package| (package.name.as_str(), package)).collect::<HashMap<_, _>>();
    let contracted_dependencies =
        packages.iter().filter(|package| retained.contains(&*package.name)).map(|package| {
            let mut dependencies = vec![];
            let mut visited = HashSet::new();
            let mut pending = package.dependencies.iter().rev().map(String::as_str).collect_vec();
            while let Some(dependency) = pending.pop() {
                if !visited.insert(dependency) {
                    continue;
                }
                if retained.contains(dependency) {
                    dependencies.push(dependency.to_owned());
                } else if let Some(package) = packages_by_name.get(dependency) {
                    pending.extend(package.dependencies.iter().rev().map(String::as_str));
                }
            }
            (String::clone(&package.name), dependencies)
        });
    let contracted_dependencies = contracted_dependencies.collect::<HashMap<_, _>>();
    let packages = packages.into_iter().filter_map(|package| {
        let sources = selected.remove(&package.name)?;
        let dependencies = contracted_dependencies[&package.name].clone();
        Some(PackageInput { sources, dependencies, ..package })
    });
    Ok(packages.collect_vec())
}

fn compile_package_source_paths(
    config: &BuildConfig<'_>,
    source_paths: BTreeSet<PathBuf>,
    packages: Vec<PackageInput>,
    started: Instant,
    package_progress: &progress::PackageProgress,
) -> Result<(), CompileError> {
    let compilation = (|| {
        let mut compilation = CompilationState::new();
        let mut source_ids = HashMap::new();
        for path in source_paths {
            let file_id = load_source(&mut compilation, &path)?;
            source_ids.insert(path, file_id);
        }
        let jobs = package_jobs(packages, &source_ids);

        if jobs.is_empty() {
            return Err(io::Error::new(io::ErrorKind::NotFound, "no input files found").into());
        }

        package_progress.begin_compilation(jobs.len());
        schedule_package_jobs(&jobs, package_progress, &|package| {
            query_package(&compilation, &package.source_ids)
        })?;
        Ok::<_, CompileError>(compilation)
    })();
    let compilation = match compilation {
        Ok(compilation) => compilation,
        Err(error) => {
            package_progress.clear();
            return Err(error);
        }
    };

    let finalization_config = BuildConfig {
        output: config.output,
        current_directory: config.current_directory,
        color: config.color,
        progress: false,
        resilience: config.resilience,
    };
    package_progress.finish(started.elapsed());
    let source_ids = compilation.input_source_ids();
    if matches!(
        finalize_build(&compilation, &source_ids, &finalization_config)?,
        BuildOutcome::Diagnostics
    ) {
        return Err(CompileError::Diagnostics);
    }
    Ok(())
}

fn package_jobs(
    packages: Vec<PackageInput>,
    source_ids: &HashMap<PathBuf, FileId>,
) -> Vec<PackageJob> {
    let package_indices = packages
        .iter()
        .enumerate()
        .map(|(index, package)| (String::clone(&package.name), index))
        .collect::<HashMap<_, _>>();
    let jobs = packages.into_iter().map(|package| PackageJob {
        name: package.name,
        source_ids: package.sources.iter().map(|source| source_ids[source]).collect_vec(),
        dependencies: package
            .dependencies
            .iter()
            .filter_map(|dependency| package_indices.get(dependency).copied())
            .collect_vec(),
    });
    jobs.collect_vec()
}

struct PackageScheduler<'a, F> {
    jobs: &'a [PackageJob],
    groups: Vec<PackageGroup>,
    remaining: Vec<AtomicUsize>,
    package_progress: &'a progress::PackageProgress,
    execute: &'a F,
    failed: AtomicBool,
    error: Mutex<Option<CompileError>>,
}

impl<'a, F> PackageScheduler<'a, F>
where
    F: Fn(&PackageJob) -> Result<(), CompileError> + Sync,
{
    fn spawn<'scope>(&'scope self, scope: &rayon::Scope<'scope>, group_index: usize) {
        scope.spawn(move |scope| {
            let results = self.groups[group_index].packages.par_iter().map(|&package_index| {
                let package = &self.jobs[package_index];
                let started = Instant::now();
                let result = (self.execute)(package);
                if result.is_ok() {
                    self.package_progress.complete(&package.name, started.elapsed());
                }
                result
            });
            let result = results.collect::<Result<Vec<_>, _>>();
            if let Err(package_error) = result {
                self.failed.store(true, Ordering::Release);
                let mut error = self
                    .error
                    .lock()
                    .expect("invariant violated: package build error is not poisoned");
                error.get_or_insert(package_error);
                return;
            }
            if self.failed.load(Ordering::Acquire) {
                return;
            }
            for &dependent in &self.groups[group_index].dependents {
                if self.remaining[dependent].fetch_sub(1, Ordering::AcqRel) == 1 {
                    self.spawn(scope, dependent);
                }
            }
        });
    }
}

fn schedule_package_jobs<F>(
    jobs: &[PackageJob],
    package_progress: &progress::PackageProgress,
    execute: &F,
) -> Result<(), CompileError>
where
    F: Fn(&PackageJob) -> Result<(), CompileError> + Sync,
{
    let groups = package_groups(jobs);
    let remaining = groups.iter().map(|group| AtomicUsize::new(group.dependencies.len()));
    let remaining = remaining.collect_vec();
    let scheduler = PackageScheduler {
        jobs,
        groups,
        remaining,
        package_progress,
        execute,
        failed: AtomicBool::new(false),
        error: Mutex::new(None),
    };

    rayon::scope(|scope| {
        for (group_index, group) in scheduler.groups.iter().enumerate() {
            if group.dependencies.is_empty() {
                scheduler.spawn(scope, group_index);
            }
        }
    });

    scheduler
        .error
        .into_inner()
        .expect("invariant violated: package build error is not poisoned")
        .map_or(Ok(()), Err)
}

fn query_package(
    compilation: &CompilationState,
    source_ids: &[FileId],
) -> Result<(), CompileError> {
    let queried = source_ids.par_iter().map(|&file_id| {
        analyse_source(compilation, file_id)?;
        elaborate_source(compilation, file_id)?;
        generate_source(compilation, file_id)
    });
    queried.collect::<Result<Vec<_>, _>>()?;
    Ok(())
}

fn analyse_source(compilation: &CompilationState, file_id: FileId) -> Result<(), CompileError> {
    let engine = compilation.snapshot();
    engine.stabilized(file_id)?;
    engine.indexed(file_id)?;
    engine.resolved(file_id)?;
    engine.lowered(file_id)?;
    Ok(())
}

fn elaborate_source(compilation: &CompilationState, file_id: FileId) -> Result<(), CompileError> {
    let engine = compilation.snapshot();
    engine.checked(file_id)?;
    engine.foreign_validation(file_id)?;
    Ok(())
}

fn generate_source(compilation: &CompilationState, file_id: FileId) -> Result<(), CompileError> {
    let engine = compilation.snapshot();
    let _ = engine.javascript(file_id)?;
    Ok(())
}

fn package_groups(jobs: &[PackageJob]) -> Vec<PackageGroup> {
    let mut graph = DiGraphMap::<usize, ()>::default();
    for package in 0..jobs.len() {
        graph.add_node(package);
    }
    for (package, job) in jobs.iter().enumerate() {
        for &dependency in &job.dependencies {
            graph.add_edge(package, dependency, ());
        }
    }

    let mut group_packages = tarjan_scc(&graph);
    for packages in &mut group_packages {
        packages.sort_unstable();
    }
    group_packages.sort_unstable_by_key(|packages| packages[0]);
    let mut package_to_group = vec![usize::MAX; jobs.len()];
    for (group, packages) in group_packages.iter().enumerate() {
        for &package in packages {
            package_to_group[package] = group;
        }
    }

    let dependencies = group_packages.iter().enumerate().map(|(group, packages)| {
        let mut dependencies = packages
            .iter()
            .flat_map(|&package| &jobs[package].dependencies)
            .map(|&package| package_to_group[package])
            .filter(|&dependency| dependency != group)
            .collect_vec();
        dependencies.sort_unstable();
        dependencies.dedup();
        dependencies
    });
    let dependencies = dependencies.collect_vec();
    let mut dependents = vec![vec![]; group_packages.len()];
    for (group, dependencies) in dependencies.iter().enumerate() {
        for &dependency in dependencies {
            dependents[dependency].push(group);
        }
    }

    let groups = group_packages.into_iter().zip(dependencies).zip(dependents).map(
        |((packages, dependencies), dependents)| PackageGroup {
            packages,
            dependencies,
            dependents,
        },
    );
    groups.collect_vec()
}

pub(crate) fn build(
    compilation: &CompilationState,
    config: &BuildConfig<'_>,
) -> Result<BuildOutcome, CompileError> {
    let source_ids = compilation.input_source_ids();
    query_sources(compilation, &source_ids, config.progress)?;
    finalize_build(compilation, &source_ids, config)
}

fn finalize_build(
    compilation: &CompilationState,
    source_ids: &[FileId],
    config: &BuildConfig<'_>,
) -> Result<BuildOutcome, CompileError> {
    let has_errors = report_diagnostics(compilation, source_ids, config)?;
    if has_errors && config.resilience == Resilience::Strict {
        return Ok(BuildOutcome::Diagnostics);
    }

    let modules = collect_modules(compilation, source_ids)?;
    let outputs = write_modules(compilation, &modules, config.output, config.progress)?;
    if has_errors { Ok(BuildOutcome::Diagnostics) } else { Ok(BuildOutcome::Succeeded(outputs)) }
}

pub(crate) fn load_source(
    compilation: &mut CompilationState,
    path: &Path,
) -> Result<FileId, CompileError> {
    let source_url =
        Url::from_file_path(path).map_err(|()| CompileError::InvalidPath(path.to_path_buf()))?;
    let foreign_path = path.with_extension("js");
    let foreign_url = Url::from_file_path(&foreign_path)
        .map_err(|()| CompileError::InvalidPath(foreign_path.clone()))?;

    let unit = SourceUnitKey::new(source_url.as_str(), foreign_url.as_str());

    let content = fs::read_to_string(path)?;
    let change = compilation
        .observe_source(SourceUnitKey::clone(&unit), DiskObservation::Found(content.into()));
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

pub(crate) fn use_color(choice: ColorChoice) -> bool {
    match choice {
        ColorChoice::Auto => {
            let no_color = std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty());
            io::stderr().is_terminal() && !no_color
        }
        ColorChoice::Always => true,
        ColorChoice::Never => false,
    }
}

fn display_source_path(source_path: &str, current_directory: &Path) -> String {
    if let Some(file_path) = Url::parse(source_path).ok().and_then(|url| url.to_file_path().ok()) {
        file_path.strip_prefix(current_directory).unwrap_or(&file_path).display().to_string()
    } else {
        source_path.to_owned()
    }
}

fn report_diagnostics(
    compilation: &CompilationState,
    source_ids: &[FileId],
    config: &BuildConfig<'_>,
) -> Result<bool, CompileError> {
    let engine = compilation.snapshot();
    let diagnostics = diagnostics::collect_diagnostics(&engine, source_ids)?;
    let has_errors = diagnostics
        .iter()
        .flat_map(diagnostics::DiagnosticCollection::diagnostics)
        .any(|diagnostic| diagnostic.severity == Severity::Error);
    let has_diagnostics = diagnostics.iter().any(|collected| !collected.diagnostics().is_empty());

    if has_diagnostics {
        if config.progress && io::stderr().is_terminal() {
            eprint!("\n\n");
        } else if !config.progress {
            eprintln!();
        }
    }
    for collected in diagnostics {
        if collected.diagnostics().is_empty() {
            continue;
        }
        let source_path =
            compilation.source_path(collected.file_id).expect("input source has no lifecycle path");
        let display_path = display_source_path(&source_path, config.current_directory);
        let line_index = line_index::LineIndex::new(&collected.content);
        let rendered = diagnostics::format_rich_with_path(
            collected.diagnostics(),
            &collected.content,
            &line_index,
            &display_path,
            config.color,
        );
        eprint!("{rendered}");
    }
    Ok(has_errors)
}

fn query_sources(
    compilation: &CompilationState,
    source_ids: &[FileId],
    show_progress: bool,
) -> Result<(), CompileError> {
    let progress = MultiProgress::new();
    progress.set_move_cursor(true);
    let analysing_progress = progress::phase(&progress, source_ids.len(), "Analyse", show_progress);
    let checking_progress =
        progress::phase(&progress, source_ids.len(), "Elaborate", show_progress);
    let generating_progress =
        progress::phase(&progress, source_ids.len(), "Generate", show_progress);

    let module_names = source_ids.par_iter().map(|&file_id| {
        let engine = compilation.snapshot();
        let content = engine.content(file_id)?;
        let (parsed, _) = engine.parsed(file_id)?;
        let module_name = parsed.module_name(&content);
        if let Some(module_name) = &module_name {
            progress::set_message(&analysing_progress, module_name);
        }
        analyse_source(compilation, file_id)?;
        analysing_progress.inc(1);
        Ok::<_, CompileError>(module_name)
    });
    let module_names = module_names.collect::<Result<Vec<_>, _>>()?;
    progress::finish(&analysing_progress);

    let elaborated = source_ids.par_iter().zip(&module_names).map(|(&file_id, module_name)| {
        if let Some(module_name) = module_name {
            progress::set_message(&checking_progress, module_name);
        }
        elaborate_source(compilation, file_id)?;
        checking_progress.inc(1);
        Ok::<_, CompileError>(())
    });
    elaborated.collect::<Result<Vec<_>, _>>()?;
    progress::finish(&checking_progress);

    let generated = source_ids.par_iter().zip(&module_names).map(|(&file_id, module_name)| {
        if let Some(module_name) = module_name {
            progress::set_message(&generating_progress, module_name);
        }
        generate_source(compilation, file_id)?;
        generating_progress.inc(1);
        Ok::<_, CompileError>(())
    });
    generated.collect::<Result<Vec<_>, _>>()?;
    progress::finish(&generating_progress);
    Ok(())
}

fn collect_modules(
    compilation: &CompilationState,
    source_ids: &[FileId],
) -> Result<Vec<Arc<javascript::Module>>, CompileError> {
    let mut pending = source_ids.to_vec();
    let mut visited = HashSet::new();

    let mut modules = vec![];
    while !pending.is_empty() {
        let frontier = pending.drain(..).filter(|file_id| visited.insert(*file_id));
        let frontier = frontier.collect::<Vec<_>>();

        let generated = frontier.par_iter().map(|&file_id| {
            let engine = compilation.snapshot();
            let module = engine.javascript(file_id)?.ok();
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
    show_progress: bool,
) -> Result<BTreeSet<PathBuf>, CompileError> {
    let mut outputs = BTreeSet::new();
    if modules.iter().any(|module| module.requires_runtime()) {
        let runtime = output.join(javascript::runtime_filename());
        fs::create_dir_all(output)?;
        write_if_changed(&runtime, javascript::runtime_source().as_bytes())?;
        outputs.insert(runtime);
    }

    let progress = progress::bar(modules.len(), "Write", show_progress);
    let module_outputs = modules.par_iter().map(|module| -> Result<Vec<PathBuf>, CompileError> {
        progress::set_message(&progress, module.name());
        let output_path = output.join(module.filename());
        let output_parent =
            output_path.parent().expect("invariant violated: module filename has no parent");

        fs::create_dir_all(output_parent)?;
        write_if_changed(&output_path, module.source().as_bytes())?;
        let mut outputs = vec![output_path];

        if let Some(kind) = module.foreign_kind() {
            let output_path = output.join(javascript::foreign_module_filename(module.name(), kind));
            let foreign = compilation
                .source_foreign_content(module.file_id())
                .expect("invariant violated: generated module requires missing foreign content");
            write_if_changed(&output_path, foreign.as_bytes())?;
            outputs.push(output_path);
        }

        progress.inc(1);
        Ok(outputs)
    });
    let module_outputs = module_outputs.collect::<Result<Vec<_>, _>>()?;
    outputs.extend(module_outputs.into_iter().flatten());
    progress::finish(&progress);
    Ok(outputs)
}

fn write_if_changed(path: &Path, content: &[u8]) -> io::Result<()> {
    match fs::read(path) {
        Ok(previous) if previous == content => Ok(()),
        Ok(_) => fs::write(path, content),
        Err(error) if error.kind() == io::ErrorKind::NotFound => fs::write(path, content),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Condvar, mpsc};
    use std::thread;
    use std::time::Duration;
    use tempfile::tempdir;

    fn package(name: &str, dependencies: Vec<usize>) -> PackageJob {
        PackageJob { name: name.to_owned(), source_ids: vec![], dependencies }
    }

    #[test]
    fn package_source_ownership_must_be_unique() {
        let directory = tempdir().unwrap();
        let source = directory.path().join("Shared.purs");
        fs::write(&source, "module Shared where\n").unwrap();
        let source_paths = BTreeSet::from([PathBuf::clone(&source)]);
        let packages = vec![
            PackageInput {
                name: "first".to_owned(),
                sources: vec![PathBuf::clone(&source)],
                dependencies: vec![],
            },
            PackageInput { name: "second".to_owned(), sources: vec![source], dependencies: vec![] },
        ];

        let result = select_package_sources(&source_paths, packages);

        assert!(matches!(
            result,
            Err(CompileError::ConflictingPackageSource {
                first_package,
                second_package,
                ..
            }) if first_package == "first" && second_package == "second"
        ));
    }

    #[test]
    fn source_empty_packages_preserve_dependency_reachability() {
        let directory = tempdir().unwrap();
        let library = directory.path().join("Library.purs");
        let application = directory.path().join("Application.purs");
        fs::write(&library, "module Library where\n").unwrap();
        fs::write(&application, "module Application where\n").unwrap();
        let source_paths = BTreeSet::from([PathBuf::clone(&library), PathBuf::clone(&application)]);
        let packages = vec![
            PackageInput {
                name: "library".to_owned(),
                sources: vec![library],
                dependencies: vec![],
            },
            PackageInput {
                name: "metadata-only".to_owned(),
                sources: vec![],
                dependencies: vec!["library".to_owned()],
            },
            PackageInput {
                name: "application".to_owned(),
                sources: vec![application],
                dependencies: vec!["metadata-only".to_owned()],
            },
        ];

        let selected = select_package_sources(&source_paths, packages).unwrap();
        let dependencies = selected
            .into_iter()
            .map(|package| (package.name, package.dependencies))
            .collect::<HashMap<_, _>>();

        assert_eq!(dependencies["application"], ["library"]);
        assert!(dependencies["library"].is_empty());
    }

    #[test]
    fn package_groups_preserve_dependency_edges_and_independent_roots() {
        let jobs = [
            package("root-a", vec![]),
            package("root-b", vec![]),
            package("middle", vec![0]),
            package("leaf", vec![1, 2]),
        ];

        let groups = package_groups(&jobs);

        assert_eq!(groups.len(), 4);
        assert!(groups[0].dependencies.is_empty());
        assert!(groups[1].dependencies.is_empty());
        assert_eq!(groups[2].dependencies, vec![0]);
        assert_eq!(groups[3].dependencies, vec![1, 2]);
        assert_eq!(groups[0].dependents, vec![2]);
        assert_eq!(groups[1].dependents, vec![3]);
        assert_eq!(groups[2].dependents, vec![3]);
    }

    #[test]
    fn package_cycles_are_scheduled_as_one_group() {
        let jobs = [
            package("root", vec![]),
            package("cycle-a", vec![0, 2]),
            package("cycle-b", vec![1]),
            package("downstream", vec![2]),
        ];

        let groups = package_groups(&jobs);

        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].packages, vec![0]);
        assert_eq!(groups[1].packages, vec![1, 2]);
        assert_eq!(groups[2].packages, vec![3]);
        assert_eq!(groups[1].dependencies, vec![0]);
        assert_eq!(groups[2].dependencies, vec![1]);
        assert_eq!(groups[0].dependents, vec![1]);
        assert_eq!(groups[1].dependents, vec![2]);
    }

    #[test]
    fn package_downstream_of_cycle_waits_for_the_cycle() {
        let jobs = vec![
            package("root", vec![]),
            package("cycle-a", vec![0, 2]),
            package("cycle-b", vec![1]),
            package("downstream", vec![2]),
        ];
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let worker_gate = Arc::clone(&gate);
        let (started, starts) = mpsc::channel();
        let worker = thread::spawn(move || {
            let pool = rayon::ThreadPoolBuilder::new().num_threads(2).build().unwrap();
            let progress = progress::packages(false, false);
            pool.install(|| {
                schedule_package_jobs(&jobs, &progress, &|package| {
                    started.send(String::clone(&package.name)).unwrap();
                    if package.name == "cycle-b" {
                        let (lock, condition) = &*worker_gate;
                        let open = lock.lock().unwrap();
                        drop(condition.wait_while(open, |open| !*open).unwrap());
                    }
                    Ok(())
                })
            })
        });

        assert_eq!(starts.recv_timeout(Duration::from_secs(1)).unwrap(), "root");
        let cycle = [
            starts.recv_timeout(Duration::from_secs(1)).unwrap(),
            starts.recv_timeout(Duration::from_secs(1)).unwrap(),
        ];
        assert_eq!(
            cycle.into_iter().collect::<BTreeSet<_>>(),
            BTreeSet::from(["cycle-a".to_owned(), "cycle-b".to_owned(),])
        );
        assert!(starts.recv_timeout(Duration::from_millis(100)).is_err());
        let (lock, condition) = &*gate;
        *lock.lock().unwrap() = true;
        condition.notify_all();

        assert_eq!(starts.recv_timeout(Duration::from_secs(1)).unwrap(), "downstream");
        worker.join().unwrap().unwrap();
    }

    #[test]
    fn independent_packages_start_concurrently() {
        let jobs = vec![package("root-a", vec![]), package("root-b", vec![])];
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let worker_gate = Arc::clone(&gate);
        let (started, starts) = mpsc::channel();
        let worker = thread::spawn(move || {
            let pool = rayon::ThreadPoolBuilder::new().num_threads(2).build().unwrap();
            let progress = progress::packages(false, false);
            pool.install(|| {
                schedule_package_jobs(&jobs, &progress, &|package| {
                    started.send(String::clone(&package.name)).unwrap();
                    let (lock, condition) = &*worker_gate;
                    let open = lock.lock().unwrap();
                    drop(condition.wait_while(open, |open| !*open).unwrap());
                    Ok(())
                })
            })
        });

        let first = starts.recv_timeout(Duration::from_secs(1)).unwrap();
        let second = starts.recv_timeout(Duration::from_secs(1));
        let (lock, condition) = &*gate;
        *lock.lock().unwrap() = true;
        condition.notify_all();

        assert!(second.is_ok(), "only {first} started before independent work was released");
        worker.join().unwrap().unwrap();
    }

    #[test]
    fn dependent_package_waits_for_its_prerequisite() {
        let jobs = vec![package("root", vec![]), package("dependent", vec![0])];
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let worker_gate = Arc::clone(&gate);
        let (started, starts) = mpsc::channel();
        let worker = thread::spawn(move || {
            let pool = rayon::ThreadPoolBuilder::new().num_threads(2).build().unwrap();
            let progress = progress::packages(false, false);
            pool.install(|| {
                schedule_package_jobs(&jobs, &progress, &|package| {
                    started.send(String::clone(&package.name)).unwrap();
                    if package.name == "root" {
                        let (lock, condition) = &*worker_gate;
                        let open = lock.lock().unwrap();
                        drop(condition.wait_while(open, |open| !*open).unwrap());
                    }
                    Ok(())
                })
            })
        });

        assert_eq!(starts.recv_timeout(Duration::from_secs(1)).unwrap(), "root");
        assert!(starts.recv_timeout(Duration::from_millis(100)).is_err());
        let (lock, condition) = &*gate;
        *lock.lock().unwrap() = true;
        condition.notify_all();

        assert_eq!(starts.recv_timeout(Duration::from_secs(1)).unwrap(), "dependent");
        worker.join().unwrap().unwrap();
    }

    #[test]
    fn packages_are_scheduled_exactly_once() {
        let jobs = vec![
            package("root-a", vec![]),
            package("root-b", vec![]),
            package("fan-in", vec![0, 1]),
            package("leaf", vec![2]),
        ];
        let executions = Mutex::new(HashMap::new());
        let progress = progress::packages(false, false);

        schedule_package_jobs(&jobs, &progress, &|package| {
            let mut executions = executions.lock().unwrap();
            *executions.entry(String::clone(&package.name)).or_insert(0) += 1;
            Ok(())
        })
        .unwrap();

        let executions = executions.into_inner().unwrap();
        assert_eq!(executions.len(), jobs.len());
        assert!(executions.values().all(|&count| count == 1));
    }
}
