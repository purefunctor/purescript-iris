use building_types::QueryProxy;
use files::FileId;
use indexing::{ImportKind, TermItemId, TypeItemId};
use itertools::Itertools;
use lowering::GraphNode;
use lsp_types::*;
use resolving::ResolvedModule;
use rustc_hash::FxHashSet;
use smol_str::SmolStr;
use syntax::ast::AstNode;
use syntax::{TokenAtOffset, cst};

use crate::AnalyzerError;

use super::edit;
use super::filter::PerfectSegmentFuzzy;
use super::item::CompletionItemSpec;
use super::prelude::{CompletionContext, CompletionSource, Filter};
use super::resolve::CompletionResolveData;

fn module_name(
    context: &CompletionContext<impl crate::AnalyzerHost>,
    file_id: FileId,
) -> Result<Option<String>, AnalyzerError> {
    let content = context.language.queries().content(file_id)?;
    let (parsed, _) = context.language.queries().parsed(file_id)?;
    Ok(parsed.module_name(&content).map(|name| name.to_string()))
}

/// Yields classes exported by the module of the current import statement.
pub struct ImportClasses;

impl CompletionSource for ImportClasses {
    type T = ();

    fn collect_into<F: Filter>(
        &self,
        context: &CompletionContext<impl crate::AnalyzerHost>,
        filter: F,
        items: &mut Vec<CompletionItem>,
    ) -> Result<Self::T, AnalyzerError> {
        let root = context.parsed.syntax_node();
        let statement = match root.token_at_offset(context.offset) {
            TokenAtOffset::None => None,
            TokenAtOffset::Single(token) => {
                token.parent_ancestors().find_map(cst::ImportStatement::cast)
            }
            TokenAtOffset::Between(left, right) => right
                .parent_ancestors()
                .find_map(cst::ImportStatement::cast)
                .or_else(|| left.parent_ancestors().find_map(cst::ImportStatement::cast)),
        };
        let Some(statement) = statement else { return Ok(()) };
        let Some(module_name) = statement.module_name() else { return Ok(()) };
        let module_name = module_name.syntax().text(context.positions.content()).to_string();
        let Some(import_file) = context.language.queries().module_file(&module_name) else {
            return Ok(());
        };

        let resolved = context.language.queries().resolved(import_file)?;
        let source = resolved.exports.iter_classes();
        let source = source.filter(move |(name, _, _)| filter.matches(name));

        for (name, file_id, type_id) in source {
            let mut item = CompletionItemSpec::new(
                name.to_string(),
                context.range,
                CompletionItemKind::STRUCT,
                CompletionResolveData::TypeItem(file_id, type_id),
            );

            item.label_description(module_name.clone());
            items.push(item.build());
        }

        Ok(())
    }
}

/// Yields the qualified names of imports.
///
/// For example:
/// ```purescript
/// import Halogen as H
/// import Halogen.HTML as HH
///
/// -- candidates -> [H, HH]
/// ```
pub struct QualifiedModules;

impl CompletionSource for QualifiedModules {
    type T = ();

    fn collect_into<F: Filter>(
        &self,
        context: &CompletionContext<impl crate::AnalyzerHost>,
        filter: F,
        items: &mut Vec<CompletionItem>,
    ) -> Result<Self::T, AnalyzerError> {
        let source = context.resolved.qualified.iter();
        let source = source.filter(move |(name, _)| filter.matches(name));

        for (name, imports) in source {
            let Some(import) = imports.first() else { continue };
            let description = module_name(context, import.file)?;

            let mut item = CompletionItemSpec::new(
                name.to_string(),
                context.range,
                CompletionItemKind::MODULE,
                CompletionResolveData::Import(import.file),
            );

            if let Some(description) = description {
                item.label_description(description);
            }

            items.push(item.build());
        }

        Ok(())
    }
}

/// Yields local terms visible from the current lexical scope.
pub struct ScopeTerms;

impl CompletionSource for ScopeTerms {
    type T = ();

