//! The Iris language server's protocol actor.
//!
//! [`Server`] is the only component that talks to the editor. It owns the stdio transport, the
//! LSP lifecycle, the IDs of requests received from the editor, and every message sent to the
//! editor. Iris state lives in a separate workspace actor that receives the messages defined in
//! [`service`] and never addresses the editor directly.

mod server;
mod service;
mod transport;

#[cfg(test)]
mod tests;

pub use server::{Server, ServerError};
pub use service::{
    Answer, ControlMessage, OrderedMessage, Rejection, SettingsResponse, WorkspaceEvent,
    WorkspaceEventSender, WorkspaceFailure, WorkspaceReceivers, WorkspaceSenders,
};
pub use transport::{Transport, TransportError};
