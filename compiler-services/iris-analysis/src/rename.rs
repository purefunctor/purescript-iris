use building_types::QueryProxy;
use files::FileId;
use indexing::{
    DeriveItemId, ImportId, ImportItemId, ImportKind, IndexedTermItemKind, IndexedTypeItemKind,
    InstanceItemId, TermItemId, TypeItemId,
};
use lowering::{
    BinderId, BinderKind, ExpressionKind, LetBindingNameGroupId, RecordPunId,
    TermVariableResolution, TypeKind, TypeVariableBindingId, TypeVariableResolution,
};
use lsp_types::*;
use syntax::ast::{AstNode, AstPtr};
use syntax::{SyntaxToken, TextRange, TextSize, TokenAtOffset, cst};

use crate::{AnalyzerContext, AnalyzerError, locate, position, references};

mod edit;

#[derive(Clone, Copy)]
enum RenameTarget {
    Term(FileId, TermItemId),
    Type(FileId, TypeItemId),
    Instance(FileId, InstanceItemId),
    Derive(FileId, DeriveItemId),
    Binder(FileId, BinderId),
    TypeVariable(FileId, TypeVariableBindingId),
    LetBinding(FileId, LetBindingNameGroupId),
    RecordPun(FileId, RecordPunId),
    Qualifier(FileId, ImportId),
    Module(FileId),
}

impl RenameTarget {
    fn file(self) -> FileId {
        match self {
            RenameTarget::Term(file_id, _)
            | RenameTarget::Type(file_id, _)
            | RenameTarget::Instance(file_id, _)
            | RenameTarget::Derive(file_id, _)
            | RenameTarget::Binder(file_id, _)
            | RenameTarget::TypeVariable(file_id, _)
            | RenameTarget::LetBinding(file_id, _)
            | RenameTarget::RecordPun(file_id, _)
            | RenameTarget::Qualifier(file_id, _)
            | RenameTarget::Module(file_id) => file_id,
        }
    }
}

#[derive(Clone, Copy)]
enum NameKind {
    Lower,
    Upper,
    Operator,
    Module,
}

pub fn implementation(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    uri: Url,
    position: Position,
    new_name: String,
) -> Result<Option<WorkspaceEdit>, AnalyzerError> {
    let (_, _, target) = target_at_position(context, &uri, position)?;

    let Some((old_name, name_kind)) = target_name(context, target)? else {
        return Ok(None);
    };
    if old_name == new_name {
        return Ok(None);
    }

    if !context.is_editable(target.file()) {
        return Ok(None);
    }

    if !valid_new_name(&new_name, name_kind) {
        let message =
            format!("Cannot rename `{old_name}` to `{new_name}` because the new name is invalid");
        return Err(AnalyzerError::RenameRejected(message));
    }

    let conflicts = rename_conflicts(context, target, &new_name)?;
    if conflicts && !context.capabilities().has_change_annotations() {
        let message = format!(
            "Cannot rename `{old_name}` to `{new_name}` because it would change name resolution"
        );
        return Err(AnalyzerError::RenameRejected(message));
    }

    let mut edits = edit::RenameEdits::new(context);
    match target {
        RenameTarget::Qualifier(file_id, import_id) => {
            edits.collect_qualifier(file_id, import_id, &new_name)?;
        }
        RenameTarget::Module(file_id) => {
            edits.collect_module(file_id, &new_name)?;
        }
        RenameTarget::Term(_, _) | RenameTarget::Type(_, _) => {
            let locations = references::implementation(context, uri, position)?.unwrap_or_default();
            edits.collect_references(locations, name_kind, &new_name)?;
            edits.collect_declaration(target, &new_name)?;
            edits.collect_item_surfaces(target, &old_name, &new_name)?;
        }
        RenameTarget::Instance(_, _) | RenameTarget::Derive(_, _) => {
            edits.collect_declaration(target, &new_name)?;
        }
        RenameTarget::Binder(_, _)
        | RenameTarget::TypeVariable(_, _)
        | RenameTarget::LetBinding(_, _)
        | RenameTarget::RecordPun(_, _) => {
            let locations = references::implementation(context, uri, position)?.unwrap_or_default();
            edits.collect_references(locations, name_kind, &new_name)?;
            edits.collect_declaration(target, &new_name)?;
        }
    }

    edits.finish(conflicts)
}