    fn collect_into<F: Filter>(
        &self,
        context: &CompletionContext<impl crate::AnalyzerHost>,
        filter: F,
        items: &mut Vec<CompletionItem>,
    ) -> Result<Self::T, AnalyzerError> {
        let Some(scope_node) = context.scope_node()? else { return Ok(()) };

        let lowered = context.language.queries().lowered(context.current_file)?;
        let mut seen = FxHashSet::default();

        for (_, node) in lowered.graph.traverse(scope_node) {
            match node {
                GraphNode::Binder { binders, puns, .. } => {
                    let mut binders = binders.iter().collect_vec();
                    binders.sort_by_key(|(left, _)| *left);

                    for (name, binder_id) in binders {
                        if !filter.matches(name) || !seen.insert(name.clone()) {
                            continue;
                        }

                        let mut item = CompletionItemSpec::new(
                            name.to_string(),
                            context.range,
                            CompletionItemKind::VARIABLE,
                            CompletionResolveData::Binder(context.current_file, *binder_id),
                        );

                        item.label_description("Local".to_string());

                        items.push(item.build());
                    }

                    let mut puns = puns.iter().collect_vec();
                    puns.sort_by_key(|(left, _)| *left);

                    for (name, pun_id) in puns {
                        if !filter.matches(name) || !seen.insert(name.clone()) {
                            continue;
                        }

                        let mut item = CompletionItemSpec::new(
                            name.to_string(),
                            context.range,
                            CompletionItemKind::VARIABLE,
                            CompletionResolveData::RecordPun(context.current_file, *pun_id),
                        );

                        item.label_description("Local".to_string());

                        items.push(item.build());
                    }
                }
                GraphNode::Let { bindings, .. } => {
                    let mut bindings = bindings.iter().collect_vec();
                    bindings.sort_by_key(|(left, _)| *left);

                    for (name, let_id) in bindings {
                        if !filter.matches(name) || !seen.insert(name.clone()) {
                            continue;
                        }

                        let mut item = CompletionItemSpec::new(
                            name.to_string(),
                            context.range,
                            CompletionItemKind::VALUE,
                            CompletionResolveData::Let(context.current_file, *let_id),
                        );

                        item.label_description("Local".to_string());

                        items.push(item.build());
                    }
                }
                GraphNode::Forall { .. } | GraphNode::Implicit { .. } => {}
            }
        }

        Ok(())
    }
}

/// Yields type variables visible from the current lexical scope.
pub struct ScopeTypes;

impl CompletionSource for ScopeTypes {
    type T = ();

    fn collect_into<F: Filter>(
        &self,
        context: &CompletionContext<impl crate::AnalyzerHost>,
        filter: F,
        items: &mut Vec<CompletionItem>,
    ) -> Result<Self::T, AnalyzerError> {
        let Some(scope_node) = context.scope_node()? else { return Ok(()) };

        let lowered = context.language.queries().lowered(context.current_file)?;
        let mut seen = FxHashSet::default();

        for (node_id, node) in lowered.graph.traverse(scope_node) {
            match node {
                GraphNode::Forall { bindings, .. } => {
                    let mut bindings = bindings.iter().collect_vec();
                    bindings.sort_by_key(|(left, _)| *left);

                    for (name, binding_id) in bindings {
                        if !filter.matches(name) || !seen.insert(name.clone()) {
                            continue;
                        }

                        let mut item = CompletionItemSpec::new(
                            name.to_string(),
                            context.range,
                            CompletionItemKind::TYPE_PARAMETER,
                            CompletionResolveData::ForallTypeVariable(
                                context.current_file,
                                *binding_id,
                            ),
                        );

                        item.label_description("Local".to_string());

                        items.push(item.build());
                    }
                }
                GraphNode::Implicit { bindings, .. } => {
                    let mut entries = bindings.iter().collect_vec();
                    entries.sort_by_key(|(left, _)| *left);

                    for (name, type_id) in entries {
                        if implicit_binding_at_cursor(context, type_id) {
                            continue;
                        }

                        if !filter.matches(name) || !seen.insert(SmolStr::from(name)) {
                            continue;
                        }
                        let Some(binding_id) = bindings.get(name) else { continue };

                        let mut item = CompletionItemSpec::new(
                            name.to_string(),
                            context.range,
                            CompletionItemKind::TYPE_PARAMETER,
                            CompletionResolveData::ImplicitTypeVariable(
                                context.current_file,
                                node_id,
                                binding_id,
                            ),
                        );

                        item.label_description("Local".to_string());

                        items.push(item.build());
                    }
                }
                GraphNode::Binder { .. } | GraphNode::Let { .. } => {}
            }
        }

        Ok(())
    }
}

fn implicit_binding_at_cursor(
    context: &CompletionContext<impl crate::AnalyzerHost>,
    type_id: &[lowering::TypeId],
) -> bool {
    let root = context.parsed.syntax_node();

    type_id.iter().any(|type_id| {
        let Some(ptr) = context.stabilized.syntax_ptr(*type_id) else { return false };
        let Some(node) = ptr.try_to_node(&root) else { return false };

        let range = node.text_range();
        range.start() <= context.offset && context.offset <= range.end()
    })
}

