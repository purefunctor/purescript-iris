use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::{fs, io};

use building::{DiskObservation, LifecycleChange, QueryError, ReloadFailure, SourceUnitKey};
use files::ForeignSourceKind;
use itertools::Itertools;
use thiserror::Error;
use url::Url;

use super::compilation::CompilationState;
use super::compile::{self, CompileError, InitialBuildReport};
use super::events::BuildOutcome;
use super::project::InitializedProject;
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
    source_modules: BTreeMap<PathBuf, String>,
    retired_modules: BTreeSet<String>,
    generated_outputs: BTreeSet<PathBuf>,
    compilation: CompilationState,
    initial_report: Option<InitialBuildReport>,
    initial_inputs: Vec<InputChange>,
    color: bool,
    diagnostics: bool,
}

impl BuildSession {
    pub fn new(
        project: InitializedProject,
        config: BuildSessionConfig,
    ) -> Result<BuildSession, SessionError> {
        BuildSession::create(project, config).map_err(SessionError)
    }

    fn create(
        initialized: InitializedProject,
        config: BuildSessionConfig,
    ) -> Result<BuildSession, SessionFailure> {
        let project = initialized.project;
        let walked = walk::walk_filtered(&project.root, &project.source_globs, [&project.output])?;
        let source_roots = walked.roots.into_iter().collect_vec();
        let initial = initialized.build.into_parts();
        let initial_inputs = initial.source_paths.iter().map(|path| {
            let source_path = PathBuf::clone(path);
            let unit = source_unit(path)?;
            let module_name = initial.compilation.module_name(unit.source())?;
            Ok::<_, SessionFailure>(InputChange { source_path, module_name })
        });
        let initial_inputs = initial_inputs.process_results(|inputs| inputs.collect_vec())?;
        let source_modules = initial_inputs.iter().filter_map(|input| {
            input
                .module_name
                .as_ref()
                .map(|name| (PathBuf::clone(&input.source_path), String::clone(name)))
        });
        let source_modules = source_modules.collect();
        Ok(BuildSession {
            root: project.root,
            output: project.output,
            inputs: project.source_globs,
            source_roots,
            source_globs: walked.globs,
            source_paths: initial.source_paths,
            source_modules,
            retired_modules: BTreeSet::new(),
            generated_outputs: BTreeSet::new(),
            compilation: initial.compilation,
            initial_report: Some(initial.report),
            initial_inputs,
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

    pub fn take_initial_inputs(&mut self) -> Vec<InputChange> {
        std::mem::take(&mut self.initial_inputs)
    }

    pub fn synchronize_paths(&mut self, paths: &[PathBuf]) -> Result<InputChanges, SessionError> {
        let change = self.synchronize(paths).map_err(SessionError)?;
        if !change.inputs.is_empty() {
            self.initial_report = None;
        }
        Ok(change.into_public())
    }

    pub fn rescan(&mut self) -> Result<InputChanges, SessionError> {
        let change = self.rescan_inputs().map_err(SessionError)?;
        if !change.inputs.is_empty() {
            self.initial_report = None;
        }
        Ok(change.into_public())
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
            change.combine(self.observe_source_path(&path)?);
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
            change.combine(self.observe_source_path(&path)?);
        }
        self.source_globs = walked.globs;
        self.source_paths = current_paths;
        Ok(change)
    }

    fn rebuild_inputs(&mut self) -> Result<RebuildOutcome, SessionFailure> {
        if let Some(report) = &mut self.initial_report {
            let result = compile::finish_initial(
                &self.compilation,
                report,
                &self.root,
                &self.output,
                self.color,
                self.diagnostics,
                &mut self.generated_outputs,
            )?;
            self.initial_report = None;
            return self.finish_rebuild(result);
        }
        if self.compilation.source_ids().next().is_none() {
            self.reconcile_outputs(BTreeSet::new())?;
            self.retired_modules.clear();
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
        self.finish_rebuild(result)
    }

    fn finish_rebuild(
        &mut self,
        result: compile::RebuildResult,
    ) -> Result<RebuildOutcome, SessionFailure> {
        match result.outcome {
            BuildOutcome::Succeeded => {
                self.reconcile_outputs(result.outputs)?;
                self.retired_modules.clear();
                Ok(RebuildOutcome::Succeeded)
            }
            BuildOutcome::Diagnostics => {
                self.reconcile_retired_outputs()?;
                self.retired_modules.clear();
                Ok(RebuildOutcome::Diagnostics)
            }
            BuildOutcome::NoInputs => {
                self.retired_modules.clear();
                Ok(RebuildOutcome::NoInputs)
            }
        }
    }

    fn observe_source_path(&mut self, path: &Path) -> Result<SessionChange, SessionFailure> {
        let previous_name = self.source_modules.get(path).cloned();
        let change = observe_source_unit(&mut self.compilation, path)?;
        if path.exists() {
            let unit = source_unit(path)?;
            if let Some(current_name) = self.compilation.module_name(unit.source())? {
                if let Some(previous_name) = previous_name
                    && previous_name != current_name
                {
                    self.retired_modules.insert(previous_name);
                }
                self.source_modules.insert(path.to_path_buf(), current_name);
            }
        } else if let Some(previous_name) = self.source_modules.remove(path) {
            self.retired_modules.insert(previous_name);
        }
        Ok(change)
    }

    fn reconcile_retired_outputs(&mut self) -> io::Result<()> {
        let active_modules = self.source_modules.values().cloned().collect::<BTreeSet<_>>();
        let retired_roots = self
            .retired_modules
            .difference(&active_modules)
            .map(|module| self.output.join(module))
            .collect_vec();
        let current_outputs = self
            .generated_outputs
            .iter()
            .filter(|output| !retired_roots.iter().any(|root| output.starts_with(root)))
            .cloned()
            .collect();
        self.reconcile_outputs(current_outputs)
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
    let mut lifecycle = compilation.observe_source(SourceUnitKey::clone(&unit), source, ());
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
