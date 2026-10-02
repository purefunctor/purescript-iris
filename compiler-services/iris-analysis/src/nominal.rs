//! Analysis of items addressed by module and name rather than by a position in a file.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use building_types::QueryProxy;
use checking::core::pretty::{Pretty, PrettyConfig};
use files::FileId;
use indexing::{IndexedTypeItemKind, InstanceSourceItemId, TermItemId, TypeItemId};
use lowering::{LoweredModule, TypeId, TypeKind};
use lsp_types::Location;

use crate::extract::AnnotationSyntaxRange;
use crate::hover::{PRETTY_CONFIG, render_annotation};
use crate::position::PositionConverter;
use crate::{AnalyzerContext, AnalyzerError, AnalyzerQueries, common, references};

/// A declaration addressed by name: a value, or a type or class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NamedItem {
    Term(FileId, TermItemId),
    Type(FileId, TypeItemId),
}

/// A qualified name split into the module that declares an item and the item's own name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QualifiedName<'a> {
    pub module: &'a str,
    pub module_file: FileId,
    pub item: &'a str,
}

/// Splits a qualified name such as `Data.Maybe.fromMaybe` into its module and item. Operators may
/// contain dots, so the module is the longest prefix that names a loaded module. An operator may
/// be written in parentheses, such as `Data.Function.(<<<)`.
pub fn split_qualified_name<'a>(
    engine: &impl AnalyzerQueries,
    name: &'a str,
) -> Option<QualifiedName<'a>> {
    let splits = name.match_indices('.').map(|(index, _)| index).collect::<Vec<_>>();
    splits.into_iter().rev().find_map(|index| {
        let (module, item) = (&name[..index], &name[index + 1..]);
        let module_file = engine.module_file(module)?;
        let item = item.strip_prefix('(').and_then(|item| item.strip_suffix(')')).unwrap_or(item);
        Some(QualifiedName { module, module_file, item })
    })
}

/// The name in the module header of `file_id`, if it has one.
pub fn module_name(
    engine: &impl AnalyzerQueries,
    file_id: FileId,
) -> Result<Option<String>, AnalyzerError> {
    let content = engine.content(file_id)?;
    let (parsed, _) = engine.parsed(file_id)?;
    Ok(parsed.module_name(&content).map(|name| name.to_string()))
}

/// The items `name` denotes in the module `file_id`: the value and the type or class with that
/// name, from the module's own declarations or, failing those, its exports.
pub fn lookup(
    engine: &impl AnalyzerQueries,
    file_id: FileId,
    name: &str,
) -> Result<Vec<NamedItem>, AnalyzerError> {
    let resolved = engine.resolved(file_id)?;
    let locals = &resolved.locals;
    let exports = &resolved.exports;
    let term = locals.lookup_term(name).or_else(|| exports.lookup_term(name));
    let type_item = locals
        .lookup_type(name)
        .or_else(|| locals.lookup_class(name))
        .or_else(|| exports.lookup_type(name))
        .or_else(|| exports.lookup_class(name));
    let term = term.map(|(file_id, term_id)| NamedItem::Term(file_id, term_id));
    let type_item = type_item.map(|(file_id, type_id)| NamedItem::Type(file_id, type_id));
    Ok(term.into_iter().chain(type_item).collect())
}

/// The item's name as declared.
pub fn name(engine: &impl AnalyzerQueries, item: NamedItem) -> Result<String, AnalyzerError> {
    let indexed = engine.indexed(item.file_id())?;
    let name = match item {
        NamedItem::Term(_, term_id) => &indexed.items[term_id].name,
        NamedItem::Type(_, type_id) => &indexed.items[type_id].name,
    };
    Ok(name.as_deref().unwrap_or("<unknown>").to_string())
}

