use std::error::Error;
use std::fmt;

use iris_lsp_server::{Server, Transport, WorkspaceEventSender, WorkspaceSenders};
use iris_lsp_workspace::{WorkspaceConfig, WorkspaceService};

mod server;

pub struct ServerConfig {
    pub name: String,
    pub version: String,
}

#[derive(Debug)]
pub struct ServerError(Box<dyn Error + Send + Sync>);

impl ServerError {
    pub(crate) fn new(error: impl Error + Send + Sync + 'static) -> ServerError {
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

pub fn start(config: ServerConfig) -> Result<(), ServerError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(ServerError::new)?;
    runtime.block_on(server::async_start(config))
}

/// Runs the server built from `iris-lsp-server` and `iris-lsp-workspace`.
pub fn start_next(config: ServerConfig) -> Result<(), ServerError> {
    let ServerConfig { name, version } = config;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(ServerError::new)?;
    let result = runtime.block_on(async {
        let (workspace_events, workspace_event_receiver) = WorkspaceEventSender::channel();
        let (workspace_senders, workspace_receivers) = WorkspaceSenders::channel();
        let workspace =
            WorkspaceService::new(WorkspaceConfig::new(name, version), workspace_events);
        let workspace = tokio::spawn(workspace.run(workspace_receivers));
        let transport = Transport::stdio();
        Server::new(transport, workspace_senders, workspace_event_receiver, workspace).run().await
    });
    // Cleanup already waited for the work it needs; blocking work that outlived its deadline
    // must not keep the process alive.
    runtime.shutdown_background();
    result.map_err(ServerError::new)
}
