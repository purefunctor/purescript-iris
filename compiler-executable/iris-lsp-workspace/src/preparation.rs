//! Workspace preparation: Spago fetch, initial build, progress messages, and retry.
//!
//! Each preparation attempt discovers the Spago workspace, runs `spago fetch` in its root, then
//! runs the `iris-build` discovery and initial compilation. Attempts run as background tasks and
//! report completion to the workspace actor. The control task can cancel the current attempt, or
//! stop preparation for shutdown, while the workspace actor is busy. A cancelled attempt is retired
//! before a request can start its replacement, so two Spago processes never overlap.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::{io, mem};

use iris_build::compile::CompileError;
use iris_build::events::{BuildEvent, BuildEventSink};
use iris_build::{PackagesError, Workspace, WorkspaceError};
use iris_lsp_server::{WorkspaceEvent, WorkspaceEventSender};
use iris_spago::{SpagoCommand, SpagoError};
use parking_lot::Mutex;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::sync::{mpsc, watch};
use tokio::task;

use crate::discovery::build_prepared_workspace;
use crate::process::ProcessTree;
use crate::service::Background;
use crate::state::PreparedWorkspace;

const TITLE: &str = "Preparing Iris workspace";
const DISCOVERING: &str = "Discovering Spago workspace";
const FETCHING: &str = "Fetching dependencies with spago fetch";
const COMPILING: &str = "Discovering packages and preparing compilation";
pub(crate) const FINISHED: &str = "Workspace preparation finished";
pub(crate) const FAILED: &str = "Workspace preparation failed";
const CANCELLED: &str = "Workspace preparation cancelled";

