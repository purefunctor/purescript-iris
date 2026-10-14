use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::{env, io};

use iris_progress::ProgressRuntime;
use itertools::Itertools;
use path_absolutize::Absolutize;
use thiserror::Error;
use url::Url;

use super::compile::{self, CompileError};
use super::events::{BuildEvent, BuildEventSink, ProgressEventSink, SilentBuildEvents};
use super::manifest::{ManifestError, WorkspaceLayout};
use super::output::BuildOutput;
use super::plan::PackageInput;
use super::workspace::{Workspace, WorkspaceError};

const NODE_RUNNER: &str = include_str!("../bundled/runner.mjs");

pub struct BuildConfig {
    pub package: Option<String>,
    pub output: Option<PathBuf>,
    pub quiet: bool,
    pub color: bool,
    pub diagnostics: bool,
    pub version: String,
}

pub struct ProjectConfig {
    pub package: Option<String>,
    pub output: Option<PathBuf>,
    pub quiet: bool,
}

pub struct RunConfig {
    pub project: ProjectConfig,
    pub color: bool,
    pub version: String,
    pub main: Option<String>,
    pub arguments: Vec<String>,
}

pub struct TestConfig {
    pub project: ProjectConfig,
    pub color: bool,
    pub version: String,
    pub main: Option<String>,
    pub arguments: Vec<String>,
}

