//! Startup preparation of the Spago workspace.
//!
//! Each preparation generation discovers the Spago workspace, runs `spago fetch`
//! in its root, then runs the existing `iris-build` discovery and
//! initial compilation. Preparation runs off the protocol loop so that document
//! notifications keep arriving and queue while it is in flight. Cancellation
//! retires the active generation before a request can start its replacement.

use std::path::PathBuf;
use std::sync::Arc;
use std::{io, mem};

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
    state: PreparationState,
    input: Option<PreparationInput>,
    retired_tasks: Vec<task::JoinHandle<()>>,
    progress_tasks: Vec<task::JoinHandle<()>>,
}

enum PreparationState {
    NotStarted,
    Transitioning,
    Running(ActiveGeneration),
    Cancelling { active: ActiveGeneration, retry: Option<RequestedGeneration> },
    Paused { generation: u64 },
    Finished,
    Failed,
    ShuttingDown { active: Option<ActiveGeneration>, retry: Option<RequestedGeneration> },
}

#[derive(Clone)]
struct PreparationInput {
    root: PathBuf,
    client: ClientSocket,
    work_done_progress: bool,
}

struct ActiveGeneration {
    generation: u64,
    kind: GenerationKind,
    cancel: watch::Sender<bool>,
    progress: Option<Arc<StartupProgress>>,
    task: Option<task::JoinHandle<()>>,
}

enum GenerationKind {
    Initial(watch::Sender<GenerationOutcome>),
    Requested(RequestedGeneration),
}

struct RequestedGeneration {
    generation: u64,
    completion: watch::Sender<GenerationOutcome>,
}