fn rename_conflicts(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    target: RenameTarget,
    new_name: &str,
) -> Result<bool, AnalyzerError> {
    match target {
        RenameTarget::Term(file_id, term_id) => {
            term_item_conflicts(context, file_id, term_id, new_name)
        }
        RenameTarget::Type(file_id, type_id) => {
            type_item_conflicts(context, file_id, type_id, new_name)
        }
        RenameTarget::Binder(file_id, binder_id) => {
            let target = TermVariableResolution::Binder(binder_id);
            local_rename_conflicts(context, file_id, target, new_name)
        }
        RenameTarget::TypeVariable(file_id, binding_id) => {
            type_variable_conflicts(context, file_id, binding_id, new_name)
        }
        RenameTarget::LetBinding(file_id, binding_id) => {
            let target = TermVariableResolution::Let(binding_id);
            local_rename_conflicts(context, file_id, target, new_name)
        }
        RenameTarget::RecordPun(file_id, pun_id) => {
            let target = TermVariableResolution::RecordPun(pun_id);
            local_rename_conflicts(context, file_id, target, new_name)
        }
        RenameTarget::Instance(_, _)
        | RenameTarget::Derive(_, _)
        | RenameTarget::Qualifier(_, _)
        | RenameTarget::Module(_) => Ok(false),
    }
}

fn term_item_conflicts(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    target_file: FileId,
    target_term: TermItemId,
    new_name: &str,
) -> Result<bool, AnalyzerError> {
    if term_item_conflicts_in_file(context, target_file, target_file, target_term, new_name)? {
        return Ok(true);
    }

    for file_id in context.active_files() {
        if file_id != target_file
            && term_item_conflicts_in_file(context, file_id, target_file, target_term, new_name)?
        {
            return Ok(true);
        }
    }

    Ok(false)
}

fn term_item_conflicts_in_file(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    file_id: FileId,
    target_file: FileId,
    target_term: TermItemId,
    new_name: &str,
) -> Result<bool, AnalyzerError> {
    let prim_id = context.queries().prim_id();
    let prim = context.queries().resolved(prim_id)?;
    let resolved = context.queries().resolved(file_id)?;

    let target_unqualified = if file_id == target_file {
        true
    } else {
        let mut imports = resolved.unqualified.values().flatten();
        imports.any(|import| import.contains_term(target_file, target_term))
    };
    if target_unqualified {
        let candidate = resolved.lookup_term(&prim, None, new_name);
        if candidate.is_some_and(|candidate| candidate != (target_file, target_term)) {
            return Ok(true);
        }
    }

    let qualified_targets = resolved.qualified.iter().filter_map(|(qualifier, imports)| {
        imports
            .iter()
            .any(|import| import.contains_term(target_file, target_term))
            .then_some(qualifier.as_str())
    });
    let qualified_targets = qualified_targets.collect::<Vec<_>>();
    for qualifier in &qualified_targets {
        let candidate = resolved.lookup_term(&prim, Some(qualifier), new_name);
        if candidate.is_some_and(|candidate| candidate != (target_file, target_term)) {
            return Ok(true);
        }
    }

    if (target_unqualified || !qualified_targets.is_empty())
        && implicit_term_reference_conflicts(context, file_id, target_file, target_term)?
    {
        return Ok(true);
    }

    if target_unqualified {
        term_reference_conflicts(context, file_id, target_file, target_term, new_name)
    } else {
        Ok(false)
    }
}

fn implicit_term_reference_conflicts(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    file_id: FileId,
    target_file: FileId,
    target_term: TermItemId,
) -> Result<bool, AnalyzerError> {
    let lowered = context.queries().lowered(file_id)?;
    let target = TermVariableResolution::Reference(target_file, target_term);
    let mut expressions = lowered.tree.iter_expression();
    Ok(expressions.any(|(_, kind)| expression_has_implicit_resolution(kind, target)))
}

fn term_reference_conflicts(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    file_id: FileId,
    target_file: FileId,
    target_term: TermItemId,
    new_name: &str,
) -> Result<bool, AnalyzerError> {
    let lowered = context.queries().lowered(file_id)?;
    let target = TermVariableResolution::Reference(target_file, target_term);

    for (expression_id, kind) in lowered.tree.iter_expression() {
        let ExpressionKind::Variable { resolution: Some(current) } = kind else { continue };
        if *current != target || !expression_is_unqualified(context, file_id, expression_id)? {
            continue;
        }
        let Some(node) = lowered.nodes.expression_node(expression_id) else { continue };
        if lowered.graph.resolve_term(node, new_name).is_some() {
            return Ok(true);
        }
    }

    for (pun_id, current) in lowered.tree.iter_expression_pun() {
        if current != target {
            continue;
        }
        let Some(node) = lowered.nodes.record_pun_node(pun_id) else { continue };
        if lowered.graph.resolve_term(node, new_name).is_some() {
            return Ok(true);
        }
    }

    Ok(false)
}

