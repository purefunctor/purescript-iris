//! The Iris language server's workspace actor.
//!
//! [`WorkspaceService`] owns Iris state: workspace preparation, settings, open documents, the
//! query engine and its snapshots, analysis requests, and diagnostics. It receives the messages
//! defined in `iris-lsp-server` and reports what happened as `WorkspaceEvent`s; it never addresses
//! the editor directly.

mod analysis;
mod capabilities;
mod diagnostics;
mod discovery;
mod handlers;
mod preparation;
mod process;
mod service;
mod settings;
mod state;

#[cfg(test)]
mod tests;

use std::num::NonZero;
use std::thread::available_parallelism;

use iris_lsp_server::{WorkspaceEventSender, WorkspaceFailure, WorkspaceReceivers};

use crate::service::Actor;

pub struct WorkspaceConfig {
    /// The server name reported in the `initialize` result.
    pub name: String,
    /// The server version reported in the `initialize` result.
    pub version: String,
    /// How many analysis requests run at once.
    pub analysis_permits: usize,
    /// How many diagnostic collections run at once.
    pub diagnostic_permits: usize,
}

impl WorkspaceConfig {
    /// Allows as many analysis requests and diagnostic collections at once as there are CPUs.
    pub fn new(name: String, version: String) -> WorkspaceConfig {
        let permits = available_parallelism().map_or(1, NonZero::get);
        WorkspaceConfig { name, version, analysis_permits: permits, diagnostic_permits: permits }
    }
}

pub struct WorkspaceService {
    actor: Actor,
}

impl WorkspaceService {
    pub fn new(config: WorkspaceConfig, events: WorkspaceEventSender) -> WorkspaceService {
        WorkspaceService { actor: Actor::new(config, events) }
    }

    /// Creates a service whose workspace holds only the Prim modules and needs no preparation,
    /// for tests that exercise the language server without a Spago project.
    #[cfg(feature = "test-support")]
    pub fn with_builtin_workspace(
        config: WorkspaceConfig,
        events: WorkspaceEventSender,
    ) -> WorkspaceService {
        WorkspaceService { actor: Actor::with_builtin_workspace(config, events) }
    }

    /// Handles messages until `iris-lsp-server` closes the channels, then runs cleanup.
    pub async fn run(self, receivers: WorkspaceReceivers) -> Result<(), WorkspaceFailure> {
        self.actor.run(receivers).await
    }
}
