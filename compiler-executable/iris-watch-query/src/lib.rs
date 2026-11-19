//! Name-based queries about the project that `iris watch` keeps compiled.
//!
//! [`answer`] reads a [`QueryContext`] and returns a query's answer as JSON; [`render`] turns that
//! JSON into text for agents. Queries address the loaded context by qualified name, such as
//! `Data.Foo.bar`, never by file position.

mod lookup;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::Arc;

use building::{QueryEngine, QueryError};
use files::FileId;
use iris_analysis::AnalyzerError;
use iris_analysis::refactor::{SourceEdit, apply_edits};
use iris_build::SourceFile;
use iris_watch_server::protocol::Query;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use similar::TextDiff;

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
pub struct DependentsAnswer {
    pub dependents: Vec<Dependent>,
}

/// A module that imports the queried module directly, or through `through`, the module it imports
/// on the way.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dependent {
    pub module: String,
    pub path: String,
    pub through: Option<String>,
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
pub struct SearchAnswer {
    pub results: Vec<SearchResult>,
    /// How many declarations matched, including those left out of `results`.
    pub matches: usize,
}

/// A matching declaration's qualified name and, if it was checked, its signature under that name
/// on one line.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchResult {
    pub name: String,
    pub signature: Option<String>,
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
    pub stylex_theme_source: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefactorAnswer {
    pub files: Vec<RefactorFile>,
    pub skipped: Vec<RefactorSkipped>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefactorFile {
    pub path: String,
    pub sha256: String,
    pub edits: Vec<RefactorEdit>,
    pub signatures_added: usize,
    pub imports_added: usize,
    pub imports_updated: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefactorEdit {
    pub start_byte: usize,
    pub end_byte: usize,
    pub line: usize,
    pub column: usize,
    pub new_text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefactorSkipped {
    pub location: String,
    pub name: String,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

pub fn sha256_hex(content: &str) -> String {
    let digest = Sha256::digest(content.as_bytes());
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

pub fn parse_refactor_answer(value: Value) -> Result<RefactorAnswer, serde_json::Error> {
    serde_json::from_value(value)
}

pub fn apply_file_edits(source: &str, edits: &[RefactorEdit]) -> Result<String, String> {
    let source_edits = edits.iter().map(|edit| SourceEdit {
        start_byte: edit.start_byte,
        end_byte: edit.end_byte,
        new_text: String::clone(&edit.new_text),
    });
    let source_edits = source_edits.collect::<Vec<_>>();
    apply_edits(source, &source_edits).map_err(|error| error.to_string())
}

pub fn render_unified_diff(path: &str, original: &str, modified: &str) -> String {
    let diff = TextDiff::from_lines(original, modified);
    let mut unified = diff.unified_diff();
    unified.context_radius(3);
    unified.header(&format!("a/{path}"), &format!("b/{path}"));
    unified.to_string()
}

pub fn render_refactor_skipped(answer: &RefactorAnswer) -> Option<String> {
    if answer.skipped.is_empty() {
        return None;
    }
    let mut lines = Vec::with_capacity(answer.skipped.len() + 1);
    lines.push(format!("Skipped {}:", answer.skipped.len()));
    for entry in &answer.skipped {
        let line = match &entry.detail {
            Some(detail) => {
                format!("{}: {} ({}: {detail})", entry.location, entry.name, entry.reason)
            }
            None => format!("{}: {} ({})", entry.location, entry.name, entry.reason),
        };
        lines.push(line);
    }
    Some(lines.join("\n"))
}

pub fn render_refactor_summary(answer: &RefactorAnswer) -> String {
    let file_count = answer.files.len();
    let signatures_added: usize = answer.files.iter().map(|file| file.signatures_added).sum();
    let imports_added: usize = answer.files.iter().map(|file| file.imports_added).sum();
    let imports_updated: usize = answer.files.iter().map(|file| file.imports_updated).sum();

    let mut lines = Vec::new();
    if file_count == 0 {
        lines.push("No changes.".to_string());
    } else {
        let mut parts = vec![format!(
            "Added {signatures_added} {} across {file_count} {}",
            plural(signatures_added, "signature", "signatures"),
            plural(file_count, "file", "files"),
        )];
        if imports_added > 0 {
            parts.push(format!(
                "added {imports_added} {}",
                plural(imports_added, "import", "imports"),
            ));
        }
        if imports_updated > 0 {
            parts.push(format!(
                "updated {imports_updated} {}",
                plural(imports_updated, "import", "imports"),
            ));
        }
        lines.push(format!("{}.", parts.join(", ")));
    }

    if let Some(skipped) = render_refactor_skipped(answer) {
        lines.push(skipped);
    }

    lines.join("\n")
}

fn plural<'a>(count: usize, singular: &'a str, plural: &'a str) -> &'a str {
    if count == 1 { singular } else { plural }
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
        Query::Dependents { name } => {
            lookup::dependents(context, name).map(|answer| to_value(&answer))
        }
        Query::Instances { name, search } => {
            lookup::instances(context, name, *search).map(|answer| to_value(&answer))
        }
        Query::Search { pattern } => {
            lookup::search(context, pattern).map(|answer| to_value(&answer))
        }
        Query::Diagnostics { name } => {
            lookup::diagnostics(context, name.as_deref()).map(|answer| to_value(&answer))
        }
        Query::Javascript { name } => {
            lookup::javascript(context, name).map(|answer| to_value(&answer))
        }
        Query::Refactor { refactor, module } => {
            lookup::refactor(context, *refactor, module.as_deref()).map(|answer| to_value(&answer))
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
        Query::Dependents { .. } => {
            let DependentsAnswer { dependents } = serde_json::from_value(value)?;
            if dependents.is_empty() {
                return Ok("No dependents.".to_string());
            }
            let dependents = dependents.iter().map(|dependent| {
                let Dependent { module, path, through } = dependent;
                if let Some(through) = through {
                    format!("{path}: {module} (through {through})")
                } else {
                    format!("{path}: {module}")
                }
            });
            dependents.collect::<Vec<_>>().join("\n")
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
        Query::Search { .. } => {
            let SearchAnswer { results, matches } = serde_json::from_value(value)?;
            if results.is_empty() {
                return Ok("No matches.".to_string());
            }
            let results = results.iter().map(|result| {
                let signature = result.signature.as_deref();
                signature.map_or_else(|| format!("{} :: <unchecked>", result.name), str::to_string)
            });
            let mut lines = results.collect::<Vec<_>>();
            if matches > lines.len() {
                let more = matches - lines.len();
                lines.push(format!("... and {more} more matches; search for a longer name"));
            }
            lines.join("\n")
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
        Query::Javascript { name } => {
            let JavascriptAnswer { source, stylex_theme_source } = serde_json::from_value(value)?;
            match stylex_theme_source {
                Some(stylex_theme_source) => {
                    let filename = javascript::module_filename(name);
                    let theme_filename = javascript::stylex_theme_module_filename(name);
                    format!("// {filename}\n{source}\n// {theme_filename}\n{stylex_theme_source}")
                }
                None => source,
            }
        }
        Query::Refactor { .. } => {
            let answer = serde_json::from_value::<RefactorAnswer>(value)?;
            render_refactor_summary(&answer)
        }
    };
    Ok(text)
}
