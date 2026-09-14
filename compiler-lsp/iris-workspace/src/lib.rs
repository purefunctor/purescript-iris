//! A single-consumer workspace service, independent of protocol transports.
//!
//! Inputs are admitted synchronously through [`Workspace::send`]. Compiler work runs on a
//! separate worker; neither input admission nor cancellation waits for compiler snapshots.
//! Call [`Delivery::release`] immediately before delivering a result, without another await.
//! Configuration replacement discards compilation state but preserves open documents.

mod controller;
mod documents;
mod events;
mod language_server;
mod transport;
mod worker;

#[cfg(feature = "test-support")]
pub mod testing;
#[cfg(not(feature = "test-support"))]
mod testing;

pub use analyzer::AnalyzerCapabilities;
pub use analyzer::position::PositionEncoding;
pub use configuration::Configuration;
pub use events::EventReceiver;
pub use language_server::{LanguageServer, LanguageServerFailure};
pub use transport::{Cancellation, Delivery, Reply, Request, Workspace};

use std::path::PathBuf;
use std::sync::Arc;

use iris_build::events::BuildEvent;
use lsp_types::{Diagnostic, TextDocumentContentChangeEvent, Url};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Generation {
    pub value: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Incarnation {
    pub value: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub struct InputSequence {
    pub value: u64,
}

impl Generation {
    pub(crate) fn advance(&mut self) {
        self.value = self.value.checked_add(1).expect("configuration generation overflow");
    }
}

impl Incarnation {
    pub(crate) fn advance(&mut self) {
        self.value = self.value.checked_add(1).expect("engine incarnation overflow");
    }
}

impl InputSequence {
    pub(crate) fn advance(&mut self) {
        self.value = self.value.checked_add(1).expect("input sequence overflow");
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AnalysisStamp {
    pub incarnation: Incarnation,
    pub revision: InputSequence,
}

#[derive(Clone, Debug)]
pub struct ConfigurationInput {
    pub root: PathBuf,
    pub settings: Configuration,
}

pub enum Command {
    Configure(ConfigurationInput),
    Reload,
    Document(Document),
    FilesChanged(Vec<Url>),
    LanguageServer(LanguageServer),
    Shutdown,
}

impl Command {
    pub(crate) fn rebuilds(&self) -> bool {
        match self {
            Command::Configure(_) | Command::Reload => true,
            Command::FilesChanged(uris) => {
                uris.iter().any(|uri| !uri.path().ends_with(".js") && !uri.path().ends_with(".jsx"))
            }
            _ => false,
        }
    }
}

pub enum Document {
    Open { uri: Url, text: Arc<str>, version: i32 },
    Change { uri: Url, version: i32, changes: Vec<TextDocumentContentChangeEvent> },
    Save(Url),
    Close(Url),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Status {
    AwaitingConfiguration,
    Rebuilding { generation: Generation, phase: Phase },
    Ready { generation: Generation, stamp: AnalysisStamp },
    Failed { generation: Generation, message: Arc<str> },
    Stopping,
    Stopped,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Phase {
    Retiring,
    Discovering,
    Building,
    Reconciling,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Outcome {
    Ready,
    Failed,
    Superseded,
    Cancelled,
}

#[derive(Debug)]
pub enum Event {
    StatusChanged(Status),
    Diagnostics { uri: Url, version: Option<i32>, diagnostics: Vec<Diagnostic> },
    Progress { generation: Generation, event: BuildEvent },
    Finished { generation: Generation, outcome: Outcome },
    InputRejected { sequence: InputSequence, failure: InputFailure },
}

#[derive(Clone, Debug, thiserror::Error, Eq, PartialEq)]
pub enum InputFailure {
    #[error("unsupported document URI: {0}")]
    UnsupportedDocument(Url),
    #[error("document is not open: {0}")]
    NotOpen(Url),
    #[error("document is already open: {0}")]
    AlreadyOpen(Url),
    #[error("document version is not newer: {0}")]
    StaleVersion(Url),
    #[error("invalid document edit range: {0}")]
    InvalidRange(Url),
}

#[derive(Clone, Debug, thiserror::Error)]
pub enum RequestFailure {
    #[error("workspace is unavailable")]
    Unavailable,
    #[error("workspace request capacity is exhausted")]
    Busy,
    #[error("analysis has been invalidated")]
    Stale,
    #[error("request was cancelled")]
    Cancelled,
    #[error(transparent)]
    InvalidInput(#[from] InputFailure),
    #[error("workspace failed: {0}")]
    Workspace(Arc<str>),
    #[error(transparent)]
    LanguageServer(#[from] LanguageServerFailure),
}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub position_encoding: PositionEncoding,
    pub capabilities: AnalyzerCapabilities,
    /// Includes executing requests; diagnostics do not consume interactive capacity.
    pub request_capacity: usize,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            position_encoding: PositionEncoding::Utf16,
            capabilities: AnalyzerCapabilities::default(),
            request_capacity: 32,
        }
    }
}
