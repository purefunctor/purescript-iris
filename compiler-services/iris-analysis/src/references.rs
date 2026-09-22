use building_types::QueryProxy;
use files::FileId;
use indexing::{ImportId, ImportItemId, ImportKind, TermItemId, TypeItemId};
use lowering::{
    BinderId, BinderKind, ExpressionId, ExpressionKind, LetBindingNameGroupId, RecordPunId,
    TermVariableResolution, TypeId, TypeKind, TypeVariableBindingId, TypeVariableResolution,
};
use lsp_types::*;
use parsing::ParsedModule;
use resolving::ResolvedImport;
use rustc_hash::{FxHashMap, FxHashSet};
use smol_str::ToSmolStr;
use stabilizing::{AstId, StabilizedModule};
use syntax::ast::{AstNode, AstPtr};
use syntax::cst;

use crate::position::PositionConverter;
use crate::{AnalyzerContext, AnalyzerError, common, locate};

pub fn implementation(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    uri: Url,
    position: Position,
) -> Result<Option<Vec<Location>>, AnalyzerError> {
    let current_file = {
        let uri = uri.as_str();
        context.file_id(uri).ok_or(AnalyzerError::NonFatal)?
    };

    let content = context.queries().content(current_file)?;
    let positions = PositionConverter::new(&content, context.position_encoding());
    let position = positions.protocol_position_to_utf8(position).ok_or(AnalyzerError::NonFatal)?;

    let located = locate::locate(context.queries(), current_file, &positions, position)?;

    match located {
        locate::Located::ModuleName(module_name) => {
            references_module_name(context, current_file, module_name)
        }
        locate::Located::ImportItem(import_id) => {
            references_import(context, current_file, import_id)
        }
        locate::Located::Binder(binder_id) => references_binder(context, current_file, binder_id),
        locate::Located::Expression(expression_id) => {
            references_expression(context, current_file, expression_id)
        }
        locate::Located::Type(type_id) => references_type(context, current_file, type_id),
        locate::Located::TermOperator(operator_id) => {
            let lowered = context.queries().lowered(current_file)?;
            let (f_id, t_id) =
                lowered.tree.get_term_operator(operator_id).ok_or(AnalyzerError::NonFatal)?;
            references_file_term(context, current_file, f_id, t_id)
        }
        locate::Located::TypeOperator(operator_id) => {
            let lowered = context.queries().lowered(current_file)?;
            let (f_id, t_id) =
                lowered.tree.get_type_operator(operator_id).ok_or(AnalyzerError::NonFatal)?;
            references_file_type(context, current_file, f_id, t_id)
        }
        locate::Located::TermReference(file_id, term_id) => {
            references_file_term(context, current_file, file_id, term_id)
        }
        locate::Located::TypeReference(file_id, type_id) => {
            references_file_type(context, current_file, file_id, type_id)
        }
        locate::Located::InstanceHead(file_id, type_id) => {
            references_file_type(context, current_file, file_id, type_id)
        }
        locate::Located::TermItem(term_id) => {
            references_file_term(context, current_file, current_file, term_id)
        }
        locate::Located::TypeItem(type_id) => {
            references_file_type(context, current_file, current_file, type_id)
        }
        locate::Located::InstanceItem(item_id) => {
            let uri = common::file_uri(context, current_file)?;
            let location = common::file_instance_location(context, uri, current_file, item_id)?;
            Ok(Some(vec![location]))
        }
        locate::Located::DeriveItem(item_id) => {
            let uri = common::file_uri(context, current_file)?;
            let location = common::file_derive_location(context, uri, current_file, item_id)?;
            Ok(Some(vec![location]))
        }
        locate::Located::LetBinding(let_id) => references_let(context, current_file, let_id),
        locate::Located::BinderPun(pun_id) => references_binder_pun(context, current_file, pun_id),
        locate::Located::ExpressionPun(pun_id) => {
            references_expression_pun(context, current_file, pun_id)
        }
        locate::Located::InstanceMember(_, _) => Ok(None),
        locate::Located::RecordAccessLabel(_) => Ok(None),
        locate::Located::TypeVariableBinding(binding_id) => {
            references_type_variable(context, current_file, binding_id)
        }
        locate::Located::Nothing => Ok(None),
    }
}