/// Yields terms defined in the current module.
pub struct LocalTerms;

impl CompletionSource for LocalTerms {
    type T = ();

    fn collect_into<F: Filter>(
        &self,
        context: &CompletionContext<impl crate::AnalyzerHost>,
        filter: F,
        items: &mut Vec<CompletionItem>,
    ) -> Result<Self::T, AnalyzerError> {
        let source = context.resolved.locals.iter_terms();
        let source = source.filter(move |(name, _, _)| filter.matches(name));

        for (name, file_id, term_id) in source {
            let mut item = CompletionItemSpec::new(
                name.to_string(),
                context.range,
                CompletionItemKind::VALUE,
                CompletionResolveData::TermItem(file_id, term_id),
            );

            item.label_description("Local".to_string());

            items.push(item.build())
        }

        Ok(())
    }
}

/// Yields types defined in the current module.
pub struct LocalTypes;

/// Yields classes defined in the current module.
pub struct LocalClasses;

impl CompletionSource for LocalTypes {
    type T = ();

    fn collect_into<F: Filter>(
        &self,
        context: &CompletionContext<impl crate::AnalyzerHost>,
        filter: F,
        items: &mut Vec<CompletionItem>,
    ) -> Result<Self::T, AnalyzerError> {
        let source = context.resolved.locals.iter_types();
        let source = source.filter(move |(name, _, _)| filter.matches(name));

        for (name, file_id, type_id) in source {
            let mut item = CompletionItemSpec::new(
                name.to_string(),
                context.range,
                CompletionItemKind::STRUCT,
                CompletionResolveData::TypeItem(file_id, type_id),
            );

            item.label_description("Local".to_string());

            items.push(item.build())
        }

        Ok(())
    }
}

impl CompletionSource for LocalClasses {
    type T = ();

    fn collect_into<F: Filter>(
        &self,
        context: &CompletionContext<impl crate::AnalyzerHost>,
        filter: F,
        items: &mut Vec<CompletionItem>,
    ) -> Result<Self::T, AnalyzerError> {
        let source = context.resolved.locals.iter_classes();
        let source = source.filter(move |(name, _, _)| filter.matches(name));

        for (name, file_id, type_id) in source {
            let mut item = CompletionItemSpec::new(
                name.to_string(),
                context.range,
                CompletionItemKind::STRUCT,
                CompletionResolveData::TypeItem(file_id, type_id),
            );

            item.label_description("Local".to_string());

            items.push(item.build())
        }

        Ok(())
    }
}

/// Yields terms from unqualified imports.
pub struct ImportedTerms;

impl CompletionSource for ImportedTerms {
    type T = ();

    fn collect_into<F: Filter>(
        &self,
        context: &CompletionContext<impl crate::AnalyzerHost>,
        filter: F,
        items: &mut Vec<CompletionItem>,
    ) -> Result<Self::T, AnalyzerError> {
        let source = context.resolved.unqualified.values().flatten();

        for import in source {
            let source = import.iter_terms().filter(move |(name, _, _, kind)| {
                filter.matches(name) && !matches!(kind, ImportKind::Hidden)
            });

            for (name, file_id, term_id, _) in source {
                let description = module_name(context, file_id)?;

                let mut item = CompletionItemSpec::new(
                    name.to_string(),
                    context.range,
                    CompletionItemKind::VALUE,
                    CompletionResolveData::TermItem(file_id, term_id),
                );

                if let Some(description) = description {
                    item.label_description(description);
                }

                items.push(item.build())
            }
        }

        Ok(())
    }
}

/// Yields types from unqualified imports.
pub struct ImportedTypes;

/// Yields classes from unqualified imports.
pub struct ImportedClasses;

impl CompletionSource for ImportedTypes {
    type T = ();

    fn collect_into<F: Filter>(
        &self,
        context: &CompletionContext<impl crate::AnalyzerHost>,
        filter: F,
        items: &mut Vec<CompletionItem>,
    ) -> Result<Self::T, AnalyzerError> {
        let source = context.resolved.unqualified.values().flatten();

        for import in source {
            let source = import.iter_types().filter(move |(name, _, _, kind)| {
                filter.matches(name) && !matches!(kind, ImportKind::Hidden)
            });
            for (name, file_id, type_id, _) in source {
                let description = module_name(context, file_id)?;

                let mut item = CompletionItemSpec::new(
                    name.to_string(),
                    context.range,
                    CompletionItemKind::STRUCT,
                    CompletionResolveData::TypeItem(file_id, type_id),
                );

                if let Some(description) = description {
                    item.label_description(description);
                }

                items.push(item.build())
            }
        }

        Ok(())
    }
}

