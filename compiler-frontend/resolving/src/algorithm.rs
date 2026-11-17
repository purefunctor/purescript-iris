use building_types::QueryResult;
use files::FileId;
use indexing::{
    ExportKind, ImplicitItems, ImportItemId, ImportKind, IndexedImport, IndexedModule,
    IndexedTypeItemKind, InstanceSourceItemId, OrderedTermItemId, TermItemId, TypeItemId,
};
use rustc_hash::FxHashMap;
use smol_str::SmolStr;
use std::sync::Arc;

use crate::{
    ExportSource, ExternalQueries, ResolvedClassMembers, ResolvedExports, ResolvedImport,
    ResolvedImportsQualified, ResolvedImportsUnqualified, ResolvedLocals, ResolvedModule,
    ResolvingError,
};

#[derive(Default)]
pub(super) struct State {
    pub(super) unqualified: ResolvedImportsUnqualified,
    pub(super) qualified: ResolvedImportsQualified,
    pub(super) exports: ResolvedExports,
    pub(super) locals: ResolvedLocals,
    pub(super) class: ResolvedClassMembers,
    pub(super) errors: Vec<ResolvingError>,
}

pub(super) fn resolve_module(queries: &impl ExternalQueries, file: FileId) -> QueryResult<State> {
    let indexed = queries.indexed(file)?;

    let mut state = State::default();
    validate_module_name(queries, &mut state, &indexed);
    validate_instance_names(&mut state, &indexed);
    resolve_imports(queries, &mut state, &indexed)?;
    resolve_exports(&mut state, &indexed, file);

    Ok(state)
}

fn validate_module_name(
    queries: &impl ExternalQueries,
    state: &mut State,
    indexed: &IndexedModule,
) {
    let Some(name) = &indexed.name else { return };
    if queries.module_files(name).is_duplicated() {
        state.errors.push(ResolvingError::DuplicateModule { name: SmolStr::clone(name) });
    }
}

fn validate_instance_names(state: &mut State, indexed: &IndexedModule) {
    let mut instances = FxHashMap::default();
    for &instance in indexed.items.instance_sources() {
        let (name, item) = match instance {
            InstanceSourceItemId::Instance(id) => {
                (&indexed.items[id].name, OrderedTermItemId::Instance(id))
            }
            InstanceSourceItemId::Derive(id) => {
                (&indexed.items[id].name, OrderedTermItemId::Derive(id))
            }
        };
        let Some(name) = name else { continue };
        let existing = *instances.entry(name).or_insert(item);
        if existing != item {
            state.errors.push(ResolvingError::InstanceNameConflict {
                name: SmolStr::clone(name),
                instance,
                existing,
            });
        }
        if let Some(term) = indexed.names.terms.lookup(name) {
            state.errors.push(ResolvingError::InstanceNameConflict {
                name: SmolStr::clone(name),
                instance,
                existing: OrderedTermItemId::Term(term),
            });
        }
    }
}

fn resolve_imports(
    queries: &impl ExternalQueries,
    state: &mut State,
    indexed: &IndexedModule,
) -> QueryResult<()> {
    for (&indexed_import_id, indexed_import) in &indexed.imports {
        let Some(name) = &indexed_import.name else {
            state.errors.push(ResolvingError::InvalidImportStatement { id: indexed_import_id });
            continue;
        };

        let Some(import_file_id) = queries.module_file(name) else {
            state.errors.push(ResolvingError::InvalidImportStatement { id: indexed_import_id });
            continue;
        };

        let import_resolved = queries.resolved(import_file_id)?;

        let mut resolved_import = ResolvedImport::new(
            indexed_import_id,
            import_file_id,
            indexed_import.kind,
            indexed_import.exported,
            Arc::clone(&import_resolved.exports),
        );

        if let Some(alias) = &indexed_import.alias {
            let alias = SmolStr::clone(alias);
            let imports = state.qualified.entry(alias).or_insert_with(|| Vec::with_capacity(1));
            imports.push(resolved_import);
            let resolved_import = imports.last_mut().unwrap();
            resolve_import(
                queries,
                &mut state.errors,
                &mut state.class,
                resolved_import,
                indexed_import,
                &import_resolved,
            )?;
        } else {
            let name = SmolStr::clone(name);
            resolve_import(
                queries,
                &mut state.errors,
                &mut state.class,
                &mut resolved_import,
                indexed_import,
                &import_resolved,
            )?;
            let imports = state.unqualified.entry(name).or_insert_with(|| Vec::with_capacity(1));
            imports.push(resolved_import);
        }
    }

    Ok(())
}

