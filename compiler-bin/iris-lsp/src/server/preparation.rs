//! Startup preparation of the Spago workspace.
//!
//! Preparation is one operation per server session: discover the Spago
//! workspace, run `spago fetch` in its root, then run the existing `iris-build`
//! discovery and initial compilation. It runs off the protocol loop so that
//! document notifications keep arriving and queue while it is in flight.

use std::path::PathBuf;
use std::process::Stdio;

use async_lsp::ClientSocket;
use iris_build::Workspace;
use iris_spago::{SpagoCommand, SpagoError};
use parking_lot::Mutex;
use tokio::process::Command;
use tokio::task;

use super::error::LspError;
use super::workspace::PreparedInitialWorkspace;

/// Delivered through the server event loop when preparation finishes.
pub(super) struct PreparationFinished {
    pub(super) generation: u64,
    pub(super) result: Result<PreparedInitialWorkspace, LspError>,
}

/// Owns the single startup preparation task.
pub(super) struct Preparation {
    inner: Mutex<PreparationInner>,
}

struct PreparationInner {
    generation: u64,
    started: bool,
}

impl Preparation {
    pub(super) fn new() -> Preparation {
        Preparation { inner: Mutex::new(PreparationInner { generation: 0, started: false }) }
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
        task::spawn(run(root, generation, client));
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
}

async fn run(root: PathBuf, generation: u64, client: ClientSocket) {
    let result = prepare(root).await;
    if let Err(error) = client.emit(PreparationFinished { generation, result }) {
        LspError::from(error).emit_trace();
    }
}

async fn prepare(root: PathBuf) -> Result<PreparedInitialWorkspace, LspError> {
    let client_root = PathBuf::clone(&root);
    let workspace = task::spawn_blocking(move || Workspace::discover(&root, None))
        .await
        .map_err(LspError::JoinError)??;
    let spago = SpagoCommand::new(&workspace.root)?;
    fetch(&spago, workspace.selected.as_deref()).await?;
    let prepared =
        task::spawn_blocking(move || super::build_prepared_workspace(workspace, client_root))
            .await
            .map_err(LspError::JoinError)??;
    Ok(prepared)
}

/// Runs `spago fetch` in the discovered workspace root.
///
/// Both output streams are captured so that Spago output can never reach the
/// LSP protocol stream.
async fn fetch(spago: &SpagoCommand, selected: Option<&str>) -> Result<(), LspError> {
    let mut command = Command::from(spago.fetch_command(selected));
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let child = command.spawn().map_err(SpagoError::Execute)?;
    let output = child.wait_with_output().await.map_err(SpagoError::Execute)?;
    if output.status.success() {
        return Ok(());
    }
    Err(SpagoError::failed("fetch", output.status, &output.stderr).into())
}