fn references_module_name(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    current_file: FileId,
    module_name: AstPtr<cst::ModuleName>,
) -> Result<Option<Vec<Location>>, AnalyzerError> {
    let engine = context.queries();
    let content = engine.content(current_file)?;
    let (parsed, _) = engine.parsed(current_file)?;

    let root = parsed.syntax_node();
    let module_name = module_name.try_to_node(&root).ok_or(AnalyzerError::NonFatal)?;

    let module_name = module_name.syntax().text(&content).to_smolstr();
    let module_id = engine.module_file(&module_name).ok_or(AnalyzerError::NonFatal)?;

    let candidates = probe_imports_for(context, module_id)?;
    let mut candidates_by_file: FxHashMap<FileId, Vec<(usize, ImportId)>> = FxHashMap::default();
    for (position, (file_id, import_id)) in candidates.into_iter().enumerate() {
        candidates_by_file.entry(file_id).or_default().push((position, import_id));
    }

    let mut locations = vec![];
    for (candidate_id, import_ids) in candidates_by_file {
        let uri = common::file_uri(context, candidate_id)?;

        let content = engine.content(candidate_id)?;
        let positions = PositionConverter::new(&content, context.position_encoding());
        let (parsed, _) = engine.parsed(candidate_id)?;
        let root = parsed.syntax_node();

        let stabilized = engine.stabilized(candidate_id)?;
        for (position, import_id) in import_ids {
            let ptr = stabilized.syntax_ptr(import_id).ok_or(AnalyzerError::NonFatal)?;
            let range = locate::syntax_range(&positions, &root, &ptr)
                .and_then(|range| positions.utf8_range_to_protocol(range))
                .ok_or(AnalyzerError::NonFatal)?;

            locations.push((position, Location { uri: uri.clone(), range }));
        }
    }

    locations.sort_by_key(|(position, _)| *position);
    let locations = locations.into_iter().map(|(_, location)| location).collect();
    Ok(Some(locations))
}

fn references_import(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    current_file: FileId,
    import_id: ImportItemId,
) -> Result<Option<Vec<Location>>, AnalyzerError> {
    let engine = context.queries();
    let content = engine.content(current_file)?;
    let (parsed, _) = engine.parsed(current_file)?;
    let stabilized = engine.stabilized(current_file)?;

    let root = parsed.syntax_node();
    let ptr = stabilized.ast_ptr(import_id).ok_or(AnalyzerError::NonFatal)?;
    let node = ptr.try_to_node(&root).ok_or(AnalyzerError::NonFatal)?;

    let statement = node
        .syntax()
        .ancestors()
        .find_map(cst::ImportStatement::cast)
        .ok_or(AnalyzerError::NonFatal)?;
    let module_name = statement
        .module_name()
        .ok_or(AnalyzerError::NonFatal)?
        .syntax()
        .text(&content)
        .to_smolstr();

    let import_resolved = {
        let import_id = engine.module_file(&module_name).ok_or(AnalyzerError::NonFatal)?;
        engine.resolved(import_id)?
    };

    let references_term = |name: &str| {
        let name = name.trim_start_matches("(").trim_end_matches(")");
        let (f_id, t_id) =
            import_resolved.exports.lookup_term(name).ok_or(AnalyzerError::NonFatal)?;
        references_file_term(context, current_file, f_id, t_id)
    };

    let references_type = |name: &str| {
        let name = name.trim_start_matches("(").trim_end_matches(")");
        let (f_id, t_id) = import_resolved
            .exports
            .lookup_type(name)
            .or_else(|| import_resolved.exports.lookup_class(name))
            .ok_or(AnalyzerError::NonFatal)?;
        references_file_type(context, current_file, f_id, t_id)
    };

    let references_class = |name: &str| {
        let name = name.trim_start_matches("(").trim_end_matches(")");
        let (f_id, t_id) = import_resolved
            .exports
            .lookup_class(name)
            .or_else(|| import_resolved.exports.lookup_type(name))
            .ok_or(AnalyzerError::NonFatal)?;
        references_file_type(context, current_file, f_id, t_id)
    };

    match node {
        cst::ImportItem::ImportValue(cst) => {
            let token = cst.name_token().ok_or(AnalyzerError::NonFatal)?;
            let name = token.text(&content);
            references_term(name)
        }
        cst::ImportItem::ImportClass(cst) => {
            let token = cst.name_token().ok_or(AnalyzerError::NonFatal)?;
            let name = token.text(&content);
            references_class(name)
        }
        cst::ImportItem::ImportType(cst) => {
            let token = cst.name_token().ok_or(AnalyzerError::NonFatal)?;
            let name = token.text(&content);
            references_type(name)
        }
        cst::ImportItem::ImportOperator(cst) => {
            let token = cst.name_token().ok_or(AnalyzerError::NonFatal)?;
            let name = token.text(&content);
            references_term(name)
        }
        cst::ImportItem::ImportTypeOperator(cst) => {
            let token = cst.name_token().ok_or(AnalyzerError::NonFatal)?;
            let name = token.text(&content);
            references_type(name)
        }
    }
}

