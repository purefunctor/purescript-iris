mod edit;
mod filter;
mod item;
mod prelude;
mod sources;

pub mod resolve;

use std::sync::Arc;

use building_types::QueryProxy;
use filter::{FuzzyMatch, NoFilter, StartsWith};
use lsp_types::*;
use prelude::{CompletionContext, CompletionSource, CursorSemantics, CursorText, Filter};
use radix_trie::Trie;
use smol_str::SmolStr;
use sources::{
    ImportClasses, ImportedClasses, ImportedTerms, ImportedTypes, LocalClasses, LocalTerms,
    LocalTypes, PrimClasses, PrimTerms, PrimTypes, QualifiedClasses, QualifiedClassesSuggestions,
    QualifiedModules, QualifiedTerms, QualifiedTermsSuggestions, QualifiedTypes,
    QualifiedTypesSuggestions, ScopeTerms, ScopeTypes, SuggestedClasses, SuggestedTerms,
    SuggestedTypes, WorkspaceModules,
};
use syntax::{SyntaxKind, TokenAtOffset};

use crate::position::PositionConverter;
use crate::{AnalyzerContext, AnalyzerError};

#[derive(Clone, Default)]
pub struct SuggestionsCacheEntry {
    pub terms: Vec<CompletionItem>,
    pub types: Vec<CompletionItem>,
    pub classes: Vec<CompletionItem>,
    pub qualified_terms: Vec<CompletionItem>,
    pub qualified_types: Vec<CompletionItem>,
    pub qualified_classes: Vec<CompletionItem>,
}

pub type SuggestionsCache = Trie<String, Arc<SuggestionsCacheEntry>>;

pub fn implementation(
    language: &AnalyzerContext<impl crate::AnalyzerHost>,
    cache: &mut SuggestionsCache,
    uri: Url,
    position: Position,
) -> Result<Option<CompletionResponse>, AnalyzerError> {
    let current_file = {
        let uri = uri.as_str();
        language.file_id(uri).ok_or(AnalyzerError::NonFatal)?
    };

    let engine = language.queries();
    let encoding = language.position_encoding();
    let prim_id = engine.prim_id();
    let content = engine.content(current_file)?;
    let positions = PositionConverter::new(&content, encoding);
    let position = positions.protocol_position_to_utf8(position).ok_or(AnalyzerError::NonFatal)?;
    let (parsed, _) = engine.parsed(current_file)?;

    let offset = positions.utf8_position_to_offset(position).ok_or(AnalyzerError::NonFatal)?;

    let node = parsed.syntax_node();
    let token = node.token_at_offset(offset);

    let token = match token {
        TokenAtOffset::None => return Ok(None),
        TokenAtOffset::Single(token) => token,
        TokenAtOffset::Between(left, right) => {
            let left_annotation = left.parent_ancestors().any(|node| {
                let kind = node.kind();
                matches!(kind, SyntaxKind::Annotation)
            });
            if left_annotation { right } else { left }
        }
    };

    let semantics = CursorSemantics::new(&positions, position);
    let (text, range) = CursorText::new(&positions, &token);

    let stabilized = engine.stabilized(current_file)?;
    let resolved = engine.resolved(current_file)?;
    let prim_resolved = engine.resolved(prim_id)?;

    let context = CompletionContext {
        language,
        current_file,
        positions: &positions,
        stabilized: &stabilized,
        parsed: &parsed,
        resolved: &resolved,
        prim_id,
        prim_resolved: &prim_resolved,
        semantics,
        text,
        range,
        offset,
    };

    let items = collect(&context, cache)?;
    let is_incomplete = items.len() > 5;

    Ok(Some(CompletionResponse::List(CompletionList { is_incomplete, items })))
}

