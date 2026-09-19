//! Startup preparation of the Spago workspace.
//!
//! Preparation is one operation per server session: discover the Spago
//! workspace, run `spago fetch` in its root, then run the existing `iris-build`
//! discovery and initial compilation. It runs off the protocol loop so that
//! document notifications keep arriving and queue while it is in flight, and
//! it owns the fetch subprocess so that shutdown can terminate and drain it.

use std::io;
use std::path::PathBuf;

use async_lsp::ClientSocket;
use iris_build::Workspace;
use iris_spago::{SpagoCommand, SpagoError};
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
    started: bool,
    cancel: watch::Sender<bool>,
    task: Option<task::JoinHandle<()>>,
}

impl Preparation {
    pub(super) fn new() -> Preparation {
        Preparation {
            inner: Mutex::new(PreparationInner {
                generation: 0,
                started: false,
                cancel: watch::channel(false).0,
                task: None,
            }),
        }
    }

    /// Starts the one startup preparation, or does nothing if it already ran.
    ///
    /// Returns the generation that identifies the resulting completion event.
    pub(super) fn start(&self, root: PathBuf, client: ClientSocket) -> Option<u64> {
        let mut inner = self.inner.lock();
        if inner.started {
            return None;
        }
        inner.started = true;
        inner.generation = inner.generation.wrapping_add(1);
        let generation = inner.generation;
        let cancel = inner.cancel.subscribe();
        inner.task = Some(task::spawn(run(root, generation, cancel, client)));
        Some(generation)
    }

    pub(super) fn is_current(&self, generation: u64) -> bool {
        let inner = self.inner.lock();
        inner.started && inner.generation == generation
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

    /// Cancels preparation, terminates the fetch subprocess, and joins every
    /// owned task, including the blocking initial compilation.
    pub(super) async fn shutdown(&self) {
        let task = {
            let mut inner = self.inner.lock();
            let _ = inner.cancel.send(true);
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
) {
    let result = prepare(root, &mut cancel).await;
    if *cancel.borrow() {
        return;
    }
    if let Err(error) = client.emit(PreparationFinished { generation, result }) {
        LspError::from(error).emit_trace();
    }
}

async fn prepare(
    root: PathBuf,
    cancel: &mut watch::Receiver<bool>,
) -> Result<PreparedInitialWorkspace, LspError> {
    let client_root = PathBuf::clone(&root);
    let workspace = task::spawn_blocking(move || Workspace::discover(&root, None))
        .await
        .map_err(LspError::JoinError)??;
    let spago = SpagoCommand::new(&workspace.root)?;
    fetch(&spago, workspace.selected.as_deref(), cancel).await?;
    let prepared =
        task::spawn_blocking(move || super::build_prepared_workspace(workspace, client_root))
            .await
            .map_err(LspError::JoinError)??;
    Ok(prepared)
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
