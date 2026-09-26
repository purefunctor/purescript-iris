//! Analysis of items addressed by module and name rather than by a position in a file.

use building_types::QueryProxy;
use checking::core::pretty::Pretty;
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

/// The item's name with its type, or with its kind for a type or class, as PureScript.
pub fn signature(engine: &impl AnalyzerQueries, item: NamedItem) -> Result<String, AnalyzerError> {
    let file_id = item.file_id();
    let indexed = engine.indexed(file_id)?;
    let checked = engine.checked(file_id)?;
    let pretty = Pretty::with_config(engine, &checked, PRETTY_CONFIG);
    let (name, signature) = match item {
        NamedItem::Term(_, term_id) => {
            let signature = checked.lookup_term_item_type(term_id);
            (&indexed.items[term_id].name, signature)
        }
        NamedItem::Type(_, type_id) => {
            let signature = checked.lookup_type_item_kind(type_id);
            (&indexed.items[type_id].name, signature)
        }
    };
    let name = name.as_deref().unwrap_or("<unknown>");
    let signature = signature.ok_or(AnalyzerError::NonFatal)?;
    Ok(pretty.render_signature(name, signature).to_string())
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

impl NamedItem {
    pub fn file_id(self) -> FileId {
        match self {
            NamedItem::Term(file_id, _) | NamedItem::Type(file_id, _) => file_id,
        }
    }
}