/// The item's name with its type, or with its kind for a type or class, as PureScript. Checking
/// records no type only for items it rejected with an error, such as an operator whose target
/// does not resolve; those have no signature.
pub fn signature(
    engine: &impl AnalyzerQueries,
    item: NamedItem,
) -> Result<Option<String>, AnalyzerError> {
    let name = name(engine, item)?;
    render_signature(engine, item, &name, PRETTY_CONFIG)
}

/// Like [`signature`], but under `name`, such as a qualified name, and never wrapped, so that it
/// fits on one line of a list.
pub fn signature_on_one_line(
    engine: &impl AnalyzerQueries,
    item: NamedItem,
    name: &str,
) -> Result<Option<String>, AnalyzerError> {
    render_signature(engine, item, name, PRETTY_CONFIG.width(usize::MAX))
}

fn render_signature(
    engine: &impl AnalyzerQueries,
    item: NamedItem,
    name: &str,
    config: PrettyConfig,
) -> Result<Option<String>, AnalyzerError> {
    let checked = engine.checked(item.file_id())?;
    let signature = match item {
        NamedItem::Term(_, term_id) => checked.lookup_term_item_type(term_id),
        NamedItem::Type(_, type_id) => checked.lookup_type_item_kind(type_id),
    };
    let Some(signature) = signature else { return Ok(None) };
    let pretty = Pretty::with_config(engine, &checked, config);
    Ok(Some(pretty.render_signature(name, signature).to_string()))
}

/// The documentation comment written above the item, if any.
pub fn documentation(
    engine: &impl AnalyzerQueries,
    item: NamedItem,
) -> Result<Option<String>, AnalyzerError> {
    let file_id = item.file_id();
    let content = engine.content(file_id)?;
    let range = match item {
        NamedItem::Term(_, term_id) => {
            AnnotationSyntaxRange::of_file_term(engine, file_id, term_id)?
        }
        NamedItem::Type(_, type_id) => {
            AnnotationSyntaxRange::of_file_type(engine, file_id, type_id)?
        }
    };
    Ok(range.annotation.and_then(|range| render_annotation(&content, range)))
}

pub fn definition(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    item: NamedItem,
) -> Result<Location, AnalyzerError> {
    let file_id = item.file_id();
    let uri = common::file_uri(context, file_id)?;
    let content = context.queries().content(file_id)?;
    let positions = PositionConverter::new(&content, context.position_encoding());
    match item {
        NamedItem::Term(_, term_id) => {
            common::file_term_location(context, uri, file_id, &positions, term_id)
        }
        NamedItem::Type(_, type_id) => {
            common::file_type_location(context, uri, file_id, &positions, type_id)
        }
    }
}

/// Every use of the item in the context's active files, excluding its declaration.
pub fn references(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    item: NamedItem,
) -> Result<Vec<Location>, AnalyzerError> {
    let locations = match item {
        NamedItem::Term(file_id, term_id) => {
            references::references_file_term(context, file_id, file_id, term_id)?
        }
        NamedItem::Type(file_id, type_id) => {
            references::references_file_type(context, file_id, file_id, type_id)?
        }
    };
    Ok(locations.unwrap_or_default())
}

/// An instance or derived instance and its head, such as `forall a. Show a => Show (Array a)`.
pub struct FoundInstance {
    pub location: Location,
    pub head: Option<String>,
}

/// Which instances [`instances`] finds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstanceSearch {
    /// The instances of a class.
    OfClass,
    /// The instances whose head mentions a type, whatever their class.
    MentioningType,
}

/// Whether the type item is a class rather than a data type, newtype, synonym, or foreign type.
pub fn is_class(
    engine: &impl AnalyzerQueries,
    (file_id, type_id): (FileId, TypeItemId),
) -> Result<bool, AnalyzerError> {
    let indexed = engine.indexed(file_id)?;
    Ok(matches!(indexed.items[type_id].kind, IndexedTypeItemKind::Class { .. }))
}

