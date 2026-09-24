//! The Iris language server's protocol actor.
//!
//! [`Server`] is the only component that talks to the editor. It owns the stdio transport, the
//! LSP lifecycle, the IDs of requests received from the editor, and every message sent to the
//! editor. Iris state lives in a separate workspace actor that receives [`OrderedMessage`]s and
//! [`ControlMessage`]s, reports [`WorkspaceEvent`]s, and never addresses the editor directly.

mod outgoing;
mod parent;
mod progress;
mod server;
mod service;
mod settings;
mod transport;

#[cfg(test)]
mod tests;

pub use server::{Server, ServerError};
pub use service::{
    Answer, ControlMessage, OrderedMessage, Rejection, SettingsResponse, WorkspaceEvent,
    WorkspaceEventSender, WorkspaceFailure, WorkspaceReceivers, WorkspaceSenders,
};
pub use transport::{Transport, TransportError};