fn references_binder(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    current_file: FileId,
    binder_id: BinderId,
) -> Result<Option<Vec<Location>>, AnalyzerError> {
    let uri = common::file_uri(context, current_file)?;

    let content = context.queries().content(current_file)?;
    let positions = PositionConverter::new(&content, context.position_encoding());
    let (parsed, _) = context.queries().parsed(current_file)?;

    let stabilized = context.queries().stabilized(current_file)?;
    let lowered = context.queries().lowered(current_file)?;

    let kind = lowered.tree.get_binder_kind(binder_id).ok_or(AnalyzerError::NonFatal)?;
    match kind {
        lowering::BinderKind::Constructor { resolution, .. } => {
            let (f_id, t_id) = resolution.as_ref().ok_or(AnalyzerError::NonFatal)?;
            references_file_term(context, current_file, *f_id, *t_id)
        }
        lowering::BinderKind::Named { .. } | lowering::BinderKind::Variable { .. } => {
            let mut locations = vec![];

            for (expression_id, expression_kind) in lowered.tree.iter_expression() {
                if let ExpressionKind::Variable {
                    resolution: Some(TermVariableResolution::Binder(candidate_id)),
                } = expression_kind
                    && *candidate_id == binder_id
                {
                    let uri = Url::clone(&uri);
                    let range = id_range(&positions, &parsed, &stabilized, expression_id)
                        .ok_or(AnalyzerError::NonFatal)?;
                    locations.push(Location { uri, range });
                }
            }

            for (expression_id, resolution) in lowered.tree.iter_expression_pun() {
                if let TermVariableResolution::Binder(resolution_id) = resolution
                    && resolution_id == binder_id
                {
                    let uri = Url::clone(&uri);
                    let range = id_range(&positions, &parsed, &stabilized, expression_id)
                        .ok_or(AnalyzerError::NonFatal)?;
                    locations.push(Location { uri, range });
                }
            }

            Ok(Some(locations))
        }
        _ => Ok(None),
    }
}

