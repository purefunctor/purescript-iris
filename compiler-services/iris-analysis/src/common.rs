use building_types::QueryProxy;
use files::FileId;
use indexing::{DeriveItemId, InstanceItemId, TermItemId, TypeItemId};
use lsp_types::*;
use syntax::ast::AstNode;
use syntax::{SyntaxNode, SyntaxNodePtr};

use crate::position::{PositionConverter, Utf8Range};
use crate::{AnalyzerContext, AnalyzerError, locate};

pub fn file_term_location(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    uri: Url,
    file_id: FileId,
    positions: &PositionConverter<'_>,
    term_id: TermItemId,
) -> Result<Location, AnalyzerError> {
    let engine = context.queries();
    let (parsed, _) = engine.parsed(file_id)?;

    let stabilized = engine.stabilized(file_id)?;
    let indexed = engine.indexed(file_id)?;

    let root = parsed.syntax_node();
    let pointers = indexed.term_item_ptr(&stabilized, term_id);

    let range = pointers_range(positions, root, pointers)?;
    let range = positions.utf8_range_to_protocol(range).ok_or(AnalyzerError::NonFatal)?;
    Ok(Location { uri, range })
}

pub fn file_type_location(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    uri: Url,
    file_id: FileId,
    positions: &PositionConverter<'_>,
    type_id: TypeItemId,
) -> Result<Location, AnalyzerError> {
    let engine = context.queries();
    let (parsed, _) = engine.parsed(file_id)?;

    let stabilized = engine.stabilized(file_id)?;
    let indexed = engine.indexed(file_id)?;

    let root = parsed.syntax_node();
    let pointers = indexed.type_item_ptr(&stabilized, type_id);

    let range = pointers_range(positions, root, pointers)?;
    let range = positions.utf8_range_to_protocol(range).ok_or(AnalyzerError::NonFatal)?;

    Ok(Location { uri, range })
}

pub fn file_instance_location(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    uri: Url,
    file_id: FileId,
    item_id: InstanceItemId,
) -> Result<Location, AnalyzerError> {
    let indexed = context.queries().indexed(file_id)?;
    file_source_location(context, uri, file_id, indexed.items[item_id].id)
}

pub fn file_derive_location(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    uri: Url,
    file_id: FileId,
    item_id: DeriveItemId,
) -> Result<Location, AnalyzerError> {
    let indexed = context.queries().indexed(file_id)?;
    file_source_location(context, uri, file_id, indexed.items[item_id].id)
}

fn file_source_location<T: AstNode>(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    uri: Url,
    file_id: FileId,
    source_id: stabilizing::AstId<T>,
) -> Result<Location, AnalyzerError> {
    let content = context.queries().content(file_id)?;
    let positions = PositionConverter::new(&content, context.position_encoding());
    let stabilized = context.queries().stabilized(file_id)?;
    let range =
        locate::id_range(&positions, &context.queries().parsed(file_id)?.0, &stabilized, source_id)
            .ok_or(AnalyzerError::NonFatal)?;
    let range = positions.utf8_range_to_protocol(range).ok_or(AnalyzerError::NonFatal)?;
    Ok(Location { uri, range })
}

pub fn file_uri(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    file_id: FileId,
) -> Result<Url, AnalyzerError> {
    context.file_uri(file_id)?.ok_or(AnalyzerError::NonFatal)
}

pub fn pointers_range(
    positions: &PositionConverter<'_>,
    root: SyntaxNode,
    pointers: impl Iterator<Item = SyntaxNodePtr>,
) -> Result<Utf8Range, AnalyzerError> {
    pointers
        .filter_map(|ptr| locate::syntax_range(positions, &root, &ptr))
        .reduce(|start, end| Utf8Range { start: start.start, end: end.end })
        .ok_or(AnalyzerError::NonFatal)
}