fn expression_is_unqualified(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    file_id: FileId,
    expression_id: lowering::ExpressionId,
) -> Result<bool, AnalyzerError> {
    let (parsed, _) = context.queries().parsed(file_id)?;
    let stabilized = context.queries().stabilized(file_id)?;
    let root = parsed.syntax_node();
    let pointer = stabilized.syntax_ptr(expression_id).ok_or(AnalyzerError::NonFatal)?;
    let node = pointer.try_to_node(&root).ok_or(AnalyzerError::NonFatal)?;
    let expression = cst::Expression::cast(node).ok_or(AnalyzerError::NonFatal)?;

    let cst::Expression::ExpressionVariable(variable) = expression else { return Ok(false) };
    Ok(variable.name().is_some_and(|name| name.qualifier().is_none()))
}

fn type_item_conflicts(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    target_file: FileId,
    target_type: TypeItemId,
    new_name: &str,
) -> Result<bool, AnalyzerError> {
    if type_item_conflicts_in_file(context, target_file, target_file, target_type, new_name)? {
        return Ok(true);
    }

    for file_id in context.active_files() {
        if file_id != target_file
            && type_item_conflicts_in_file(context, file_id, target_file, target_type, new_name)?
        {
            return Ok(true);
        }
    }

    Ok(false)
}