impl CompletionSource for ImportedClasses {
    type T = ();

    fn collect_into<F: Filter>(
        &self,
        context: &CompletionContext<impl crate::AnalyzerHost>,
        filter: F,
        items: &mut Vec<CompletionItem>,
    ) -> Result<Self::T, AnalyzerError> {
        let source = context.resolved.unqualified.values().flatten();

        for import in source {
            let source = import.iter_classes().filter(move |(name, _, _, kind)| {
                filter.matches(name) && !matches!(kind, ImportKind::Hidden)
            });
            for (name, file_id, type_id, _) in source {
                let description = module_name(context, file_id)?;

                let mut item = CompletionItemSpec::new(
                    name.to_string(),
                    context.range,
                    CompletionItemKind::STRUCT,
                    CompletionResolveData::TypeItem(file_id, type_id),
                );

                if let Some(description) = description {
                    item.label_description(description);
                }

                items.push(item.build())
            }
        }

        Ok(())
    }
}

/// Yields terms from qualified imports.
pub struct QualifiedTerms<'a>(pub &'a str);

impl CompletionSource for QualifiedTerms<'_> {
    type T = ();

    fn collect_into<F: Filter>(
        &self,
        context: &CompletionContext<impl crate::AnalyzerHost>,
        filter: F,
        items: &mut Vec<CompletionItem>,
    ) -> Result<Self::T, AnalyzerError> {
        let Some(imports) = context.resolved.qualified.get(self.0) else {
            return Ok(());
        };

        for import in imports {
            let source = import.iter_terms().filter(|(name, _, _, kind)| {
                filter.matches(name) && !matches!(kind, ImportKind::Hidden)
            });

            for (name, file_id, term_id, _) in source {
                let description = module_name(context, file_id)?;

                let mut item = CompletionItemSpec::new(
                    name.to_string(),
                    context.range,
                    CompletionItemKind::VALUE,
                    CompletionResolveData::TermItem(file_id, term_id),
                );

                item.edit_text(format!("{}.{name}", self.0));
                if let Some(description) = description {
                    item.label_description(description);
                }

                items.push(item.build())
            }
        }

        Ok(())
    }
}

/// Yields types from qualified imports.
pub struct QualifiedTypes<'a>(pub &'a str);

/// Yields classes from qualified imports.
pub struct QualifiedClasses<'a>(pub &'a str);

impl CompletionSource for QualifiedTypes<'_> {
    type T = ();

    fn collect_into<F: Filter>(
        &self,
        context: &CompletionContext<impl crate::AnalyzerHost>,
        filter: F,
        items: &mut Vec<CompletionItem>,
    ) -> Result<Self::T, AnalyzerError> {
        let Some(imports) = context.resolved.qualified.get(self.0) else {
            return Ok(());
        };

        for import in imports {
            let source = import.iter_types().filter(|(name, _, _, kind)| {
                filter.matches(name) && !matches!(kind, ImportKind::Hidden)
            });

            for (name, file_id, type_id, _) in source {
                let description = module_name(context, file_id)?;

                let mut item = CompletionItemSpec::new(
                    name.to_string(),
                    context.range,
                    CompletionItemKind::STRUCT,
                    CompletionResolveData::TypeItem(file_id, type_id),
                );

                item.edit_text(format!("{}.{name}", self.0));
                if let Some(description) = description {
                    item.label_description(description);
                }

                items.push(item.build())
            }
        }

        Ok(())
    }
}

impl CompletionSource for QualifiedClasses<'_> {
    type T = ();

    fn collect_into<F: Filter>(
        &self,
        context: &CompletionContext<impl crate::AnalyzerHost>,
        filter: F,
        items: &mut Vec<CompletionItem>,
    ) -> Result<Self::T, AnalyzerError> {
        let Some(imports) = context.resolved.qualified.get(self.0) else {
            return Ok(());
        };

        for import in imports {
            let source = import.iter_classes().filter(|(name, _, _, kind)| {
                filter.matches(name) && !matches!(kind, ImportKind::Hidden)
            });

            for (name, file_id, type_id, _) in source {
                let description = module_name(context, file_id)?;

                let mut item = CompletionItemSpec::new(
                    name.to_string(),
                    context.range,
                    CompletionItemKind::STRUCT,
                    CompletionResolveData::TypeItem(file_id, type_id),
                );

                item.edit_text(format!("{}.{name}", self.0));
                if let Some(description) = description {
                    item.label_description(description);
                }

                items.push(item.build())
            }
        }

        Ok(())
    }
}

