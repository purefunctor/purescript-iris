//! Startup preparation of the Spago workspace.
//!
//! Preparation is one operation per server session: discover the Spago
//! workspace, run `spago fetch` in its root, then run the existing `iris-build`
//! discovery and initial compilation. It runs off the protocol loop so that
//! document notifications keep arriving and queue while it is in flight, and
//! it owns the fetch subprocess so that shutdown can terminate and drain it.

use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_lsp::{ClientSocket, LanguageClient};
use iris_build::Workspace;
use iris_build::events::{BuildEvent, BuildEventSink};
use iris_spago::{SpagoCommand, SpagoError};
use lsp_types::{
    NumberOrString, ProgressParams, ProgressParamsValue, ProgressToken, WorkDoneProgress,
    WorkDoneProgressBegin, WorkDoneProgressCreateParams, WorkDoneProgressEnd,
    WorkDoneProgressReport,
};
use parking_lot::Mutex;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::sync::watch;
use tokio::task;
use tokio::time::timeout;

use super::error::LspError;
use super::process::ProcessTree;
use super::workspace::PreparedInitialWorkspace;

/// Delivered through the server event loop when preparation finishes.
pub(super) struct PreparationFinished {
    pub(super) generation: u64,
    pub(super) result: Result<PreparedInitialWorkspace, LspError>,
}

/// Owns the single startup preparation task and its fetch subprocess.
pub(super) struct Preparation {
    inner: Mutex<PreparationInner>,
}

struct PreparationInner {
    generation: u64,
    started: bool,
    cancelled: bool,
    cancel: watch::Sender<bool>,
    progress: Option<Arc<StartupProgress>>,
    task: Option<task::JoinHandle<()>>,
}

impl Preparation {
    pub(super) fn new() -> Preparation {
        Preparation {
            inner: Mutex::new(PreparationInner {
                generation: 0,
                started: false,
                cancelled: false,
                cancel: watch::channel(false).0,
                progress: None,
                task: None,
            }),
        }
    }

    /// Starts the one startup preparation, or does nothing if it already ran.
    ///
    /// Returns the generation that identifies the resulting completion event.
    pub(super) fn start(
        &self,
        root: PathBuf,
        client: ClientSocket,
        work_done_progress: bool,
    ) -> Option<u64> {
        let mut inner = self.inner.lock();
        if inner.started || inner.cancelled {
            return None;
        }
        inner.started = true;
        inner.generation = inner.generation.wrapping_add(1);
        let generation = inner.generation;
        let cancel = inner.cancel.subscribe();
        let progress = work_done_progress.then(|| {
            Arc::new(StartupProgress::new(
                ClientSocket::clone(&client),
                NumberOrString::String(format!("iris/startup/{generation}")),
            ))
        });
        inner.progress.clone_from(&progress);
        inner.task = Some(task::spawn(run(root, generation, cancel, client, progress)));
        Some(generation)
    }

    pub(super) fn is_current(&self, generation: u64) -> bool {
        let inner = self.inner.lock();
        inner.started && !inner.cancelled && inner.generation == generation
    }

    /// Marks preparation as started without spawning a task.
    ///
    /// Tests use this to drive completion events directly, without running a
    /// real Spago process.
    #[cfg(test)]
    pub(super) fn test_arm(&self) -> u64 {
        let mut inner = self.inner.lock();
        inner.started = true;
        inner.generation = inner.generation.wrapping_add(1);
        inner.generation
    }

    /// Stops preparation from producing more protocol-visible effects.
    pub(super) fn cancel(&self) {
        let progress = {
            let mut inner = self.inner.lock();
            inner.cancelled = true;
            let _ = inner.cancel.send(true);
            Option::clone(&inner.progress)
        };
        if let Some(progress) = progress {
            progress.end("Workspace preparation cancelled");
        }
    }

    /// Cancels preparation, terminates the fetch subprocess, and joins every
    /// owned task, including the blocking initial compilation.
    pub(super) async fn shutdown(&self) {
        self.cancel();
        let task = {
            let mut inner = self.inner.lock();
            inner.task.take()
        };
        if let Some(task) = task {
            let _ = task.await;
        }
    }
}

async fn run(
    root: PathBuf,
    generation: u64,
    mut cancel: watch::Receiver<bool>,
    client: ClientSocket,
    progress: Option<Arc<StartupProgress>>,
) {
    if let Some(progress) = &progress {
        progress.begin(&mut cancel).await;
    }
    if *cancel.borrow() {
        return;
    }
    let result = prepare(root, &mut cancel, Option::clone(&progress)).await;
    if *cancel.borrow() {
        return;
    }
    if let Some(progress) = &progress {
        let message = if result.is_ok() {
            "Workspace preparation finished"
        } else {
            "Workspace preparation failed"
        };
        progress.end(message);
    }
    if let Err(error) = client.emit(PreparationFinished { generation, result }) {
        LspError::from(error).emit_trace();
    }
}

