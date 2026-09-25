use building_types::QueryResult;
use files::FileId;
use indexing::{
    ExportKind, ImplicitItems, ImportItemId, ImportKind, IndexedImport, IndexedModule,
    IndexedTypeItemKind, InstanceSourceItemId, OrderedTermItemId, TermItemId, TypeItemId,
};
use rustc_hash::FxHashMap;
use smol_str::SmolStr;

use crate::{
    ExportSource, ExternalQueries, ResolvedClassMembers, ResolvedExports, ResolvedImport,
    ResolvedImportsQualified, ResolvedImportsUnqualified, ResolvedLocals, ResolvingError,
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
    validate_instance_names(&mut state, &indexed);
    resolve_imports(queries, &mut state, &indexed)?;
    resolve_exports(&mut state, &indexed, file);

    Ok(state)
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

        let mut resolved_import = ResolvedImport::new(
            indexed_import_id,
            import_file_id,
            indexed_import.kind,
            indexed_import.exported,
        );

        if let Some(alias) = &indexed_import.alias {
            let alias = SmolStr::clone(alias);
            let imports = state.qualified.entry(alias).or_default();
            imports.push(resolved_import);
            let resolved_import = imports.last_mut().unwrap();
            resolve_import(
                queries,
                &mut state.errors,
                &mut state.class,
                resolved_import,
                indexed_import,
                import_file_id,
            )?;
        } else {
            let name = SmolStr::clone(name);
            resolve_import(
                queries,
                &mut state.errors,
                &mut state.class,
                &mut resolved_import,
                indexed_import,
                import_file_id,
            )?;
            state.unqualified.entry(name).or_default().push(resolved_import);
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
    import_file_id: FileId,
) -> QueryResult<()> {
    let kind = match indexed_import.kind {
        ImportKind::Implicit => ImportKind::Implicit,
        ImportKind::Explicit => ImportKind::Hidden,
        ImportKind::Hidden => ImportKind::Implicit,
    };

    let import_resolved = queries.resolved(import_file_id)?;

    let terms = import_resolved.exports.iter_terms().map(|(name, file, id)| (name, file, id, kind));
    let types = import_resolved.exports.iter_types().map(|(name, file, id)| (name, file, id, kind));
    let classes =
        import_resolved.exports.iter_classes().map(|(name, file, id)| (name, file, id, kind));

    add_imported_terms(resolved, terms);
    add_imported_types(resolved, types);
    add_imported_classes(resolved, classes);

    // Adjust import kinds for explicit/hidden imports BEFORE copying class members
    if !matches!(indexed_import.kind, ImportKind::Implicit) {
        for (name, &id) in &indexed_import.terms {
            if let Some((_, _, kind)) = resolved.terms.get_mut(name) {
                *kind = indexed_import.kind;
            } else {
                errors.push(ResolvingError::InvalidImportItem { id });
            }
        }

        for (name, &(import_item_id, ref implicit)) in &indexed_import.types {
            if let Some((file, type_id, kind)) = resolved.types.get_mut(name) {
                *kind = indexed_import.kind;
                let Some(implicit) = implicit else { continue };
                let item = (*file, *type_id, import_item_id, implicit);
                resolve_implicit(queries, errors, resolved, indexed_import, item)?;
            } else if let Some((_, _, kind)) = resolved.classes.get_mut(name) {
                *kind = indexed_import.kind;
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
        class_members.insert_class(class_file, type_id, &import_resolved.class);
    }

    Ok(())
}

fn resolve_implicit(
    queries: &impl ExternalQueries,
    errors: &mut Vec<ResolvingError>,
    resolved: &mut ResolvedImport,
    indexed_import: &IndexedImport,
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
                if let Some((_, _, term_kind)) = resolved.terms.get_mut(name) {
                    *term_kind = indexed_import.kind;
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

                if let Some((_, _, term_kind)) = resolved.terms.get_mut(name) {
                    *term_kind = indexed_import.kind;
                } else {
                    errors.push(ResolvingError::InvalidImportItem { id: import_item_id });
                }
            }
        }
    }
    Ok(())
}

fn add_imported_terms<'a>(
    resolved: &mut ResolvedImport,
    terms: impl Iterator<Item = (&'a SmolStr, FileId, TermItemId, ImportKind)>,
) {
    let (additional, _) = terms.size_hint();
    resolved.terms.reserve(additional);
    for (name, file, id, kind) in terms {
        let name = SmolStr::clone(name);
        resolved.terms.insert(name, (file, id, kind));
    }
}

fn add_imported_types<'a>(
    resolved: &mut ResolvedImport,
    types: impl Iterator<Item = (&'a SmolStr, FileId, TypeItemId, ImportKind)>,
) {
    let (additional, _) = types.size_hint();
    resolved.types.reserve(additional);
    for (name, file, id, kind) in types {
        let name = SmolStr::clone(name);
        resolved.types.insert(name, (file, id, kind));
    }
}

fn add_imported_classes<'a>(
    resolved: &mut ResolvedImport,
    classes: impl Iterator<Item = (&'a SmolStr, FileId, TypeItemId, ImportKind)>,
) {
    let (additional, _) = classes.size_hint();
    resolved.classes.reserve(additional);
    for (name, file, id, kind) in classes {
        let name = SmolStr::clone(name);
        resolved.classes.insert(name, (file, id, kind));
    }
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
        for member_term_id in indexed.class_members(type_id) {
            let member_item = &indexed.items[member_term_id];
            if let Some(name) = &member_item.name {
                let name = SmolStr::clone(name);
                state.class.insert(file, type_id, name, file, member_term_id);
            }
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
            errors.push(ResolvingError::TermExportConflict { existing, duplicate });
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
            errors.push(ResolvingError::TypeExportConflict { existing, duplicate });
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
            errors.push(ResolvingError::TypeExportConflict { existing, duplicate });
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
