//! The queries themselves, over a snapshot of the engine.

use std::fmt;
use std::path::Path;

use files::FileId;
use iris_analysis::nominal::{self, NamedItem};
use iris_analysis::position::PositionEncoding;
use iris_analysis::{AnalyzerCapabilities, AnalyzerContext, AnalyzerError, AnalyzerHost};
use iris_build::SourceKind;
use iris_diagnostics::Severity;
use iris_watch_server::protocol::{InstanceSearch, Namespace};
use lsp_types::{Location, Uri};
use url::Url;

use crate::{
    BuildState, Declaration, DeclarationsAnswer, DiagnosticEntry, DiagnosticSeverity,
    DiagnosticsAnswer, InstanceEntry, InstancesAnswer, JavascriptAnswer, LocationsAnswer,
    QueryContext, QueryFailure,
};

pub(crate) fn signature(
    context: &QueryContext,
    name: &str,
    namespace: Option<Namespace>,
) -> Result<DeclarationsAnswer, QueryFailure> {
    let items = named_items(context, name, namespace)?;
    let declarations = items.into_iter().map(|item| declaration(context, item));
    let declarations = declarations.collect::<Result<Vec<_>, _>>()?;
    let declarations = declarations.into_iter().flatten().collect::<Vec<_>>();
    if declarations.is_empty() {
        let message =
            format!("`{name}` has no checked signature; see `iris watch query diagnostics`");
        return Err(QueryFailure::Failed(message));
    }
    Ok(DeclarationsAnswer { declarations })
}

pub(crate) fn module(
    context: &QueryContext,
    name: &str,
) -> Result<DeclarationsAnswer, QueryFailure> {
    let file_id = module_file(context, name)?;
    let resolved = context.engine.resolved(file_id)?;
    let exports = &resolved.exports;
    let mut items = exports
        .iter_types()
        .chain(exports.iter_classes())
        .map(|(name, file_id, type_id)| (name, NamedItem::Type(file_id, type_id)))
        .collect::<Vec<_>>();
    let mut terms = exports
        .iter_terms()
        .map(|(name, file_id, term_id)| (name, NamedItem::Term(file_id, term_id)))
        .collect::<Vec<_>>();
    items.sort_by_key(|(name, _)| *name);
    terms.sort_by_key(|(name, _)| *name);
    let declarations = items.into_iter().chain(terms).map(|(_, item)| declaration(context, item));
    let declarations = declarations.collect::<Result<Vec<_>, _>>()?;
    Ok(DeclarationsAnswer { declarations: declarations.into_iter().flatten().collect() })
}

pub(crate) fn definition(
    context: &QueryContext,
    name: &str,
    namespace: Option<Namespace>,
) -> Result<LocationsAnswer, QueryFailure> {
    let items = named_items(context, name, namespace)?;
    let host = WatchHost { context };
    let analyzer =
        AnalyzerContext::new(&host, PositionEncoding::Utf32, AnalyzerCapabilities::default());
    let locations = items.into_iter().map(|item| {
        let location = nominal::definition(&analyzer, item)?;
        Ok::<_, AnalyzerError>(source_position(context, &location).to_string())
    });
    let locations = locations.collect::<Result<Vec<_>, _>>()?;
    Ok(LocationsAnswer { locations })
}

pub(crate) fn references(
    context: &QueryContext,
    name: &str,
    namespace: Option<Namespace>,
) -> Result<LocationsAnswer, QueryFailure> {
    let items = named_items(context, name, namespace)?;
    let host = WatchHost { context };
    let analyzer =
        AnalyzerContext::new(&host, PositionEncoding::Utf32, AnalyzerCapabilities::default());
    let mut positions = Vec::new();
    for item in items {
        let references = nominal::references(&analyzer, item)?;
        positions.extend(references.iter().map(|location| source_position(context, location)));
    }
    positions.sort();
    positions.dedup();
    let locations = positions.iter().map(SourcePosition::to_string).collect();
    Ok(LocationsAnswer { locations })
}