/// Yields suggestions for terms.
pub struct SuggestedTerms;

/// Yields suggestions for types.
pub struct SuggestedTypes;

/// Yields suggestions for classes.
pub struct SuggestedClasses;

trait SuggestionsHelper {
    type ItemId;

    fn exports<'a>(
        &self,
        resolved: &'a ResolvedModule,
    ) -> impl Iterator<Item = (&'a SmolStr, FileId, Self::ItemId)>;

    fn candidate(
        &self,
        context: &CompletionContext<impl crate::AnalyzerHost>,
        name: &SmolStr,
        import_id: FileId,
        file_id: FileId,
        item_id: Self::ItemId,
    ) -> Result<Option<CompletionItem>, AnalyzerError>;
}

impl SuggestionsHelper for SuggestedTerms {
    type ItemId = TermItemId;

    fn exports<'a>(
        &self,
        resolved: &'a ResolvedModule,
    ) -> impl Iterator<Item = (&'a SmolStr, FileId, TermItemId)> {
        resolved.exports.iter_terms()
    }

    fn candidate(
        &self,
        context: &CompletionContext<impl crate::AnalyzerHost>,
        name: &SmolStr,
        import_id: FileId,
        file_id: FileId,
        item_id: Self::ItemId,
    ) -> Result<Option<CompletionItem>, AnalyzerError> {
        assert_eq!(import_id, file_id);

        if context.has_term_import(None, name) {
            return Ok(None);
        }

        let Some(module_name) = module_name(context, file_id)? else {
            return Ok(None);
        };

        let mut item = CompletionItemSpec::new(
            name.to_string(),
            context.range,
            CompletionItemKind::VALUE,
            CompletionResolveData::TermItem(file_id, item_id),
        );

        let (import_text, import_range) =
            edit::term_import_item(context, &module_name, name, file_id, item_id);

        let import_edit = match (import_range, import_text) {
            (Some(range), Some(new_text)) => Some(TextEdit { range, new_text }),
            (None, Some(new_text)) => context.insert_import_edit(new_text),
            (_, None) => None,
        };

        item.label_detail(format!(" (import {module_name})"));
        item.label_description(module_name.to_string());
        item.sort_text(format!("{module_name}.{name}"));

        if let Some(import_edit) = import_edit {
            item.additional_text_edits(vec![import_edit]);
        }

        Ok(Some(item.build()))
    }
}

impl SuggestionsHelper for SuggestedTypes {
    type ItemId = TypeItemId;

    fn exports<'a>(
        &self,
        resolved: &'a ResolvedModule,
    ) -> impl Iterator<Item = (&'a SmolStr, FileId, Self::ItemId)> {
        resolved.exports.iter_types()
    }

    fn candidate(
        &self,
        context: &CompletionContext<impl crate::AnalyzerHost>,
        name: &SmolStr,
        import_id: FileId,
        file_id: FileId,
        item_id: Self::ItemId,
    ) -> Result<Option<CompletionItem>, AnalyzerError> {
        assert_eq!(import_id, file_id);

        if context.has_type_import(None, name) {
            return Ok(None);
        }

        let Some(module_name) = module_name(context, file_id)? else {
            return Ok(None);
        };

        let import_edit = edit::type_import_item(context, &module_name, name, file_id, item_id);
        let item =
            suggested_type_candidate(context, name, &module_name, file_id, item_id, import_edit);

        Ok(Some(item))
    }
}

impl SuggestionsHelper for SuggestedClasses {
    type ItemId = TypeItemId;

    fn exports<'a>(
        &self,
        resolved: &'a ResolvedModule,
    ) -> impl Iterator<Item = (&'a SmolStr, FileId, Self::ItemId)> {
        resolved.exports.iter_classes()
    }

    fn candidate(
        &self,
        context: &CompletionContext<impl crate::AnalyzerHost>,
        name: &SmolStr,
        import_id: FileId,
        file_id: FileId,
        item_id: Self::ItemId,
    ) -> Result<Option<CompletionItem>, AnalyzerError> {
        assert_eq!(import_id, file_id);

        if context.has_class_import(None, name) {
            return Ok(None);
        }

        let Some(module_name) = module_name(context, file_id)? else {
            return Ok(None);
        };

        let import_edit = edit::class_import_item(context, &module_name, name, file_id, item_id);
        let item =
            suggested_type_candidate(context, name, &module_name, file_id, item_id, import_edit);

        Ok(Some(item))
    }
}