/// Every instance in the context's active files that `search` finds for `target`.
pub fn instances(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    target: (FileId, TypeItemId),
    search: InstanceSearch,
) -> Result<Vec<FoundInstance>, AnalyzerError> {
    let engine = context.queries();
    let mut found = Vec::new();
    for file_id in context.active_files() {
        let indexed = engine.indexed(file_id)?;
        let lowered = engine.lowered(file_id)?;
        let checked = engine.checked(file_id)?;
        for &source in indexed.items.instance_sources() {
            let (resolution, arguments, checked_instance) = match source {
                InstanceSourceItemId::Instance(id) => {
                    let Some(item) = lowered.tree.get_instance_item(id) else { continue };
                    let checked_instance = checked.lookup_instance(indexed.items[id].id);
                    (item.resolution, &item.arguments, checked_instance)
                }
                InstanceSourceItemId::Derive(id) => {
                    let Some(item) = lowered.tree.get_derive_item(id) else { continue };
                    let checked_instance = checked.lookup_derived_instance(indexed.items[id].id);
                    (item.resolution, &item.arguments, checked_instance)
                }
            };
            let matches = match search {
                InstanceSearch::OfClass => resolution == Some(target),
                InstanceSearch::MentioningType => {
                    arguments.iter().any(|&argument| mentions(&lowered, argument, target))
                }
            };
            if !matches {
                continue;
            }
            let uri = common::file_uri(context, file_id)?;
            let location = match source {
                InstanceSourceItemId::Instance(id) => {
                    common::file_instance_location(context, uri, file_id, id)?
                }
                InstanceSourceItemId::Derive(id) => {
                    common::file_derive_location(context, uri, file_id, id)?
                }
            };
            let pretty = Pretty::with_config(engine, &checked, PRETTY_CONFIG);
            let head =
                checked_instance.map(|instance| pretty.render(instance.signature).to_string());
            found.push(FoundInstance { location, head });
        }
    }
    Ok(found)
}

/// Whether the type `type_id` refers to the type `target` anywhere within it.
fn mentions(lowered: &LoweredModule, type_id: TypeId, target: (FileId, TypeItemId)) -> bool {
    let Some(kind) = lowered.tree.get_type_kind(type_id) else { return false };
    match kind {
        TypeKind::Constructor { resolution } | TypeKind::Operator { resolution } => {
            *resolution == Some(target)
        }
        TypeKind::ApplicationChain { function, arguments } => {
            mentions_any(lowered, function.iter().chain(arguments.iter()).copied(), target)
        }
        TypeKind::Arrow { argument, result } => {
            mentions_any(lowered, argument.iter().chain(result).copied(), target)
        }
        TypeKind::Constrained { constraint, constrained } => {
            mentions_any(lowered, constraint.iter().chain(constrained).copied(), target)
        }
        TypeKind::Forall { inner, .. } => mentions_any(lowered, inner.iter().copied(), target),
        TypeKind::Kinded { type_, kind } => {
            mentions_any(lowered, type_.iter().chain(kind).copied(), target)
        }
        TypeKind::OperatorChain { head, tail } => {
            let elements = tail.iter().filter_map(|pair| pair.element);
            mentions_any(lowered, head.iter().copied().chain(elements), target)
        }
        TypeKind::Record { items, tail } | TypeKind::Row { items, tail } => {
            let items = items.iter().filter_map(|item| item.type_);
            mentions_any(lowered, items.chain(tail.iter().copied()), target)
        }
        TypeKind::Parenthesized { parenthesized } => {
            mentions_any(lowered, parenthesized.iter().copied(), target)
        }
        TypeKind::Hole
        | TypeKind::Integer { .. }
        | TypeKind::String { .. }
        | TypeKind::Variable { .. }
        | TypeKind::Wildcard => false,
    }
}

fn mentions_any(
    lowered: &LoweredModule,
    mut types: impl Iterator<Item = TypeId>,
    target: (FileId, TypeItemId),
) -> bool {
    types.any(|type_id| mentions(lowered, type_id, target))
}