pub(crate) fn instances(
    context: &QueryContext,
    name: &str,
    search: InstanceSearch,
) -> Result<InstancesAnswer, QueryFailure> {
    let items = named_items(context, name, Some(Namespace::Type))?;
    let Some(&NamedItem::Type(file_id, type_id)) = items.first() else {
        unreachable!("invariant violated: the type namespace holds only types and classes");
    };
    let target = (file_id, type_id);
    match (search, nominal::is_class(&context.engine, target)?) {
        (InstanceSearch::Class, false) => {
            let message = format!("`{name}` is not a class; see `instances type {name}`");
            return Err(QueryFailure::Failed(message));
        }
        (InstanceSearch::Type, true) => {
            let message = format!("`{name}` is a class; see `instances class {name}`");
            return Err(QueryFailure::Failed(message));
        }
        (InstanceSearch::Class, true) | (InstanceSearch::Type, false) => {}
    }
    let search = match search {
        InstanceSearch::Class => nominal::InstanceSearch::OfClass,
        InstanceSearch::Type => nominal::InstanceSearch::MentioningType,
    };
    let host = WatchHost { context };
    let analyzer =
        AnalyzerContext::new(&host, PositionEncoding::Utf32, AnalyzerCapabilities::default());
    let found = nominal::instances(&analyzer, target, search)?;
    let positioned = found
        .into_iter()
        .map(|instance| (source_position(context, &instance.location), instance.head));
    let mut positioned = positioned.collect::<Vec<_>>();
    positioned.sort_by(|(left, _), (right, _)| left.cmp(right));
    let instances = positioned
        .into_iter()
        .map(|(position, head)| InstanceEntry { location: position.to_string(), head });
    Ok(InstancesAnswer { instances: instances.collect() })
}

pub(crate) fn diagnostics(
    context: &QueryContext,
    name: Option<&str>,
) -> Result<DiagnosticsAnswer, QueryFailure> {
    let files = match name {
        Some(name) => vec![module_file(context, name)?],
        None => {
            let files = context.files.iter();
            let project = files.filter(|(_, file)| file.kind == SourceKind::Project);
            project.map(|(file_id, _)| *file_id).collect()
        }
    };
    let collections = iris_diagnostics::collect_diagnostics(&context.engine, &files)?;
    let mut diagnostics = Vec::new();
    for collection in collections {
        let content = context.engine.content(collection.file_id)?;
        let path = display_path(context, collection.file_id);
        for diagnostic in collection.diagnostics() {
            let (line, column) = line_column(&content, diagnostic.span.start as usize);
            let severity = match diagnostic.severity {
                Severity::Error => DiagnosticSeverity::Error,
                Severity::Warning => DiagnosticSeverity::Warning,
            };
            diagnostics.push(DiagnosticEntry {
                location: format!("{path}:{line}:{column}"),
                severity,
                code: diagnostic.code.to_string(),
                message: String::clone(&diagnostic.message),
            });
        }
    }
    Ok(DiagnosticsAnswer { diagnostics })
}

pub(crate) fn javascript(
    context: &QueryContext,
    name: &str,
) -> Result<JavascriptAnswer, QueryFailure> {
    let file_id = module_file(context, name)?;
    let reason = match &context.build {
        BuildState::Succeeded => None,
        BuildState::Diagnostics => Some("some module has errors".to_string()),
        BuildState::NoInputs => Some("the project has no sources".to_string()),
        BuildState::Failed { message } => Some(format!("the rebuild failed: {message}")),
    };
    if let Some(reason) = reason {
        return Err(QueryFailure::Failed(format!(
            "the latest rebuild did not write output/ because {reason}; see `iris watch query wait`"
        )));
    }
    match context.engine.javascript(file_id)? {
        Ok(module) => Ok(JavascriptAnswer { source: module.source().to_string() }),
        Err(_) => Err(QueryFailure::Failed(format!("no JavaScript was generated for {name}"))),
    }
}

fn declaration(
    context: &QueryContext,
    item: NamedItem,
) -> Result<Option<Declaration>, QueryFailure> {
    let signature = match nominal::signature(&context.engine, item) {
        Ok(signature) => signature,
        // Items without a checked signature, such as those in modules that failed to check,
        // are left out rather than failing the whole answer.
        Err(AnalyzerError::NonFatal) => return Ok(None),
        Err(error) => return Err(QueryFailure::from(error)),
    };
    let documentation = nominal::documentation(&context.engine, item)?;
    Ok(Some(Declaration { signature, documentation }))
}

