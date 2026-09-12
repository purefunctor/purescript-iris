use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use iris_build::{
    BuildSession, BuildSessionConfig, InputChange, InputChanges, PreparedProject, RebuildOutcome,
    SessionError,
};
use iris_progress::{WatchOutcome, WatchSummary, render_watch_summary};
use itertools::Itertools;
use notify::event::{CreateKind, ModifyKind, RemoveKind};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use thiserror::Error;

const DEBOUNCE_DURATION: Duration = Duration::from_millis(100);

pub struct WatchConfig {
    pub quiet: bool,
    pub color: bool,
    pub diagnostics: bool,
}

#[derive(Debug, Error)]
enum WatchFailure {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Notify(#[from] notify::Error),
    #[error(transparent)]
    Session(#[from] SessionError),
}

#[derive(Debug, Error)]
#[error(transparent)]
pub struct WatchError(WatchFailure);

pub fn watch(project: PreparedProject, config: WatchConfig) -> Result<(), WatchError> {
    watch_project(project, config).map_err(WatchError)
}

fn watch_project(project: PreparedProject, config: WatchConfig) -> Result<(), WatchFailure> {
    let root = project.root_directory().to_path_buf();
    let mut session = BuildSession::new(
        project,
        BuildSessionConfig { color: config.color, diagnostics: config.diagnostics },
    )?;
    let (sender, receiver) = mpsc::channel();
    let mut watcher = RecommendedWatcher::new(sender, notify::Config::default())?;
    for root in session.source_roots() {
        let root = persistent_watch_root(root);
        watcher.watch(&root, RecursiveMode::Recursive)?;
    }

    let changes = session.rescan()?;
    report_warnings(&changes);
    let mut pending_inputs = changes.inputs;
    let mut pending_rebuild =
        !rebuild_and_report(&mut session, &config, &root, &pending_inputs, true);

    let mut needs_rescan = false;
    loop {
        let batch = receive_batch(&receiver)?;
        if batch.paths.is_empty() && !batch.rescan {
            continue;
        }
        let changes = if needs_rescan || batch.rescan {
            session.rescan()
        } else {
            session.synchronize_paths(&batch.paths)
        };
        let changes = match changes {
            Ok(changes) => {
                needs_rescan = false;
                changes
            }
            Err(error) => {
                needs_rescan = true;
                pending_rebuild = true;
                report_operational_failure(
                    &config,
                    &root,
                    &pending_inputs,
                    false,
                    Duration::ZERO,
                    error,
                );
                continue;
            }
        };
        report_warnings(&changes);
        if !changes.is_empty() {
            pending_inputs = changes.inputs;
            pending_rebuild = true;
        }
        if !pending_rebuild {
            continue;
        }
        pending_rebuild = !rebuild_and_report(&mut session, &config, &root, &pending_inputs, false);
    }
}

fn rebuild_and_report(
    session: &mut BuildSession,
    config: &WatchConfig,
    root: &Path,
    inputs: &[InputChange],
    initial: bool,
) -> bool {
    let started = Instant::now();
    match session.rebuild() {
        Ok(outcome) => {
            let outcome = match outcome {
                RebuildOutcome::Succeeded => WatchOutcome::Succeeded,
                RebuildOutcome::Diagnostics => WatchOutcome::Diagnostics,
                RebuildOutcome::NoInputs => WatchOutcome::Waiting,
            };
            report_summary(config, root, inputs, initial, started.elapsed(), outcome);
            true
        }
        Err(error) => {
            report_operational_failure(config, root, inputs, initial, started.elapsed(), error);
            false
        }
    }
}

fn report_operational_failure(
    config: &WatchConfig,
    root: &Path,
    inputs: &[InputChange],
    initial: bool,
    duration: Duration,
    error: SessionError,
) {
    tracing::error!(?error, "Watch compilation failed");
    eprintln!("Watch build failed: {error}");
    report_summary(config, root, inputs, initial, duration, WatchOutcome::Failed);
}

fn report_warnings(changes: &InputChanges) {
    for warning in &changes.warnings {
        tracing::warn!("{warning}");
        eprintln!("Watch warning: {warning}");
    }
}

fn report_summary(
    config: &WatchConfig,
    root: &Path,
    inputs: &[InputChange],
    initial: bool,
    duration: Duration,
    outcome: WatchOutcome,
) {
    if config.quiet {
        return;
    }
    let changed_inputs = inputs.iter().map(|input| input_label(input, root));
    let mut changed_inputs = changed_inputs.collect_vec();
    changed_inputs.sort();
    let timestamp = jiff::Zoned::now().strftime("%H:%M:%S").to_string();
    println!(
        "{}",
        render_watch_summary(
            WatchSummary {
                timestamp: &timestamp,
                initial,
                changed_inputs: &changed_inputs,
                duration,
                outcome,
            },
            config.color,
        )
    );
}

fn input_label(input: &InputChange, root: &Path) -> String {
    input.module_name.clone().unwrap_or_else(|| {
        input.source_path.strip_prefix(root).unwrap_or(&input.source_path).display().to_string()
    })
}

struct EventBatch {
    paths: Vec<PathBuf>,
    rescan: bool,
}

fn receive_batch(receiver: &Receiver<notify::Result<Event>>) -> Result<EventBatch, WatchFailure> {
    let first = receiver.recv().map_err(|error| {
        io::Error::new(io::ErrorKind::BrokenPipe, format!("watch channel closed: {error}"))
    })??;
    let deadline = Instant::now() + DEBOUNCE_DURATION;
    let mut events = vec![first];
    loop {
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            break;
        };
        match receiver.recv_timeout(remaining) {
            Ok(event) => events.push(event?),
            Err(RecvTimeoutError::Timeout) => break,
            Err(RecvTimeoutError::Disconnected) => {
                return Err(
                    io::Error::new(io::ErrorKind::BrokenPipe, "watch channel closed").into()
                );
            }
        }
    }

    let mut paths = BTreeSet::new();
    let mut rescan = false;
    for event in events {
        if matches!(event.kind, EventKind::Access(_)) {
            continue;
        }
        rescan |= event.need_rescan() || requires_rescan(event.kind);
        paths.extend(event.paths);
    }
    Ok(EventBatch { paths: paths.into_iter().collect_vec(), rescan })
}

fn requires_rescan(kind: EventKind) -> bool {
    matches!(
        kind,
        EventKind::Create(CreateKind::Folder)
            | EventKind::Modify(ModifyKind::Name(_))
            | EventKind::Remove(RemoveKind::Folder)
            | EventKind::Any
            | EventKind::Other
    )
}

fn persistent_watch_root(root: &Path) -> PathBuf {
    let mut candidate = root.parent().unwrap_or(root);
    while !candidate.exists() {
        let Some(parent) = candidate.parent() else {
            break;
        };
        candidate = parent;
    }
    if candidate.is_file() {
        candidate.parent().unwrap_or(candidate).to_path_buf()
    } else {
        candidate.to_path_buf()
    }
}