fn resolve_import(
    queries: &impl ExternalQueries,
    errors: &mut Vec<ResolvingError>,
    class_members: &mut ResolvedClassMembers,
    resolved: &mut ResolvedImport,
    indexed_import: &IndexedImport,
    import_resolved: &ResolvedModule,
) -> QueryResult<()> {
    // Adjust import kinds for explicit/hidden imports BEFORE copying class members
    if !matches!(indexed_import.kind, ImportKind::Implicit) {
        for (name, &id) in &indexed_import.terms {
            if let Some(item) = resolved.exports.lookup_term(name) {
                resolved.listed_terms.insert(item);
            } else {
                errors.push(ResolvingError::InvalidImportItem { id });
            }
        }

        for (name, &(import_item_id, ref implicit)) in &indexed_import.types {
            if let Some((file, type_id)) = resolved.exports.lookup_type(name) {
                resolved.listed_types.insert((file, type_id));
                let Some(implicit) = implicit else { continue };
                let item = (file, type_id, import_item_id, implicit);
                resolve_implicit(queries, errors, resolved, item)?;
            } else if let Some(item) = resolved.exports.lookup_class(name) {
                resolved.listed_classes.insert(item);
            } else {
                errors.push(ResolvingError::InvalidImportItem { id: import_item_id });
            };
        }
    }

    // Copy class members AFTER kind adjustments so hidden types are properly filtered
    for (_, class_file, type_id, import_kind) in resolved.iter_classes() {
        if matches!(import_kind, ImportKind::Hidden) {
            continue;
        }
        let class = (class_file, type_id);
        if let Some(members) = import_resolved.class.classes.get(&class) {
            class_members.classes.insert(class, Arc::clone(members));
        }
    }

    Ok(())
}

fn resolve_implicit(
    queries: &impl ExternalQueries,
    errors: &mut Vec<ResolvingError>,
    resolved: &mut ResolvedImport,
    item: (FileId, TypeItemId, ImportItemId, &ImplicitItems),
) -> QueryResult<()> {
    let (f_id, t_id, import_item_id, implicit) = item;
    let import_indexed = queries.indexed(f_id)?;
    match implicit {
        ImplicitItems::Everything => {
            for term_id in import_indexed.data_constructors(t_id) {
                let item = &import_indexed.items[term_id];
                if matches!(import_indexed.kind, ExportKind::Explicit) && !item.exported {
                    continue;
                }
                let Some(name) = &item.name else {
                    continue;
                };
                if let Some(item) = resolved.exports.lookup_term(name) {
                    resolved.listed_terms.insert(item);
                }
            }
        }
        ImplicitItems::Enumerated(names) => {
            for name in names {
                let Some(term_id) = import_indexed.data_constructors(t_id).find(|term_id| {
                    let item = &import_indexed.items[*term_id];
                    item.name.as_deref() == Some(name.as_str())
                }) else {
                    errors.push(ResolvingError::InvalidImportItem { id: import_item_id });
                    continue;
                };

                let item = &import_indexed.items[term_id];
                if matches!(import_indexed.kind, ExportKind::Explicit) && !item.exported {
                    errors.push(ResolvingError::InvalidImportItem { id: import_item_id });
                    continue;
                }

                if let Some(item) = resolved.exports.lookup_term(name) {
                    resolved.listed_terms.insert(item);
                } else {
                    errors.push(ResolvingError::InvalidImportItem { id: import_item_id });
                }
            }
        }
    }
    Ok(())
}

fn resolve_exports(state: &mut State, indexed: &IndexedModule, file: FileId) {
    export_module_items(state, indexed, file);
    export_module_imports(state, indexed);
    export_class_members(state, indexed, file);
}

fn export_class_members(state: &mut State, indexed: &IndexedModule, file: FileId) {
    for (type_id, type_item) in indexed.items.iter_types() {
        if !matches!(type_item.kind, IndexedTypeItemKind::Class { .. }) {
            continue;
        }
        let mut members: Vec<(SmolStr, TermItemId)> = Vec::new();
        for member_term_id in indexed.class_members(type_id) {
            let member_item = &indexed.items[member_term_id];
            let Some(name) = &member_item.name else { continue };
            if let Some((_, existing)) = members.iter_mut().find(|(member, _)| member == name) {
                *existing = member_term_id;
            } else {
                members.push((SmolStr::clone(name), member_term_id));
            }
        }
        if !members.is_empty() {
            state.class.classes.insert((file, type_id), members.into());
        }
    }
}

fn add_local_terms<'k>(
    items: &mut ResolvedLocals,
    errors: &mut Vec<ResolvingError>,
    iterator: impl Iterator<Item = (&'k SmolStr, FileId, TermItemId)>,
) {
    let (additional, _) = iterator.size_hint();
    items.terms.reserve(additional);
    iterator.for_each(move |(name, file, id)| {
        add_local_term(items, errors, name, file, id);
    });
}

