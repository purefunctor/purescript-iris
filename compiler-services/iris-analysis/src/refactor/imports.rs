use std::collections::BTreeMap;

use files::FileId;
use indexing::{ImportId, ImportKind, IndexedTypeItemKind, TypeItemId};
use resolving::{ResolvedModule, TypeNamespace};
use rustc_hash::{FxHashMap, FxHashSet};
use smol_str::{SmolStr, format_smolstr};
use stabilizing::StabilizedModule;
use syntax::ast::AstNode;
use syntax::{SyntaxNode, cst};

use crate::refactor::SourceEdit;
use crate::{AnalyzerError, AnalyzerQueries, nominal};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportableType {
    pub file_id: FileId,
    pub type_id: TypeItemId,
    pub module_name: SmolStr,
    pub name: SmolStr,
    pub namespace: TypeNamespace,
    pub import_item_spelling: SmolStr,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Importability {
    Importable(ImportableType),
    Unimportable(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlannedImports {
    pub spellings: FxHashMap<(FileId, TypeItemId), SmolStr>,
    pub edits: Vec<SourceEdit>,
    pub imports_added: usize,
    pub imports_updated: usize,
}

pub fn check_importable(
    engine: &impl AnalyzerQueries,
    file_id: FileId,
    type_id: TypeItemId,
) -> Result<Importability, AnalyzerError> {
    let indexed = engine.indexed(file_id)?;
    let item = &indexed.items[type_id];
    let Some(name) = item.name.as_ref() else {
        return Ok(Importability::Unimportable("<unnamed>".to_string()));
    };
    let Some(module_name) = nominal::module_name(engine, file_id)? else {
        return Ok(Importability::Unimportable(name.to_string()));
    };
    let namespace = if matches!(item.kind, IndexedTypeItemKind::Class { .. }) {
        TypeNamespace::Class
    } else {
        TypeNamespace::Type
    };
    let target_resolved = engine.resolved(file_id)?;
    let exported = match namespace {
        TypeNamespace::Type => {
            target_resolved.exports.lookup_type(name) == Some((file_id, type_id))
        }
        TypeNamespace::Class => {
            target_resolved.exports.lookup_class(name) == Some((file_id, type_id))
        }
    };
    if !exported {
        return Ok(Importability::Unimportable(format!("{module_name}.{name}")));
    }
    let import_item_spelling = match item.kind {
        IndexedTypeItemKind::Class { .. } => format_smolstr!("class {name}"),
        IndexedTypeItemKind::Operator { .. } => format_smolstr!("type ({name})"),
        _ => SmolStr::clone(name),
    };
    Ok(Importability::Importable(ImportableType {
        file_id,
        type_id,
        module_name: SmolStr::new(module_name),
        name: SmolStr::clone(name),
        namespace,
        import_item_spelling,
    }))
}

pub fn plan_imports(
    content: &str,
    root: &SyntaxNode,
    stabilized: &StabilizedModule,
    resolved: &ResolvedModule,
    prim: &ResolvedModule,
    missing: &[ImportableType],
) -> PlannedImports {
    if missing.is_empty() {
        return PlannedImports::default();
    }

    let mut ordered = missing.to_vec();
    ordered.sort_by(|left, right| {
        (left.module_name.as_str(), left.name.as_str(), left.file_id, left.type_id).cmp(&(
            right.module_name.as_str(),
            right.name.as_str(),
            right.file_id,
            right.type_id,
        ))
    });
    ordered.dedup_by(|left, right| left.file_id == right.file_id && left.type_id == right.type_id);

    let mut claimed_unqualified_types = FxHashSet::<SmolStr>::default();
    let mut claimed_unqualified_classes = FxHashSet::<SmolStr>::default();
    let mut claimed_qualified_types = FxHashMap::<SmolStr, FxHashSet<SmolStr>>::default();
    let mut claimed_qualified_classes = FxHashMap::<SmolStr, FxHashSet<SmolStr>>::default();

    let mut spellings = FxHashMap::<(FileId, TypeItemId), SmolStr>::default();
    let mut existing_additions = BTreeMap::<ImportId, Vec<SmolStr>>::new();
    let mut new_unqualified = BTreeMap::<SmolStr, Vec<SmolStr>>::new();
    let mut new_qualified = BTreeMap::<(SmolStr, SmolStr), Vec<SmolStr>>::new();

    for item in ordered {
        let claimed_unqualified = match item.namespace {
            TypeNamespace::Type => &claimed_unqualified_types,
            TypeNamespace::Class => &claimed_unqualified_classes,
        };
        let bare_free =
            is_bare_name_free(resolved, prim, &item.name, item.namespace, claimed_unqualified);

        if bare_free {
            if let Some(import_id) = find_editable_explicit_unqualified_import(
                content,
                root,
                stabilized,
                resolved,
                &item.module_name,
                item.file_id,
            ) {
                existing_additions
                    .entry(import_id)
                    .or_default()
                    .push(SmolStr::clone(&item.import_item_spelling));
                match item.namespace {
                    TypeNamespace::Type => {
                        claimed_unqualified_types.insert(SmolStr::clone(&item.name));
                    }
                    TypeNamespace::Class => {
                        claimed_unqualified_classes.insert(SmolStr::clone(&item.name));
                    }
                }
                spellings.insert((item.file_id, item.type_id), SmolStr::clone(&item.name));
                continue;
            }

            new_unqualified
                .entry(SmolStr::clone(&item.module_name))
                .or_default()
                .push(SmolStr::clone(&item.import_item_spelling));
            match item.namespace {
                TypeNamespace::Type => {
                    claimed_unqualified_types.insert(SmolStr::clone(&item.name));
                }
                TypeNamespace::Class => {
                    claimed_unqualified_classes.insert(SmolStr::clone(&item.name));
                }
            }
            spellings.insert((item.file_id, item.type_id), SmolStr::clone(&item.name));
            continue;
        }

        let claimed_qualified = match item.namespace {
            TypeNamespace::Type => &claimed_qualified_types,
            TypeNamespace::Class => &claimed_qualified_classes,
        };
        if let Some((alias, import_id)) = find_editable_explicit_qualified_import(
            content,
            root,
            stabilized,
            resolved,
            prim,
            item.file_id,
            &item.name,
            item.namespace,
            claimed_qualified,
        ) {
            existing_additions
                .entry(import_id)
                .or_default()
                .push(SmolStr::clone(&item.import_item_spelling));
            match item.namespace {
                TypeNamespace::Type => {
                    claimed_qualified_types
                        .entry(SmolStr::clone(&alias))
                        .or_default()
                        .insert(SmolStr::clone(&item.name));
                }
                TypeNamespace::Class => {
                    claimed_qualified_classes
                        .entry(SmolStr::clone(&alias))
                        .or_default()
                        .insert(SmolStr::clone(&item.name));
                }
            }
            spellings
                .insert((item.file_id, item.type_id), format_smolstr!("{alias}.{}", item.name));
            continue;
        }

        let alias = choose_qualified_alias(
            resolved,
            prim,
            &item.module_name,
            &item.name,
            item.namespace,
            claimed_qualified,
        );
        new_qualified
            .entry((SmolStr::clone(&item.module_name), SmolStr::clone(&alias)))
            .or_default()
            .push(SmolStr::clone(&item.import_item_spelling));
        match item.namespace {
            TypeNamespace::Type => {
                claimed_qualified_types
                    .entry(SmolStr::clone(&alias))
                    .or_default()
                    .insert(SmolStr::clone(&item.name));
            }
            TypeNamespace::Class => {
                claimed_qualified_classes
                    .entry(SmolStr::clone(&alias))
                    .or_default()
                    .insert(SmolStr::clone(&item.name));
            }
        }
        spellings.insert((item.file_id, item.type_id), format_smolstr!("{alias}.{}", item.name));
    }

    let newline = if content.contains("\r\n") { "\r\n" } else { "\n" };
    let mut edits = Vec::new();
    let mut imports_updated = 0;

    for (import_id, additions) in existing_additions {
        if let Some(edit) =
            build_extend_import_edit(content, root, stabilized, import_id, &additions, newline)
        {
            edits.push(edit);
            imports_updated += 1;
        }
    }

    let mut new_import_lines = Vec::new();
    for (module_name, items) in new_unqualified {
        let joined = items.join(", ");
        new_import_lines.push(format!("import {module_name} ({joined})"));
    }
    for ((module_name, alias), items) in new_qualified {
        let joined = items.join(", ");
        new_import_lines.push(format!("import {module_name} ({joined}) as {alias}"));
    }
    new_import_lines.sort();

    let imports_added = new_import_lines.len();
    if !new_import_lines.is_empty()
        && let Some(edit) = build_insert_imports_edit(content, root, &new_import_lines, newline)
    {
        edits.push(edit);
    }

    PlannedImports { spellings, edits, imports_added, imports_updated }
}

fn is_bare_name_free(
    resolved: &ResolvedModule,
    prim: &ResolvedModule,
    name: &str,
    namespace: TypeNamespace,
    claimed: &FxHashSet<SmolStr>,
) -> bool {
    if claimed.contains(name) {
        return false;
    }
    let local_occupied = match namespace {
        TypeNamespace::Type => resolved.locals.lookup_type(name).is_some(),
        TypeNamespace::Class => resolved.locals.lookup_class(name).is_some(),
    };
    if local_occupied {
        return false;
    }
    let visible_in_scope = match namespace {
        TypeNamespace::Type => resolved.lookup_type(prim, None, name).is_some(),
        TypeNamespace::Class => resolved.lookup_class(prim, None, name).is_some(),
    };
    if visible_in_scope {
        return false;
    }
    let explicitly_hidden = resolved.unqualified.values().flatten().any(|import| {
        if !matches!(import.kind, ImportKind::Hidden) {
            return false;
        }
        let lookup = match namespace {
            TypeNamespace::Type => import.lookup_type(name),
            TypeNamespace::Class => import.lookup_class(name),
        };
        matches!(lookup, Some((_, _, ImportKind::Hidden)))
    });
    !explicitly_hidden
}

fn is_qualified_name_free(
    resolved: &ResolvedModule,
    prim: &ResolvedModule,
    alias: &str,
    name: &str,
    namespace: TypeNamespace,
    claimed: &FxHashMap<SmolStr, FxHashSet<SmolStr>>,
) -> bool {
    if claimed.get(alias).is_some_and(|names| names.contains(name)) {
        return false;
    }
    let visible = match namespace {
        TypeNamespace::Type => resolved.lookup_type(prim, Some(alias), name).is_some(),
        TypeNamespace::Class => resolved.lookup_class(prim, Some(alias), name).is_some(),
    };
    if visible {
        return false;
    }
    let hidden = resolved.qualified.get(alias).is_some_and(|imports| {
        imports.iter().any(|import| {
            if !matches!(import.kind, ImportKind::Hidden) {
                return false;
            }
            let lookup = match namespace {
                TypeNamespace::Type => import.lookup_type(name),
                TypeNamespace::Class => import.lookup_class(name),
            };
            matches!(lookup, Some((_, _, ImportKind::Hidden)))
        })
    });
    !hidden
}

fn choose_qualified_alias(
    resolved: &ResolvedModule,
    prim: &ResolvedModule,
    module_name: &str,
    name: &str,
    namespace: TypeNamespace,
    claimed: &FxHashMap<SmolStr, FxHashSet<SmolStr>>,
) -> SmolStr {
    if is_qualified_name_free(resolved, prim, module_name, name, namespace, claimed) {
        return SmolStr::new(module_name);
    }
    for suffix in 1u32.. {
        let candidate = format_smolstr!("{module_name}{suffix}");
        if is_qualified_name_free(resolved, prim, &candidate, name, namespace, claimed) {
            return candidate;
        }
    }
    unreachable!("invariant violated: exhausted qualified alias suffixes")
}

fn find_editable_explicit_unqualified_import(
    content: &str,
    root: &SyntaxNode,
    stabilized: &StabilizedModule,
    resolved: &ResolvedModule,
    module_name: &str,
    target_file: FileId,
) -> Option<ImportId> {
    let imports = resolved.unqualified.get(module_name)?;
    imports.iter().find_map(|import| {
        if import.file == target_file
            && matches!(import.kind, ImportKind::Explicit)
            && is_import_list_editable(content, root, stabilized, import.id)
        {
            Some(import.id)
        } else {
            None
        }
    })
}

#[allow(clippy::too_many_arguments)]
fn find_editable_explicit_qualified_import(
    content: &str,
    root: &SyntaxNode,
    stabilized: &StabilizedModule,
    resolved: &ResolvedModule,
    prim: &ResolvedModule,
    target_file: FileId,
    name: &str,
    namespace: TypeNamespace,
    claimed: &FxHashMap<SmolStr, FxHashSet<SmolStr>>,
) -> Option<(SmolStr, ImportId)> {
    let mut best: Option<(SmolStr, ImportId)> = None;
    for (alias, imports) in &resolved.qualified {
        if !is_qualified_name_free(resolved, prim, alias, name, namespace, claimed) {
            continue;
        }
        let matching = imports.iter().find(|import| {
            import.file == target_file
                && matches!(import.kind, ImportKind::Explicit)
                && is_import_list_editable(content, root, stabilized, import.id)
        });
        let Some(import) = matching else { continue };
        let replace = best.as_ref().is_none_or(|(current, _)| {
            (alias.len(), alias.as_str()) < (current.len(), current.as_str())
        });
        if replace {
            best = Some((SmolStr::clone(alias), import.id));
        }
    }
    best
}

fn is_import_list_editable(
    content: &str,
    root: &SyntaxNode,
    stabilized: &StabilizedModule,
    import_id: ImportId,
) -> bool {
    let Some(statement) = resolve_import_statement(root, stabilized, import_id) else {
        return false;
    };
    let Some(import_list) = statement.import_list() else {
        return false;
    };
    if import_list.hiding().is_some() {
        return false;
    }
    if import_list.children().next().is_none() {
        return false;
    }
    let range = import_list.syntax().text_range();
    let text = &content[usize::from(range.start())..usize::from(range.end())];
    !text.contains("--") && !text.contains("{-")
}

fn resolve_import_statement(
    root: &SyntaxNode,
    stabilized: &StabilizedModule,
    import_id: ImportId,
) -> Option<cst::ImportStatement> {
    let pointer = stabilized.ast_ptr(import_id)?;
    pointer.try_to_node(root)
}

fn build_extend_import_edit(
    content: &str,
    root: &SyntaxNode,
    stabilized: &StabilizedModule,
    import_id: ImportId,
    additions: &[SmolStr],
    newline: &str,
) -> Option<SourceEdit> {
    let statement = resolve_import_statement(root, stabilized, import_id)?;
    let import_list = statement.import_list()?;
    let last_item = import_list.children().last()?;
    let last_item_end = usize::from(last_item.syntax().text_range().end());
    let list_end = usize::from(import_list.syntax().text_range().end());
    let between = &content[last_item_end..list_end];

    let new_text = if let Some(last_newline) = between.rfind('\n') {
        let closing_line = &between[last_newline + 1..];
        let indent_len = closing_line.bytes().take_while(|byte| *byte == b' ').count();
        let indent = &closing_line[..indent_len];
        let mut buffer = String::new();
        for addition in additions {
            buffer.push_str(newline);
            buffer.push_str(indent);
            buffer.push_str(", ");
            buffer.push_str(addition);
        }
        buffer
    } else {
        format!(", {}", additions.join(", "))
    };

    Some(SourceEdit { start_byte: last_item_end, end_byte: last_item_end, new_text })
}

fn build_insert_imports_edit(
    content: &str,
    root: &SyntaxNode,
    import_lines: &[String],
    newline: &str,
) -> Option<SourceEdit> {
    let module = cst::Module::cast(root.clone())?;
    let joined = import_lines.join(newline);

    if let Some(last_import) = module.imports().and_then(|imports| imports.children().last()) {
        let end = usize::from(last_import.syntax().text_range().end());
        return Some(insert_after_line(content, end, &joined, false, newline));
    }

    let header = module.header()?;
    let end = usize::from(header.syntax().text_range().end());
    Some(insert_after_line(content, end, &joined, true, newline))
}

fn insert_after_line(
    content: &str,
    offset: usize,
    text: &str,
    leading_blank_line: bool,
    newline: &str,
) -> SourceEdit {
    let rest = &content[offset..];
    if let Some(relative) = rest.find('\n') {
        let insert_at = offset + relative + 1;
        let new_text = if leading_blank_line {
            format!("{newline}{text}{newline}")
        } else {
            format!("{text}{newline}")
        };
        SourceEdit { start_byte: insert_at, end_byte: insert_at, new_text }
    } else {
        let insert_at = content.len();
        let new_text = if leading_blank_line {
            format!("{newline}{newline}{text}")
        } else {
            format!("{newline}{text}")
        };
        SourceEdit { start_byte: insert_at, end_byte: insert_at, new_text }
    }
}
