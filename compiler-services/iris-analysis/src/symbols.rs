use std::sync::Arc;

use building_types::QueryProxy;
use indexing::{IndexedTermItemKind, IndexedTypeItemKind};
use lsp_types::*;
use radix_trie::Trie;

use crate::position::PositionConverter;
use crate::{AnalyzerContext, AnalyzerError, common};

fn term_symbol_kind(kind: &IndexedTermItemKind) -> SymbolKind {
    match kind {
        IndexedTermItemKind::Constructor { .. } => SymbolKind::Constructor,
        IndexedTermItemKind::ClassMember { .. } => SymbolKind::Method,
        IndexedTermItemKind::Operator { .. } => SymbolKind::Operator,
        IndexedTermItemKind::Value { .. } | IndexedTermItemKind::Foreign { .. } => {
            SymbolKind::Function
        }
    }
}

fn type_symbol_kind(kind: &IndexedTypeItemKind) -> SymbolKind {
    match kind {
        // Note: type classes are partitioned out of `iter_types()` and exposed via `iter_classes()`.
        // Keep this arm for exhaustiveness in case that invariant changes.
        IndexedTypeItemKind::Class { .. } => SymbolKind::Interface,
        IndexedTypeItemKind::Operator { .. } => SymbolKind::Operator,
        IndexedTypeItemKind::Data { .. } => SymbolKind::Enum,
        IndexedTypeItemKind::Synonym { .. } => SymbolKind::TypeParameter,
        IndexedTypeItemKind::Newtype { .. } | IndexedTypeItemKind::Foreign { .. } => {
            SymbolKind::Struct
        }
    }
}

pub fn document(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    uri: Uri,
) -> Result<Option<DocumentSymbolResponse>, AnalyzerError> {
    let engine = context.queries();

    let current_file = {
        let uri = uri.as_str();
        context.file_id(uri).ok_or(AnalyzerError::NonFatal)?
    };

    let resolved = engine.resolved(current_file)?;
    let indexed = engine.indexed(current_file)?;
    let content = engine.content(current_file)?;
    let positions = PositionConverter::new(&content, context.position_encoding());

    let mut symbols = vec![];

    for (name, file_id, term_id) in resolved.locals.iter_terms() {
        if file_id != current_file {
            continue;
        }
        let kind = term_symbol_kind(&indexed.items[term_id].kind);
        let uri = Uri::clone(&uri);
        let location = common::file_term_location(context, uri, current_file, &positions, term_id)?;
        symbols.push(symbol_information(name, kind, location));
    }

    for (name, file_id, type_id) in resolved.locals.iter_types() {
        if file_id != current_file {
            continue;
        }
        let kind = type_symbol_kind(&indexed.items[type_id].kind);
        let uri = Uri::clone(&uri);
        let location = common::file_type_location(context, uri, current_file, &positions, type_id)?;
        symbols.push(symbol_information(name, kind, location));
    }

    for (name, file_id, type_id) in resolved.locals.iter_classes() {
        if file_id != current_file {
            continue;
        }
        let kind = SymbolKind::Interface;
        let uri = Uri::clone(&uri);
        let location = common::file_type_location(context, uri, current_file, &positions, type_id)?;
        symbols.push(symbol_information(name, kind, location));
    }

    symbols.sort_by_key(|s| (s.location.range.start.line, s.location.range.start.character));
    Ok(Some(DocumentSymbolResponse::SymbolInformationList(symbols)))
}

pub fn workspace(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    cache: &mut WorkspaceSymbolsCache,
    query: &str,
) -> Result<Option<WorkspaceSymbolResponse>, AnalyzerError> {
    if query.is_empty() {
        return Ok(None);
    }

    let query = query.to_lowercase();

    if let Some(exact_symbols) = cache.get(&query) {
        tracing::debug!("Found exact match for '{query}'");
        let flat = Vec::clone(exact_symbols);
        return Ok(Some(WorkspaceSymbolResponse::SymbolInformationList(flat)));
    }

    let symbols = if let Some(prefix_symbols) = cache.get_ancestor_value(&query) {
        tracing::debug!("Found prefix match for '{query}'");
        let filtered_symbols = filter_symbols(prefix_symbols, &query);
        if filtered_symbols.len() == prefix_symbols.len() {
            Arc::clone(prefix_symbols)
        } else {
            Arc::new(filtered_symbols)
        }
    } else {
        tracing::debug!("Initialising cache for '{query}'");
        let filtered_symbols = build_symbol_list(context, &query)?;
        Arc::new(filtered_symbols)
    };

    let key = String::clone(&query);
    let value = Arc::clone(&symbols);
    cache.insert(key, value);

    let flat = Vec::clone(&*symbols);
    Ok(Some(WorkspaceSymbolResponse::SymbolInformationList(flat)))
}