fn collect(
    context: &CompletionContext<impl crate::AnalyzerHost>,
    cache: &mut SuggestionsCache,
) -> Result<Vec<CompletionItem>, AnalyzerError> {
    let mut items = vec![];
    let into = &mut items;

    if context.collect_import_classes() {
        match &context.text {
            CursorText::None => ImportClasses.collect_into(context, NoFilter, into)?,
            CursorText::Name(name) => {
                ImportClasses.collect_into(context, StartsWith(name), into)?;
            }
            CursorText::Prefix(_) | CursorText::Both(_, _) => {}
        }
        return Ok(items);
    }

    match &context.text {
        CursorText::None => {
            if context.collect_modules() {
                WorkspaceModules.collect_into(context, NoFilter, into)?;
            } else {
                QualifiedModules.collect_into(context, NoFilter, into)?;
            }
            if context.collect_terms() {
                ScopeTerms.collect_into(context, NoFilter, into)?;
                LocalTerms.collect_into(context, NoFilter, into)?;
                ImportedTerms.collect_into(context, NoFilter, into)?;
            }
            if context.collect_types() {
                ScopeTypes.collect_into(context, NoFilter, into)?;
                LocalTypes.collect_into(context, NoFilter, into)?;
                ImportedTypes.collect_into(context, NoFilter, into)?;
            }
            if context.collect_classes() {
                LocalClasses.collect_into(context, NoFilter, into)?;
                ImportedClasses.collect_into(context, NoFilter, into)?;
            }
        }
        CursorText::Prefix(p) => {
            let p = p.trim_end_matches('.');

            if context.collect_modules() {
                WorkspaceModules.collect_into(context, StartsWith(p), into)?;
            } else {
                QualifiedModules.collect_into(context, StartsWith(p), into)?;
            }
            if context.collect_terms() {
                QualifiedTerms(p).collect_into(context, NoFilter, into)?;
            }
            if context.collect_types() {
                QualifiedTypes(p).collect_into(context, NoFilter, into)?;
            }
            if context.collect_classes() {
                QualifiedClasses(p).collect_into(context, NoFilter, into)?;
            }

            let query = format!("prefix:{p}");
            let suggestions =
                get_or_populate_suggestions(cache, &query, context, Some(p), NoFilter)?;

            if context.collect_terms() && !context.has_qualified_import(p) {
                items.extend(suggestions.qualified_terms.iter().cloned());
            }
            if context.collect_types() && !context.has_qualified_import(p) {
                items.extend(suggestions.qualified_types.iter().cloned());
            }
            if context.collect_classes() && !context.has_qualified_import(p) {
                items.extend(suggestions.qualified_classes.iter().cloned());
            }
        }
        CursorText::Name(n) => {
            if context.collect_modules() {
                WorkspaceModules.collect_into(context, StartsWith(n), into)?;
            } else {
                QualifiedModules.collect_into(context, StartsWith(n), into)?;
            }
            if context.collect_terms() {
                ScopeTerms.collect_into(context, FuzzyMatch(n), into)?;
                LocalTerms.collect_into(context, FuzzyMatch(n), into)?;
                ImportedTerms.collect_into(context, FuzzyMatch(n), into)?;
                if context.collect_implicit_prim() {
                    PrimTerms.collect_into(context, FuzzyMatch(n), into)?;
                }
            }
            if context.collect_types() {
                ScopeTypes.collect_into(context, FuzzyMatch(n), into)?;
                LocalTypes.collect_into(context, FuzzyMatch(n), into)?;
                ImportedTypes.collect_into(context, FuzzyMatch(n), into)?;
                if context.collect_implicit_prim() {
                    PrimTypes.collect_into(context, FuzzyMatch(n), into)?;
                }
            }
            if context.collect_classes() {
                LocalClasses.collect_into(context, FuzzyMatch(n), into)?;
                ImportedClasses.collect_into(context, FuzzyMatch(n), into)?;
                if context.collect_implicit_prim() {
                    PrimClasses.collect_into(context, FuzzyMatch(n), into)?;
                }
            }

            let query = format!("name:{n}");
            let suggestions =
                get_or_populate_suggestions(cache, &query, context, None, StartsWith(n))?;

            if context.collect_terms() {
                items.extend(suggestions.terms.iter().cloned());
            }

            if context.collect_types() {
                items.extend(suggestions.types.iter().cloned());
            }
            if context.collect_classes() {
                items.extend(suggestions.classes.iter().cloned());
            }
        }
        CursorText::Both(p, n) => {
            let t: SmolStr = p.chars().chain(n.chars()).collect();
            let p = p.trim_end_matches('.');

            if context.collect_modules() {
                WorkspaceModules.collect_into(context, StartsWith(&t), into)?;
            } else {
                QualifiedModules.collect_into(context, StartsWith(&t), into)?;
            }
            if context.collect_terms() {
                QualifiedTerms(p).collect_into(context, FuzzyMatch(n), into)?;
            }
            if context.collect_types() {
                QualifiedTypes(p).collect_into(context, FuzzyMatch(n), into)?;
            }
            if context.collect_classes() {
                QualifiedClasses(p).collect_into(context, FuzzyMatch(n), into)?;
            }

            let query = format!("both:{t}");
            let suggestions =
                get_or_populate_suggestions(cache, &query, context, Some(p), FuzzyMatch(n))?;

            if context.collect_terms() && !context.has_qualified_import(p) {
                items.extend(suggestions.qualified_terms.iter().cloned());
            }

            if context.collect_types() && !context.has_qualified_import(p) {
                items.extend(suggestions.qualified_types.iter().cloned());
            }
            if context.collect_classes() && !context.has_qualified_import(p) {
                items.extend(suggestions.qualified_classes.iter().cloned());
            }
        }
    }

    Ok(items)
}