fn suggested_type_candidate(
    context: &CompletionContext<impl crate::AnalyzerHost>,
    name: &SmolStr,
    module_name: &str,
    file_id: FileId,
    item_id: TypeItemId,
    import_edit: (Option<String>, Option<Range>),
) -> CompletionItem {
    let mut item = CompletionItemSpec::new(
        name.to_string(),
        context.range,
        CompletionItemKind::STRUCT,
        CompletionResolveData::TypeItem(file_id, item_id),
    );

    let (import_text, import_range) = import_edit;
    let import_edit = match (import_range, import_text) {
        (Some(range), Some(new_text)) => Some(TextEdit { range, new_text }),
        (None, Some(new_text)) => context.insert_import_edit(new_text),
        (_, None) => None,
    };

    item.label_detail(format!(" (import {module_name})"));
    item.label_description(module_name.to_string());
    item.sort_text(format!("{module_name}.{name}"));

    if let Some(import_edit) = import_edit {
        item.additional_text_edits(vec![import_edit]);
    }

    item.build()
}

fn suggestions_candidates<T: SuggestionsHelper>(
    this: &T,
    context: &CompletionContext<impl crate::AnalyzerHost>,
    filter: impl Filter,
    items: &mut Vec<CompletionItem>,
) -> Result<(), AnalyzerError> {
    let has_prim = context
        .resolved
        .unqualified
        .values()
        .flatten()
        .any(|import| import.file == context.prim_id);

    let file_ids = context.language.active_files().filter(move |&id| {
        let not_self = id != context.current_file;
        let not_prim = id != context.prim_id;
        not_self && (not_prim || has_prim)
    });

    for import_id in file_ids {
        let resolved = context.language.queries().resolved(import_id)?;

        let source = this
            .exports(&resolved)
            .filter(|(name, file_id, _)| filter.matches(name) && *file_id == import_id);

        for (name, file_id, item_id) in source {
            if let Some(item) = this.candidate(context, name, import_id, file_id, item_id)? {
                items.push(item);
            }
        }
    }

    Ok(())
}

impl CompletionSource for SuggestedTerms {
    type T = ();

    fn collect_into<F: Filter>(
        &self,
        context: &CompletionContext<impl crate::AnalyzerHost>,
        filter: F,
        items: &mut Vec<CompletionItem>,
    ) -> Result<Self::T, AnalyzerError> {
        suggestions_candidates(self, context, filter, items)
    }
}

impl CompletionSource for SuggestedTypes {
    type T = ();

    fn collect_into<F: Filter>(
        &self,
        context: &CompletionContext<impl crate::AnalyzerHost>,
        filter: F,
        items: &mut Vec<CompletionItem>,
    ) -> Result<Self::T, AnalyzerError> {
        suggestions_candidates(self, context, filter, items)
    }
}

impl CompletionSource for SuggestedClasses {
    type T = ();

    fn collect_into<F: Filter>(
        &self,
        context: &CompletionContext<impl crate::AnalyzerHost>,
        filter: F,
        items: &mut Vec<CompletionItem>,
    ) -> Result<Self::T, AnalyzerError> {
        suggestions_candidates(self, context, filter, items)
    }
}

/// Yields terms for implicit Prim.
pub struct PrimTerms;

impl CompletionSource for PrimTerms {
    type T = ();

    fn collect_into<F: Filter>(
        &self,
        context: &CompletionContext<impl crate::AnalyzerHost>,
        filter: F,
        items: &mut Vec<CompletionItem>,
    ) -> Result<Self::T, AnalyzerError> {
        let source = context
            .prim_resolved
            .exports
            .iter_terms()
            .filter(move |(name, _, _)| filter.matches(name));

        for (name, file_id, term_id) in source {
            let mut item = CompletionItemSpec::new(
                name.to_string(),
                context.range,
                CompletionItemKind::VALUE,
                CompletionResolveData::TermItem(file_id, term_id),
            );

            item.label_description("Prim".to_string());

            items.push(item.build())
        }

        Ok(())
    }
}

/// Yields types for implicit Prim.
pub struct PrimTypes;

/// Yields classes for implicit Prim.
pub struct PrimClasses;

impl CompletionSource for PrimTypes {
    type T = ();

