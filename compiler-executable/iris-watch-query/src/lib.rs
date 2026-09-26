//! Name-based queries about the project that `iris watch` keeps compiled.
//!
//! [`answer`] reads a [`QueryContext`] and returns a query's answer as JSON; [`render`] turns that
//! JSON into text for agents. Queries address the loaded context by qualified name, such as
//! `Data.Foo.bar`, never by file position.

mod lookup;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use building::{QueryEngine, QueryError};
use files::FileId;
use iris_analysis::AnalyzerError;
use iris_build::SourceFile;
use iris_watch_server::protocol::Query;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What the latest rebuild did, which decides whether `output/` matches the engine.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "lowercase")]
pub enum BuildState {
    /// Every module was written to `output/`.
    Succeeded,
    /// Some module has errors, so nothing was written.
    Diagnostics,
    /// The project has no sources.
    NoInputs,
    /// Reading sources or writing `output/` failed.
    Failed { message: String },
}

/// A snapshot of the build that one query reads.
pub struct QueryContext {
    pub engine: QueryEngine,
    pub files: Arc<BTreeMap<FileId, SourceFile>>,
    pub build: BuildState,
    pub root: PathBuf,
}

#[derive(Debug, PartialEq, Eq)]
pub enum QueryFailure {
    /// A change to the engine's inputs cancelled the query.
    Cancelled,
    /// The query failed, for example because it named an unknown module.
    Failed(String),
}

impl From<QueryError> for QueryFailure {
    fn from(error: QueryError) -> QueryFailure {
        match error {
            QueryError::Cancelled => QueryFailure::Cancelled,
            error => QueryFailure::Failed(error.to_string()),
        }
    }
}

impl From<AnalyzerError> for QueryFailure {
    fn from(error: AnalyzerError) -> QueryFailure {
        match error {
            AnalyzerError::QueryError(error) => QueryFailure::from(error),
            error => QueryFailure::Failed(error.to_string()),
        }
    }
}

/// The answer to `wait`, which the watcher computes itself.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaitAnswer {
    pub build: BuildState,
}

/// A declaration's signature and documentation comment.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Declaration {
    pub signature: String,
    pub documentation: Option<String>,
}

/// The answer to `signature` and `module`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclarationsAnswer {
    pub declarations: Vec<Declaration>,
}

/// The answer to `definition` and `references`: `path:line:column` locations.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocationsAnswer {
    pub locations: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstancesAnswer {
    pub instances: Vec<InstanceEntry>,
}

/// An instance's location and head, such as `forall a. Show a => Show (Array a)`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceEntry {
    pub location: String,
    pub head: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticsAnswer {
    pub diagnostics: Vec<DiagnosticEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticEntry {
    pub location: String,
    pub severity: DiagnosticSeverity,
    pub code: String,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticSeverity {
    Error,
    Warning,
}

impl DiagnosticSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            DiagnosticSeverity::Error => "error",
            DiagnosticSeverity::Warning => "warning",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JavascriptAnswer {
    pub source: String,
}

/// Serializes an answer for the protocol.
pub fn to_value(answer: &impl Serialize) -> Value {
    serde_json::to_value(answer).expect("invariant violated: an answer failed to serialize")
}

/// Answers `query` from `context`.
pub fn answer(query: &Query, context: &QueryContext) -> Result<Value, QueryFailure> {
    match query {
        Query::Wait => unreachable!("invariant violated: the watcher answers `wait` itself"),
        Query::Signature { name, namespace } => {
            lookup::signature(context, name, *namespace).map(|answer| to_value(&answer))
        }
        Query::Module { name } => lookup::module(context, name).map(|answer| to_value(&answer)),
        Query::Definition { name, namespace } => {
            lookup::definition(context, name, *namespace).map(|answer| to_value(&answer))
        }
        Query::References { name, namespace } => {
            lookup::references(context, name, *namespace).map(|answer| to_value(&answer))
        }
        Query::Instances { name, search } => {
            lookup::instances(context, name, *search).map(|answer| to_value(&answer))
        }
        Query::Diagnostics { name } => {
            lookup::diagnostics(context, name.as_deref()).map(|answer| to_value(&answer))
        }
        Query::Javascript { name } => {
            lookup::javascript(context, name).map(|answer| to_value(&answer))
        }
    }
}

/// Renders the answer to `query` as text for agents: PureScript syntax for signatures,
/// `path:line:column` for locations, and no decoration.
pub fn render(query: &Query, value: Value) -> Result<String, serde_json::Error> {
    let text = match query {
        Query::Wait => {
            let WaitAnswer { build } = serde_json::from_value(value)?;
            match build {
                BuildState::Succeeded => "Build succeeded.".to_string(),
                BuildState::Diagnostics => {
                    "Build has diagnostics; see `iris watch query diagnostics`.".to_string()
                }
                BuildState::NoInputs => "No input files.".to_string(),
                BuildState::Failed { message } => format!("Build failed: {message}"),
            }
        }
        Query::Signature { .. } | Query::Module { .. } => {
            let DeclarationsAnswer { declarations } = serde_json::from_value(value)?;
            if declarations.is_empty() {
                return Ok("No exports.".to_string());
            }
            let declarations = declarations.iter().map(|declaration| {
                let documentation = declaration.documentation.iter().flat_map(|documentation| {
                    documentation.lines().map(|line| format!("-- | {line}").trim_end().to_string())
                });
                let lines = documentation.chain([String::clone(&declaration.signature)]);
                lines.collect::<Vec<_>>().join("\n")
            });
            declarations.collect::<Vec<_>>().join("\n\n")
        }
        Query::Definition { .. } | Query::References { .. } => {
            let LocationsAnswer { locations } = serde_json::from_value(value)?;
            if locations.is_empty() {
                return Ok("No locations.".to_string());
            }
            locations.join("\n")
        }
        Query::Instances { .. } => {
            let InstancesAnswer { instances } = serde_json::from_value(value)?;
            if instances.is_empty() {
                return Ok("No instances.".to_string());
            }
            let instances = instances.iter().map(|instance| {
                let head = instance.head.as_deref().unwrap_or("<unchecked>");
                format!("{}: instance {head}", instance.location)
            });
            instances.collect::<Vec<_>>().join("\n")
        }
        Query::Diagnostics { .. } => {
            let DiagnosticsAnswer { diagnostics } = serde_json::from_value(value)?;
            if diagnostics.is_empty() {
                return Ok("No diagnostics.".to_string());
            }
            let diagnostics = diagnostics.iter().map(|diagnostic| {
                let DiagnosticEntry { location, severity, code, message } = diagnostic;
                format!("{location}: {}[{code}]: {message}", severity.as_str())
            });
            diagnostics.collect::<Vec<_>>().join("\n")
        }
        Query::Javascript { .. } => {
            let JavascriptAnswer { source } = serde_json::from_value(value)?;
            source
        }
    };
    Ok(text)
}
