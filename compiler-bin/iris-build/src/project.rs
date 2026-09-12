use std::path::PathBuf;
use std::{env, io};

use iris_progress::ProgressRuntime;
use itertools::Itertools;
use path_absolutize::Absolutize;
use thiserror::Error;

use super::compile::{self, CompileError};
use super::events::{BuildEvent, BuildEventSink, ProgressEventSink};
use super::plan::PackageInput;
use super::workspace::{Workspace, WorkspaceError};

pub struct BuildConfig {
    pub package: Option<String>,
    pub output: Option<PathBuf>,
    pub quiet: bool,
    pub color: bool,
    pub resilient: bool,
    pub diagnostics: bool,
}

pub struct ProjectConfig {
    pub package: Option<String>,
    pub output: Option<PathBuf>,
    pub quiet: bool,
}

#[derive(Debug, Error)]
enum ProjectFailure {
    #[error(transparent)]
    Compile(#[from] CompileError),
    #[error(transparent)]
    Spago(#[from] spago::SpagoError),
    #[error(transparent)]
    SpagoLock(#[from] spago::LockfileGlobSetError),
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error("failed to determine the current directory: {0}")]
    CurrentDirectory(io::Error),
    #[error("failed to normalize output directory {}: {error}", path.display())]
    NormalizeOutput { path: PathBuf, error: io::Error },
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

pub struct PreparedProject {
    pub(crate) root: PathBuf,
    pub(crate) output: PathBuf,
    pub(crate) source_globs: Vec<PathBuf>,
}

impl PreparedProject {
    pub fn root_directory(&self) -> &std::path::Path {
        &self.root
    }
}

pub fn build(config: BuildConfig) -> Result<(), BuildError> {
    build_project(config).map_err(BuildError)
}

pub fn prepare_project(config: ProjectConfig) -> Result<PreparedProject, ProjectError> {
    prepare_project_inner(config).map_err(ProjectError)
}

fn build_project(config: BuildConfig) -> Result<(), ProjectFailure> {
    let project = prepare_project_inner(ProjectConfig {
        package: config.package,
        output: config.output,
        quiet: config.quiet,
    })?;
    let package_sources = spago::source_files_by_package(&project.root)?;

    let progress = ProgressRuntime::start(!config.quiet, config.color);
    let events = ProgressEventSink::new(progress.reporter());
    events.send(BuildEvent::Preparing);
    let packages = package_sources.into_iter().map(|(name, package)| {
        let dependencies = package.dependencies.into_iter().map(|name| name.to_string());
        PackageInput {
            name: name.to_string(),
            source_identities: package.sources,
            dependencies: dependencies.collect_vec(),
        }
    });
    let packages = packages.collect_vec();
    compile::build(compile::BuildConfig {
        root: project.root,
        output: project.output,
        source_globs: project.source_globs,
        packages,
        color: config.color,
        diagnostics: config.diagnostics,
        resilient: config.resilient,
        events: &events,
    })?;
    Ok(())
}

fn prepare_project_inner(config: ProjectConfig) -> Result<PreparedProject, ProjectFailure> {
    let current_directory = env::current_dir().map_err(ProjectFailure::CurrentDirectory)?;
    let workspace = Workspace::discover(&current_directory, config.package.as_deref())?;
    let spago = spago::SpagoCommand::new(&current_directory)?;
    spago.fetch(workspace.selected.as_deref(), !config.quiet)?;
    let source_globs = spago.source_globs(workspace.selected.as_deref(), !config.quiet)?;
    let output = if let Some(output) = config.output {
        output
            .absolutize()
            .map_err(|error| ProjectFailure::NormalizeOutput {
                path: PathBuf::clone(&output),
                error,
            })?
            .into_owned()
    } else {
        workspace.root.join("output")
    };
    Ok(PreparedProject { root: workspace.root, output, source_globs })
}