fn add_local_term(
    items: &mut ResolvedLocals,
    errors: &mut Vec<ResolvingError>,
    name: &SmolStr,
    file: FileId,
    id: TermItemId,
) {
    if let Some(&existing) = items.terms.get(name) {
        let duplicate = (file, id);
        if existing != duplicate {
            errors.push(ResolvingError::ExistingTerm { existing, duplicate });
        }
    } else {
        let name = SmolStr::clone(name);
        items.terms.insert(name, (file, id));
    }
}

fn add_local_types<'k>(
    items: &mut ResolvedLocals,
    errors: &mut Vec<ResolvingError>,
    iterator: impl Iterator<Item = (&'k SmolStr, FileId, TypeItemId)>,
) {
    let (additional, _) = iterator.size_hint();
    items.types.reserve(additional);
    iterator.for_each(move |(name, file, id)| {
        add_local_type(items, errors, name, file, id);
    });
}

fn add_local_type(
    items: &mut ResolvedLocals,
    errors: &mut Vec<ResolvingError>,
    name: &SmolStr,
    file: FileId,
    id: TypeItemId,
) {
    if let Some(&existing) = items.types.get(name) {
        let duplicate = (file, id);
        if existing != duplicate {
            errors.push(ResolvingError::ExistingType { existing, duplicate });
        }
    } else {
        let name = SmolStr::clone(name);
        items.types.insert(name, (file, id));
    }
}

fn add_local_classes<'k>(
    items: &mut ResolvedLocals,
    errors: &mut Vec<ResolvingError>,
    iterator: impl Iterator<Item = (&'k SmolStr, FileId, TypeItemId)>,
) {
    let (additional, _) = iterator.size_hint();
    items.classes.reserve(additional);
    iterator.for_each(move |(name, file, id)| {
        add_local_class(items, errors, name, file, id);
    });
}

fn add_local_class(
    items: &mut ResolvedLocals,
    errors: &mut Vec<ResolvingError>,
    name: &SmolStr,
    file: FileId,
    id: TypeItemId,
) {
    if let Some(&existing) = items.classes.get(name) {
        let duplicate = (file, id);
        if existing != duplicate {
            errors.push(ResolvingError::ExistingType { existing, duplicate });
        }
    } else {
        let name = SmolStr::clone(name);
        items.classes.insert(name, (file, id));
    }
}

fn export_module_items(state: &mut State, indexed: &IndexedModule, file: FileId) {
    let local_terms = indexed.names.terms.iter().map(|(name, id)| (name, file, id));

    let local_types = indexed.names.types.iter().map(|(name, id)| {
        let item = &indexed.items[id];
        (name, file, id, &item.kind)
    });

    let (local_class_items, local_type_items): (Vec<_>, Vec<_>) =
        local_types.partition(|(_, _, _, kind)| matches!(kind, IndexedTypeItemKind::Class { .. }));

    let local_types = local_type_items.into_iter().map(|(name, file, id, _)| (name, file, id));
    let local_classes = local_class_items.into_iter().map(|(name, file, id, _)| (name, file, id));

    add_local_terms(&mut state.locals, &mut state.errors, local_terms);
    add_local_types(&mut state.locals, &mut state.errors, local_types);
    add_local_classes(&mut state.locals, &mut state.errors, local_classes);

    let exported_terms = indexed.names.terms.iter().filter_map(|(name, id)| {
        let item = &indexed.items[id];
        if matches!(indexed.kind, ExportKind::Explicit) && !item.exported {
            return None;
        }
        Some((name, file, id, ExportSource::Local))
    });

    let exported_types = indexed.names.types.iter().filter_map(|(name, id)| {
        let item = &indexed.items[id];
        if matches!(indexed.kind, ExportKind::Explicit) && !item.exported {
            return None;
        }
        Some((name, file, id, ExportSource::Local, &item.kind))
    });

    let (exported_class_items, exported_type_items): (Vec<_>, Vec<_>) = exported_types
        .partition(|(_, _, _, _, kind)| matches!(kind, IndexedTypeItemKind::Class { .. }));

    let exported_types =
        exported_type_items.into_iter().map(|(name, file, id, source, _)| (name, file, id, source));

    let exported_classes = exported_class_items
        .into_iter()
        .map(|(name, file, id, source, _)| (name, file, id, source));

    add_export_terms(&mut state.exports, &mut state.errors, exported_terms);
    add_export_types(&mut state.exports, &mut state.errors, exported_types);
    add_export_classes(&mut state.exports, &mut state.errors, exported_classes);
}