fn references_expression(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    current_file: FileId,
    expression_id: ExpressionId,
) -> Result<Option<Vec<Location>>, AnalyzerError> {
    let lowered = context.queries().lowered(current_file)?;
    let kind = lowered.tree.get_expression_kind(expression_id).ok_or(AnalyzerError::NonFatal)?;
    match kind {
        ExpressionKind::Constructor { resolution, .. } => {
            let (f_id, t_id) = resolution.as_ref().ok_or(AnalyzerError::NonFatal)?;
            references_file_term(context, current_file, *f_id, *t_id)
        }
        ExpressionKind::Variable { resolution, .. } => {
            let resolution = resolution.as_ref().ok_or(AnalyzerError::NonFatal)?;
            match resolution {
                TermVariableResolution::Binder(binder_id) => {
                    references_binder(context, current_file, *binder_id)
                }
                TermVariableResolution::Let(let_id) => {
                    references_let(context, current_file, *let_id)
                }
                TermVariableResolution::RecordPun(pun_id) => {
                    references_binder_pun(context, current_file, *pun_id)
                }
                TermVariableResolution::Reference(f_id, t_id) => {
                    references_file_term(context, current_file, *f_id, *t_id)
                }
            }
        }
        ExpressionKind::OperatorName { resolution, .. } => {
            let (f_id, t_id) = resolution.as_ref().ok_or(AnalyzerError::NonFatal)?;
            references_file_term(context, current_file, *f_id, *t_id)
        }
        _ => Ok(None),
    }
}

fn references_type(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    current_file: FileId,
    type_id: TypeId,
) -> Result<Option<Vec<Location>>, AnalyzerError> {
    let lowered = context.queries().lowered(current_file)?;
    let kind = lowered.tree.get_type_kind(type_id).ok_or(AnalyzerError::NonFatal)?;
    match kind {
        TypeKind::Constructor { resolution, .. } => {
            let (f_id, t_id) = resolution.as_ref().ok_or(AnalyzerError::NonFatal)?;
            references_file_type(context, current_file, *f_id, *t_id)
        }
        TypeKind::Operator { resolution, .. } => {
            let (f_id, t_id) = resolution.as_ref().ok_or(AnalyzerError::NonFatal)?;
            references_file_type(context, current_file, *f_id, *t_id)
        }
        TypeKind::Variable {
            resolution: Some(TypeVariableResolution::Forall(binding_id)), ..
        } => references_type_variable(context, current_file, *binding_id),
        _ => Ok(None),
    }
}

fn references_type_variable(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    current_file: FileId,
    binding_id: TypeVariableBindingId,
) -> Result<Option<Vec<Location>>, AnalyzerError> {
    let uri = common::file_uri(context, current_file)?;
    let content = context.queries().content(current_file)?;
    let positions = PositionConverter::new(&content, context.position_encoding());
    let (parsed, _) = context.queries().parsed(current_file)?;
    let stabilized = context.queries().stabilized(current_file)?;
    let lowered = context.queries().lowered(current_file)?;

    let mut locations = vec![];
    for (type_id, kind) in lowered.tree.iter_type() {
        let TypeKind::Variable {
            resolution: Some(TypeVariableResolution::Forall(candidate_id)),
            ..
        } = kind
        else {
            continue;
        };
        if *candidate_id != binding_id {
            continue;
        }

        let range =
            id_range(&positions, &parsed, &stabilized, type_id).ok_or(AnalyzerError::NonFatal)?;
        locations.push(Location { uri: uri.clone(), range });
    }

    Ok(Some(locations))
}

fn id_range<T>(
    positions: &PositionConverter<'_>,
    parsed: &ParsedModule,
    stabilized: &StabilizedModule,
    item_id: AstId<T>,
) -> Option<Range>
where
    T: AstNode,
{
    let root = parsed.syntax_node();
    let ptr = stabilized.syntax_ptr(item_id)?;
    let range = locate::syntax_range(positions, &root, &ptr)?;
    positions.utf8_range_to_protocol(range)
}