#[derive(Error, Debug)]
pub(crate) enum PreparationError {
    #[error("CompileError: {0}")]
    CompileError(#[from] CompileError),
    #[error("Iris workspace preparation was cancelled")]
    Cancelled,
    #[error("WorkspaceError: {0}")]
    WorkspaceError(#[from] WorkspaceError),
    #[error("PackagesError: {0}")]
    PackagesError(#[from] PackagesError),
    #[error("SpagoError: {0}")]
    SpagoError(#[from] SpagoError),
    #[error("IoError: {0}")]
    IoError(#[from] io::Error),
    #[error("JoinError: {0}")]
    JoinError(#[from] task::JoinError),
}

/// The state of preparation, shared by the workspace actor and the control task.
pub(crate) struct Preparation {
    inner: Mutex<Inner>,
    events: WorkspaceEventSender,
    background: mpsc::UnboundedSender<Background>,
    prepare: Prepare,
}

/// Runs one preparation attempt: [`prepare`], or a stand-in whose outcome a test controls.
pub(crate) type Prepare =
    fn(
        PathBuf,
        watch::Receiver<bool>,
        ProgressSink,
    ) -> Pin<Box<dyn Future<Output = Result<PreparedWorkspace, PreparationError>> + Send>>;

struct Inner {
    state: PreparationState,
    root: Option<PathBuf>,
    /// Tasks of attempts that no longer affect the workspace but may still be running.
    retired: Vec<task::JoinHandle<()>>,
}

enum PreparationState {
    NotStarted,
    Running(Attempt),
    /// The attempt was cancelled and has not returned yet. `retry` is the generation a request
    /// reserved while waiting; it starts once the cancelled attempt returns.
    Cancelling {
        attempt: Attempt,
        retry: Option<u64>,
    },
    /// The attempt `generation` was cancelled and returned; a request starts the next one.
    Paused {
        generation: u64,
    },
    Finished,
    Failed,
    ShuttingDown,
}

struct Attempt {
    generation: u64,
    cancel: watch::Sender<bool>,
    task: task::JoinHandle<()>,
}

/// How a request that waits for attempt `generation` should proceed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Pending,
    Cancelled,
    ShuttingDown,
}

/// What the workspace actor does with the result of a finished attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Disposition {
    /// The attempt is current: install its result, or record its failure.
    Apply,
    /// The attempt was cancelled; its result is dropped.
    Discard,
}

impl Preparation {
    pub(crate) fn new(
        events: WorkspaceEventSender,
        background: mpsc::UnboundedSender<Background>,
    ) -> Preparation {
        Preparation::with_prepare(events, background, |root, cancel, progress| {
            Box::pin(prepare(root, cancel, progress))
        })
    }

    pub(crate) fn with_prepare(
        events: WorkspaceEventSender,
        background: mpsc::UnboundedSender<Background>,
        prepare: Prepare,
    ) -> Preparation {
        let inner = Inner { state: PreparationState::NotStarted, root: None, retired: Vec::new() };
        Preparation { inner: Mutex::new(inner), events, background, prepare }
    }

    /// Records a workspace that needs no preparation.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn finished(
        events: WorkspaceEventSender,
        background: mpsc::UnboundedSender<Background>,
    ) -> Preparation {
        let preparation = Preparation::new(events, background);
        preparation.inner.lock().state = PreparationState::Finished;
        preparation
    }

    /// Starts the first attempt, or does nothing if preparation already started.
    pub(crate) fn start(&self, root: PathBuf) -> bool {
        let mut inner = self.inner.lock();
        if !matches!(inner.state, PreparationState::NotStarted) {
            return false;
        }
        tracing::info!("Preparing the Spago workspace at {}.", root.display());
        inner.root = Some(root);
        let attempt = self.start_attempt(&inner, 1);
        inner.state = PreparationState::Running(attempt);
        true
    }

    /// Returns the attempt a request should wait for, starting a retry after a cancellation.
    ///
    /// Returns `None` when no attempt can make the workspace ready: before preparation started,
    /// and after it finished, failed, or stopped for shutdown.
    pub(crate) fn demand(&self) -> Option<u64> {
        let mut inner = self.inner.lock();
        let state = mem::replace(&mut inner.state, PreparationState::NotStarted);
        let (state, generation) = match state {
            PreparationState::Running(attempt) => {
                let generation = attempt.generation;
                (PreparationState::Running(attempt), Some(generation))
            }
            PreparationState::Cancelling { attempt, retry } => {
                let retry = retry.unwrap_or(attempt.generation + 1);
                (PreparationState::Cancelling { attempt, retry: Some(retry) }, Some(retry))
            }
            PreparationState::Paused { generation } => {
                let attempt = self.start_attempt(&inner, generation + 1);
                let generation = attempt.generation;
                (PreparationState::Running(attempt), Some(generation))
            }
            state => (state, None),
        };
        inner.state = state;
        generation
    }

    pub(crate) fn outcome(&self, generation: u64) -> Outcome {
        let inner = self.inner.lock();
        match &inner.state {
            PreparationState::ShuttingDown => Outcome::ShuttingDown,
            PreparationState::Running(attempt) if attempt.generation == generation => {
                Outcome::Pending
            }
            PreparationState::Cancelling { retry: Some(retry), .. } if *retry == generation => {
                Outcome::Pending
            }
            // The workspace actor installs or fails the workspace in the same step that commits
            // the attempt, so waiting requests read the result from the workspace state.
            PreparationState::Finished | PreparationState::Failed => Outcome::Pending,
            PreparationState::NotStarted
            | PreparationState::Running(_)
            | PreparationState::Cancelling { .. }
            | PreparationState::Paused { .. } => Outcome::Cancelled,
        }
    }

    /// Commits the result of attempt `generation`, or returns `None` if the attempt is stale.
    ///
    /// Committing a current attempt records whether it succeeded, so a later cancellation of
    /// its progress no longer affects it. Committing a cancelled attempt retires it and starts a
    /// retry a request reserved.
    pub(crate) fn finish(&self, generation: u64, succeeded: bool) -> Option<Disposition> {
        let mut inner = self.inner.lock();
        let state = mem::replace(&mut inner.state, PreparationState::NotStarted);
        let (state, disposition) = match state {
            PreparationState::Running(attempt) if attempt.generation == generation => {
                inner.retired.push(attempt.task);
                let state =
                    if succeeded { PreparationState::Finished } else { PreparationState::Failed };
                (state, Some(Disposition::Apply))
            }
            PreparationState::Cancelling { attempt, retry } if attempt.generation == generation => {
                inner.retired.push(attempt.task);
                let state = match retry {
                    Some(retry) => PreparationState::Running(self.start_attempt(&inner, retry)),
                    None => PreparationState::Paused { generation },
                };
                (state, Some(Disposition::Discard))
            }
            state => (state, None),
        };
        inner.state = state;
        inner.retired.retain(|task| !task.is_finished());
        disposition
    }

    /// Cancels attempt `generation` for the editor. Returns `false` if it is not running.
    pub(crate) fn cancel(&self, generation: u64) -> bool {
        let mut inner = self.inner.lock();
        let state = mem::replace(&mut inner.state, PreparationState::NotStarted);
        let (state, cancelled) = match state {
            PreparationState::Running(attempt) if attempt.generation == generation => {
                let _ = attempt.cancel.send(true);
                self.ended(generation, CANCELLED);
                (PreparationState::Cancelling { attempt, retry: None }, true)
            }
            state => (state, false),
        };
        inner.state = state;
        cancelled
    }

    /// Cancels the running attempt and stops preparation for the rest of the session.
    pub(crate) fn shut_down(&self) {
        let mut inner = self.inner.lock();
        let state = mem::replace(&mut inner.state, PreparationState::ShuttingDown);
        match state {
            PreparationState::Running(attempt) => {
                let _ = attempt.cancel.send(true);
                self.ended(attempt.generation, CANCELLED);
                inner.retired.push(attempt.task);
            }
            // Its progress already ended when it was cancelled.
            PreparationState::Cancelling { attempt, .. } => inner.retired.push(attempt.task),
            _ => {}
        }
    }

    /// Stops preparation and returns every attempt task that may still be running, so cleanup
    /// can wait for Spago process trees to be reaped and for blocking work to return.
    pub(crate) fn close(&self) -> Vec<task::JoinHandle<()>> {
        let mut inner = self.inner.lock();
        let state = mem::replace(&mut inner.state, PreparationState::ShuttingDown);
        let mut tasks = mem::take(&mut inner.retired);
        match state {
            PreparationState::Running(attempt) | PreparationState::Cancelling { attempt, .. } => {
                let _ = attempt.cancel.send(true);
                tasks.push(attempt.task);
            }
            _ => {}
        }
        tasks
    }

    pub(crate) fn ended(&self, generation: u64, message: &str) {
        self.events
            .send(WorkspaceEvent::PreparationEnded { generation, message: message.to_string() });
    }

    fn start_attempt(&self, inner: &Inner, generation: u64) -> Attempt {
        let root = PathBuf::clone(inner.root.as_ref().expect("invariant violated: no root"));
        let (cancel, cancelled) = watch::channel(false);
        self.events.send(WorkspaceEvent::PreparationStarted {
            generation,
            title: TITLE.to_string(),
            message: DISCOVERING.to_string(),
        });
        let events = WorkspaceEventSender::clone(&self.events);
        let background = mpsc::UnboundedSender::clone(&self.background);
        let prepare = self.prepare;
        let task = task::spawn(async move {
            let progress = ProgressSink::new(generation, events);
            let result = prepare(root, cancelled, progress).await.map(Box::new);
            let _ = background.send(Background::PreparationFinished { generation, result });
        });
        Attempt { generation, cancel, task }
    }
}

async fn prepare(
    root: PathBuf,
    mut cancel: watch::Receiver<bool>,
    progress: ProgressSink,
) -> Result<PreparedWorkspace, PreparationError> {
    let client_root = PathBuf::clone(&root);
    let workspace = task::spawn_blocking(move || Workspace::discover(&root, None)).await??;
    if *cancel.borrow() {
        return Err(PreparationError::Cancelled);
    }
    progress.report(FETCHING);
    let spago = SpagoCommand::new(&workspace.root)?;
    fetch(&spago, workspace.selected.as_deref(), &mut cancel).await?;
    if *cancel.borrow() {
        return Err(PreparationError::Cancelled);
    }
    progress.report(COMPILING);
    let prepared = task::spawn_blocking(move || {
        if *cancel.borrow() {
            return Err(PreparationError::Cancelled);
        }
        build_prepared_workspace(workspace, client_root, &progress)
    })
    .await??;
    Ok(prepared)
}

/// Runs `spago fetch` in the discovered workspace root.
///
/// Both output streams are drained concurrently so a chatty Spago cannot fill a pipe, and so its
/// output can never reach the LSP protocol stream. On cancellation the process tree is killed,
/// reaped, and drained before returning.
async fn fetch(
    spago: &SpagoCommand,
    selected: Option<&str>,
    cancel: &mut watch::Receiver<bool>,
) -> Result<(), PreparationError> {
    let mut process =
        ProcessTree::spawn(spago.fetch_command(selected)).map_err(SpagoError::Execute)?;

    let stdout = task::spawn(drain(process.take_standard_output()));
    let stderr = task::spawn(drain(process.take_standard_error()));

    let status = loop {
        tokio::select! {
            status = process.wait() => break status,
            changed = cancel.changed() => {
                if changed.is_err() || *cancel.borrow() {
                    terminate(&mut process).await;
                    let _ = await_drains(stdout, stderr).await;
                    return Err(PreparationError::Cancelled);
                }
            }
        }
    };
    let status = match status {
        Ok(status) => status,
        Err(error) => {
            terminate(&mut process).await;
            let _ = await_drains(stdout, stderr).await;
            return Err(SpagoError::Execute(error).into());
        }
    };
    let mut drains = Box::pin(await_drains(stdout, stderr));
    let (_, stderr) = loop {
        tokio::select! {
            output = &mut drains => break output?,
            changed = cancel.changed() => {
                if changed.is_err() || *cancel.borrow() {
                    terminate(&mut process).await;
                    let _ = drains.await;
                    return Err(PreparationError::Cancelled);
                }
            }
        }
    };
    if status.success() {
        return Ok(());
    }
    Err(SpagoError::failed("fetch", status, &stderr).into())
}

async fn terminate(process: &mut ProcessTree) {
    if let Err(error) = process.terminate().await {
        tracing::warn!("Failed to terminate Spago process tree: {error}");
    }
}

async fn await_drains(
    stdout: task::JoinHandle<io::Result<Vec<u8>>>,
    stderr: task::JoinHandle<io::Result<Vec<u8>>>,
) -> Result<(Vec<u8>, Vec<u8>), PreparationError> {
    let (stdout, stderr) = tokio::join!(stdout, stderr);
    Ok((stdout??, stderr??))
}

async fn drain<R>(pipe: Option<R>) -> io::Result<Vec<u8>>
where
    R: AsyncRead + Unpin,
{
    let Some(mut pipe) = pipe else {
        return Ok(Vec::new());
    };
    let mut output = Vec::new();
    pipe.read_to_end(&mut output).await?;
    Ok(output)
}

/// Reports the stages of one attempt as `WorkspaceEvent::PreparationProgress`.
pub(crate) struct ProgressSink {
    generation: u64,
    events: WorkspaceEventSender,
    build: Mutex<BuildProgress>,
}

/// Turns build events into progress messages with monotonic percentages.
#[derive(Default)]
struct BuildProgress {
    package_count: Option<usize>,
    completed_count: usize,
    percentage: u32,
}

impl ProgressSink {
    fn new(generation: u64, events: WorkspaceEventSender) -> ProgressSink {
        ProgressSink { generation, events, build: Mutex::new(BuildProgress::default()) }
    }

    fn report(&self, message: &str) {
        self.send(message.to_string(), 0);
    }

    fn send(&self, message: String, percentage: u32) {
        self.events.send(WorkspaceEvent::PreparationProgress {
            generation: self.generation,
            message,
            percentage: Some(percentage),
        });
    }
}

impl BuildEventSink for ProgressSink {
    fn send(&self, event: BuildEvent) {
        let report = self.build.lock().apply(event);
        if let Some((message, percentage)) = report {
            ProgressSink::send(self, message, percentage);
        }
    }
}

impl BuildProgress {
    fn apply(&mut self, event: BuildEvent) -> Option<(String, u32)> {
        match event {
            BuildEvent::PlanReady { package_count } => {
                self.package_count = Some(package_count);
                self.completed_count = 0;
                self.percentage = if package_count == 0 { 100 } else { 0 };
                let message = if package_count == 0 {
                    "No packages to compile".to_string()
                } else {
                    format!("Compiling packages (0/{package_count} completed)")
                };
                Some((message, self.percentage))
            }
            BuildEvent::PackageCompleted { package_name, duration: _ } => {
                let package_count = self.package_count?;
                self.completed_count += 1;
                self.percentage = if package_count == 0 {
                    100
                } else {
                    self.completed_count.saturating_mul(100).saturating_div(package_count).min(100)
                        as u32
                };
                let message = format!(
                    "Completed {package_name} ({}/{} packages)",
                    self.completed_count, package_count
                );
                Some((message, self.percentage))
            }
            BuildEvent::Finalizing { duration: _ } => {
                Some(("Finalizing initial compilation".to_string(), self.percentage))
            }
            BuildEvent::Preparing | BuildEvent::Finished { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn build_events_report_completed_packages_with_monotonic_percentages() {
        let mut progress = BuildProgress::default();
        let plan = progress.apply(BuildEvent::PlanReady { package_count: 3 }).unwrap();
        assert_eq!(plan, ("Compiling packages (0/3 completed)".to_string(), 0));

        for (package_name, completed, percentage) in
            [("alpha", 1, 33), ("beta", 2, 66), ("gamma", 3, 100)]
        {
            let report = progress
                .apply(BuildEvent::PackageCompleted {
                    package_name: package_name.into(),
                    duration: Duration::ZERO,
                })
                .unwrap();
            assert_eq!(
                report,
                (format!("Completed {package_name} ({completed}/3 packages)"), percentage)
            );
        }

        let finalizing =
            progress.apply(BuildEvent::Finalizing { duration: Duration::ZERO }).unwrap();
        assert_eq!(finalizing, ("Finalizing initial compilation".to_string(), 100));
    }

    #[test]
    fn empty_build_plan_reports_completion_without_package_events() {
        let mut progress = BuildProgress::default();
        let report = progress.apply(BuildEvent::PlanReady { package_count: 0 }).unwrap();
        assert_eq!(report, ("No packages to compile".to_string(), 100));
    }
}
