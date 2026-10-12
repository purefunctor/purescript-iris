use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{fs, io};

use building::{
    DiskObservation, LifecycleChange, QueryEngine, QueryError, ReloadFailure, SourceUnitKey,
};
use files::{FileId, ForeignSourceKind};
use itertools::Itertools;
use prim_constants::MODULE_MAP;
use smol_str::SmolStr;
use thiserror::Error;
use url::Url;

use super::compilation::{CompilationState, SourceOwner};
use super::compile::{self, CompileError, InitialBuildReport};
use super::events::BuildOutcome;
use super::manifest::{BuildManifest, ProjectDescription};
use super::output::{self, BuildOutput};
use super::packages::{ConflictingSource, SourceOwnership};
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

/// A read-only view of a [`BuildSession`]'s query engine.
///
/// The engine is a snapshot: a change to the session's inputs waits until it is dropped.
pub struct SessionSnapshot {
    pub engine: QueryEngine,
    /// Every source file the engine knows, including the built-in Prim modules.
    pub files: Arc<BTreeMap<FileId, SourceFile>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceFile {
    pub path: PathBuf,
    pub kind: SourceKind,
}

/// Where a source file comes from, as the language server's `SourceMetadata` distinguishes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    /// A built-in Prim module, materialized in a temporary directory so that it can be read.
    Builtin,
    /// A source file of the project or of one of its dependencies.
    Project,
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
    #[error(transparent)]
    ConflictingSource(#[from] ConflictingSource),
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
    description: ProjectDescription,
    inputs: Vec<PathBuf>,
    source_roots: Vec<PathBuf>,
    ownership: SourceOwnership,
    source_paths: BTreeSet<PathBuf>,
    /// The last manifest the session wrote: its only record of the files it owns in the output
    /// directory. There is none while the session has no current build.
    record: Option<BuildManifest>,
    compilation: CompilationState,
    files: Arc<BTreeMap<FileId, SourceFile>>,
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
        let ownership = SourceOwnership::new(
            &project.root,
            project.packages.iter().map(|package| (&package.name, package.source_globs.as_slice())),
        )?;
        let initial = initialized.build.into_parts();
        let initial_inputs = initial.source_paths.iter().map(|path| {
            let source_path = PathBuf::clone(path);
            let unit = source_unit(path)?;
            let module_name = initial.compilation.module_name(unit.source())?;
            Ok::<_, SessionFailure>(InputChange { source_path, module_name })
        });
        let initial_inputs = initial_inputs.process_results(|inputs| inputs.collect_vec())?;
        let files = Arc::new(source_files(&initial.compilation)?);
        Ok(BuildSession {
            root: project.root,
            output: project.output,
            description: project.description,
            inputs: project.source_globs,
            source_roots,
            ownership,
            source_paths: initial.source_paths,
            record: None,
            compilation: initial.compilation,
            files,
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

    pub fn snapshot(&self) -> SessionSnapshot {
        SessionSnapshot { engine: self.compilation.snapshot(), files: Arc::clone(&self.files) }
    }

    pub fn synchronize_paths(&mut self, paths: &[PathBuf]) -> Result<InputChanges, SessionError> {
        let change = self.synchronize(paths).map_err(SessionError)?;
        self.finish_change(change).map_err(SessionError)
    }

    pub fn rescan(&mut self) -> Result<InputChanges, SessionError> {
        let change = self.rescan_inputs().map_err(SessionError)?;
        self.finish_change(change).map_err(SessionError)
    }

    fn finish_change(&mut self, change: SessionChange) -> Result<InputChanges, SessionFailure> {
        if !change.inputs.is_empty() {
            self.initial_report = None;
            self.files = Arc::new(source_files(&self.compilation)?);
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
                    source_paths.insert(PathBuf::clone(path));
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
        let sources = self.assign_sources(source_paths)?;

        let mut change = SessionChange::default();
        for (path, owner) in sources {
            change.combine(observe_source_unit(&mut self.compilation, &path, owner)?);
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
        let affected_paths = self.source_paths.union(&current_paths).cloned();
        let sources = self.assign_sources(affected_paths)?;

        let mut change = SessionChange::default();
        for (path, owner) in sources {
            change.combine(observe_source_unit(&mut self.compilation, &path, owner)?);
        }
        self.source_paths = current_paths;
        Ok(change)
    }

    /// The package of every path that a package's source globs match, resolved before any of
    /// them is observed so that a conflict leaves the session as it was.
    fn assign_sources(
        &self,
        paths: impl IntoIterator<Item = PathBuf>,
    ) -> Result<Vec<(PathBuf, SmolStr)>, SessionFailure> {
        let mut sources = Vec::new();
        for path in paths {
            match self.ownership.owner(&path) {
                Ok(Some(owner)) => sources.push((path, SmolStr::clone(owner))),
                Ok(None) => {}
                // A path two packages claim is only a conflict while a file exists there.
                Err(_) if !path.exists() => {}
                Err(conflict) => return Err(conflict.into()),
            }
        }
        Ok(sources)
    }

    fn rebuild_inputs(&mut self) -> Result<RebuildOutcome, SessionFailure> {
        let result = if let Some(report) = &mut self.initial_report {
            let result = compile::finish_initial(
                &self.compilation,
                report,
                &self.root,
                self.color,
                self.diagnostics,
            )?;
            self.initial_report = None;
            result
        } else if self.compilation.source_ids().next().is_none() {
            compile::RebuildResult { outcome: BuildOutcome::NoInputs, modules: Vec::new() }
        } else {
            compile::rebuild(&self.compilation, &self.root, self.color, self.diagnostics)?
        };
        self.finish_rebuild(result)
    }

    /// Brings the output directory to the state of the rebuild, and records it. A failed update
    /// leaves the record as it was, since it puts the files it changed back.
    fn finish_rebuild(
        &mut self,
        result: compile::RebuildResult,
    ) -> Result<RebuildOutcome, SessionFailure> {
        let next = match result.outcome {
            BuildOutcome::Succeeded => {
                Some(BuildOutput::new(&self.description, &self.compilation, &result.modules)?)
            }
            BuildOutcome::Diagnostics => {
                self.record.as_ref().map(|record| BuildOutput::retained(record, &self.compilation))
            }
            BuildOutcome::NoInputs => None,
        };
        output::update(&self.output, self.record.as_ref(), next.as_ref())?;
        self.record = next.map(|next| next.manifest);
        Ok(match result.outcome {
            BuildOutcome::Succeeded => RebuildOutcome::Succeeded,
            BuildOutcome::Diagnostics => RebuildOutcome::Diagnostics,
            BuildOutcome::NoInputs => RebuildOutcome::NoInputs,
        })
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
    owner: SmolStr,
) -> Result<SessionChange, SessionFailure> {
    let unit = source_unit(source_path)?;
    let previous_source = compilation.source_content(unit.source())?;
    let previous_foreign =
        ForeignSourceKind::ALL.map(|kind| compilation.foreign_content(unit.foreign_for(kind)));
    let previous_name = compilation.module_name(unit.source())?;

    let source = observe_disk(source_path);
    let owner = SourceOwner::Package(owner);
    let mut lifecycle = compilation.observe_source(SourceUnitKey::clone(&unit), source, owner);
    for kind in ForeignSourceKind::ALL {
        let foreign = observe_disk(&source_path.with_extension(kind.extension()));
        lifecycle.combine(compilation.observe_foreign(SourceUnitKey::clone(&unit), kind, foreign));
    }

    let current_source = compilation.source_content(unit.source())?;
    let current_foreign =
        ForeignSourceKind::ALL.map(|kind| compilation.foreign_content(unit.foreign_for(kind)));
    let mut inputs = Vec::new();
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
    let mut inputs = Vec::new();
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

fn source_files(
    compilation: &CompilationState,
) -> Result<BTreeMap<FileId, SourceFile>, SessionFailure> {
    let engine = compilation.query_engine();
    let builtin = MODULE_MAP.iter().filter_map(|(name, _)| engine.module_file(name));
    let builtin = builtin.map(|file_id| (file_id, SourceKind::Builtin));
    let project = compilation.source_ids().map(|file_id| (file_id, SourceKind::Project));
    let files = project.chain(builtin).map(|(file_id, kind)| {
        let locator = compilation
            .source_path(file_id)
            .expect("invariant violated: a source file has no lifecycle path");
        let path = Url::parse(&locator)
            .ok()
            .and_then(|url| url.to_file_path().ok())
            .ok_or_else(|| SessionFailure::InvalidPath(PathBuf::from(&*locator)))?;
        Ok((file_id, SourceFile { path, kind }))
    });
    files.collect()
}

fn observe_disk(path: &Path) -> DiskObservation {
    match fs::read_to_string(path) {
        Ok(content) => DiskObservation::Found(content.into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => DiskObservation::NotFound,
        Err(error) => DiskObservation::Failed(ReloadFailure::new(error.kind(), error.to_string())),
    }
}