fn references_file_term(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    current_file: FileId,
    file_id: FileId,
    term_id: TermItemId,
) -> Result<Option<Vec<Location>>, AnalyzerError> {
    let engine = context.queries();
    let candidates = probe_term_references(context, current_file, file_id, term_id)?;

    let mut locations = vec![];
    for candidate_id in candidates {
        let uri = common::file_uri(context, candidate_id)?;

        let content = engine.content(candidate_id)?;
        let positions = PositionConverter::new(&content, context.position_encoding());
        let (parsed, _) = engine.parsed(candidate_id)?;
        let stabilized = engine.stabilized(candidate_id)?;
        let indexed = engine.indexed(candidate_id)?;
        let lowered = engine.lowered(candidate_id)?;

        for (expr_id, expr_kind) in lowered.tree.iter_expression() {
            if let ExpressionKind::Constructor { resolution: Some((f_id, t_id)) } = expr_kind
                && (*f_id, *t_id) == (file_id, term_id)
            {
                let range = id_range(&positions, &parsed, &stabilized, expr_id)
                    .ok_or(AnalyzerError::NonFatal)?;
                locations.push(Location { uri: uri.clone(), range });
            } else if let ExpressionKind::OperatorName { resolution: Some((f_id, t_id)) } =
                expr_kind
                && (*f_id, *t_id) == (file_id, term_id)
            {
                let range = id_range(&positions, &parsed, &stabilized, expr_id)
                    .ok_or(AnalyzerError::NonFatal)?;
                locations.push(Location { uri: uri.clone(), range });
            } else if let ExpressionKind::Variable { resolution: Some(resolution) } = expr_kind
                && let TermVariableResolution::Reference(f_id, t_id) = resolution
                && (*f_id, *t_id) == (file_id, term_id)
            {
                let range = id_range(&positions, &parsed, &stabilized, expr_id)
                    .ok_or(AnalyzerError::NonFatal)?;
                locations.push(Location { uri: uri.clone(), range });
            }
        }

        for (pun_id, resolution) in lowered.tree.iter_expression_pun() {
            if let TermVariableResolution::Reference(f_id, t_id) = resolution
                && (f_id, t_id) == (file_id, term_id)
            {
                let range = id_range(&positions, &parsed, &stabilized, pun_id)
                    .ok_or(AnalyzerError::NonFatal)?;
                locations.push(Location { uri: uri.clone(), range });
            }
        }

        for (binder_id, binder_kind) in lowered.tree.iter_binder() {
            if let BinderKind::Constructor { resolution: Some((f_id, t_id)), .. } = binder_kind
                && (*f_id, *t_id) == (file_id, term_id)
            {
                let range = id_range(&positions, &parsed, &stabilized, binder_id)
                    .ok_or(AnalyzerError::NonFatal)?;
                locations.push(Location { uri: uri.clone(), range });
            }
        }

        for (operator_id, f_id, t_id) in lowered.tree.iter_term_operator() {
            if (f_id, t_id) == (file_id, term_id) {
                let range = id_range(&positions, &parsed, &stabilized, operator_id)
                    .ok_or(AnalyzerError::NonFatal)?;
                locations.push(Location { uri: uri.clone(), range });
            }
        }

        let ranges = locate::term_infix_reference_ranges(
            &positions,
            &parsed,
            &stabilized,
            &indexed,
            &lowered,
            (file_id, term_id),
        );
        for range in ranges {
            let range = positions.utf8_range_to_protocol(range).ok_or(AnalyzerError::NonFatal)?;
            locations.push(Location { uri: uri.clone(), range });
        }
    }

    Ok(Some(locations))
}

