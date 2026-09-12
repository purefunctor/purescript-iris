use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::{fs, io};

use building::{DiskObservation, LifecycleChange, QueryError, ReloadFailure, SourceUnitKey};
use files::ForeignSourceKind;
use itertools::Itertools;
use thiserror::Error;
use url::Url;

use super::compilation::CompilationState;
use super::compile::{self, CompileError};
use super::events::BuildOutcome;
use super::project::PreparedProject;
use super::walk;

pub struct BuildSessionConfig {
    pub color: bool,
    pub diagnostics: bool,
}

#[derive(Debug, Eq, PartialEq)]
pub struct InputChange {
    pub source_path: PathBuf,
    pub module_name: Option<String>,
}

#[derive(Debug, Default, Eq, PartialEq)]
pub struct InputChanges {
    pub inputs: Vec<InputChange>,
    pub warnings: Vec<String>,
}

impl InputChanges {
    pub fn is_empty(&self) -> bool {
        self.inputs.is_empty()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RebuildOutcome {
    Succeeded,
    Diagnostics,
    NoInputs,
}

#[derive(Debug, Error)]
enum SessionFailure {
    #[error(transparent)]
    Compile(#[from] CompileError),
    #[error("failed to convert path to a file URL: {0}")]
    InvalidPath(PathBuf),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Query(#[from] QueryError),
    #[error(transparent)]
    Walk(#[from] walk::Error),
}

#[derive(Debug, Error)]
#[error(transparent)]
pub struct SessionError(SessionFailure);

pub struct BuildSession {
    root: PathBuf,
    output: PathBuf,
    inputs: Vec<PathBuf>,
    source_roots: Vec<PathBuf>,
    source_globs: globset::GlobSet,
    source_paths: BTreeSet<PathBuf>,
    generated_outputs: BTreeSet<PathBuf>,
    compilation: CompilationState,
    color: bool,
    diagnostics: bool,
}

impl BuildSession {
    pub fn new(
        project: PreparedProject,
        config: BuildSessionConfig,
    ) -> Result<BuildSession, SessionError> {
        BuildSession::create(project, config).map_err(SessionError)
    }

    fn create(
        project: PreparedProject,
        config: BuildSessionConfig,
    ) -> Result<BuildSession, SessionFailure> {
        let walked = walk::walk_filtered(&project.root, &project.source_globs, [&project.output])?;
        let source_roots = walked.roots.into_iter().collect_vec();
        Ok(BuildSession {
            root: project.root,
            output: project.output,
            inputs: project.source_globs,
            source_roots,
            source_globs: walked.globs,
            source_paths: BTreeSet::new(),
            generated_outputs: BTreeSet::new(),
            compilation: CompilationState::new(),
            color: config.color,
            diagnostics: config.diagnostics,
        })
    }

    pub fn source_roots(&self) -> &[PathBuf] {
        &self.source_roots
    }

    pub fn root_directory(&self) -> &Path {
        &self.root
    }

    pub fn synchronize_paths(&mut self, paths: &[PathBuf]) -> Result<InputChanges, SessionError> {
        self.synchronize(paths).map(SessionChange::into_public).map_err(SessionError)
    }

    pub fn rescan(&mut self) -> Result<InputChanges, SessionError> {
        self.rescan_inputs().map(SessionChange::into_public).map_err(SessionError)
    }

    pub fn rebuild(&mut self) -> Result<RebuildOutcome, SessionError> {
        self.rebuild_inputs().map_err(SessionError)
    }

    fn synchronize(&mut self, paths: &[PathBuf]) -> Result<SessionChange, SessionFailure> {
        let mut source_paths = BTreeSet::new();
        let mut foreign_paths = BTreeSet::new();
        for path in paths {
            if path.starts_with(&self.output) {
                continue;
            }
            match path.extension().and_then(|extension| extension.to_str()) {
                Some("purs") => {
                    if self.source_paths.contains(path)
                        || !self.source_globs.matches(path).is_empty()
                    {
                        source_paths.insert(PathBuf::clone(path));
                    }
                }
                Some("js" | "jsx") => {
                    let source_path = path.with_extension("purs");
                    if self.source_paths.contains(&source_path) {
                        foreign_paths.insert(source_path);
                    }
                }
                _ => {}
            }
        }

        let mut change = SessionChange::default();
        for path in source_paths {
            change.combine(observe_source_unit(&mut self.compilation, &path)?);
            if path.exists() {
                self.source_paths.insert(path);
            } else {
                self.source_paths.remove(&path);
            }
        }
        for source_path in foreign_paths {
            change.combine(observe_foreign(&mut self.compilation, &source_path)?);
        }
        Ok(change)
    }

    fn rescan_inputs(&mut self) -> Result<SessionChange, SessionFailure> {
        let walked = walk::walk_filtered(&self.root, &self.inputs, [&self.output])?;
        let current_paths = walked.files.into_iter().collect::<BTreeSet<_>>();
        let affected_paths = self.source_paths.union(&current_paths).cloned().collect_vec();

        let mut change = SessionChange::default();
        for path in affected_paths {
            change.combine(observe_source_unit(&mut self.compilation, &path)?);
        }
        self.source_globs = walked.globs;
        self.source_paths = current_paths;
        Ok(change)
    }

    fn rebuild_inputs(&mut self) -> Result<RebuildOutcome, SessionFailure> {
        if self.compilation.input_sources().is_empty() {
            self.reconcile_outputs(BTreeSet::new())?;
            return Ok(RebuildOutcome::NoInputs);
        }

        let result = compile::rebuild(
            &self.compilation,
            &self.root,
            &self.output,
            self.color,
            self.diagnostics,
            &mut self.generated_outputs,
        )?;
        match result.outcome {
            BuildOutcome::Succeeded => {
                self.reconcile_outputs(result.outputs)?;
                Ok(RebuildOutcome::Succeeded)
            }
            BuildOutcome::Diagnostics => Ok(RebuildOutcome::Diagnostics),
        }
    }

    fn reconcile_outputs(&mut self, current: BTreeSet<PathBuf>) -> io::Result<()> {
        let stale_outputs = self.generated_outputs.difference(&current).cloned().collect_vec();
        let mut retained_outputs = current;
        let mut failure = None;
        for stale in stale_outputs {
            match fs::remove_file(&stale) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => {
                    retained_outputs.insert(stale);
                    if failure.is_none() {
                        failure = Some(error);
                    }
                }
            }
        }
        self.generated_outputs = retained_outputs;
        failure.map_or(Ok(()), Err)
    }
}

#[derive(Default)]
struct SessionChange {
    lifecycle: LifecycleChange,
    inputs: Vec<InputChange>,
}

impl SessionChange {
    fn combine(&mut self, other: SessionChange) {
        self.lifecycle.combine(other.lifecycle);
        self.inputs.extend(other.inputs);
    }

    fn into_public(self) -> InputChanges {
        let warnings = self.lifecycle.warnings().iter().map(|warning| warning.to_string());
        let warnings = warnings.collect_vec();
        InputChanges { inputs: self.inputs, warnings }
    }
}

fn observe_source_unit(
    compilation: &mut CompilationState,
    source_path: &Path,
) -> Result<SessionChange, SessionFailure> {
    let unit = source_unit(source_path)?;
    let previous_source = compilation.source_content(unit.source())?;
    let previous_foreign =
        ForeignSourceKind::ALL.map(|kind| compilation.foreign_content(unit.foreign_for(kind)));
    let previous_name = compilation.module_name(unit.source())?;

    let source = observe_disk(source_path);
    let mut lifecycle = compilation.observe_source(SourceUnitKey::clone(&unit), source);
    for kind in ForeignSourceKind::ALL {
        let foreign = observe_disk(&source_path.with_extension(kind.extension()));
        lifecycle.combine(compilation.observe_foreign(SourceUnitKey::clone(&unit), kind, foreign));
    }

    let current_source = compilation.source_content(unit.source())?;
    let current_foreign =
        ForeignSourceKind::ALL.map(|kind| compilation.foreign_content(unit.foreign_for(kind)));
    let mut inputs = vec![];
    if previous_source != current_source || previous_foreign != current_foreign {
        let module_name = compilation.module_name(unit.source())?.or(previous_name);
        inputs.push(InputChange { source_path: source_path.to_path_buf(), module_name });
    }
    Ok(SessionChange { lifecycle, inputs })
}

fn observe_foreign(
    compilation: &mut CompilationState,
    source_path: &Path,
) -> Result<SessionChange, SessionFailure> {
    let unit = source_unit(source_path)?;
    let previous_foreign =
        ForeignSourceKind::ALL.map(|kind| compilation.foreign_content(unit.foreign_for(kind)));
    let mut lifecycle = LifecycleChange::default();
    for kind in ForeignSourceKind::ALL {
        let foreign = observe_disk(&source_path.with_extension(kind.extension()));
        lifecycle.combine(compilation.observe_foreign(SourceUnitKey::clone(&unit), kind, foreign));
    }
    let current_foreign =
        ForeignSourceKind::ALL.map(|kind| compilation.foreign_content(unit.foreign_for(kind)));
    let mut inputs = vec![];
    if previous_foreign != current_foreign {
        let module_name = compilation.module_name(unit.source())?;
        inputs.push(InputChange { source_path: source_path.to_path_buf(), module_name });
    }
    Ok(SessionChange { lifecycle, inputs })
}

fn source_unit(source_path: &Path) -> Result<SourceUnitKey, SessionFailure> {
    let source_url = Url::from_file_path(source_path)
        .map_err(|_| SessionFailure::InvalidPath(source_path.to_path_buf()))?;
    let foreign_path = source_path.with_extension("js");
    let foreign_url = Url::from_file_path(&foreign_path)
        .map_err(|_| SessionFailure::InvalidPath(PathBuf::clone(&foreign_path)))?;
    Ok(SourceUnitKey::new(source_url.as_str(), foreign_url.as_str()))
}

fn observe_disk(path: &Path) -> DiskObservation {
    match fs::read_to_string(path) {
        Ok(content) => DiskObservation::Found(content.into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => DiskObservation::NotFound,
        Err(error) => DiskObservation::Failed(ReloadFailure::new(error.kind(), error.to_string())),
    }
}