/// The value and the type or class that a qualified name such as `Data.Maybe.Maybe` denotes, or
/// only those in `namespace`.
fn named_items(
    context: &QueryContext,
    name: &str,
    namespace: Option<Namespace>,
) -> Result<Vec<NamedItem>, QueryFailure> {
    let unknown = || {
        QueryFailure::Failed(format!("`{name}` is not a qualified name of a loaded module's item"))
    };
    // Operators may contain dots, so the module is the longest prefix that names one.
    let splits = name.match_indices('.').map(|(index, _)| index).collect::<Vec<_>>();
    for index in splits.into_iter().rev() {
        let (module, item) = (&name[..index], &name[index + 1..]);
        let Some(file_id) = context.engine.module_file(module) else { continue };
        let item = item.strip_prefix('(').and_then(|item| item.strip_suffix(')')).unwrap_or(item);
        let items = nominal::lookup(&context.engine, file_id, item)?;
        let items = items.into_iter().filter(|named| match (namespace, named) {
            (None, _) => true,
            (Some(Namespace::Value), NamedItem::Term(..)) => true,
            (Some(Namespace::Type), NamedItem::Type(..)) => true,
            (Some(_), _) => false,
        });
        let items = items.collect::<Vec<_>>();
        if items.is_empty() {
            let kind = match namespace {
                None => "item",
                Some(Namespace::Value) => "value",
                Some(Namespace::Type) => "type or class",
            };
            return Err(QueryFailure::Failed(format!(
                "module {module} has no {kind} named `{item}`"
            )));
        }
        return Ok(items);
    }
    Err(unknown())
}

fn module_file(context: &QueryContext, name: &str) -> Result<FileId, QueryFailure> {
    context
        .engine
        .module_file(name)
        .ok_or_else(|| QueryFailure::Failed(format!("no loaded module is named {name}")))
}

/// A location as `path:line:column`, counted from 1, which orders by line and column numerically
/// rather than as text.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct SourcePosition {
    path: String,
    line: u32,
    column: u32,
}

impl fmt::Display for SourcePosition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}:{}", self.path, self.line, self.column)
    }
}

fn source_position(context: &QueryContext, location: &Location) -> SourcePosition {
    let path = Url::parse(location.uri.as_str()).ok().and_then(|url| url.to_file_path().ok());
    let path =
        path.map_or_else(|| location.uri.as_str().to_string(), |path| relative(context, &path));
    let line = location.range.start.line + 1;
    let column = location.range.start.character + 1;
    SourcePosition { path, line, column }
}

fn display_path(context: &QueryContext, file_id: FileId) -> String {
    let file = context.files.get(&file_id);
    file.map_or_else(|| "<unknown>".to_string(), |file| relative(context, &file.path))
}

/// The path relative to the project root when inside it, with forward slashes on every platform
/// as in the build's diagnostics.
fn relative(context: &QueryContext, path: &Path) -> String {
    let path = path.strip_prefix(&context.root).unwrap_or(path);
    path.to_string_lossy().replace('\\', "/")
}

/// The 1-based line and character column of a byte offset.
fn line_column(content: &str, offset: usize) -> (usize, usize) {
    let before = &content[..offset.min(content.len())];
    let line_start = before.rfind('\n').map_or(0, |index| index + 1);
    let line = before.matches('\n').count() + 1;
    let column = before[line_start..].chars().count() + 1;
    (line, column)
}

struct WatchHost<'a> {
    context: &'a QueryContext,
}

impl AnalyzerHost for WatchHost<'_> {
    type Queries = building::QueryEngine;

    fn queries(&self) -> &building::QueryEngine {
        &self.context.engine
    }

    fn file_id(&self, uri: &str) -> Option<FileId> {
        let path = Url::parse(uri).ok()?.to_file_path().ok()?;
        let mut files = self.context.files.iter();
        files.find(|(_, file)| file.path == path).map(|(file_id, _)| *file_id)
    }

    fn file_uri(&self, file_id: FileId) -> Result<Option<Uri>, url::ParseError> {
        let Some(file) = self.context.files.get(&file_id) else { return Ok(None) };
        let Ok(url) = Url::from_file_path(&file.path) else { return Ok(None) };
        Uri::parse(url.as_str()).map(Some)
    }

    fn active_files(&self) -> impl Iterator<Item = FileId> + '_ {
        self.context.files.keys().copied()
    }

    fn is_editable(&self, file_id: FileId) -> bool {
        self.context.files.get(&file_id).is_some_and(|file| file.kind == SourceKind::Project)
    }
}
