//! The Iris language server's workspace actor.
//!
//! [`WorkspaceService`] owns Iris state: workspace preparation, settings, open documents, the
//! query engine and its snapshots, analysis requests, and diagnostics. It receives the messages
//! defined in `iris-lsp-server` and reports what happened as `WorkspaceEvent`s; it never addresses
//! the editor directly.

mod analysis;
mod capabilities;
mod service;
mod state;

use iris_lsp_server::{WorkspaceEventSender, WorkspaceFailure, WorkspaceReceivers};

use crate::service::Actor;

/// The server identity reported in the `initialize` result.
pub struct WorkspaceConfig {
    pub name: String,
    pub version: String,
}

pub struct WorkspaceService {
    actor: Actor,
}

impl WorkspaceService {
    pub fn new(config: WorkspaceConfig, events: WorkspaceEventSender) -> WorkspaceService {
        WorkspaceService { actor: Actor::new(config, events) }
    }

    /// Handles messages until `iris-lsp-server` closes the channels, then runs cleanup.
    pub async fn run(self, receivers: WorkspaceReceivers) -> Result<(), WorkspaceFailure> {
        self.actor.run(receivers).await
    }
}
