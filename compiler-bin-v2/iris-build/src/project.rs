use std::path::PathBuf;
use std::{env, io};

use iris_progress::ProgressRuntime;
use itertools::Itertools;
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

#[derive(Debug, Error)]
enum ProjectError {
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
}

#[derive(Debug, Error)]
#[error(transparent)]
pub struct BuildError(ProjectError);

impl BuildError {
    pub fn diagnostics_were_suppressed(&self) -> bool {
        matches!(self.0, ProjectError::Compile(CompileError::Diagnostics { reported: false }))
    }
}

pub fn build(config: BuildConfig) -> Result<(), BuildError> {
    build_project(config).map_err(BuildError)
}

fn build_project(config: BuildConfig) -> Result<(), ProjectError> {
    let current_directory = env::current_dir().map_err(ProjectError::CurrentDirectory)?;
    let workspace = Workspace::discover(&current_directory, config.package.as_deref())?;
    let spago = spago::SpagoCommand::new(&current_directory)?;
    spago.fetch(workspace.selected.as_deref(), !config.quiet)?;
    let source_globs = spago.source_globs(workspace.selected.as_deref(), !config.quiet)?;
    let package_sources = spago::source_files_by_package(&workspace.root)?;

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
    let output = config.output.unwrap_or_else(|| workspace.root.join("output"));

    compile::build(compile::BuildConfig {
        root: workspace.root,
        output,
        source_globs,
        packages,
        color: config.color,
        diagnostics: config.diagnostics,
        resilient: config.resilient,
        events: &events,
    })?;
    Ok(())
}