fn type_item_conflicts_in_file(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    file_id: FileId,
    target_file: FileId,
    target_type: TypeItemId,
    new_name: &str,
) -> Result<bool, AnalyzerError> {
    let prim_id = context.queries().prim_id();
    let prim = context.queries().resolved(prim_id)?;
    let resolved = context.queries().resolved(file_id)?;

    let import_contains_target = |import: &resolving::ResolvedImport| {
        let types = import.iter_types();
        let classes = import.iter_classes();
        types.chain(classes).any(|(_, imported_file, imported_type, kind)| {
            kind != ImportKind::Hidden
                && (imported_file, imported_type) == (target_file, target_type)
        })
    };
    let target_unqualified = if file_id == target_file {
        true
    } else {
        let mut imports = resolved.unqualified.values().flatten();
        imports.any(&import_contains_target)
    };
    if target_unqualified {
        let candidate = resolved
            .lookup_type(&prim, None, new_name)
            .or_else(|| resolved.lookup_class(&prim, None, new_name));
        if candidate.is_some_and(|candidate| candidate != (target_file, target_type)) {
            return Ok(true);
        }
    }

    for (qualifier, imports) in &resolved.qualified {
        if imports.iter().any(&import_contains_target) {
            let candidate = resolved
                .lookup_type(&prim, Some(qualifier), new_name)
                .or_else(|| resolved.lookup_class(&prim, Some(qualifier), new_name));
            if candidate.is_some_and(|candidate| candidate != (target_file, target_type)) {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

fn local_rename_conflicts(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    file_id: FileId,
    target: TermVariableResolution,
    new_name: &str,
) -> Result<bool, AnalyzerError> {
    let lowered = context.queries().lowered(file_id)?;
    let Some(target_node) = lowered.nodes.term_node(target) else { return Ok(false) };

    if lowered.graph.resolve_term(target_node, new_name).is_some() {
        return Ok(true);
    }

    let prim_id = context.queries().prim_id();
    let prim = context.queries().resolved(prim_id)?;
    let resolved = context.queries().resolved(file_id)?;
    if resolved.lookup_term(&prim, None, new_name).is_some() {
        return Ok(true);
    }

    for (expression_id, kind) in lowered.tree.iter_expression() {
        if expression_has_implicit_resolution(kind, target) {
            return Ok(true);
        }
        let ExpressionKind::Variable { resolution: Some(current) } = kind else { continue };
        let Some(node) = lowered.nodes.expression_node(expression_id) else { continue };
        if resolution_changes(&lowered, node, target_node, target, *current, new_name) {
            return Ok(true);
        }
    }

    for (pun_id, current) in lowered.tree.iter_expression_pun() {
        let Some(node) = lowered.nodes.record_pun_node(pun_id) else { continue };
        if resolution_changes(&lowered, node, target_node, target, current, new_name) {
            return Ok(true);
        }
    }

    Ok(false)
}

fn type_variable_conflicts(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    file_id: FileId,
    binding_id: TypeVariableBindingId,
    new_name: &str,
) -> Result<bool, AnalyzerError> {
    let lowered = context.queries().lowered(file_id)?;
    let target = TypeVariableResolution::Forall(binding_id);
    let target_node =
        lowered.nodes.type_variable_binding_node(binding_id).ok_or(AnalyzerError::NonFatal)?;

    if lowered.graph.resolve_type(target_node, new_name).is_some() {
        return Ok(true);
    }

    for (type_id, kind) in lowered.tree.iter_type() {
        let TypeKind::Variable { resolution: Some(current), .. } = kind else {
            continue;
        };
        if *current != target {
            continue;
        }

        let Some(node) = lowered.nodes.type_node(type_id) else {
            continue;
        };
        if lowered.graph.resolve_type(node, new_name).is_some() {
            return Ok(true);
        }
    }

    Ok(false)
}

fn expression_has_implicit_resolution(
    kind: &ExpressionKind,
    target: TermVariableResolution,
) -> bool {
    match kind {
        ExpressionKind::Negate { negate, .. } => *negate == Some(target),
        ExpressionKind::Do { bind, discard, .. } => {
            *bind == Some(target) || *discard == Some(target)
        }
        ExpressionKind::Ado { map, apply, pure, .. } => {
            *map == Some(target) || *apply == Some(target) || *pure == Some(target)
        }
        _ => false,
    }
}

fn resolution_changes(
    lowered: &lowering::LoweredModule,
    node: lowering::GraphNodeId,
    target_node: lowering::GraphNodeId,
    target: TermVariableResolution,
    current: TermVariableResolution,
    new_name: &str,
) -> bool {
    let Some(candidate) = lowered.graph.resolve_term(node, new_name) else { return false };
    let Some(candidate_node) = lowered.nodes.term_node(candidate) else { return false };

    let mut scopes = lowered.graph.traverse(node).map(|(node, _)| node);
    let target_position = scopes.position(|node| node == target_node);
    let mut scopes = lowered.graph.traverse(node).map(|(node, _)| node);
    let candidate_position = scopes.position(|node| node == candidate_node);
    let Some((target_position, candidate_position)) = target_position.zip(candidate_position)
    else {
        return false;
    };

    if current == target {
        candidate_position <= target_position
    } else if current == candidate {
        target_position <= candidate_position
    } else {
        false
    }
}

pub fn prepare(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    uri: Url,
    position: Position,
) -> Result<Option<PrepareRenameResponse>, AnalyzerError> {
    let (current_file, utf8_position, target) = target_at_position(context, &uri, position)?;
    let Some((old_name, _)) = target_name(context, target)? else {
        return Ok(None);
    };
    if !context.is_editable(target.file()) {
        return Ok(None);
    }

    let range = rename_range(context, current_file, utf8_position, &old_name)?;

    Ok(Some(PrepareRenameResponse::RangeWithPlaceholder { range, placeholder: old_name }))
}

fn target_at_position(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    uri: &Url,
    position: Position,
) -> Result<(FileId, position::Utf8Position, RenameTarget), AnalyzerError> {
    let current_file = {
        let uri = uri.as_str();
        context.file_id(uri).ok_or(AnalyzerError::NonFatal)?
    };

    let content = context.queries().content(current_file)?;
    let positions = position::PositionConverter::new(&content, context.position_encoding());
    let utf8_position =
        positions.protocol_position_to_utf8(position).ok_or(AnalyzerError::NonFatal)?;

    let target = if let Some(target) =
        qualifier_target(context, current_file, &positions, utf8_position)?
    {
        target
    } else {
        let located = locate::locate(context.queries(), current_file, &positions, utf8_position)?;
        rename_target(context, current_file, located)?
    };

    Ok((current_file, utf8_position, target))
}

fn rename_range(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    current_file: FileId,
    position: position::Utf8Position,
    old_name: &str,
) -> Result<Range, AnalyzerError> {
    let content = context.queries().content(current_file)?;
    let positions = position::PositionConverter::new(&content, context.position_encoding());
    let offset = positions.utf8_position_to_offset(position).ok_or(AnalyzerError::NonFatal)?;
    let (parsed, _) = context.queries().parsed(current_file)?;
    let root = parsed.syntax_node();

    let name_range = |token: SyntaxToken| -> Option<TextRange> {
        let token_text = token.text(&content);
        if token_text == old_name {
            return Some(token.text_range());
        }
        if token_text.strip_suffix('.') == Some(old_name) {
            let range = token.text_range();
            let end = range.end() - TextSize::from(1);
            return Some(TextRange::new(range.start(), end));
        }

        token
            .parent_ancestors()
            .find_map(|node| (node.text(&content) == old_name).then(|| node.text_range()))
    };

    let range = match root.token_at_offset(offset) {
        TokenAtOffset::None => None,
        TokenAtOffset::Single(token) => name_range(token),
        TokenAtOffset::Between(left, right) => name_range(right).or_else(|| name_range(left)),
    };

    let range = range.ok_or(AnalyzerError::NonFatal)?;
    positions.text_range_to_protocol(range).ok_or(AnalyzerError::NonFatal)
}

fn qualifier_target(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    current_file: FileId,
    positions: &position::PositionConverter<'_>,
    position: position::Utf8Position,
) -> Result<Option<RenameTarget>, AnalyzerError> {
    let offset = positions.utf8_position_to_offset(position).ok_or(AnalyzerError::NonFatal)?;
    let content = positions.content();
    let (parsed, _) = context.queries().parsed(current_file)?;
    let root = parsed.syntax_node();

    let token = match root.token_at_offset(offset) {
        TokenAtOffset::None => return Ok(None),
        TokenAtOffset::Single(token) => token,
        TokenAtOffset::Between(_, right) => right,
    };

    let qualifier_name = token.parent_ancestors().find_map(|node| {
        let qualifier = cst::Qualifier::cast(node)?;
        let parent = qualifier.syntax().parent()?;
        if !cst::QualifiedName::can_cast(parent.kind()) {
            return None;
        }

        let text = qualifier.text()?.text(content);
        Some(text.trim_end_matches('.').to_string())
    });

    let alias_name = token.parent_ancestors().find_map(|node| {
        let module_name = cst::ModuleName::cast(node)?;
        let parent = module_name.syntax().parent()?;
        let parent_kind = parent.kind();
        let is_alias = cst::ImportAlias::can_cast(parent_kind);
        let is_export = cst::ExportModule::can_cast(parent_kind);

        if !is_alias && !is_export {
            return None;
        }

        Some(module_name.syntax().text(content).to_string())
    });

    let Some(name) = qualifier_name.or(alias_name) else {
        return Ok(None);
    };

    let indexed = context.queries().indexed(current_file)?;
    let import_id = indexed.imports.iter().find_map(|(import_id, import)| {
        (import.alias.as_deref() == Some(name.as_str())).then_some(*import_id)
    });

    Ok(import_id.map(|import_id| RenameTarget::Qualifier(current_file, import_id)))
}

fn rename_target(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    current_file: FileId,
    located: locate::Located,
) -> Result<RenameTarget, AnalyzerError> {
    let lowered = context.queries().lowered(current_file)?;

    let target = match located {
        locate::Located::ModuleName(module_name) => {
            module_target(context, current_file, module_name)?
        }
        locate::Located::ImportItem(import_id) => import_target(context, current_file, import_id)?,
        locate::Located::Binder(binder_id) => {
            let kind = lowered.tree.get_binder_kind(binder_id).ok_or(AnalyzerError::NonFatal)?;

            match kind {
                BinderKind::Constructor { resolution: Some((file_id, term_id)), .. } => {
                    RenameTarget::Term(*file_id, *term_id)
                }
                BinderKind::Variable { variable: Some(_) }
                | BinderKind::Named { named: Some(_), .. } => {
                    RenameTarget::Binder(current_file, binder_id)
                }
                _ => return Err(AnalyzerError::NonFatal),
            }
        }
        locate::Located::Expression(expression_id) => {
            let kind =
                lowered.tree.get_expression_kind(expression_id).ok_or(AnalyzerError::NonFatal)?;

            match kind {
                ExpressionKind::Constructor { resolution: Some(resolution) }
                | ExpressionKind::OperatorName { resolution: Some(resolution) } => {
                    RenameTarget::Term(resolution.0, resolution.1)
                }
                ExpressionKind::Variable { resolution: Some(resolution) } => {
                    target_from_term_resolution(current_file, *resolution)
                }
                _ => return Err(AnalyzerError::NonFatal),
            }
        }
        locate::Located::Type(type_id) => {
            let kind = lowered.tree.get_type_kind(type_id).ok_or(AnalyzerError::NonFatal)?;

            let resolution = match kind {
                TypeKind::Constructor { resolution: Some(resolution) }
                | TypeKind::Operator { resolution: Some(resolution) } => *resolution,
                TypeKind::Variable {
                    resolution: Some(TypeVariableResolution::Forall(binding_id)),
                    ..
                } => return Ok(RenameTarget::TypeVariable(current_file, *binding_id)),
                _ => return Err(AnalyzerError::NonFatal),
            };

            RenameTarget::Type(resolution.0, resolution.1)
        }
        locate::Located::TypeVariableBinding(binding_id) => {
            RenameTarget::TypeVariable(current_file, binding_id)
        }
        locate::Located::TermOperator(operator_id) => {
            let (file_id, term_id) =
                lowered.tree.get_term_operator(operator_id).ok_or(AnalyzerError::NonFatal)?;

            RenameTarget::Term(file_id, term_id)
        }
        locate::Located::TypeOperator(operator_id) => {
            let (file_id, type_id) =
                lowered.tree.get_type_operator(operator_id).ok_or(AnalyzerError::NonFatal)?;

            RenameTarget::Type(file_id, type_id)
        }
        locate::Located::TermReference(file_id, term_id) => RenameTarget::Term(file_id, term_id),
        locate::Located::TypeReference(file_id, type_id) => RenameTarget::Type(file_id, type_id),
        locate::Located::InstanceHead(file_id, type_id) => RenameTarget::Type(file_id, type_id),
        locate::Located::TermItem(term_id) => RenameTarget::Term(current_file, term_id),
        locate::Located::TypeItem(type_id) => RenameTarget::Type(current_file, type_id),
        locate::Located::InstanceItem(item_id) => RenameTarget::Instance(current_file, item_id),
        locate::Located::DeriveItem(item_id) => RenameTarget::Derive(current_file, item_id),
        locate::Located::LetBinding(binding_id) => {
            RenameTarget::LetBinding(current_file, binding_id)
        }
        locate::Located::BinderPun(pun_id) => RenameTarget::RecordPun(current_file, pun_id),
        locate::Located::ExpressionPun(pun_id) => {
            let resolution =
                lowered.tree.get_expression_pun(pun_id).ok_or(AnalyzerError::NonFatal)?;
            target_from_term_resolution(current_file, resolution)
        }
        _ => return Err(AnalyzerError::NonFatal),
    };

    Ok(target)
}

fn target_from_term_resolution(
    current_file: FileId,
    resolution: TermVariableResolution,
) -> RenameTarget {
    match resolution {
        TermVariableResolution::Binder(binder_id) => RenameTarget::Binder(current_file, binder_id),
        TermVariableResolution::Let(binding_id) => {
            RenameTarget::LetBinding(current_file, binding_id)
        }
        TermVariableResolution::RecordPun(pun_id) => RenameTarget::RecordPun(current_file, pun_id),
        TermVariableResolution::Reference(file_id, term_id) => RenameTarget::Term(file_id, term_id),
    }
}

fn module_target(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    current_file: FileId,
    module_name: AstPtr<cst::ModuleName>,
) -> Result<RenameTarget, AnalyzerError> {
    let content = context.queries().content(current_file)?;
    let (parsed, _) = context.queries().parsed(current_file)?;
    let root = parsed.syntax_node();
    let module_name = module_name.try_to_node(&root).ok_or(AnalyzerError::NonFatal)?;
    let parent = module_name.syntax().parent().ok_or(AnalyzerError::NonFatal)?;
    let parent_kind = parent.kind();

    if cst::ModuleHeader::can_cast(parent_kind) {
        return Ok(RenameTarget::Module(current_file));
    }

    if !cst::ImportStatement::can_cast(parent_kind) && !cst::ExportModule::can_cast(parent_kind) {
        return Err(AnalyzerError::NonFatal);
    }

    let name = module_name.syntax().text(&content);
    let file_id = context.queries().module_file(name).ok_or(AnalyzerError::NonFatal)?;

    Ok(RenameTarget::Module(file_id))
}

fn import_target(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    current_file: FileId,
    import_id: ImportItemId,
) -> Result<RenameTarget, AnalyzerError> {
    let content = context.queries().content(current_file)?;
    let (parsed, _) = context.queries().parsed(current_file)?;
    let stabilized = context.queries().stabilized(current_file)?;

    let root = parsed.syntax_node();
    let ptr = stabilized.ast_ptr(import_id).ok_or(AnalyzerError::NonFatal)?;
    let node = ptr.try_to_node(&root).ok_or(AnalyzerError::NonFatal)?;

    let statement = node
        .syntax()
        .ancestors()
        .find_map(cst::ImportStatement::cast)
        .ok_or(AnalyzerError::NonFatal)?;
    let module_name =
        statement.module_name().ok_or(AnalyzerError::NonFatal)?.syntax().text(&content).to_string();

    let imported_file =
        context.queries().module_file(&module_name).ok_or(AnalyzerError::NonFatal)?;
    let resolved = context.queries().resolved(imported_file)?;

    let target = match node {
        cst::ImportItem::ImportValue(item) => {
            let name = item.name_token().ok_or(AnalyzerError::NonFatal)?.text(&content);

            let (file_id, term_id) =
                resolved.exports.lookup_term(name).ok_or(AnalyzerError::NonFatal)?;

            RenameTarget::Term(file_id, term_id)
        }
        cst::ImportItem::ImportClass(item) => {
            let name = item.name_token().ok_or(AnalyzerError::NonFatal)?.text(&content);

            let (file_id, type_id) = resolved
                .exports
                .lookup_class(name)
                .or_else(|| resolved.exports.lookup_type(name))
                .ok_or(AnalyzerError::NonFatal)?;

            RenameTarget::Type(file_id, type_id)
        }
        cst::ImportItem::ImportType(item) => {
            let name = item.name_token().ok_or(AnalyzerError::NonFatal)?.text(&content);

            let (file_id, type_id) = resolved
                .exports
                .lookup_type(name)
                .or_else(|| resolved.exports.lookup_class(name))
                .ok_or(AnalyzerError::NonFatal)?;

            RenameTarget::Type(file_id, type_id)
        }
        cst::ImportItem::ImportOperator(item) => {
            let name = item.name_token().ok_or(AnalyzerError::NonFatal)?.text(&content);

            let (file_id, term_id) = resolved
                .exports
                .lookup_term(trim_operator_name(name))
                .ok_or(AnalyzerError::NonFatal)?;

            RenameTarget::Term(file_id, term_id)
        }
        cst::ImportItem::ImportTypeOperator(item) => {
            let name = item.name_token().ok_or(AnalyzerError::NonFatal)?.text(&content);

            let (file_id, type_id) = resolved
                .exports
                .lookup_type(trim_operator_name(name))
                .ok_or(AnalyzerError::NonFatal)?;

            RenameTarget::Type(file_id, type_id)
        }
    };

    Ok(target)
}

fn trim_operator_name(name: &str) -> &str {
    name.trim_start_matches('(').trim_end_matches(')')
}

fn target_name(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    target: RenameTarget,
) -> Result<Option<(String, NameKind)>, AnalyzerError> {
    let result = match target {
        RenameTarget::Term(file_id, term_id) => {
            let indexed = context.queries().indexed(file_id)?;
            let item = &indexed.items[term_id];

            let Some(name) = item.name.as_ref() else {
                return Ok(None);
            };

            let kind = match item.kind {
                IndexedTermItemKind::Constructor { .. } => NameKind::Upper,
                IndexedTermItemKind::Operator { .. } => NameKind::Operator,
                _ => NameKind::Lower,
            };

            Some((name.to_string(), kind))
        }
        RenameTarget::Type(file_id, type_id) => {
            let indexed = context.queries().indexed(file_id)?;
            let item = &indexed.items[type_id];

            let Some(name) = item.name.as_ref() else {
                return Ok(None);
            };

            let kind = match item.kind {
                IndexedTypeItemKind::Operator { .. } => NameKind::Operator,
                _ => NameKind::Upper,
            };

            Some((name.to_string(), kind))
        }
        RenameTarget::Instance(file_id, item_id) => {
            let indexed = context.queries().indexed(file_id)?;
            indexed.items[item_id].name.as_ref().map(|name| (name.to_string(), NameKind::Lower))
        }
        RenameTarget::Derive(file_id, item_id) => {
            let indexed = context.queries().indexed(file_id)?;
            indexed.items[item_id].name.as_ref().map(|name| (name.to_string(), NameKind::Lower))
        }
        RenameTarget::Binder(file_id, binder_id) => {
            let lowered = context.queries().lowered(file_id)?;
            let kind = lowered.tree.get_binder_kind(binder_id).ok_or(AnalyzerError::NonFatal)?;

            let name = match kind {
                BinderKind::Variable { variable } => variable.as_ref(),
                BinderKind::Named { named, .. } => named.as_ref(),
                _ => None,
            };

            name.map(|name| (name.to_string(), NameKind::Lower))
        }
        RenameTarget::TypeVariable(file_id, binding_id) => {
            let content = context.queries().content(file_id)?;
            let (parsed, _) = context.queries().parsed(file_id)?;
            let stabilized = context.queries().stabilized(file_id)?;

            let root = parsed.syntax_node();
            let ptr = stabilized.ast_ptr(binding_id).ok_or(AnalyzerError::NonFatal)?;
            let binding = ptr.try_to_node(&root).ok_or(AnalyzerError::NonFatal)?;
            let name = binding.name().ok_or(AnalyzerError::NonFatal)?;
            Some((name.text(&content).to_string(), NameKind::Lower))
        }
        RenameTarget::LetBinding(file_id, binding_id) => {
            let lowered = context.queries().lowered(file_id)?;
            let binding = lowered.tree.get_let_binding_group(binding_id);
            binding.name.as_ref().map(|name| (name.to_string(), NameKind::Lower))
        }
        RenameTarget::RecordPun(file_id, pun_id) => {
            let content = context.queries().content(file_id)?;
            let (parsed, _) = context.queries().parsed(file_id)?;
            let stabilized = context.queries().stabilized(file_id)?;

            let root = parsed.syntax_node();
            let ptr = stabilized.syntax_ptr(pun_id).ok_or(AnalyzerError::NonFatal)?;
            let node = ptr.try_to_node(&root).ok_or(AnalyzerError::NonFatal)?;
            let pun = cst::RecordPun::cast(node).ok_or(AnalyzerError::NonFatal)?;
            let name = pun.name().ok_or(AnalyzerError::NonFatal)?;

            let name = name.syntax().text(&content);
            Some((name.trim().to_string(), NameKind::Lower))
        }
        RenameTarget::Qualifier(file_id, import_id) => {
            let indexed = context.queries().indexed(file_id)?;
            let name = indexed.imports.get(&import_id).and_then(|import| import.alias.as_ref());

            name.map(|name| (name.to_string(), NameKind::Upper))
        }
        RenameTarget::Module(file_id) => {
            let content = context.queries().content(file_id)?;
            let (parsed, _) = context.queries().parsed(file_id)?;

            parsed.module_name(&content).map(|name| (name.to_string(), NameKind::Module))
        }
    };

    Ok(result)
}

fn valid_new_name(new_name: &str, kind: NameKind) -> bool {
    match kind {
        NameKind::Lower => lexing::is_lower_name(new_name),
        NameKind::Upper => lexing::is_upper_name(new_name),
        NameKind::Operator => lexing::is_operator_name(new_name),
        NameKind::Module => new_name.split('.').all(lexing::is_upper_name),
    }
}

#[cfg(test)]
mod tests {
    use super::{NameKind, valid_new_name};

    #[test]
    fn validates_new_names_by_kind() {
        assert!(valid_new_name("renamed", NameKind::Lower));
        assert!(valid_new_name("éclair", NameKind::Lower));
        assert!(valid_new_name("Renamed", NameKind::Upper));
        assert!(valid_new_name("Éclair", NameKind::Upper));
        assert!(valid_new_name("Library.Renamed", NameKind::Module));
        assert!(!valid_new_name("Renamed", NameKind::Lower));
        assert!(!valid_new_name("renamed", NameKind::Upper));
        assert!(!valid_new_name("module", NameKind::Lower));
        assert!(!valid_new_name("_", NameKind::Lower));
        assert!(!valid_new_name("", NameKind::Module));
        assert!(!valid_new_name("Library.renamed", NameKind::Module));
        assert!(!valid_new_name("Library..Renamed", NameKind::Module));
        assert!(!valid_new_name("Library Renamed", NameKind::Module));
        assert!(!valid_new_name("two names", NameKind::Lower));
        assert!(!valid_new_name("Library.renamed", NameKind::Lower));
        assert!(!valid_new_name("renamed ", NameKind::Lower));
    }

    #[test]
    fn validates_operator_names() {
        assert!(valid_new_name("<~>", NameKind::Operator));
        assert!(valid_new_name(":", NameKind::Operator));
        assert!(valid_new_name("-", NameKind::Operator));
        assert!(valid_new_name("..", NameKind::Operator));
        assert!(valid_new_name("<=", NameKind::Operator));
        assert!(valid_new_name("⊗", NameKind::Operator));
        assert!(!valid_new_name("renamed", NameKind::Operator));
        assert!(!valid_new_name("(<~>)", NameKind::Operator));
        assert!(!valid_new_name("=", NameKind::Operator));
        assert!(!valid_new_name("->", NameKind::Operator));
        assert!(!valid_new_name("--", NameKind::Operator));
    }
}