/// A module in `files` that imports the queried module, directly or through `through`, the
/// module it imports on the way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dependent {
    pub file_id: FileId,
    pub through: Option<FileId>,
}

/// The modules in `files` that import `target`, directly or through other modules, closest first.
/// A module that imports `target` through others is reported through a module it imports itself,
/// so the report can be checked against its import list.
pub fn dependents(
    engine: &impl AnalyzerQueries,
    files: impl Iterator<Item = FileId>,
    target: FileId,
) -> Result<Vec<Dependent>, AnalyzerError> {
    let mut importers = BTreeMap::<FileId, Vec<FileId>>::new();
    for file_id in files {
        let resolved = engine.resolved(file_id)?;
        let imports = resolved.unqualified.values().chain(resolved.qualified.values());
        let imported = imports.flatten().map(|import| import.file).collect::<BTreeSet<_>>();
        for imported in imported {
            importers.entry(imported).or_default().push(file_id);
        }
    }

    let mut dependents = Vec::new();
    let mut visited = BTreeSet::from([target]);
    let mut pending = VecDeque::from([target]);
    while let Some(file_id) = pending.pop_front() {
        let through = (file_id != target).then_some(file_id);
        for &importer in importers.get(&file_id).into_iter().flatten() {
            if visited.insert(importer) {
                dependents.push(Dependent { file_id: importer, through });
                pending.push_back(importer);
            }
        }
    }
    Ok(dependents)
}

/// A declaration whose name matches a search pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchMatch {
    /// How well the name matches, lower being better; see [`search`].
    pub rank: u8,
    pub module: String,
    pub name: String,
    pub item: NamedItem,
}

/// The declarations in `files` whose names match `pattern`, ignoring case, best first: the whole
/// name, then a prefix, then a substring, then the pattern's characters in order anywhere in the
/// name. Ties go to the shorter qualified name, then alphabetical order.
pub fn search(
    engine: &impl AnalyzerQueries,
    files: impl Iterator<Item = FileId>,
    pattern: &str,
) -> Result<Vec<SearchMatch>, AnalyzerError> {
    let pattern = pattern.to_lowercase();
    let mut matches = Vec::new();
    for file_id in files {
        let resolved = engine.resolved(file_id)?;
        let locals = &resolved.locals;
        let terms = locals
            .iter_terms()
            .map(|(name, file_id, term_id)| (name, NamedItem::Term(file_id, term_id)));
        let types = locals.iter_types().chain(locals.iter_classes());
        let types = types.map(|(name, file_id, type_id)| (name, NamedItem::Type(file_id, type_id)));
        let found = terms
            .chain(types)
            .filter_map(|(name, item)| search_rank(name, &pattern).map(|rank| (rank, name, item)));
        let found = found.collect::<Vec<_>>();
        if found.is_empty() {
            continue;
        }
        let module = module_name(engine, file_id)?.unwrap_or_default();
        let found = found.into_iter().map(|(rank, name, item)| SearchMatch {
            rank,
            module: String::clone(&module),
            name: name.to_string(),
            item,
        });
        matches.extend(found);
    }
    matches.sort_by(|left, right| {
        let length = |search: &SearchMatch| search.module.len() + search.name.len();
        let left_key = (left.rank, length(left), &left.module, &left.name);
        left_key.cmp(&(right.rank, length(right), &right.module, &right.name))
    });
    Ok(matches)
}

fn search_rank(name: &str, pattern: &str) -> Option<u8> {
    let name = name.to_lowercase();
    if name == pattern {
        Some(0)
    } else if name.starts_with(pattern) {
        Some(1)
    } else if name.contains(pattern) {
        Some(2)
    } else {
        let mut characters = name.chars();
        let subsequence = pattern.chars().all(|wanted| characters.any(|found| found == wanted));
        subsequence.then_some(3)
    }
}

impl NamedItem {
    pub fn file_id(self) -> FileId {
        match self {
            NamedItem::Term(file_id, _) | NamedItem::Type(file_id, _) => file_id,
        }
    }
}