fn references_file_type(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    current_file: FileId,
    file_id: FileId,
    type_id: TypeItemId,
) -> Result<Option<Vec<Location>>, AnalyzerError> {
    let engine = context.queries();
    let candidates = probe_type_references(context, current_file, file_id, type_id)?;

    let mut locations = vec![];
    for candidate_id in candidates {
        let uri = common::file_uri(context, candidate_id)?;

        let content = engine.content(candidate_id)?;
        let positions = PositionConverter::new(&content, context.position_encoding());
        let (parsed, _) = engine.parsed(candidate_id)?;

        let stabilized = engine.stabilized(candidate_id)?;
        let indexed = engine.indexed(candidate_id)?;
        let lowered = engine.lowered(candidate_id)?;

        for (ty_id, ty_kind) in lowered.tree.iter_type() {
            if let TypeKind::Constructor { resolution: Some((f_id, t_id)) } = ty_kind
                && (*f_id, *t_id) == (file_id, type_id)
            {
                let range = id_range(&positions, &parsed, &stabilized, ty_id)
                    .ok_or(AnalyzerError::NonFatal)?;
                locations.push(Location { uri: uri.clone(), range });
            }
            if let TypeKind::Operator { resolution: Some((f_id, t_id)) } = ty_kind
                && (*f_id, *t_id) == (file_id, type_id)
            {
                let range = id_range(&positions, &parsed, &stabilized, ty_id)
                    .ok_or(AnalyzerError::NonFatal)?;
                locations.push(Location { uri: uri.clone(), range });
            }
        }

        for (operator_id, f_id, t_id) in lowered.tree.iter_type_operator() {
            if (f_id, t_id) == (file_id, type_id) {
                let range = id_range(&positions, &parsed, &stabilized, operator_id)
                    .ok_or(AnalyzerError::NonFatal)?;
                locations.push(Location { uri: uri.clone(), range });
            }
        }

        let ranges = locate::type_infix_reference_ranges(
            &positions,
            &parsed,
            &stabilized,
            &indexed,
            &lowered,
            (file_id, type_id),
        );
        for range in ranges {
            let range = positions.utf8_range_to_protocol(range).ok_or(AnalyzerError::NonFatal)?;
            locations.push(Location { uri: uri.clone(), range });
        }

        let ranges = locate::instance_head_ranges(
            &positions,
            &parsed,
            &stabilized,
            &indexed,
            &lowered,
            (file_id, type_id),
        );
        for range in ranges {
            let range = positions.utf8_range_to_protocol(range).ok_or(AnalyzerError::NonFatal)?;
            locations.push(Location { uri: uri.clone(), range });
        }
    }

    Ok(Some(locations))
}

fn probe_term_references(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    current_file: FileId,
    file_id: FileId,
    term_id: TermItemId,
) -> Result<FxHashSet<FileId>, AnalyzerError> {
    probe_workspace_imports(context, current_file, file_id, |import| {
        import.iter_terms().any(|(_, f_id, t_id, kind)| {
            kind != ImportKind::Hidden && (f_id, t_id) == (file_id, term_id)
        })
    })
}

fn probe_type_references(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    current_file: FileId,
    file_id: FileId,
    type_id: TypeItemId,
) -> Result<FxHashSet<FileId>, AnalyzerError> {
    probe_workspace_imports(context, current_file, file_id, |import| {
        import.iter_types().any(|(_, f_id, t_id, kind)| {
            kind != ImportKind::Hidden && (f_id, t_id) == (file_id, type_id)
        }) || import.iter_classes().any(|(_, f_id, t_id, kind)| {
            kind != ImportKind::Hidden && (f_id, t_id) == (file_id, type_id)
        })
    })
}

fn probe_workspace_imports(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    current_file: FileId,
    source_file: FileId,
    check_import: impl Fn(&ResolvedImport) -> bool,
) -> Result<FxHashSet<FileId>, AnalyzerError> {
    let mut probe = FxHashSet::from_iter([current_file, source_file]);

    for workspace_file_id in context.active_files() {
        if workspace_file_id == current_file || workspace_file_id == source_file {
            continue;
        }

        let resolved = context.queries().resolved(workspace_file_id)?;

        let unqualified = resolved.unqualified.values().flatten();
        let qualified = resolved.qualified.values().flatten();
        let imports = unqualified.chain(qualified);

        for import in imports {
            if check_import(import) {
                probe.insert(workspace_file_id);
            }
        }
    }

    Ok(probe)
}

fn probe_imports_for(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    module_id: FileId,
) -> Result<FxHashSet<(FileId, ImportId)>, AnalyzerError> {
    let mut probe = FxHashSet::default();

    for workspace_file_id in context.active_files() {
        let resolved = context.queries().resolved(workspace_file_id)?;

        let unqualified = resolved.unqualified.values().flatten();
        let qualified = resolved.qualified.values().flatten();
        let imports = unqualified.chain(qualified);

        for import in imports {
            if import.file == module_id {
                probe.insert((workspace_file_id, import.id));
            }
        }
    }

    Ok(probe)
}

