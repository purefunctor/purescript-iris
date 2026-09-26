//! `iris watch`: rebuild when inputs change, and answer queries about the project through a local
//! socket.
//!
//! The socket is bound and named in the socket file before the initial build, so a client that
//! connects early waits for the build rather than finding no watcher. The build actor handles
//! filesystem events and requests; `iris-watch-server` owns the socket and its clients.

mod actor;
mod filesystem;
mod report;
mod signals;

use std::{env, io, process};

use iris_build::{PreparedProject, ProjectError, SessionError};
use iris_watch_server::discovery::{Discovery, DiscoveryError, OutputLock};
use iris_watch_server::transport::{Endpoint, Listener};
use thiserror::Error;
use tokio::sync::mpsc;

use crate::actor::BuildActor;

pub struct WatchConfig {
    pub quiet: bool,
    pub color: bool,
    pub diagnostics: bool,
    /// The Iris version the socket file reports.
    pub version: String,
}

#[derive(Debug, Error)]
enum WatchFailure {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Notify(#[from] notify::Error),
    #[error(transparent)]
    Session(#[from] SessionError),
    #[error(transparent)]
    Project {
        #[from]
        source: ProjectError,
    },
    #[error(transparent)]
    Discovery(#[from] DiscoveryError),
    #[error("failed to create the query socket: {0}")]
    Socket(io::Error),
}

#[derive(Debug, Error)]
#[error(transparent)]
pub struct WatchError(WatchFailure);

/// Watches `project`, whose output directory `lock` holds, until a termination signal arrives.
/// Returns the exit status of a process killed by that signal.
pub fn watch(
    project: PreparedProject,
    lock: OutputLock,
    config: WatchConfig,
) -> Result<i32, WatchError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| WatchError(WatchFailure::Io(error)))?;
    let result = runtime.block_on(serve(project, &lock, config));
    // A rebuild may still be running on a blocking thread; its result is no longer needed.
    runtime.shutdown_background();
    result.map_err(WatchError)
}

async fn serve(
    project: PreparedProject,
    lock: &OutputLock,
    config: WatchConfig,
) -> Result<i32, WatchFailure> {
    let endpoint = Endpoint::create().map_err(WatchFailure::Socket)?;
    let listener = Listener::bind(&endpoint).map_err(WatchFailure::Socket)?;
    let published = lock.publish(&Discovery {
        socket: endpoint.name().to_string(),
        pid: process::id(),
        version: String::clone(&config.version),
        executable: env::current_exe().ok(),
    })?;

    let (requests, receiver) = mpsc::unbounded_channel();
    tokio::spawn(iris_watch_server::serve(listener, requests));
    let status = tokio::select! {
        biased;
        status = signals::terminated() => status?,
        result = run_build(project, config, receiver) => {
            result?;
            unreachable!("invariant violated: the build actor stopped while the server was running")
        }
    };
    drop(published);
    drop(endpoint);
    Ok(status)
}

async fn run_build(
    project: PreparedProject,
    config: WatchConfig,
    requests: mpsc::UnboundedReceiver<iris_watch_server::QueryRequest>,
) -> Result<(), WatchFailure> {
    let actor = BuildActor::start(project, config).await?;
    actor.run(requests).await
}
