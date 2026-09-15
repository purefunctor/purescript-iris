use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{fs, io, str};

use async_lsp::ClientSocket;
use building::{Cancellation, DiskObservation, QueryError};
use configuration::{Configuration, SourceDiscovery};
use files::ForeignSourceKind;
use iris_build::compile::{InitialBuildConfig, PackageExecution, build_initial};
use iris_build::events::SilentBuildEvents;
use iris_build::plan::PackageInput;
use itertools::Itertools;
use smol_str::SmolStr;
use tokio::io::AsyncReadExt;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::task;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use super::analysis::SourceMetadata;
use super::error::LspError;
use super::process::ChildProcess;
use super::workspace::{
    PreparedInitialWorkspace, PreparedSource, PreparedSourceReconfiguration, SourceRoot,
};
use super::{DiscoveredWorkspace, source_unit_from_source_uri, source_uri, walk};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PreparationGeneration {
    value: u64,
}

impl PreparationGeneration {
    fn next(self) -> PreparationGeneration {
        let value = self
            .value
            .checked_add(1)
            .expect("invariant violated: preparation generation overflowed");
        PreparationGeneration { value }
    }
}

#[derive(Clone, Copy)]
pub(super) enum PreparationKind {
    Initial,
    Reconfiguration,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum PreparationOrigin {
    Client,
    Startup,
}

pub(super) struct PreparationInput {
    pub(super) root: PathBuf,
    pub(super) configuration: Arc<Configuration>,
    pub(super) kind: PreparationKind,
    pub(super) origin: PreparationOrigin,
}

pub(super) enum PreparedWorkspace {
    Initial(PreparedInitialWorkspace),
    Reconfiguration(PreparedSourceReconfiguration),
}

pub(super) struct WorkspacePrepared {
    pub(super) generation: PreparationGeneration,
    pub(super) origin: PreparationOrigin,
    pub(super) result: Result<PreparedWorkspace, LspError>,
}

struct PreparationCancellation {
    asynchronous: CancellationToken,
    compilation: Cancellation,
}

impl PreparationCancellation {
    fn new() -> PreparationCancellation {
        PreparationCancellation {
            asynchronous: CancellationToken::new(),
            compilation: Cancellation::default(),
        }
    }

    fn cancel(&self) {
        self.asynchronous.cancel();
        self.compilation.cancel();
    }
}

pub(super) struct Preparation {
    generation: PreparationGeneration,
    cancellation: Option<PreparationCancellation>,
    tasks: TaskTracker,
    blocking: Arc<Semaphore>,
}

impl Preparation {
    pub(super) fn new(tasks: TaskTracker) -> Preparation {
        Preparation {
            generation: PreparationGeneration { value: 0 },
            cancellation: None,
            tasks,
            blocking: Arc::new(Semaphore::new(1)),
        }
    }

    pub(super) fn admit(&mut self) -> PreparationGeneration {
        self.cancel();
        self.generation = self.generation.next();
        self.generation
    }

    pub(super) fn current(&self, generation: PreparationGeneration) -> bool {
        self.generation == generation
    }

    pub(super) fn start(
        &mut self,
        generation: PreparationGeneration,
        input: PreparationInput,
        client: ClientSocket,
    ) {
        if !self.current(generation) {
            return;
        }

        self.cancel();

        let cancellation = PreparationCancellation::new();
        let asynchronous = CancellationToken::clone(&cancellation.asynchronous);
        let compilation = Cancellation::clone(&cancellation.compilation);
        let origin = input.origin;
        let blocking = Arc::clone(&self.blocking);
        self.cancellation = Some(cancellation);

        self.tasks.spawn(async move {
            let task_cancellation = CancellationToken::clone(&asynchronous);
            let result = prepare(input, task_cancellation, compilation, blocking).await;
            if asynchronous.is_cancelled() {
                return;
            }

            let prepared = WorkspacePrepared { generation, origin, result };
            if let Err(error) = client.emit(prepared) {
                tracing::error!("Failed to deliver prepared workspace: {error}");
            }
        });
    }

