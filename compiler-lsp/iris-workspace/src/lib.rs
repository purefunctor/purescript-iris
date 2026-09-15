//! A single-consumer workspace service, independent of protocol transports.
//!
//! Inputs are admitted synchronously through [`Workspace::send`]. Compiler work runs on a
//! separate worker; neither input admission nor cancellation waits for compiler snapshots.
//! Interactive replies settle once; background publications are checked at service-loop handoff.
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
pub use configuration::{Configuration, SourceDiscovery};
pub use events::EventReceiver;
pub use language_server::{LanguageServer, LanguageServerFailure};
pub use transport::{
    Cancellation, Delivery, Reply, Request, Workspace, WorkspaceJoin, WorkspaceSession,
};

use std::path::PathBuf;
use std::sync::Arc;

use lsp_types::{Diagnostic, SemanticTokensLegend, TextDocumentContentChangeEvent, Url};

/// Token indices in analysis responses refer to these analyzer-owned tables.
pub fn semantic_tokens_legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: analyzer::semantic_tokens::TOKEN_TYPES.to_vec(),
        token_modifiers: analyzer::semantic_tokens::TOKEN_MODIFIERS.to_vec(),
    }
}

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
    /// Sequence of the last potential analysis write, excluding configuration policy updates.
    pub revision: InputSequence,
}

#[derive(Clone, Debug)]
pub struct ConfigurationInput {
    /// Discovery commands run in this directory, with arguments passed without shell evaluation.
    pub root: PathBuf,
    pub settings: Configuration,
}

pub enum Command {
    /// Ready workspaces retain their compiler when root and source discovery are unchanged.
    /// Diagnostic triggers affect future inputs; pending diagnostics finish against current inputs.
    /// Other states start a new preparation attempt, including retries of a failed configuration.
    /// Every accepted command produces a retained `Event::ConfigurationFinished`.
    Configure(ConfigurationInput),
    /// Rediscover and reload all sources, then schedule diagnostics for editable sources.
    Reload,
    Document(Document),
    /// Foreign-only changes reconcile locally. Other changes conservatively rediscover all sources.
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

/// Documents use file URLs that round-trip through their local path unchanged, without a query
/// or fragment. This does not resolve filesystem aliases or require a file to exist.
pub enum Document {
    Open { uri: Url, text: Arc<str>, version: i32 },
    Change { uri: Url, version: i32, changes: Vec<TextDocumentContentChangeEvent> },
    Save(Url),
    Close(Url),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Status {
    AwaitingConfiguration,
    Rebuilding {
        generation: Generation,
        phase: Phase,
    },
    Ready {
        generation: Generation,
        /// Last reconciled input, including inputs that do not invalidate analysis.
        sequence: InputSequence,
        stamp: AnalysisStamp,
    },
    Failed {
        generation: Generation,
        message: Arc<str>,
    },
    Stopping,
    /// The worker has stopped. The controller is joined separately through `WorkspaceJoin`.
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConfigurationOutcome {
    Unchanged,
    PolicyUpdated,
    Rebuilt,
    Failed { message: Arc<str> },
    Superseded,
    Cancelled,
}

#[derive(Debug)]
pub enum Event {
    StatusChanged(Status),
    Diagnostics {
        uri: Url,
        version: Option<i32>,
        diagnostics: Vec<Diagnostic>,
    },
    /// Indeterminate progress: the current phase, not a delta or percentage.
    Progress {
        generation: Generation,
        phase: Phase,
    },
    DiagnosticsFailed {
        uri: Url,
        message: Arc<str>,
    },
    Finished {
        generation: Generation,
        outcome: Outcome,
    },
    InputRejected {
        sequence: InputSequence,
        failure: InputFailure,
    },
    /// Exactly one retained terminal outcome for each admitted Configure, keyed by its sequence.
    ConfigurationFinished {
        sequence: InputSequence,
        outcome: ConfigurationOutcome,
    },
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