pub(super) struct PreparationTicket {
    generation: u64,
    completion: watch::Receiver<GenerationOutcome>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GenerationOutcome {
    Pending,
    Ready,
    Failed,
    Cancelled,
    Shutdown,
}

pub(super) enum CompletionDisposition {
    Apply,
    Discarded,
    Stale,
}

impl Preparation {
    pub(super) fn new() -> Preparation {
        Preparation {
            inner: Mutex::new(PreparationInner {
                generation: 0,
                state: PreparationState::NotStarted,
                input: None,
                retired_tasks: vec![],
                progress_tasks: vec![],
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
        if !matches!(inner.state, PreparationState::NotStarted) {
            return None;
        }
        inner.input = Some(PreparationInput { root, client, work_done_progress });
        inner.generation = inner.generation.wrapping_add(1);
        let generation = inner.generation;
        let (completion, _) = watch::channel(GenerationOutcome::Pending);
        let active = start_generation(&mut inner, generation, GenerationKind::Initial(completion));
        inner.state = PreparationState::Running(active);
        Some(generation)
    }

    /// Subscribes to the active initial generation without starting or replacing preparation.
    pub(super) fn initial_ticket(&self) -> Option<PreparationTicket> {
        let inner = self.inner.lock();
        let PreparationState::Running(active) = &inner.state else { return None };
        matches!(&active.kind, GenerationKind::Initial(_)).then(|| active.ticket())
    }

    pub(super) fn demand_retry(&self) -> Option<PreparationTicket> {
        let mut inner = self.inner.lock();
        let state = mem::replace(&mut inner.state, PreparationState::Transitioning);
        match state {
            PreparationState::Running(active) => {
                let ticket =
                    matches!(&active.kind, GenerationKind::Requested(_)).then(|| active.ticket());
                inner.state = PreparationState::Running(active);
                ticket
            }
            PreparationState::Cancelling { active, retry } => {
                let retry = retry.unwrap_or_else(|| requested(active.generation.wrapping_add(1)));
                let ticket = retry.ticket();
                inner.state = PreparationState::Cancelling { active, retry: Some(retry) };
                Some(ticket)
            }
            PreparationState::Paused { generation } => {
                let requested = requested(generation.wrapping_add(1));
                let ticket = requested.ticket();
                inner.generation = requested.generation;
                let active = start_generation(
                    &mut inner,
                    requested.generation,
                    GenerationKind::Requested(requested),
                );
                inner.state = PreparationState::Running(active);
                Some(ticket)
            }
            state => {
                inner.state = state;
                None
            }
        }
    }

    pub(super) fn finish_disposition(&self, generation: u64) -> CompletionDisposition {
        let mut inner = self.inner.lock();
        let state = mem::replace(&mut inner.state, PreparationState::Transitioning);
        match state {
            PreparationState::Running(active) if active.generation == generation => {
                inner.state = PreparationState::Running(active);
                CompletionDisposition::Apply
            }
            PreparationState::Cancelling { active, retry } if active.generation == generation => {
                if let Some(task) = active.task {
                    inner.retired_tasks.push(task);
                }
                if let Some(retry) = retry {
                    inner.generation = retry.generation;
                    let active = start_generation(
                        &mut inner,
                        retry.generation,
                        GenerationKind::Requested(retry),
                    );
                    inner.state = PreparationState::Running(active);
                } else {
                    inner.state = PreparationState::Paused { generation };
                }
                prune_tasks(&mut inner);
                CompletionDisposition::Discarded
            }
            state => {
                inner.state = state;
                CompletionDisposition::Stale
            }
        }
    }

    pub(super) fn finish_success(&self, generation: u64, message: &str) -> bool {
        self.finish(generation, GenerationOutcome::Ready, message, true)
    }

    pub(super) fn finish_failure(&self, generation: u64, message: &str) -> bool {
        self.finish(generation, GenerationOutcome::Failed, message, false)
    }

    fn finish(
        &self,
        generation: u64,
        outcome: GenerationOutcome,
        message: &str,
        success: bool,
    ) -> bool {
        let mut inner = self.inner.lock();
        let state = mem::replace(&mut inner.state, PreparationState::Transitioning);
        let PreparationState::Running(active) = state else {
            inner.state = state;
            return false;
        };
        if active.generation != generation {
            inner.state = PreparationState::Running(active);
            return false;
        }
        active.publish(outcome);
        if let Some(progress) = &active.progress {
            progress.end(message);
        }
        if let Some(task) = active.task {
            inner.retired_tasks.push(task);
        }
        inner.state = if success { PreparationState::Finished } else { PreparationState::Failed };
        prune_tasks(&mut inner);
        true
    }

    /// Marks preparation as started without spawning a task.
    ///
    /// Tests use this to drive completion events directly, without running a
    /// real Spago process.
    #[cfg(test)]
    pub(super) fn test_arm(&self) -> u64 {
        let mut inner = self.inner.lock();
        inner.generation = inner.generation.wrapping_add(1);
        let generation = inner.generation;
        let (cancel, _) = watch::channel(false);
        let (completion, _) = watch::channel(GenerationOutcome::Pending);
        inner.state = PreparationState::Running(ActiveGeneration {
            generation,
            kind: GenerationKind::Initial(completion),
            cancel,
            progress: None,
            task: None,
        });
        inner.generation
    }

    pub(super) fn cancel_progress(&self, token: &ProgressToken) -> bool {
        let mut inner = self.inner.lock();
        let state = mem::replace(&mut inner.state, PreparationState::Transitioning);
        let PreparationState::Running(active) = state else {
            inner.state = state;
            return false;
        };
        let Some(progress) = &active.progress else {
            inner.state = PreparationState::Running(active);
            return false;
        };
        if !progress.cancel(token) {
            inner.state = PreparationState::Running(active);
            return false;
        }
        active.publish(GenerationOutcome::Cancelled);
        let _ = active.cancel.send(true);
        inner.state = PreparationState::Cancelling { active, retry: None };
        true
    }

    /// Stops preparation from producing more protocol-visible effects.
    pub(super) fn cancel(&self) {
        let mut inner = self.inner.lock();
        let state = mem::replace(&mut inner.state, PreparationState::Transitioning);
        let (active, retry) = match state {
            PreparationState::Running(active) => (Some(active), None),
            PreparationState::Cancelling { active, retry } => (Some(active), retry),
            PreparationState::ShuttingDown { active, retry } => {
                inner.state = PreparationState::ShuttingDown { active, retry };
                return;
            }
            _ => (None, None),
        };
        if let Some(active) = &active {
            active.publish(GenerationOutcome::Shutdown);
            let _ = active.cancel.send(true);
            if let Some(progress) = &active.progress {
                progress.end("Workspace preparation cancelled");
            }
        }
        if let Some(retry) = &retry {
            retry.publish(GenerationOutcome::Shutdown);
        }
        inner.state = PreparationState::ShuttingDown { active, retry };
    }

    /// Cancels preparation, terminates the fetch subprocess, and joins every
    /// owned task, including the blocking initial compilation.
    pub(super) async fn shutdown(&self) {
        self.cancel();
        let (tasks, progress_tasks) = {
            let mut inner = self.inner.lock();
            let state = mem::replace(
                &mut inner.state,
                PreparationState::ShuttingDown { active: None, retry: None },
            );
            let mut tasks = mem::take(&mut inner.retired_tasks);
            if let PreparationState::ShuttingDown { active: Some(active), .. } = state {
                if let Some(task) = active.task {
                    tasks.push(task);
                }
            }
            (tasks, mem::take(&mut inner.progress_tasks))
        };
        for task in tasks {
            let _ = task.await;
        }
        for task in progress_tasks {
            task.abort();
            let _ = task.await;
        }
    }
}

impl ActiveGeneration {
    fn ticket(&self) -> PreparationTicket {
        let completion = match &self.kind {
            GenerationKind::Initial(completion) => completion.subscribe(),
            GenerationKind::Requested(requested) => requested.completion.subscribe(),
        };
        PreparationTicket { generation: self.generation, completion }
    }

    fn publish(&self, outcome: GenerationOutcome) {
        match &self.kind {
            GenerationKind::Initial(completion) => {
                completion.send_replace(outcome);
            }
            GenerationKind::Requested(requested) => requested.publish(outcome),
        }
    }
}

impl RequestedGeneration {
    fn ticket(&self) -> PreparationTicket {
        PreparationTicket { generation: self.generation, completion: self.completion.subscribe() }
    }

    fn publish(&self, outcome: GenerationOutcome) {
        self.completion.send_replace(outcome);
    }
}

impl PreparationTicket {
    pub(super) async fn wait(mut self) -> Result<(), LspError> {
        loop {
            let outcome = *self.completion.borrow_and_update();
            match outcome {
                GenerationOutcome::Pending => {}
                GenerationOutcome::Ready => return Ok(()),
                GenerationOutcome::Failed => return Err(LspError::WorkspaceFailed),
                GenerationOutcome::Cancelled => {
                    tracing::debug!(
                        generation = self.generation,
                        "Preparation generation cancelled"
                    );
                    return Err(LspError::WorkspaceCancelled);
                }
                GenerationOutcome::Shutdown => return Err(LspError::WorkspaceNotReady),
            }
            self.completion.changed().await.map_err(|_| LspError::WorkspaceFailed)?;
        }
    }

    #[cfg(test)]
    pub(super) fn generation(&self) -> u64 {
        self.generation
    }
}

fn requested(generation: u64) -> RequestedGeneration {
    let (completion, _) = watch::channel(GenerationOutcome::Pending);
    RequestedGeneration { generation, completion }
}

fn start_generation(
    inner: &mut PreparationInner,
    generation: u64,
    kind: GenerationKind,
) -> ActiveGeneration {
    let input = inner.input.clone().expect("invariant violated: preparation has no input");
    let (cancel, receiver) = watch::channel(false);
    let progress = input.work_done_progress.then(|| {
        Arc::new(StartupProgress::new(
            ClientSocket::clone(&input.client),
            NumberOrString::String(format!("iris/startup/{generation}")),
        ))
    });
    if let Some(progress) = &progress {
        inner.progress_tasks.push(progress.start());
    }
    let task =
        task::spawn(run(input.root, generation, receiver, input.client, Option::clone(&progress)));
    ActiveGeneration { generation, kind, cancel, progress, task: Some(task) }
}

fn prune_tasks(inner: &mut PreparationInner) {
    inner.retired_tasks.retain(|task| !task.is_finished());
    inner.progress_tasks.retain(|task| !task.is_finished());
}

async fn run(
    root: PathBuf,
    generation: u64,
    mut cancel: watch::Receiver<bool>,
    client: ClientSocket,
    progress: Option<Arc<StartupProgress>>,
) {
    let result = prepare(root, &mut cancel, Option::clone(&progress)).await;
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
        return Err(LspError::WorkspaceCancelled);
    }
    if let Some(progress) = &progress {
        progress.report_message("Fetching dependencies with spago fetch", 0);
    }
    let spago = SpagoCommand::new(&workspace.root)?;
    fetch(&spago, workspace.selected.as_deref(), cancel).await?;
    if *cancel.borrow() {
        return Err(LspError::WorkspaceCancelled);
    }
    if let Some(progress) = &progress {
        progress.report_message("Discovering packages and preparing compilation", 0);
    }
    let blocking_cancel = cancel.clone();
    let prepared = task::spawn_blocking(move || {
        if *blocking_cancel.borrow() {
            return Err(LspError::WorkspaceCancelled);
        }
        match progress {
            Some(progress) => {
                super::build_prepared_workspace(workspace, client_root, progress.as_ref())
            }
            None => super::build_prepared_workspace(
                workspace,
                client_root,
                &iris_build::events::SilentBuildEvents,
            ),
        }
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
    Creating(ProgressRecord),
    Active(ActiveProgress),
    Rejected,
    Closed,
}

#[derive(Default)]
struct ActiveProgress {
    package_count: Option<usize>,
    completed_count: usize,
    percentage: u32,
    latest_report: Option<WorkDoneProgressReport>,
}

#[derive(Default)]
struct ProgressRecord {
    active: ActiveProgress,
    terminal: Option<String>,
}

impl StartupProgress {
    fn new(client: ClientSocket, token: ProgressToken) -> StartupProgress {
        StartupProgress {
            client,
            token,
            state: Mutex::new(StartupProgressState::Creating(ProgressRecord::default())),
        }
    }

    fn start(self: &Arc<StartupProgress>) -> task::JoinHandle<()> {
        let progress = Arc::clone(self);
        task::spawn(async move {
            let result = {
                let mut client = ClientSocket::clone(&progress.client);
                client
                    .work_done_progress_create(WorkDoneProgressCreateParams {
                        token: progress.token.clone(),
                    })
                    .await
            };
            progress.created(result);
        })
    }

    fn created(&self, result: async_lsp::Result<()>) {
        let mut state = self.state.lock();
        let current = mem::replace(&mut *state, StartupProgressState::Closed);
        let StartupProgressState::Creating(record) = current else {
            *state = current;
            return;
        };
        if let Err(error) = result {
            tracing::warn!("Failed to create workspace preparation progress: {error}");
            *state = StartupProgressState::Rejected;
            return;
        }
        let begin = WorkDoneProgressBegin {
            title: "Preparing Iris workspace".to_string(),
            cancellable: Some(true),
            message: Some("Discovering Spago workspace".to_string()),
            percentage: Some(0),
        };
        if self.notify(WorkDoneProgress::Begin(begin)).is_err() {
            return;
        }
        if let Some(report) = record.active.latest_report.clone()
            && self.notify(WorkDoneProgress::Report(report)).is_err()
        {
            return;
        }
        if let Some(message) = record.terminal {
            let end = WorkDoneProgressEnd { message: Some(message) };
            let _ = self.notify(WorkDoneProgress::End(end));
            return;
        }
        *state = StartupProgressState::Active(record.active);
    }

    fn report_message(&self, message: &str, percentage: u32) {
        let mut state = self.state.lock();
        let report = WorkDoneProgressReport {
            cancellable: Some(true),
            message: Some(message.to_string()),
            percentage: Some(percentage),
        };
        match &mut *state {
            StartupProgressState::Creating(record) => {
                record.active.percentage = percentage;
                record.active.latest_report = Some(report);
            }
            StartupProgressState::Active(active) => {
                active.percentage = percentage;
                active.latest_report = Some(report.clone());
                if self.notify(WorkDoneProgress::Report(report)).is_err() {
                    *state = StartupProgressState::Closed;
                }
            }
            StartupProgressState::Rejected | StartupProgressState::Closed => {}
        }
    }

    fn cancel(&self, token: &ProgressToken) -> bool {
        if self.token != *token {
            return false;
        }
        self.terminate("Workspace preparation cancelled")
    }

    fn end(&self, message: &str) {
        let _ = self.terminate(message);
    }

    fn terminate(&self, message: &str) -> bool {
        let mut state = self.state.lock();
        match &mut *state {
            StartupProgressState::Creating(record) if record.terminal.is_none() => {
                record.terminal = Some(message.to_string());
                true
            }
            StartupProgressState::Active(_) => {
                let end = WorkDoneProgressEnd { message: Some(message.to_string()) };
                let _ = self.notify(WorkDoneProgress::End(end));
                *state = StartupProgressState::Closed;
                true
            }
            StartupProgressState::Creating(_)
            | StartupProgressState::Rejected
            | StartupProgressState::Closed => false,
        }
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
        match &mut *state {
            StartupProgressState::Creating(record) => {
                if let Some(report) = record.active.apply(event) {
                    record.active.latest_report = Some(report);
                }
            }
            StartupProgressState::Active(active) => {
                let Some(report) = active.apply(event) else { return };
                active.latest_report = Some(report.clone());
                if self.notify(WorkDoneProgress::Report(report)).is_err() {
                    *state = StartupProgressState::Closed;
                }
            }
            StartupProgressState::Rejected | StartupProgressState::Closed => {}
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
            cancellable: Some(true),
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
            status = process.wait() => break status,
            changed = cancel.changed() => {
                if changed.is_err() || *cancel.borrow() {
                    terminate(&mut process).await;
                    let _ = await_drains(stdout, stderr).await;
                    return Err(LspError::WorkspaceCancelled);
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
                    return Err(LspError::WorkspaceCancelled);
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
) -> Result<(Vec<u8>, Vec<u8>), LspError> {
    let (stdout, stderr) = tokio::join!(stdout, stderr);
    let stdout = stdout.map_err(LspError::JoinError)?.map_err(LspError::IoError)?;
    let stderr = stderr.map_err(LspError::JoinError)?.map_err(LspError::IoError)?;
    Ok((stdout, stderr))
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
    use std::time::Duration;
    use tempfile::tempdir;

    fn cancel_active(preparation: &Preparation) -> u64 {
        let mut inner = preparation.inner.lock();
        let state = mem::replace(&mut inner.state, PreparationState::Transitioning);
        let PreparationState::Running(active) = state else {
            panic!("invariant violated: preparation is not running");
        };
        let generation = active.generation;
        active.publish(GenerationOutcome::Cancelled);
        let _ = active.cancel.send(true);
        inner.state = PreparationState::Cancelling { active, retry: None };
        generation
    }

    fn arm_with_input(preparation: &Preparation, root: PathBuf) -> u64 {
        preparation.inner.lock().input = Some(PreparationInput {
            root,
            client: ClientSocket::new_closed(),
            work_done_progress: false,
        });
        preparation.test_arm()
    }

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

    #[test]
    fn initial_ticket_observes_only_the_running_initial_generation() {
        let preparation = Preparation::new();
        assert!(preparation.initial_ticket().is_none());

        let generation = preparation.test_arm();
        let first = preparation.initial_ticket().unwrap();
        let second = preparation.initial_ticket().unwrap();

        assert_eq!(first.generation(), generation);
        assert_eq!(second.generation(), generation);
        assert!(preparation.demand_retry().is_none());

        cancel_active(&preparation);
        assert!(preparation.initial_ticket().is_none());
    }

    #[tokio::test]
    async fn initial_generation_wakes_only_after_success_is_committed() {
        let preparation = Preparation::new();
        let generation = preparation.test_arm();
        let mut ticket = preparation.initial_ticket().unwrap();

        assert!(matches!(preparation.finish_disposition(generation), CompletionDisposition::Apply));
        assert_eq!(*ticket.completion.borrow_and_update(), GenerationOutcome::Pending);
        assert!(!preparation.finish_success(generation.wrapping_add(1), "stale"));
        assert_eq!(*ticket.completion.borrow_and_update(), GenerationOutcome::Pending);

        assert!(preparation.finish_success(generation, "finished"));
        assert!(ticket.wait().await.is_ok());
    }

    #[tokio::test]
    async fn initial_generation_failure_resolves_all_waiters() {
        let preparation = Preparation::new();
        let generation = preparation.test_arm();
        let first = preparation.initial_ticket().unwrap();
        let second = preparation.initial_ticket().unwrap();

        assert!(preparation.finish_failure(generation, "failed"));

        assert!(matches!(first.wait().await, Err(LspError::WorkspaceFailed)));
        assert!(matches!(second.wait().await, Err(LspError::WorkspaceFailed)));
        assert!(preparation.initial_ticket().is_none());
        assert!(preparation.demand_retry().is_none());
    }

    #[tokio::test]
    async fn initial_generation_cancellation_does_not_migrate_its_waiter() {
        let preparation = Preparation::new();
        let generation = preparation.test_arm();
        let initial = preparation.initial_ticket().unwrap();

        assert_eq!(cancel_active(&preparation), generation);
        let retry = preparation.demand_retry().unwrap();

        assert!(matches!(initial.wait().await, Err(LspError::WorkspaceCancelled)));
        assert_eq!(retry.generation(), generation.wrapping_add(1));
    }

    #[tokio::test]
    async fn shutdown_resolves_initial_generation_waiters() {
        let preparation = Preparation::new();
        let generation = preparation.test_arm();
        let ticket = preparation.initial_ticket().unwrap();

        preparation.cancel();

        assert!(matches!(ticket.wait().await, Err(LspError::WorkspaceNotReady)));
        assert!(matches!(preparation.finish_disposition(generation), CompletionDisposition::Stale));
    }

    #[test]
    fn requests_during_cancellation_reserve_one_generation() {
        let preparation = Preparation::new();
        let generation = preparation.test_arm();
        assert_eq!(cancel_active(&preparation), generation);

        let first = preparation.demand_retry().unwrap();
        let second = preparation.demand_retry().unwrap();

        assert_eq!(first.generation(), generation.wrapping_add(1));
        assert_eq!(second.generation(), first.generation());
        let inner = preparation.inner.lock();
        assert!(matches!(
            &inner.state,
            PreparationState::Cancelling { retry: Some(retry), .. }
                if retry.generation == first.generation()
        ));
    }

    #[tokio::test]
    async fn reserved_retry_starts_only_after_cancelled_generation_retires() {
        let directory = tempdir().unwrap();
        let preparation = Preparation::new();
        let generation = arm_with_input(&preparation, directory.path().to_path_buf());
        cancel_active(&preparation);
        let ticket = preparation.demand_retry().unwrap();

        {
            let inner = preparation.inner.lock();
            assert!(matches!(inner.state, PreparationState::Cancelling { .. }));
        }
        assert!(matches!(
            preparation.finish_disposition(generation),
            CompletionDisposition::Discarded
        ));
        {
            let inner = preparation.inner.lock();
            let PreparationState::Running(active) = &inner.state else {
                panic!("invariant violated: requested generation did not start");
            };
            assert_eq!(active.generation, ticket.generation());
            assert!(!*active.cancel.borrow());
        }
        cancel_active(&preparation);
        assert!(matches!(ticket.wait().await, Err(LspError::WorkspaceCancelled)));
        preparation.shutdown().await;
    }

    #[tokio::test]
    async fn generation_ticket_retains_cancellation_after_a_newer_generation_exists() {
        let first = requested(2);
        let first_ticket = first.ticket();
        first.publish(GenerationOutcome::Cancelled);
        let second = requested(3);
        second.publish(GenerationOutcome::Ready);

        assert!(matches!(first_ticket.wait().await, Err(LspError::WorkspaceCancelled)));
        assert!(second.ticket().wait().await.is_ok());
    }

    #[tokio::test]
    async fn requested_generation_wakes_only_after_success_is_committed() {
        let directory = tempdir().unwrap();
        let preparation = Preparation::new();
        let generation = arm_with_input(&preparation, directory.path().to_path_buf());
        cancel_active(&preparation);
        let mut ticket = preparation.demand_retry().unwrap();
        let requested_generation = ticket.generation();
        preparation.finish_disposition(generation);
        assert_eq!(*ticket.completion.borrow_and_update(), GenerationOutcome::Pending);

        assert!(matches!(
            preparation.finish_disposition(requested_generation),
            CompletionDisposition::Apply
        ));
        assert!(preparation.finish_success(requested_generation, "finished"));
        assert!(ticket.wait().await.is_ok());
        preparation.shutdown().await;
    }
}