    fn collect_into<F: Filter>(
        &self,
        context: &CompletionContext<impl crate::AnalyzerHost>,
        filter: F,
        items: &mut Vec<CompletionItem>,
    ) -> Result<Self::T, AnalyzerError> {
        let source = context
            .prim_resolved
            .exports
            .iter_types()
            .filter(move |(name, _, _)| filter.matches(name));

        for (name, file_id, type_item) in source {
            let mut item = CompletionItemSpec::new(
                name.to_string(),
                context.range,
                CompletionItemKind::STRUCT,
                CompletionResolveData::TypeItem(file_id, type_item),
            );

            item.label_description("Prim".to_string());

            items.push(item.build())
        }

        Ok(())
    }
}

impl CompletionSource for PrimClasses {
    type T = ();

    fn collect_into<F: Filter>(
        &self,
        context: &CompletionContext<impl crate::AnalyzerHost>,
        filter: F,
        items: &mut Vec<CompletionItem>,
    ) -> Result<Self::T, AnalyzerError> {
        let source = context
            .prim_resolved
            .exports
            .iter_classes()
            .filter(move |(name, _, _)| filter.matches(name));

        for (name, file_id, type_item) in source {
            let mut item = CompletionItemSpec::new(
                name.to_string(),
                context.range,
                CompletionItemKind::STRUCT,
                CompletionResolveData::TypeItem(file_id, type_item),
            );

            item.label_description("Prim".to_string());

            items.push(item.build())
        }

        Ok(())
    }
}

/// Yields suggestions for qualified terms.
pub struct QualifiedTermsSuggestions<'a>(pub &'a str);

/// Yields suggestions for qualified types.
pub struct QualifiedTypesSuggestions<'a>(pub &'a str);

/// Yields suggestions for qualified classes.
pub struct QualifiedClassesSuggestions<'a>(pub &'a str);

impl SuggestionsHelper for QualifiedTermsSuggestions<'_> {
    type ItemId = TermItemId;

    fn exports<'a>(
        &self,
        resolved: &'a ResolvedModule,
    ) -> impl Iterator<Item = (&'a SmolStr, FileId, Self::ItemId)> {
        resolved.exports.iter_terms()
    }

    fn candidate(
        &self,
        context: &CompletionContext<impl crate::AnalyzerHost>,
        name: &SmolStr,
        import_id: FileId,
        file_id: FileId,
        item_id: Self::ItemId,
    ) -> Result<Option<CompletionItem>, AnalyzerError> {
        let Some(module_name) = module_name(context, import_id)? else {
            return Ok(None);
        };

        let mut item = CompletionItemSpec::new(
            name.to_string(),
            context.range,
            CompletionItemKind::VALUE,
            CompletionResolveData::TermItem(file_id, item_id),
        );

        item.label_detail(format!(" (import {module_name} as {})", self.0));
        item.label_description(module_name.to_string());

        item.edit_text(format!("{}.{name}", self.0));
        item.sort_text(format!("{module_name}.{name}"));

        let new_text = format!("import {module_name} as {}\n", self.0);
        if let Some(import_edit) = context.insert_import_edit(new_text) {
            item.additional_text_edits(vec![import_edit]);
        }

        Ok(Some(item.build()))
    }
}

impl SuggestionsHelper for QualifiedTypesSuggestions<'_> {
    type ItemId = TypeItemId;

    fn exports<'a>(
        &self,
        resolved: &'a ResolvedModule,
    ) -> impl Iterator<Item = (&'a SmolStr, FileId, Self::ItemId)> {
        resolved.exports.iter_types()
    }

    fn candidate(
        &self,
        context: &CompletionContext<impl crate::AnalyzerHost>,
        name: &SmolStr,
        import_id: FileId,
        file_id: FileId,
        item_id: Self::ItemId,
    ) -> Result<Option<CompletionItem>, AnalyzerError> {
        let Some(module_name) = module_name(context, import_id)? else {
            return Ok(None);
        };

        let item = qualified_type_candidate(context, self.0, name, &module_name, file_id, item_id);

        Ok(Some(item))
    }
}

impl SuggestionsHelper for QualifiedClassesSuggestions<'_> {
    type ItemId = TypeItemId;

    fn exports<'a>(
        &self,
        resolved: &'a ResolvedModule,
    ) -> impl Iterator<Item = (&'a SmolStr, FileId, Self::ItemId)> {
        resolved.exports.iter_classes()
    }

    fn candidate(
        &self,
        context: &CompletionContext<impl crate::AnalyzerHost>,
        name: &SmolStr,
        import_id: FileId,
        file_id: FileId,
        item_id: Self::ItemId,
    ) -> Result<Option<CompletionItem>, AnalyzerError> {
        let Some(module_name) = module_name(context, import_id)? else {
            return Ok(None);
        };

        let item = qualified_type_candidate(context, self.0, name, &module_name, file_id, item_id);

        Ok(Some(item))
    }
}