async fn prepare(
    root: PathBuf,
    cancel: &mut watch::Receiver<bool>,
    progress: Option<Arc<StartupProgress>>,
) -> Result<PreparedInitialWorkspace, LspError> {
    let client_root = PathBuf::clone(&root);
    let workspace = task::spawn_blocking(move || Workspace::discover(&root, None))
        .await
        .map_err(LspError::JoinError)??;
    if *cancel.borrow() {
        return Err(LspError::WorkspaceFailed);
    }
    if let Some(progress) = &progress {
        progress.report_message("Fetching dependencies with spago fetch", 0);
    }
    let spago = SpagoCommand::new(&workspace.root)?;
    fetch(&spago, workspace.selected.as_deref(), cancel).await?;
    if *cancel.borrow() {
        return Err(LspError::WorkspaceFailed);
    }
    if let Some(progress) = &progress {
        progress.report_message("Discovering packages and preparing compilation", 0);
    }
    let prepared = task::spawn_blocking(move || match progress {
        Some(progress) => {
            super::build_prepared_workspace(workspace, client_root, progress.as_ref())
        }
        None => super::build_prepared_workspace(
            workspace,
            client_root,
            &iris_build::events::SilentBuildEvents,
        ),
    })
    .await
    .map_err(LspError::JoinError)??;
    Ok(prepared)
}

struct StartupProgress {
    client: ClientSocket,
    token: ProgressToken,
    state: Mutex<StartupProgressState>,
}

enum StartupProgressState {
    Pending,
    Active(ActiveProgress),
    Closed,
}

#[derive(Default)]
struct ActiveProgress {
    package_count: Option<usize>,
    completed_count: usize,
    percentage: u32,
}

impl StartupProgress {
    fn new(client: ClientSocket, token: ProgressToken) -> StartupProgress {
        StartupProgress { client, token, state: Mutex::new(StartupProgressState::Pending) }
    }

    async fn begin(&self, cancel: &mut watch::Receiver<bool>) {
        let mut client = ClientSocket::clone(&self.client);
        let creation = timeout(
            Duration::from_secs(1),
            client.work_done_progress_create(WorkDoneProgressCreateParams {
                token: self.token.clone(),
            }),
        );
        let created = tokio::select! {
            result = creation => match result {
                Ok(Ok(())) => true,
                Ok(Err(error)) => {
                    tracing::warn!("Failed to create workspace preparation progress: {error}");
                    false
                }
                Err(_) => {
                    tracing::warn!("Timed out creating workspace preparation progress");
                    false
                }
            },
            changed = cancel.changed() => {
                let _ = changed;
                false
            }
        };

        let mut state = self.state.lock();
        if !created || *cancel.borrow() {
            *state = StartupProgressState::Closed;
            return;
        }
        if !matches!(*state, StartupProgressState::Pending) {
            return;
        }
        let begin = WorkDoneProgressBegin {
            title: "Preparing Iris workspace".to_string(),
            cancellable: Some(false),
            message: Some("Discovering Spago workspace".to_string()),
            percentage: Some(0),
        };
        if self.notify(WorkDoneProgress::Begin(begin)).is_ok() {
            *state = StartupProgressState::Active(ActiveProgress::default());
        } else {
            *state = StartupProgressState::Closed;
        }
    }

    fn report_message(&self, message: &str, percentage: u32) {
        let mut state = self.state.lock();
        let StartupProgressState::Active(active) = &mut *state else { return };
        active.percentage = percentage;
        let report = WorkDoneProgressReport {
            cancellable: Some(false),
            message: Some(message.to_string()),
            percentage: Some(percentage),
        };
        if self.notify(WorkDoneProgress::Report(report)).is_err() {
            *state = StartupProgressState::Closed;
        }
    }

    fn end(&self, message: &str) {
        let mut state = self.state.lock();
        if !matches!(*state, StartupProgressState::Active(_)) {
            *state = StartupProgressState::Closed;
            return;
        }
        *state = StartupProgressState::Closed;
        let end = WorkDoneProgressEnd { message: Some(message.to_string()) };
        let _ = self.notify(WorkDoneProgress::End(end));
    }