fn get_or_populate_suggestions<F: Filter>(
    cache: &mut SuggestionsCache,
    query: &str,
    context: &CompletionContext<impl crate::AnalyzerHost>,
    prefix: Option<&str>,
    filter: F,
) -> Result<Arc<SuggestionsCacheEntry>, AnalyzerError> {
    let query = query.to_lowercase();

    if let Some(cached) = cache.get(&query) {
        tracing::debug!("Found exact match for '{query}'");
        let filtered = filter_suggestions(cached, prefix, &filter, context);
        return Ok(Arc::new(filtered));
    }

    if let Some(cached) = cache.get_ancestor_value(&query) {
        tracing::debug!("Found prefix match for '{query}'");
        let filtered = filter_suggestions(cached, prefix, &filter, context);

        let key = query.to_string();
        let value = Arc::new(filtered);
        cache.insert(key, Arc::clone(&value));

        return Ok(value);
    }

    tracing::debug!("Initialising cache for '{query}'");

    let mut suggestions = SuggestionsCacheEntry::default();

    if let Some(prefix) = prefix {
        QualifiedTermsSuggestions(prefix).collect_into(
            context,
            filter,
            &mut suggestions.qualified_terms,
        )?;
        QualifiedTypesSuggestions(prefix).collect_into(
            context,
            filter,
            &mut suggestions.qualified_types,
        )?;
        QualifiedClassesSuggestions(prefix).collect_into(
            context,
            filter,
            &mut suggestions.qualified_classes,
        )?;
    } else {
        SuggestedTerms.collect_into(context, filter, &mut suggestions.terms)?;
        SuggestedTypes.collect_into(context, filter, &mut suggestions.types)?;
        SuggestedClasses.collect_into(context, filter, &mut suggestions.classes)?;
    }

    let key = query.to_string();
    let value = Arc::new(suggestions);
    cache.insert(key, Arc::clone(&value));

    Ok(value)
}

fn filter_suggestions<F>(
    cached: &SuggestionsCacheEntry,
    prefix: Option<&str>,
    filter: &F,
    context: &CompletionContext<impl crate::AnalyzerHost>,
) -> SuggestionsCacheEntry
where
    F: Filter,
{
    SuggestionsCacheEntry {
        terms: collect_entries(&cached.terms, filter, prefix, context, ImportNamespace::Term),
        types: collect_entries(&cached.types, filter, prefix, context, ImportNamespace::Type),
        classes: collect_entries(&cached.classes, filter, prefix, context, ImportNamespace::Class),
        qualified_terms: collect_entries(
            &cached.qualified_terms,
            filter,
            prefix,
            context,
            ImportNamespace::Term,
        ),
        qualified_types: collect_entries(
            &cached.qualified_types,
            filter,
            prefix,
            context,
            ImportNamespace::Type,
        ),
        qualified_classes: collect_entries(
            &cached.qualified_classes,
            filter,
            prefix,
            context,
            ImportNamespace::Class,
        ),
    }
}

#[derive(Clone, Copy)]
enum ImportNamespace {
    Term,
    Type,
    Class,
}

fn collect_entries<F>(
    items: &[CompletionItem],
    filter: &F,
    prefix: Option<&str>,
    context: &CompletionContext<impl crate::AnalyzerHost>,
    namespace: ImportNamespace,
) -> Vec<CompletionItem>
where
    F: Filter,
{
    let entries = items.iter().filter(|item| {
        if !filter.matches(&item.label) {
            return false;
        }
        if item.additional_text_edits.is_some() {
            let has_import = match namespace {
                ImportNamespace::Term => context.has_term_import(prefix, &item.label),
                ImportNamespace::Type => context.has_type_import(prefix, &item.label),
                ImportNamespace::Class => context.has_class_import(prefix, &item.label),
            };
            if has_import {
                return false;
            }
        }
        true
    });

    let entries = entries.cloned().map(|mut item| {
        let Some(range) = context.range else {
            return item;
        };
        if let Some(CompletionTextEdit::Edit(text_edit)) = &mut item.text_edit {
            text_edit.range = range;
        }
        item
    });

    entries.collect()
}