fn references_let(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    current_file: FileId,
    let_id: LetBindingNameGroupId,
) -> Result<Option<Vec<Location>>, AnalyzerError> {
    let engine = context.queries();
    let uri = common::file_uri(context, current_file)?;

    let content = engine.content(current_file)?;
    let positions = PositionConverter::new(&content, context.position_encoding());
    let (parsed, _) = engine.parsed(current_file)?;

    let stabilized = engine.stabilized(current_file)?;
    let lowered = engine.lowered(current_file)?;

    let mut locations = vec![];

    for (expression_id, expression_kind) in lowered.tree.iter_expression() {
        if let ExpressionKind::Variable {
            resolution: Some(TermVariableResolution::Let(candidate_id)),
            ..
        } = expression_kind
            && *candidate_id == let_id
        {
            let uri = Url::clone(&uri);
            let range = id_range(&positions, &parsed, &stabilized, expression_id)
                .ok_or(AnalyzerError::NonFatal)?;
            locations.push(Location { uri, range });
        }
    }

    for (expression_id, resolution) in lowered.tree.iter_expression_pun() {
        if let TermVariableResolution::Let(resolution_id) = resolution
            && resolution_id == let_id
        {
            let uri = Url::clone(&uri);
            let range = id_range(&positions, &parsed, &stabilized, expression_id)
                .ok_or(AnalyzerError::NonFatal)?;
            locations.push(Location { uri, range });
        }
    }

    Ok(Some(locations))
}

fn references_binder_pun(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    current_file: FileId,
    pun_id: RecordPunId,
) -> Result<Option<Vec<Location>>, AnalyzerError> {
    let engine = context.queries();
    let uri = common::file_uri(context, current_file)?;

    let content = engine.content(current_file)?;
    let positions = PositionConverter::new(&content, context.position_encoding());
    let (parsed, _) = engine.parsed(current_file)?;

    let stabilized = engine.stabilized(current_file)?;
    let lowered = engine.lowered(current_file)?;

    let mut locations = vec![];

    for (expression_id, expression_kind) in lowered.tree.iter_expression() {
        if let ExpressionKind::Variable {
            resolution: Some(TermVariableResolution::RecordPun(candidate_id)),
        } = expression_kind
            && *candidate_id == pun_id
        {
            let uri = Url::clone(&uri);
            let range = id_range(&positions, &parsed, &stabilized, expression_id)
                .ok_or(AnalyzerError::NonFatal)?;
            locations.push(Location { uri, range });
        }
    }

    for (expression_id, resolution) in lowered.tree.iter_expression_pun() {
        if let TermVariableResolution::RecordPun(resolution_id) = resolution
            && resolution_id == pun_id
        {
            let uri = Url::clone(&uri);
            let range = id_range(&positions, &parsed, &stabilized, expression_id)
                .ok_or(AnalyzerError::NonFatal)?;
            locations.push(Location { uri, range });
        }
    }

    Ok(Some(locations))
}

fn references_expression_pun(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    current_file: FileId,
    pun_id: RecordPunId,
) -> Result<Option<Vec<Location>>, AnalyzerError> {
    let lowered = context.queries().lowered(current_file)?;
    match lowered.tree.get_expression_pun(pun_id).ok_or(AnalyzerError::NonFatal)? {
        TermVariableResolution::Binder(binder_id) => {
            references_binder(context, current_file, binder_id)
        }
        TermVariableResolution::Let(let_id) => references_let(context, current_file, let_id),
        TermVariableResolution::RecordPun(pun_id) => {
            references_binder_pun(context, current_file, pun_id)
        }
        TermVariableResolution::Reference(file_id, term_id) => {
            references_file_term(context, current_file, file_id, term_id)
        }
    }
}