fn name_starts_with_folded(name: &str, folded_query: &str) -> bool {
    // `folded_query` is already lowercased by the caller. When the query is
    // ASCII, lowercasing preserves byte length, so compare bytes directly
    // without allocating a lowered copy. Otherwise the allocating comparison
    // below remains authoritative: Unicode lowercasing can expand (e.g. `İ`
    // folds to `i` plus U+0307), so a byte-length rejection is only valid
    // when both sides are ASCII.
    if folded_query.is_ascii() {
        if let Some(prefix) = name.get(..folded_query.len()) {
            if prefix.is_ascii() {
                return prefix
                    .bytes()
                    .map(|byte| byte.to_ascii_lowercase())
                    .eq(folded_query.bytes());
            }
        } else if name.is_ascii() {
            return false;
        }
    }
    name.to_lowercase().starts_with(folded_query)
}

fn filter_symbols(cached: &[SymbolInformation], query: &str) -> Vec<SymbolInformation> {
    cached
        .iter()
        .filter(|symbol| name_starts_with_folded(&symbol.base_symbol_information.name, query))
        .cloned()
        .collect()
}

fn build_symbol_list(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    query: &str,
) -> Result<Vec<SymbolInformation>, AnalyzerError> {
    let mut symbols = vec![];

    for file_id in context.active_files() {
        let resolved = context.queries().resolved(file_id)?;
        let indexed = context.queries().indexed(file_id)?;
        let content = context.queries().content(file_id)?;
        let mut positions = None;
        let uri = common::file_uri(context, file_id)?;

        for (name, _, term_id) in resolved.locals.iter_terms() {
            if !name_starts_with_folded(name, query) {
                continue;
            }
            let kind = term_symbol_kind(&indexed.items[term_id].kind);
            let uri = Uri::clone(&uri);
            let positions = positions.get_or_insert_with(|| {
                PositionConverter::new(&content, context.position_encoding())
            });
            let location = common::file_term_location(context, uri, file_id, positions, term_id)?;
            symbols.push(symbol_information(name, kind, location));
        }

        for (name, _, type_id) in resolved.locals.iter_types() {
            if !name_starts_with_folded(name, query) {
                continue;
            }
            let kind = type_symbol_kind(&indexed.items[type_id].kind);
            let uri = Uri::clone(&uri);
            let positions = positions.get_or_insert_with(|| {
                PositionConverter::new(&content, context.position_encoding())
            });
            let location = common::file_type_location(context, uri, file_id, positions, type_id)?;
            symbols.push(symbol_information(name, kind, location));
        }

        for (name, _, type_id) in resolved.locals.iter_classes() {
            if !name_starts_with_folded(name, query) {
                continue;
            }
            let uri = Uri::clone(&uri);
            let positions = positions.get_or_insert_with(|| {
                PositionConverter::new(&content, context.position_encoding())
            });
            let location = common::file_type_location(context, uri, file_id, positions, type_id)?;
            symbols.push(symbol_information(name, SymbolKind::Interface, location));
        }
    }

    Ok(symbols)
}

pub type WorkspaceSymbolsCache = Trie<String, Arc<Vec<SymbolInformation>>>;

fn symbol_information(name: &str, kind: SymbolKind, location: Location) -> SymbolInformation {
    let name = name.to_string();
    let base_symbol_information =
        BaseSymbolInformation { name, kind, tags: None, container_name: None };
    #[allow(deprecated)]
    SymbolInformation { deprecated: None, location, base_symbol_information }
}