    fn notify(&self, progress: WorkDoneProgress) -> async_lsp::Result<()> {
        let mut client = ClientSocket::clone(&self.client);
        let result = client.progress(ProgressParams {
            token: self.token.clone(),
            value: ProgressParamsValue::WorkDone(progress),
        });
        if let Err(error) = &result {
            tracing::warn!("Failed to report workspace preparation progress: {error}");
        }
        result
    }
}

impl BuildEventSink for StartupProgress {
    fn send(&self, event: BuildEvent) {
        let mut state = self.state.lock();
        let StartupProgressState::Active(active) = &mut *state else { return };
        let Some(report) = active.apply(event) else { return };
        if self.notify(WorkDoneProgress::Report(report)).is_err() {
            *state = StartupProgressState::Closed;
        }
    }
}

impl ActiveProgress {
    fn apply(&mut self, event: BuildEvent) -> Option<WorkDoneProgressReport> {
        let (message, percentage) = match event {
            BuildEvent::PlanReady { package_count } => {
                self.package_count = Some(package_count);
                self.completed_count = 0;
                self.percentage = if package_count == 0 { 100 } else { 0 };
                let message = if package_count == 0 {
                    "No packages to compile".to_string()
                } else {
                    format!("Compiling packages (0/{package_count} completed)")
                };
                (message, self.percentage)
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
                (
                    format!(
                        "Completed {package_name} ({}/{} packages)",
                        self.completed_count, package_count
                    ),
                    self.percentage,
                )
            }
            BuildEvent::Finalizing { duration: _ } => {
                ("Finalizing initial compilation".to_string(), self.percentage)
            }
            BuildEvent::Preparing | BuildEvent::Finished { .. } => return None,
        };
        Some(WorkDoneProgressReport {
            cancellable: Some(false),
            message: Some(message),
            percentage: Some(percentage),
        })
    }
}

/// Runs `spago fetch` in the discovered workspace root.
///
/// Both output streams are drained concurrently so a chatty Spago cannot fill
/// a pipe, and so its output can never reach the LSP protocol stream. On
/// cancellation the process tree is killed, reaped, and drained before returning.
async fn fetch(
    spago: &SpagoCommand,
    selected: Option<&str>,
    cancel: &mut watch::Receiver<bool>,
) -> Result<(), LspError> {
    let mut process =
        ProcessTree::spawn(spago.fetch_command(selected)).map_err(SpagoError::Execute)?;

    let stdout = task::spawn(drain(process.take_standard_output()));
    let stderr = task::spawn(drain(process.take_standard_error()));

    let status = loop {
        tokio::select! {
            status = process.wait() => break Some(status),
            changed = cancel.changed() => {
                if changed.is_err() || *cancel.borrow() {
                    break None;
                }
            }
        }
    };

    let Some(status) = status else {
        let _ = process.terminate().await;
        let _ = stdout.await;
        let _ = stderr.await;
        return Err(LspError::WorkspaceFailed);
    };

    let status = status.map_err(SpagoError::Execute)?;
    let _ = stdout.await.map_err(LspError::JoinError)?;
    let stderr = stderr.await.map_err(LspError::JoinError)?.map_err(LspError::IoError)?;
    if status.success() {
        return Ok(());
    }
    Err(SpagoError::failed("fetch", status, &stderr).into())
}

async fn drain<R>(pipe: Option<R>) -> io::Result<Vec<u8>>
where
    R: AsyncRead + Unpin,
{
    let Some(mut pipe) = pipe else {
        return Ok(vec![]);
    };
    let mut output = vec![];
    pipe.read_to_end(&mut output).await?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_events_report_completed_packages_with_monotonic_percentages() {
        let mut progress = ActiveProgress::default();
        let plan = progress.apply(BuildEvent::PlanReady { package_count: 3 }).unwrap();
        assert_eq!(plan.message.as_deref(), Some("Compiling packages (0/3 completed)"));
        assert_eq!(plan.percentage, Some(0));

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
                report.message,
                Some(format!("Completed {package_name} ({completed}/3 packages)"))
            );
            assert_eq!(report.percentage, Some(percentage));
        }

        let finalizing =
            progress.apply(BuildEvent::Finalizing { duration: Duration::ZERO }).unwrap();
        assert_eq!(finalizing.message.as_deref(), Some("Finalizing initial compilation"));
        assert_eq!(finalizing.percentage, Some(100));
    }

    #[test]
    fn empty_build_plan_reports_completion_without_package_events() {
        let mut progress = ActiveProgress::default();
        let report = progress.apply(BuildEvent::PlanReady { package_count: 0 }).unwrap();

        assert_eq!(report.message.as_deref(), Some("No packages to compile"));
        assert_eq!(report.percentage, Some(100));
    }
}
