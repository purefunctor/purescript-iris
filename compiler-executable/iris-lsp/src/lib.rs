//! The Iris language server: [`start`] connects the protocol actor from `iris-lsp-server` with
//! the workspace actor from `iris-lsp-workspace`. See the crate README for how they interact.

use std::error::Error;
use std::fmt;

use iris_lsp_server::{Server, Transport, WorkspaceEventSender, WorkspaceSenders};
use iris_lsp_workspace::{WorkspaceConfig, WorkspaceService};

#[cfg(test)]
mod tests;

pub struct ServerConfig {
    pub name: String,
    pub version: String,
}

#[derive(Debug)]
pub struct ServerError(Box<dyn Error + Send + Sync>);

impl ServerError {
    fn new(error: impl Error + Send + Sync + 'static) -> ServerError {
        ServerError(Box::new(error))
    }
}

impl fmt::Display for ServerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl Error for ServerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.0.as_ref())
    }
}

/// Runs the language server over standard input and output until the editor ends the session.
pub fn start(config: ServerConfig) -> Result<(), ServerError> {
    let ServerConfig { name, version } = config;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(ServerError::new)?;
    let result = runtime.block_on(connect(Transport::stdio(), |events| {
        WorkspaceService::new(WorkspaceConfig::new(name, version), events)
    }));
    // Cleanup already waited for the work it needs; blocking work that outlived its deadline
    // must not keep the process alive.
    runtime.shutdown_background();
    result.map_err(ServerError::new)
}

/// Creates the protocol actor and the workspace actor, connects them, and runs them until the
/// server stops.
async fn connect(
    transport: Transport,
    workspace: impl FnOnce(WorkspaceEventSender) -> WorkspaceService,
) -> Result<(), iris_lsp_server::ServerError> {
    let (workspace_events, workspace_event_receiver) = WorkspaceEventSender::channel();
    let (workspace_senders, workspace_receivers) = WorkspaceSenders::channel();
    let workspace = workspace(workspace_events);
    let workspace = tokio::spawn(workspace.run(workspace_receivers));
    Server::new(transport, workspace_senders, workspace_event_receiver, workspace).run().await
}