#[derive(Debug, Error)]
enum ProjectFailure {
    #[error(transparent)]
    Compile(#[from] CompileError),
    #[error(transparent)]
    Spago(#[from] iris_spago::SpagoError),
    #[error(transparent)]
    Packages(#[from] super::packages::PackagesError),
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error("failed to determine the current directory: {0}")]
    CurrentDirectory(io::Error),
    #[error("failed to normalize output directory {}: {error}", path.display())]
    NormalizeOutput { path: PathBuf, error: io::Error },
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error("failed to execute Node.js: {source}")]
    Node { source: io::Error },
    #[error("Node.js exited without a status code")]
    MissingStatus,
    #[error("Node.js exited with status {status}")]
    NodeFailed { status: ExitStatus },
    #[error("module {module} is not part of the build")]
    MissingModule { module: String },
    #[error("failed to canonicalize module output {path}: {source}")]
    CanonicalizeModule {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to convert module output to a file URL: {path}")]
    ModuleUrl { path: PathBuf },
    #[error("no selected packages contain tests")]
    NoTests,
}

#[derive(Debug, Error)]
#[error(transparent)]
pub struct BuildError(ProjectFailure);

impl BuildError {
    pub fn diagnostics_were_suppressed(&self) -> bool {
        matches!(self.0, ProjectFailure::Compile(CompileError::Diagnostics { reported: false }))
    }
}

#[derive(Debug, Error)]
#[error(transparent)]
pub struct ProjectError(ProjectFailure);

#[derive(Debug, Error)]
#[error(transparent)]
pub struct ExecutionError(ProjectFailure);

impl ExecutionError {
    pub fn exit_code(&self) -> i32 {
        match &self.0 {
            ProjectFailure::NodeFailed { status } => status.code().unwrap_or(1),
            _ => 1,
        }
    }
}

pub struct PreparedProject {
    pub(crate) root: PathBuf,
    pub(crate) output: PathBuf,
    pub(crate) source_globs: Vec<PathBuf>,
    pub(crate) packages: Vec<PackageInput>,
    pub(crate) layout: WorkspaceLayout,
}

pub struct InitializedProject {
    pub(crate) project: PreparedProject,
    pub(crate) build: compile::InitialBuild<(), ()>,
}

impl PreparedProject {
    pub fn root_directory(&self) -> &std::path::Path {
        &self.root
    }

    pub fn source_roots(&self) -> Result<Vec<PathBuf>, ProjectError> {
        let source_globs = self.layout.source_globs();
        let walked = super::walk::walk_filtered(&self.root, source_globs, [&self.output])
            .map_err(CompileError::from)
            .map_err(ProjectFailure::from)
            .map_err(ProjectError)?;
        Ok(walked.roots.into_iter().collect_vec())
    }
}

pub fn build(config: BuildConfig) -> Result<(), BuildError> {
    build_project(config).map_err(BuildError)
}

pub fn prepare_project(config: ProjectConfig) -> Result<PreparedProject, ProjectError> {
    prepare_project_inner(config).map_err(ProjectError)
}

/// Resolves the output directory the way [`prepare_project`] does, without running Spago.
pub fn resolve_output(output: Option<&Path>) -> Result<PathBuf, ProjectError> {
    resolve_output_inner(output).map_err(ProjectError)
}

pub fn initialize_project(project: PreparedProject) -> Result<InitializedProject, ProjectError> {
    initialize_project_inner(project).map_err(ProjectError)
}

pub fn run(config: RunConfig) -> Result<(), ExecutionError> {
    run_project(config).map_err(ExecutionError)
}

pub fn test(config: TestConfig) -> Result<(), ExecutionError> {
    test_project(config).map_err(ExecutionError)
}

fn build_project(config: BuildConfig) -> Result<(), ProjectFailure> {
    let project = prepare_project_inner(ProjectConfig {
        package: Option::clone(&config.package),
        output: Option::clone(&config.output),
        quiet: config.quiet,
    })?;
    compile_project(project, config)?;
    Ok(())
}

fn compile_project(
    project: PreparedProject,
    config: BuildConfig,
) -> Result<BTreeSet<String>, ProjectFailure> {
    let progress = ProgressRuntime::start(!config.quiet, config.color);
    let events = ProgressEventSink::new(progress.reporter());
    events.send(BuildEvent::Preparing);
    let compiled = compile::build(compile::BuildConfig {
        root: project.root,
        output: BuildOutput::new(project.output),
        layout: project.layout,
        version: config.version,
        source_globs: project.source_globs,
        packages: project.packages,
        color: config.color,
        diagnostics: config.diagnostics,
        events: &events,
    })?;
    Ok(compiled)
}

fn initialize_project_inner(
    project: PreparedProject,
) -> Result<InitializedProject, ProjectFailure> {
    let PreparedProject { root, output, source_globs, packages, .. } = &project;
    let excluded = [PathBuf::clone(output)];
    let build = compile::build_initial(compile::InitialBuildConfig {
        root,
        source_globs,
        excluded: &excluded,
        packages: Vec::clone(packages),
        prim_metadata: (),
        source_metadata: |_: &Path| (),
        execution: compile::PackageExecution::Parallel,
        events: &SilentBuildEvents,
    })?;
    Ok(InitializedProject { project, build })
}

fn run_project(config: RunConfig) -> Result<(), ProjectFailure> {
    let current_directory = env::current_dir().map_err(ProjectFailure::CurrentDirectory)?;
    let workspace = Workspace::discover(&current_directory, config.project.package.as_deref())?;
    let package = workspace.require_selected()?;
    let execution = Option::clone(&package.manifest.run).unwrap_or_default();
    let main = config.main.or(execution.main).unwrap_or_else(|| "Main".to_owned());
    let arguments =
        if config.arguments.is_empty() { execution.exec_args } else { config.arguments };
    let output = project_output(&workspace.root, config.project.output.as_deref())?;

    let project = prepare_workspace(
        &workspace,
        &current_directory,
        config.project.quiet,
        PathBuf::clone(&output),
    )?;
    let build_config = execution_build_config(config.project, config.color, config.version);
    let compiled = compile_project(project, build_config)?;
    execute_module(&workspace.root, &output, &compiled, &main, &arguments)
}

fn test_project(config: TestConfig) -> Result<(), ProjectFailure> {
    let current_directory = env::current_dir().map_err(ProjectFailure::CurrentDirectory)?;
    let workspace = Workspace::discover(&current_directory, config.project.package.as_deref())?;
    if workspace.selected.is_some() {
        if !workspace.require_selected()?.has_tests {
            return Err(ProjectFailure::NoTests);
        }
    } else if !workspace.packages.values().any(|package| package.has_tests) {
        return Err(ProjectFailure::NoTests);
    }

    let output = project_output(&workspace.root, config.project.output.as_deref())?;
    let project = prepare_workspace(
        &workspace,
        &current_directory,
        config.project.quiet,
        PathBuf::clone(&output),
    )?;
    let build_config = execution_build_config(config.project, config.color, config.version);
    let compiled = compile_project(project, build_config)?;

    let packages = workspace.packages.values().filter(|package| {
        workspace
            .selected
            .as_ref()
            .is_some_and(|selected| selected.as_str() == package.manifest.name.as_str())
            || (workspace.selected.is_none() && package.has_tests)
    });
    for package in packages {
        let execution = Option::clone(&package.manifest.test);
        let main = Option::clone(&config.main)
            .or_else(|| execution.as_ref().map(|execution| execution.main.clone()))
            .unwrap_or_else(|| "Test.Main".to_owned());
        let arguments = if config.arguments.is_empty() {
            execution.map(|execution| execution.exec_args).unwrap_or_default()
        } else {
            Vec::clone(&config.arguments)
        };
        execute_module(&workspace.root, &output, &compiled, &main, &arguments)?;
    }
    Ok(())
}

fn execution_build_config(project: ProjectConfig, color: bool, version: String) -> BuildConfig {
    BuildConfig {
        package: project.package,
        output: project.output,
        quiet: project.quiet,
        color,
        diagnostics: true,
        version,
    }
}

fn prepare_project_inner(config: ProjectConfig) -> Result<PreparedProject, ProjectFailure> {
    let current_directory = env::current_dir().map_err(ProjectFailure::CurrentDirectory)?;
    let workspace = Workspace::discover(&current_directory, config.package.as_deref())?;
    let output = project_output(&workspace.root, config.output.as_deref())?;
    prepare_workspace(&workspace, &current_directory, config.quiet, output)
}

fn resolve_output_inner(output: Option<&Path>) -> Result<PathBuf, ProjectFailure> {
    let current_directory = env::current_dir().map_err(ProjectFailure::CurrentDirectory)?;
    if output.is_some() {
        return project_output(&current_directory, output);
    }
    let workspace = Workspace::discover(&current_directory, None)?;
    project_output(&workspace.root, output)
}

fn prepare_workspace(
    workspace: &Workspace,
    current_directory: &Path,
    quiet: bool,
    output: PathBuf,
) -> Result<PreparedProject, ProjectFailure> {
    let spago = iris_spago::SpagoCommand::with_workspace_root(current_directory, &workspace.root)?;
    spago.fetch(None, !quiet)?;
    let workspace_packages = super::packages::discover_workspace_packages(workspace)?;
    let layout = WorkspaceLayout::new(&workspace.root, &workspace_packages)?;
    let discovered = if workspace.selected.is_some() {
        super::packages::discover_packages(workspace)?
    } else {
        workspace_packages
    };
    let packages = discovered.packages.into_iter().map(|package| PackageInput {
        name: package.name,
        source_identities: package.files,
        dependencies: package.dependencies,
    });

    let packages = packages.collect_vec();
    Ok(PreparedProject {
        root: PathBuf::clone(&workspace.root),
        output,
        source_globs: discovered.source_globs,
        packages,
        layout,
    })
}

fn project_output(root: &Path, configured: Option<&Path>) -> Result<PathBuf, ProjectFailure> {
    let output = if let Some(output) = configured {
        output
            .absolutize()
            .map_err(|error| ProjectFailure::NormalizeOutput { path: output.to_path_buf(), error })?
            .into_owned()
    } else {
        root.join("output")
    };
    Ok(dunce::canonicalize(&output).unwrap_or_else(|_| dunce::simplified(&output).to_path_buf()))
}

fn execute_module(
    workspace_root: &Path,
    output: &Path,
    compiled: &BTreeSet<String>,
    module: &str,
    arguments: &[String],
) -> Result<(), ProjectFailure> {
    if !compiled.contains(module) {
        return Err(ProjectFailure::MissingModule { module: module.to_owned() });
    }
    let module = output.join(javascript::module_filename(module));
    let canonicalization_failure =
        |source| ProjectFailure::CanonicalizeModule { path: PathBuf::clone(&module), source };
    let module = dunce::canonicalize(&module).map_err(canonicalization_failure)?;
    let module_url = Url::from_file_path(&module)
        .map_err(|_| ProjectFailure::ModuleUrl { path: PathBuf::clone(&module) })?
        .to_string();
    let status = Command::new("node")
        .arg("--input-type=module")
        .arg("--eval")
        .arg(NODE_RUNNER)
        .arg(module_url)
        .arg("--")
        .args(arguments)
        .current_dir(workspace_root)
        .status()
        .map_err(|source| ProjectFailure::Node { source })?;
    ensure_node_success(status)
}

fn ensure_node_success(status: ExitStatus) -> Result<(), ProjectFailure> {
    if status.success() {
        return Ok(());
    }
    if status.code().is_none() {
        return Err(ProjectFailure::MissingStatus);
    }
    Err(ProjectFailure::NodeFailed { status })
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn configured_output_uses_a_compatible_windows_path() {
        let output = project_output(
            Path::new(r"C:\workspace"),
            Some(Path::new(r"\\?\C:\workspace\src\generated")),
        )
        .unwrap();

        assert_eq!(output, Path::new(r"C:\workspace\src\generated"));
    }
}
