//! The watcher's query socket.
//!
//! `iris watch` answers questions about the project it keeps compiled through a local socket: a
//! Unix domain socket on Unix and a named pipe on Windows. [`serve`] accepts clients and forwards
//! each request to the watcher as a [`QueryRequest`]; [`client`] connects from the other side,
//! finding the socket through the socket file that [`discovery`] manages.

pub mod client;
pub mod discovery;
pub mod protocol;
mod server;
pub mod transport;

pub use server::{QueryRequest, serve};