fn export_module_imports(state: &mut State, indexed: &IndexedModule) {
    if matches!(indexed.kind, ExportKind::Implicit) {
        return;
    }

    let unqualified = state.unqualified.values().flatten();
    let qualified = state.qualified.values().flatten();
    let imports = unqualified.chain(qualified);

    for import in imports {
        if !import.exported {
            continue;
        }
        let source = ExportSource::Import(import.id);
        let terms = import.iter_terms().filter_map(|(k, f, i, d)| {
            if matches!(d, ImportKind::Implicit | ImportKind::Explicit) {
                Some((k, f, i, source))
            } else {
                None
            }
        });
        let types = import.iter_types().filter_map(|(k, f, i, d)| {
            if matches!(d, ImportKind::Implicit | ImportKind::Explicit) {
                Some((k, f, i, source))
            } else {
                None
            }
        });
        let classes = import.iter_classes().filter_map(|(k, f, i, d)| {
            if matches!(d, ImportKind::Implicit | ImportKind::Explicit) {
                Some((k, f, i, source))
            } else {
                None
            }
        });
        add_export_terms(&mut state.exports, &mut state.errors, terms);
        add_export_types(&mut state.exports, &mut state.errors, types);
        add_export_classes(&mut state.exports, &mut state.errors, classes);
    }
}

fn add_export_terms<'k>(
    items: &mut ResolvedExports,
    errors: &mut Vec<ResolvingError>,
    iterator: impl Iterator<Item = (&'k SmolStr, FileId, TermItemId, ExportSource)>,
) {
    let (additional, _) = iterator.size_hint();
    items.terms.reserve(additional);
    iterator.for_each(move |(name, file, id, source)| {
        add_export_term(items, errors, name, file, id, source);
    });
}

fn add_export_term(
    items: &mut ResolvedExports,
    errors: &mut Vec<ResolvingError>,
    name: &SmolStr,
    file: FileId,
    id: TermItemId,
    source: ExportSource,
) {
    if let Some(&existing) = items.terms.get(name) {
        let duplicate = (file, id, source);
        if !same_term_export(existing, duplicate) {
            errors.push(ResolvingError::TermExportConflict {
                name: SmolStr::clone(name),
                existing,
                duplicate,
            });
        }
    } else {
        let name = SmolStr::clone(name);
        items.terms.insert(name, (file, id, source));
    }
}

fn add_export_types<'k>(
    items: &mut ResolvedExports,
    errors: &mut Vec<ResolvingError>,
    iterator: impl Iterator<Item = (&'k SmolStr, FileId, TypeItemId, ExportSource)>,
) {
    let (additional, _) = iterator.size_hint();
    items.types.reserve(additional);
    iterator.for_each(move |(name, file, id, source)| {
        add_export_type(items, errors, name, file, id, source);
    });
}

fn add_export_type(
    items: &mut ResolvedExports,
    errors: &mut Vec<ResolvingError>,
    name: &SmolStr,
    file: FileId,
    id: TypeItemId,
    source: ExportSource,
) {
    if let Some(&existing) = items.types.get(name) {
        let duplicate = (file, id, source);
        if !same_type_export(existing, duplicate) {
            errors.push(ResolvingError::TypeExportConflict {
                name: SmolStr::clone(name),
                existing,
                duplicate,
            });
        }
    } else {
        let name = SmolStr::clone(name);
        items.types.insert(name, (file, id, source));
    }
}

fn add_export_classes<'k>(
    items: &mut ResolvedExports,
    errors: &mut Vec<ResolvingError>,
    iterator: impl Iterator<Item = (&'k SmolStr, FileId, TypeItemId, ExportSource)>,
) {
    let (additional, _) = iterator.size_hint();
    items.classes.reserve(additional);
    iterator.for_each(move |(name, file, id, source)| {
        add_export_class(items, errors, name, file, id, source);
    });
}

fn add_export_class(
    items: &mut ResolvedExports,
    errors: &mut Vec<ResolvingError>,
    name: &SmolStr,
    file: FileId,
    id: TypeItemId,
    source: ExportSource,
) {
    if let Some(&existing) = items.classes.get(name) {
        let duplicate = (file, id, source);
        if !same_type_export(existing, duplicate) {
            errors.push(ResolvingError::TypeExportConflict {
                name: SmolStr::clone(name),
                existing,
                duplicate,
            });
        }
    } else {
        let name = SmolStr::clone(name);
        items.classes.insert(name, (file, id, source));
    }
}

fn same_term_export(
    (existing_file, existing_id, _): (FileId, TermItemId, ExportSource),
    (duplicate_file, duplicate_id, _): (FileId, TermItemId, ExportSource),
) -> bool {
    existing_file == duplicate_file && existing_id == duplicate_id
}

fn same_type_export(
    (existing_file, existing_id, _): (FileId, TypeItemId, ExportSource),
    (duplicate_file, duplicate_id, _): (FileId, TypeItemId, ExportSource),
) -> bool {
    existing_file == duplicate_file && existing_id == duplicate_id
}