    pub(super) fn cancel(&mut self) {
        if let Some(cancellation) = self.cancellation.take() {
            cancellation.cancel();
        }
    }
}

impl Drop for Preparation {
    fn drop(&mut self) {
        self.cancel();
    }
}

async fn prepare(
    input: PreparationInput,
    asynchronous: CancellationToken,
    compilation: Cancellation,
    blocking: Arc<Semaphore>,
) -> Result<PreparedWorkspace, LspError> {
    let PreparationInput { root, configuration, kind, origin: _ } = input;
    let discovered = discover(&root, &configuration, &asynchronous, Arc::clone(&blocking)).await?;

    if asynchronous.is_cancelled() {
        return Err(QueryError::Cancelled.into());
    }

    let permit = acquire_blocking(blocking, &asynchronous).await?;
    let worker = task::spawn_blocking(move || {
        let _permit = permit;
        match kind {
            PreparationKind::Initial => {
                prepare_initial(root, configuration, discovered, compilation)
                    .map(PreparedWorkspace::Initial)
            }
            PreparationKind::Reconfiguration => {
                prepare_reconfiguration(configuration, discovered, compilation)
                    .map(PreparedWorkspace::Reconfiguration)
            }
        }
    });
    worker.await.map_err(LspError::JoinError)?
}

async fn acquire_blocking(
    blocking: Arc<Semaphore>,
    cancellation: &CancellationToken,
) -> Result<OwnedSemaphorePermit, LspError> {
    tokio::select! {
        permit = blocking.acquire_owned() => {
            Ok(permit.expect("invariant violated: preparation semaphore was closed"))
        }
        () = cancellation.cancelled() => Err(QueryError::Cancelled.into()),
    }
}

async fn discover(
    root: &Path,
    configuration: &Configuration,
    cancellation: &CancellationToken,
    blocking: Arc<Semaphore>,
) -> Result<DiscoveredWorkspace, LspError> {
    match &configuration.sources {
        SourceDiscovery::Spago {} => {
            let permit = acquire_blocking(blocking, cancellation).await?;
            let root = root.to_path_buf();
            let mut worker = task::spawn_blocking(move || {
                let _permit = permit;
                super::discover_spago(&root)
            });

            tokio::select! {
                result = &mut worker => result.map_err(LspError::JoinError)?,
                () = cancellation.cancelled() => {
                    let _ = worker.await;
                    Err(QueryError::Cancelled.into())
                },
            }
        }
        SourceDiscovery::Command { program, arguments } => {
            discover_manual(root, program, arguments, cancellation, blocking).await
        }
    }
}

async fn discover_manual(
    root: &Path,
    program: &str,
    arguments: &[String],
    cancellation: &CancellationToken,
    blocking: Arc<Semaphore>,
) -> Result<DiscoveredWorkspace, LspError> {
    tracing::info!("Using '{}'", program);

    let output = run_source_command(program, arguments, cancellation).await?;
    if cancellation.is_cancelled() {
        return Err(QueryError::Cancelled.into());
    }

    let permit = acquire_blocking(blocking, cancellation).await?;
    let root = root.to_path_buf();
    let mut worker = task::spawn_blocking(move || {
        let _permit = permit;
        discover_manual_output(&root, &output)
    });

    tokio::select! {
        result = &mut worker => result.map_err(LspError::JoinError)?,
        () = cancellation.cancelled() => {
            let _ = worker.await;
            Err(QueryError::Cancelled.into())
        },
    }
}

fn discover_manual_output(root: &Path, output: &[u8]) -> Result<DiscoveredWorkspace, LspError> {
    let output = str::from_utf8(&output)?;
    let walk::Walk { files, .. } = walk::walk(root, output.lines())?;

    let metadata = files.iter().map(|file| {
        let editable = file.starts_with(root);
        (PathBuf::clone(file), SourceMetadata::Unmanaged { editable })
    });
    let metadata = metadata.collect();

    let package = PackageInput {
        name: SmolStr::new("unmanaged"),
        source_identities: Vec::clone(&files),
        dependencies: vec![],
    };

    let source_root = SourceRoot {
        path: root.to_path_buf(),
        metadata: SourceMetadata::Unmanaged { editable: true },
    };
    let discovered = DiscoveredWorkspace {
        source_globs: files,
        packages: vec![package],
        metadata,
        source_roots: vec![source_root],
    };

    Ok(discovered)
}

async fn run_source_command(
    program: &str,
    arguments: &[String],
    cancellation: &CancellationToken,
) -> Result<Vec<u8>, LspError> {
    let mut child = ChildProcess::spawn(program, arguments)?;

    let mut stdout = child.take_stdout().expect("invariant violated: source command has no stdout");
    let mut stderr = child.take_stderr().expect("invariant violated: source command has no stderr");
    let stdout = async move {
        let mut output = vec![];
        stdout.read_to_end(&mut output).await.map(|_| output)
    };
    let stderr = async move {
        let mut output = vec![];
        stderr.read_to_end(&mut output).await.map(|_| output)
    };

    let completed = {
        let command = async {
            let (status, output, error_output) = tokio::try_join!(child.wait(), stdout, stderr)?;
            Ok::<_, io::Error>((status, output, error_output))
        };
        tokio::pin!(command);

        tokio::select! {
            result = &mut command => Some(result),
            () = cancellation.cancelled() => None,
        }
    };

    let completed = match completed {
        Some(result) => result,
        None => {
            terminate_source_command(&mut child).await;
            return Err(QueryError::Cancelled.into());
        }
    };
    let (status, output, _error_output) = match completed {
        Ok(completed) => completed,
        Err(error) => {
            terminate_source_command(&mut child).await;
            return Err(error.into());
        }
    };

    if !status.success() {
        return Err(LspError::SourceCommandFailed(status));
    }

    Ok(output)
}

async fn terminate_source_command(child: &mut ChildProcess) {
    let _ = child.kill().await;
}

fn prepare_initial(
    root: PathBuf,
    configuration: Arc<Configuration>,
    discovered: DiscoveredWorkspace,
    cancellation: Cancellation,
) -> Result<PreparedInitialWorkspace, LspError> {
    let selected_sources = discovered.source_globs.iter().map(source_uri);
    let selected_sources = selected_sources.collect::<Result<_, _>>()?;

    let build = InitialBuildConfig {
        root: &root,
        source_globs: &discovered.source_globs,
        excluded: &[],
        packages: discovered.packages,
        prim_metadata: SourceMetadata::Builtin,
        source_metadata: |path: &Path| {
            let metadata = discovered
                .metadata
                .get(path)
                .expect("invariant violated: discovered source has no LSP metadata");
            SourceMetadata::clone(metadata)
        },
        execution: PackageExecution::Parallel,
        events: &SilentBuildEvents,
        cancellation,
    };
    let initial = build_initial::<i32, SourceMetadata, _>(build)?;

    let prepared = PreparedInitialWorkspace {
        configuration,
        compilation: initial.into_compilation(),
        source_roots: discovered.source_roots,
        selected_sources,
    };

    Ok(prepared)
}

fn prepare_reconfiguration(
    configuration: Arc<Configuration>,
    discovered: DiscoveredWorkspace,
    cancellation: Cancellation,
) -> Result<PreparedSourceReconfiguration, LspError> {
    let mut sources = BTreeMap::new();
    for path in &discovered.source_globs {
        cancellation.check()?;

        let content = Arc::from(fs::read_to_string(path)?);
        let metadata = discovered
            .metadata
            .get(path)
            .expect("invariant violated: discovered source has no LSP metadata");
        let metadata = SourceMetadata::clone(metadata);
        let uri = lsp_types::Url::from_file_path(path)
            .map_err(|_| LspError::PathParseFail(PathBuf::clone(path)))?;
        let unit = source_unit_from_source_uri(&uri)?;

        let foreign = ForeignSourceKind::ALL
            .into_iter()
            .map(|kind| prepare_foreign_observation(path, kind))
            .collect_vec();

        let locator = Arc::from(unit.source());
        let prepared = PreparedSource { unit, content, metadata, foreign };
        sources.insert(locator, prepared);
    }

    cancellation.check()?;

    Ok(PreparedSourceReconfiguration {
        configuration,
        source_roots: discovered.source_roots,
        sources,
    })
}

fn prepare_foreign_observation(
    source_path: &Path,
    kind: ForeignSourceKind,
) -> (ForeignSourceKind, DiskObservation) {
    let foreign_path = source_path.with_extension(kind.extension());
    let disk = match fs::read_to_string(foreign_path) {
        Ok(content) => DiskObservation::Found(Arc::from(content)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => DiskObservation::NotFound,
        Err(error) => {
            let kind = error.kind();
            let failure = building::ReloadFailure::new(kind, error.to_string());
            DiskObservation::Failed(failure)
        }
    };

    (kind, disk)
}
