//! Analysis of items addressed by module and name rather than by a position in a file.

use building_types::QueryProxy;
use checking::core::pretty::Pretty;
use files::FileId;
use indexing::{TermItemId, TypeItemId};
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

impl NamedItem {
    pub fn file_id(self) -> FileId {
        match self {
            NamedItem::Term(file_id, _) | NamedItem::Type(file_id, _) => file_id,
        }
    }
}