fn qualified_type_candidate(
    context: &CompletionContext<impl crate::AnalyzerHost>,
    qualifier: &str,
    name: &SmolStr,
    module_name: &str,
    file_id: FileId,
    item_id: TypeItemId,
) -> CompletionItem {
    let mut item = CompletionItemSpec::new(
        name.to_string(),
        context.range,
        CompletionItemKind::STRUCT,
        CompletionResolveData::TypeItem(file_id, item_id),
    );

    item.label_detail(format!(" (import {module_name} as {qualifier})"));
    item.label_description(module_name.to_string());

    item.edit_text(format!("{qualifier}.{name}"));
    item.sort_text(format!("{module_name}.{name}"));

    let new_text = format!("import {module_name} as {qualifier}\n");
    if let Some(import_edit) = context.insert_import_edit(new_text) {
        item.additional_text_edits(vec![import_edit]);
    }

    item.build()
}

fn suggestions_candidates_qualified<T: SuggestionsHelper>(
    this: &T,
    prefix: &str,
    context: &CompletionContext<impl crate::AnalyzerHost>,
    filter: impl Filter,
    items: &mut Vec<CompletionItem>,
) -> Result<(), AnalyzerError> {
    let has_prim =
        context.resolved.qualified.values().flatten().any(|import| import.file == context.prim_id);

    let file_ids = context.language.active_files().filter(move |&id| {
        let not_self = id != context.current_file;
        let not_prim = id != context.prim_id;
        not_self && (not_prim || has_prim)
    });

    for import_id in file_ids {
        let module_name = module_name(context, import_id)?;
        let resolved = context.language.queries().resolved(import_id)?;

        if module_name.is_some_and(|module_name| {
            let filter = PerfectSegmentFuzzy(&module_name);
            !filter.matches(prefix)
        }) {
            continue;
        }

        let source = this.exports(&resolved).filter(|(name, _, _)| filter.matches(name));

        for (name, file_id, item_id) in source {
            if let Some(item) = this.candidate(context, name, import_id, file_id, item_id)? {
                items.push(item);
            }
        }
    }

    Ok(())
}

impl CompletionSource for QualifiedTermsSuggestions<'_> {
    type T = ();

    fn collect_into<F: Filter>(
        &self,
        context: &CompletionContext<impl crate::AnalyzerHost>,
        filter: F,
        items: &mut Vec<CompletionItem>,
    ) -> Result<Self::T, AnalyzerError> {
        suggestions_candidates_qualified(self, self.0, context, filter, items)
    }
}

impl CompletionSource for QualifiedTypesSuggestions<'_> {
    type T = ();

    fn collect_into<F: Filter>(
        &self,
        context: &CompletionContext<impl crate::AnalyzerHost>,
        filter: F,
        items: &mut Vec<CompletionItem>,
    ) -> Result<Self::T, AnalyzerError> {
        suggestions_candidates_qualified(self, self.0, context, filter, items)
    }
}

impl CompletionSource for QualifiedClassesSuggestions<'_> {
    type T = ();

    fn collect_into<F: Filter>(
        &self,
        context: &CompletionContext<impl crate::AnalyzerHost>,
        filter: F,
        items: &mut Vec<CompletionItem>,
    ) -> Result<Self::T, AnalyzerError> {
        suggestions_candidates_qualified(self, self.0, context, filter, items)
    }
}

/// Yields module names in the workspace.
pub struct WorkspaceModules;

impl CompletionSource for WorkspaceModules {
    type T = ();

    fn collect_into<F: Filter>(
        &self,
        context: &CompletionContext<impl crate::AnalyzerHost>,
        filter: F,
        items: &mut Vec<CompletionItem>,
    ) -> Result<Self::T, AnalyzerError> {
        for id in context.language.active_files() {
            let Some(module_name) = module_name(context, id)? else {
                continue;
            };

            if !filter.matches(&module_name) {
                continue;
            }

            let mut item = CompletionItemSpec::new(
                module_name.to_string(),
                context.range,
                CompletionItemKind::MODULE,
                CompletionResolveData::Import(id),
            );

            item.label_description(module_name.to_string());

            items.push(item.build())
        }

        Ok(())
    }
}
