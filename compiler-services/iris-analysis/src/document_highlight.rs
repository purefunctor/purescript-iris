use std::iter;

use building_types::QueryProxy;
use files::FileId;
use indexing::{
    ImportItemId, ImportKind, IndexedTermItemKind, IndexedTypeItemKind, TermItemId, TypeItemId,
};
use lowering::{
    BinderId, BinderKind, ExpressionId, ExpressionKind, LetBindingNameGroupId, RecordPunId,
    TermOperatorId, TermVariableResolution, TypeId, TypeKind, TypeOperatorId,
    TypeVariableBindingId, TypeVariableResolution,
};
use lsp_types::*;
use smol_str::ToSmolStr;
use stabilizing::AstId;
use syntax::ast::AstNode;
use syntax::{SyntaxNode, SyntaxNodePtr, cst};

use crate::extract::AnnotationSyntaxRange;
use crate::position::{PositionEncoding, Utf8Range};
use crate::{AnalyzerContext, AnalyzerError, locate, position};

pub fn implementation(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    uri: Url,
    position: Position,
) -> Result<Option<Vec<DocumentHighlight>>, AnalyzerError> {
    let current_file = {
        let uri = uri.as_str();
        context.file_id(uri).ok_or(AnalyzerError::NonFatal)?
    };

    let content = context.queries().content(current_file)?;
    let positions = position::PositionConverter::new(&content, context.position_encoding());
    let position = positions.protocol_position_to_utf8(position).ok_or(AnalyzerError::NonFatal)?;

    let located = locate::locate(context.queries(), current_file, &positions, position)?;
    match located {
        locate::Located::ImportItem(import_id) => {
            highlight_import(context, current_file, import_id)
        }
        locate::Located::Binder(binder_id) => highlight_binder(context, current_file, binder_id),
        locate::Located::Expression(expression_id) => {
            highlight_expression(context, current_file, expression_id)
        }
        locate::Located::Type(type_id) => highlight_type(context, current_file, type_id),
        locate::Located::TermItem(term_id) => {
            highlight_file_term(context, current_file, current_file, term_id)
        }
        locate::Located::TypeItem(type_id) => {
            highlight_file_type(context, current_file, current_file, type_id)
        }
        locate::Located::InstanceItem(item_id) => {
            let indexed = context.queries().indexed(current_file)?;
            let mut highlights = vec![];
            push_name_highlight(
                context,
                current_file,
                &mut highlights,
                Some(indexed.items[item_id].id),
                position::instance_declaration_name_range,
            )?;
            Ok(finish_highlights(highlights))
        }
        locate::Located::DeriveItem(item_id) => {
            let indexed = context.queries().indexed(current_file)?;
            let mut highlights = vec![];
            push_name_highlight(
                context,
                current_file,
                &mut highlights,
                Some(indexed.items[item_id].id),
                position::declaration_name_range,
            )?;
            Ok(finish_highlights(highlights))
        }
        locate::Located::LetBinding(let_binding_id) => {
            highlight_let(context, current_file, let_binding_id)
        }
        locate::Located::BinderPun(pun_id) => highlight_binder_pun(context, current_file, pun_id),
        locate::Located::ExpressionPun(pun_id) => {
            highlight_expression_pun(context, current_file, pun_id)
        }
        locate::Located::TermOperator(operator_id) => {
            highlight_term_operator(context, current_file, operator_id)
        }
        locate::Located::TypeOperator(operator_id) => {
            highlight_type_operator(context, current_file, operator_id)
        }
        locate::Located::TermReference(file_id, term_id) => {
            highlight_file_term(context, current_file, file_id, term_id)
        }
        locate::Located::TypeReference(file_id, type_id) => {
            highlight_file_type(context, current_file, file_id, type_id)
        }
        locate::Located::InstanceHead(file_id, type_id) => {
            highlight_file_type(context, current_file, file_id, type_id)
        }
        locate::Located::TypeVariableBinding(binding_id) => {
            highlight_type_variable(context, current_file, binding_id)
        }
        locate::Located::ModuleName(_)
        | locate::Located::InstanceMember(_, _)
        | locate::Located::RecordAccessLabel(_)
        | locate::Located::Nothing => Ok(None),
    }
}

enum HighlightTarget {
    Term(FileId, TermItemId),
    Type(FileId, TypeItemId),
}

fn highlight_import(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    current_file: FileId,
    import_id: ImportItemId,
) -> Result<Option<Vec<DocumentHighlight>>, AnalyzerError> {
    let target = import_target(context, current_file, import_id)?;

    let mut highlights = match target {
        HighlightTarget::Term(file_id, term_id) => {
            highlight_file_term(context, current_file, file_id, term_id)?
        }
        HighlightTarget::Type(file_id, type_id) => {
            highlight_file_type(context, current_file, file_id, type_id)?
        }
    }
    .unwrap_or_default();

    let content = context.queries().content(current_file)?;
    let positions = position::PositionConverter::new(&content, context.position_encoding());
    let (parsed, _) = context.queries().parsed(current_file)?;
    let root = parsed.syntax_node();
    let stabilized = context.queries().stabilized(current_file)?;

    let ptr = stabilized.ast_ptr(import_id).ok_or(AnalyzerError::NonFatal)?;
    let node = ptr.try_to_node(&root).ok_or(AnalyzerError::NonFatal)?;

    highlights.extend(
        position::import_item_name_range(&positions, node)
            .and_then(|range| document_highlight(&content, context.position_encoding(), range)),
    );

    Ok(finish_highlights(highlights))
}

fn import_target(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    current_file: FileId,
    import_id: ImportItemId,
) -> Result<HighlightTarget, AnalyzerError> {
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
    let module_name = statement
        .module_name()
        .ok_or(AnalyzerError::NonFatal)?
        .syntax()
        .text(&content)
        .to_smolstr();

    let imported_file =
        context.queries().module_file(&module_name).ok_or(AnalyzerError::NonFatal)?;
    let imported_resolved = context.queries().resolved(imported_file)?;

    let term_target = |name: &str| {
        let name = name.trim_start_matches("(").trim_end_matches(")");
        let (file_id, term_id) =
            imported_resolved.exports.lookup_term(name).ok_or(AnalyzerError::NonFatal)?;
        Ok(HighlightTarget::Term(file_id, term_id))
    };

    let type_target = |name: &str| {
        let name = name.trim_start_matches("(").trim_end_matches(")");
        let (file_id, type_id) = imported_resolved
            .exports
            .lookup_type(name)
            .or_else(|| imported_resolved.exports.lookup_class(name))
            .ok_or(AnalyzerError::NonFatal)?;
        Ok(HighlightTarget::Type(file_id, type_id))
    };

    let class_target = |name: &str| {
        let name = name.trim_start_matches("(").trim_end_matches(")");
        let (file_id, type_id) = imported_resolved
            .exports
            .lookup_class(name)
            .or_else(|| imported_resolved.exports.lookup_type(name))
            .ok_or(AnalyzerError::NonFatal)?;
        Ok(HighlightTarget::Type(file_id, type_id))
    };

    match node {
        cst::ImportItem::ImportValue(cst) => {
            let token = cst.name_token().ok_or(AnalyzerError::NonFatal)?;
            term_target(token.text(&content))
        }
        cst::ImportItem::ImportClass(cst) => {
            let token = cst.name_token().ok_or(AnalyzerError::NonFatal)?;
            class_target(token.text(&content))
        }
        cst::ImportItem::ImportType(cst) => {
            let token = cst.name_token().ok_or(AnalyzerError::NonFatal)?;
            type_target(token.text(&content))
        }
        cst::ImportItem::ImportOperator(cst) => {
            let token = cst.name_token().ok_or(AnalyzerError::NonFatal)?;
            term_target(token.text(&content))
        }
        cst::ImportItem::ImportTypeOperator(cst) => {
            let token = cst.name_token().ok_or(AnalyzerError::NonFatal)?;
            type_target(token.text(&content))
        }
    }
}

fn highlight_binder(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    current_file: FileId,
    binder_id: BinderId,
) -> Result<Option<Vec<DocumentHighlight>>, AnalyzerError> {
    let content = context.queries().content(current_file)?;
    let positions = position::PositionConverter::new(&content, context.position_encoding());
    let (parsed, _) = context.queries().parsed(current_file)?;
    let stabilized = context.queries().stabilized(current_file)?;
    let lowered = context.queries().lowered(current_file)?;

    let kind = lowered.tree.get_binder_kind(binder_id).ok_or(AnalyzerError::NonFatal)?;

    if let BinderKind::Constructor { resolution: Some((file_id, term_id)), .. } = kind {
        return highlight_file_term(context, current_file, *file_id, *term_id);
    }

    let root = parsed.syntax_node();
    let ptr = stabilized.syntax_ptr(binder_id).ok_or(AnalyzerError::NonFatal)?;

    let mut highlights: Vec<DocumentHighlight> = vec![];

    highlights.extend(
        binder_name_range(&content, &root, &ptr)
            .or_else(|| locate::syntax_range(&positions, &root, &ptr))
            .and_then(|range| document_highlight(&content, context.position_encoding(), range)),
    );

    for (expr_id, expr_kind) in lowered.tree.iter_expression() {
        if let ExpressionKind::Variable {
            resolution: Some(TermVariableResolution::Binder(id)), ..
        } = expr_kind
            && *id == binder_id
        {
            highlights.extend(
                highlight_id_range(&content, &parsed, &stabilized, expr_id).and_then(|range| {
                    document_highlight(&content, context.position_encoding(), range)
                }),
            );
        }
    }

    for (pun_id, resolution) in lowered.tree.iter_expression_pun() {
        if let TermVariableResolution::Binder(id) = resolution
            && id == binder_id
        {
            highlights.extend(highlight_id_range(&content, &parsed, &stabilized, pun_id).and_then(
                |range| document_highlight(&content, context.position_encoding(), range),
            ));
        }
    }

    Ok(finish_highlights(highlights))
}

fn highlight_expression(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    current_file: FileId,
    expression_id: ExpressionId,
) -> Result<Option<Vec<DocumentHighlight>>, AnalyzerError> {
    let lowered = context.queries().lowered(current_file)?;
    let kind = lowered.tree.get_expression_kind(expression_id).ok_or(AnalyzerError::NonFatal)?;

    match kind {
        ExpressionKind::Constructor { resolution: Some((file_id, term_id)) }
        | ExpressionKind::OperatorName { resolution: Some((file_id, term_id)) } => {
            highlight_file_term(context, current_file, *file_id, *term_id)
        }
        ExpressionKind::Variable { resolution: Some(resolution), .. } => match resolution {
            TermVariableResolution::Binder(binder_id) => {
                highlight_binder(context, current_file, *binder_id)
            }
            TermVariableResolution::Let(let_binding_id) => {
                highlight_let(context, current_file, *let_binding_id)
            }
            TermVariableResolution::Reference(file_id, term_id) => {
                highlight_file_term(context, current_file, *file_id, *term_id)
            }
            TermVariableResolution::RecordPun(pun_id) => {
                highlight_binder_pun(context, current_file, *pun_id)
            }
        },
        _ => Ok(None),
    }
}

fn highlight_type(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    current_file: FileId,
    type_id: TypeId,
) -> Result<Option<Vec<DocumentHighlight>>, AnalyzerError> {
    let lowered = context.queries().lowered(current_file)?;
    let kind = lowered.tree.get_type_kind(type_id).ok_or(AnalyzerError::NonFatal)?;

    match kind {
        TypeKind::Constructor { resolution: Some((file_id, type_id)) }
        | TypeKind::Operator { resolution: Some((file_id, type_id)) } => {
            highlight_file_type(context, current_file, *file_id, *type_id)
        }
        TypeKind::Variable {
            resolution: Some(TypeVariableResolution::Forall(binding_id)), ..
        } => highlight_type_variable(context, current_file, *binding_id),
        _ => Ok(None),
    }
}

fn highlight_type_variable(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    current_file: FileId,
    binding_id: TypeVariableBindingId,
) -> Result<Option<Vec<DocumentHighlight>>, AnalyzerError> {
    let content = context.queries().content(current_file)?;
    let (parsed, _) = context.queries().parsed(current_file)?;
    let stabilized = context.queries().stabilized(current_file)?;
    let lowered = context.queries().lowered(current_file)?;

    let mut highlights = vec![];
    push_name_highlight(
        context,
        current_file,
        &mut highlights,
        Some(binding_id),
        position::type_variable_binding_name_range,
    )?;

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

        highlights
            .extend(highlight_id_range(&content, &parsed, &stabilized, type_id).and_then(
                |range| document_highlight(&content, context.position_encoding(), range),
            ));
    }

    Ok(finish_highlights(highlights))
}

fn highlight_file_term(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    current_file: FileId,
    file_id: FileId,
    term_id: TermItemId,
) -> Result<Option<Vec<DocumentHighlight>>, AnalyzerError> {
    let content = context.queries().content(current_file)?;
    let positions = position::PositionConverter::new(&content, context.position_encoding());
    let (parsed, _) = context.queries().parsed(current_file)?;
    let root = parsed.syntax_node();
    let stabilized = context.queries().stabilized(current_file)?;
    let lowered = context.queries().lowered(current_file)?;
    let indexed = context.queries().indexed(current_file)?;
    let resolved = context.queries().resolved(current_file)?;

    let mut highlights = vec![];

    for (expression_id, expression_kind) in lowered.tree.iter_expression() {
        if let ExpressionKind::Constructor { resolution: Some((f_id, t_id)) }
        | ExpressionKind::OperatorName { resolution: Some((f_id, t_id)) }
        | ExpressionKind::Variable {
            resolution: Some(TermVariableResolution::Reference(f_id, t_id)),
            ..
        } = expression_kind
            && (*f_id, *t_id) == (file_id, term_id)
        {
            highlights.extend(
                highlight_id_range(&content, &parsed, &stabilized, expression_id).and_then(
                    |range| document_highlight(&content, context.position_encoding(), range),
                ),
            );
        }
    }

    for (binder_id, binder_kind) in lowered.tree.iter_binder() {
        if let BinderKind::Constructor { resolution: Some((f_id, t_id)), .. } = binder_kind
            && (*f_id, *t_id) == (file_id, term_id)
        {
            highlights.extend(
                highlight_id_range(&content, &parsed, &stabilized, binder_id).and_then(|range| {
                    document_highlight(&content, context.position_encoding(), range)
                }),
            );
        }
    }

    for (operator_id, f_id, t_id) in lowered.tree.iter_term_operator() {
        if (f_id, t_id) == (file_id, term_id) {
            highlights.extend(
                highlight_id_range(&content, &parsed, &stabilized, operator_id).and_then(|range| {
                    document_highlight(&content, context.position_encoding(), range)
                }),
            );
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
        highlights.extend(document_highlight(&content, context.position_encoding(), range));
    }

    for (pun_id, resolution) in lowered.tree.iter_expression_pun() {
        if let TermVariableResolution::Reference(f_id, t_id) = resolution
            && (f_id, t_id) == (file_id, term_id)
        {
            highlights.extend(highlight_id_range(&content, &parsed, &stabilized, pun_id).and_then(
                |range| document_highlight(&content, context.position_encoding(), range),
            ));
        }
    }

    let unqualified = resolved.unqualified.values();
    let qualified = resolved.qualified.values();

    for imports in iter::chain(unqualified, qualified) {
        for import in imports {
            for (name, f_id, t_id, kind) in import.iter_terms() {
                if kind != ImportKind::Hidden
                    && (f_id, t_id) == (file_id, term_id)
                    && let Some(indexed_import) = indexed.imports.get(&import.id)
                    && let Some(import_item_id) = indexed_import.terms.get(name)
                {
                    highlights.extend(stabilized.ast_ptr(*import_item_id).and_then(|ptr| {
                        let node = ptr.try_to_node(&root)?;
                        let range = position::import_item_name_range(&positions, node)?;
                        document_highlight(&content, context.position_encoding(), range)
                    }));
                }
            }
        }
    }

    if file_id == current_file
        && let Some(definition_highlights) = term_item_highlights(context, current_file, term_id)?
    {
        highlights.extend(definition_highlights);
    }

    Ok(finish_highlights(highlights))
}

fn highlight_file_type(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    current_file: FileId,
    file_id: FileId,
    type_id: TypeItemId,
) -> Result<Option<Vec<DocumentHighlight>>, AnalyzerError> {
    let content = context.queries().content(current_file)?;
    let positions = position::PositionConverter::new(&content, context.position_encoding());
    let (parsed, _) = context.queries().parsed(current_file)?;
    let root = parsed.syntax_node();
    let stabilized = context.queries().stabilized(current_file)?;
    let lowered = context.queries().lowered(current_file)?;
    let indexed = context.queries().indexed(current_file)?;
    let resolved = context.queries().resolved(current_file)?;

    let mut highlights = vec![];

    for (ty_id, ty_kind) in lowered.tree.iter_type() {
        if let TypeKind::Constructor { resolution: Some((f_id, t_id)) }
        | TypeKind::Operator { resolution: Some((f_id, t_id)) } = ty_kind
            && (*f_id, *t_id) == (file_id, type_id)
        {
            highlights.extend(highlight_id_range(&content, &parsed, &stabilized, ty_id).and_then(
                |range| document_highlight(&content, context.position_encoding(), range),
            ));
        }
    }

    for (operator_id, f_id, t_id) in lowered.tree.iter_type_operator() {
        if (f_id, t_id) == (file_id, type_id) {
            highlights.extend(
                highlight_id_range(&content, &parsed, &stabilized, operator_id).and_then(|range| {
                    document_highlight(&content, context.position_encoding(), range)
                }),
            );
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
        highlights.extend(document_highlight(&content, context.position_encoding(), range));
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
        highlights.extend(document_highlight(&content, context.position_encoding(), range));
    }

    for imports in resolved.unqualified.values().chain(resolved.qualified.values()) {
        for import in imports {
            for (name, f_id, t_id, kind) in import.iter_types().chain(import.iter_classes()) {
                if kind != ImportKind::Hidden
                    && (f_id, t_id) == (file_id, type_id)
                    && let Some(indexed_import) = indexed.imports.get(&import.id)
                    && let Some((import_item_id, _)) = indexed_import.types.get(name)
                {
                    highlights.extend(stabilized.ast_ptr(*import_item_id).and_then(|ptr| {
                        let node = ptr.try_to_node(&root)?;
                        let range = position::import_item_name_range(&positions, node)?;
                        document_highlight(&content, context.position_encoding(), range)
                    }));
                }
            }
        }
    }

    if file_id == current_file
        && let Some(definition_highlights) = type_item_highlights(context, current_file, type_id)?
    {
        highlights.extend(definition_highlights);
    }

    Ok(finish_highlights(highlights))
}

fn highlight_term_operator(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    current_file: FileId,
    operator_id: TermOperatorId,
) -> Result<Option<Vec<DocumentHighlight>>, AnalyzerError> {
    let lowered = context.queries().lowered(current_file)?;
    let (file_id, term_id) =
        lowered.tree.get_term_operator(operator_id).ok_or(AnalyzerError::NonFatal)?;
    highlight_file_term(context, current_file, file_id, term_id)
}

fn highlight_type_operator(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    current_file: FileId,
    operator_id: TypeOperatorId,
) -> Result<Option<Vec<DocumentHighlight>>, AnalyzerError> {
    let lowered = context.queries().lowered(current_file)?;
    let (file_id, type_id) =
        lowered.tree.get_type_operator(operator_id).ok_or(AnalyzerError::NonFatal)?;
    highlight_file_type(context, current_file, file_id, type_id)
}

fn highlight_let(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    current_file: FileId,
    let_binding_id: LetBindingNameGroupId,
) -> Result<Option<Vec<DocumentHighlight>>, AnalyzerError> {
    let content = context.queries().content(current_file)?;
    let positions = position::PositionConverter::new(&content, context.position_encoding());
    let (parsed, _) = context.queries().parsed(current_file)?;
    let stabilized = context.queries().stabilized(current_file)?;
    let lowered = context.queries().lowered(current_file)?;

    let root = parsed.syntax_node();
    let binding = lowered.tree.get_let_binding_group(let_binding_id);

    let mut highlights: Vec<DocumentHighlight> = vec![];

    if let Some(signature) = binding.signature {
        let ptr = stabilized.syntax_ptr(signature).ok_or(AnalyzerError::NonFatal)?;
        highlights.extend(
            let_signature_name_range(&content, &root, &ptr)
                .or_else(|| locate::syntax_range(&positions, &root, &ptr))
                .and_then(|range| document_highlight(&content, context.position_encoding(), range)),
        );
    }

    for &equation in binding.equations.iter() {
        let ptr = stabilized.syntax_ptr(equation).ok_or(AnalyzerError::NonFatal)?;
        highlights.extend(
            let_equation_name_range(&content, &root, &ptr)
                .or_else(|| locate::syntax_range(&positions, &root, &ptr))
                .and_then(|range| document_highlight(&content, context.position_encoding(), range)),
        );
    }

    for (expr_id, expr_kind) in lowered.tree.iter_expression() {
        if let ExpressionKind::Variable {
            resolution: Some(TermVariableResolution::Let(id)), ..
        } = expr_kind
            && *id == let_binding_id
        {
            highlights.extend(
                highlight_id_range(&content, &parsed, &stabilized, expr_id).and_then(|range| {
                    document_highlight(&content, context.position_encoding(), range)
                }),
            );
        }
    }

    for (pun_id, resolution) in lowered.tree.iter_expression_pun() {
        if let TermVariableResolution::Let(id) = resolution
            && id == let_binding_id
        {
            highlights.extend(highlight_id_range(&content, &parsed, &stabilized, pun_id).and_then(
                |range| document_highlight(&content, context.position_encoding(), range),
            ));
        }
    }

    Ok(finish_highlights(highlights))
}

fn highlight_binder_pun(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    current_file: FileId,
    pun_id: RecordPunId,
) -> Result<Option<Vec<DocumentHighlight>>, AnalyzerError> {
    let content = context.queries().content(current_file)?;
    let (parsed, _) = context.queries().parsed(current_file)?;
    let stabilized = context.queries().stabilized(current_file)?;
    let lowered = context.queries().lowered(current_file)?;

    let mut highlights = vec![];

    highlights.extend(
        highlight_id_range(&content, &parsed, &stabilized, pun_id)
            .and_then(|range| document_highlight(&content, context.position_encoding(), range)),
    );

    for (expression_id, expression_kind) in lowered.tree.iter_expression() {
        if let ExpressionKind::Variable {
            resolution: Some(TermVariableResolution::RecordPun(candidate_id)),
        } = expression_kind
            && *candidate_id == pun_id
        {
            highlights.extend(
                highlight_id_range(&content, &parsed, &stabilized, expression_id).and_then(
                    |range| document_highlight(&content, context.position_encoding(), range),
                ),
            );
        }
    }

    for (expression_pun_id, resolution) in lowered.tree.iter_expression_pun() {
        if let TermVariableResolution::RecordPun(candidate_id) = resolution
            && candidate_id == pun_id
        {
            highlights.extend(
                highlight_id_range(&content, &parsed, &stabilized, expression_pun_id).and_then(
                    |range| document_highlight(&content, context.position_encoding(), range),
                ),
            );
        }
    }

    Ok(finish_highlights(highlights))
}

fn highlight_expression_pun(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    current_file: FileId,
    pun_id: RecordPunId,
) -> Result<Option<Vec<DocumentHighlight>>, AnalyzerError> {
    let lowered = context.queries().lowered(current_file)?;
    match lowered.tree.get_expression_pun(pun_id).ok_or(AnalyzerError::NonFatal)? {
        TermVariableResolution::Binder(binder_id) => {
            highlight_binder(context, current_file, binder_id)
        }
        TermVariableResolution::Let(let_binding_id) => {
            highlight_let(context, current_file, let_binding_id)
        }
        TermVariableResolution::RecordPun(pun_id) => {
            highlight_binder_pun(context, current_file, pun_id)
        }
        TermVariableResolution::Reference(file_id, term_id) => {
            highlight_file_term(context, current_file, file_id, term_id)
        }
    }
}

fn term_item_highlights(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    current_file: FileId,
    term_id: TermItemId,
) -> Result<Option<Vec<DocumentHighlight>>, AnalyzerError> {
    let indexed = context.queries().indexed(current_file)?;

    let mut highlights = vec![];

    macro_rules! push_name_highlights {
        ($range:expr; $($id:expr),+ $(,)?) => {
            $(
                push_name_highlight(
                    context,
                    current_file,
                    &mut highlights,
                    $id,
                    $range,
                )?;
            )+
        };
    }

    match &indexed.items[term_id].kind {
        IndexedTermItemKind::ClassMember { id, .. } => {
            push_name_highlights!(position::class_member_name_range; Some(*id));
        }
        IndexedTermItemKind::Constructor { id, .. } => {
            push_name_highlights!(position::data_constructor_name_range; Some(*id));
        }
        IndexedTermItemKind::Foreign { id } => {
            push_name_highlights!(position::declaration_name_range; Some(*id));
        }
        IndexedTermItemKind::Operator { id } => {
            push_name_highlights!(position::infix_operator_range; Some(*id));
        }
        IndexedTermItemKind::Value { signature, equations } => {
            push_name_highlights!(position::declaration_name_range; *signature);

            for &equation in equations {
                push_name_highlights!(position::declaration_name_range; Some(equation));
            }
        }
    }

    Ok(finish_highlights(highlights))
}

fn type_item_highlights(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    current_file: FileId,
    type_id: TypeItemId,
) -> Result<Option<Vec<DocumentHighlight>>, AnalyzerError> {
    let indexed = context.queries().indexed(current_file)?;

    let mut highlights = vec![];

    macro_rules! push_name_highlights {
        ($range:expr; $($id:expr),+ $(,)?) => {
            $(
                push_name_highlight(
                    context,
                    current_file,
                    &mut highlights,
                    $id,
                    $range,
                )?;
            )+
        };
    }

    match indexed.items[type_id].kind {
        IndexedTypeItemKind::Data { signature, equation, role, .. } => {
            push_name_highlights!(position::declaration_name_range; signature, equation, role);
        }
        IndexedTypeItemKind::Newtype { signature, equation, role, .. } => {
            push_name_highlights!(position::declaration_name_range; signature, equation, role);
        }
        IndexedTypeItemKind::Synonym { signature, equation } => {
            push_name_highlights!(position::declaration_name_range; signature, equation);
        }
        IndexedTypeItemKind::Class { signature, declaration, .. } => {
            push_name_highlights!(position::declaration_name_range; signature, declaration);
        }
        IndexedTypeItemKind::Foreign { id, role } => {
            push_name_highlights!(position::declaration_name_range; Some(id), role);
        }
        IndexedTypeItemKind::Operator { id } => {
            push_name_highlights!(position::infix_operator_range; Some(id));
        }
    }

    Ok(finish_highlights(highlights))
}

fn binder_name_range(content: &str, root: &SyntaxNode, ptr: &SyntaxNodePtr) -> Option<Utf8Range> {
    let positions = position::PositionConverter::new(content, PositionEncoding::Utf8);
    let node = ptr.try_to_node(root)?;

    if let Some(binder) = cst::BinderVariable::cast(node.clone()) {
        let token = binder.name_token()?;
        return positions.text_range_to_utf8_range(token.text_range());
    }

    if let Some(binder) = cst::BinderNamed::cast(node) {
        let token = binder.name_token()?;
        return positions.text_range_to_utf8_range(token.text_range());
    }

    None
}

fn let_signature_name_range(
    content: &str,
    root: &SyntaxNode,
    ptr: &SyntaxNodePtr,
) -> Option<Utf8Range> {
    let positions = position::PositionConverter::new(content, PositionEncoding::Utf8);
    let node = ptr.try_to_node(root)?;
    let signature = cst::LetBindingSignature::cast(node)?;
    let token = signature.name_token()?;
    positions.text_range_to_utf8_range(token.text_range())
}

fn let_equation_name_range(
    content: &str,
    root: &SyntaxNode,
    ptr: &SyntaxNodePtr,
) -> Option<Utf8Range> {
    let positions = position::PositionConverter::new(content, PositionEncoding::Utf8);
    let node = ptr.try_to_node(root)?;
    let equation = cst::LetBindingEquation::cast(node)?;
    let token = equation.name_token()?;
    positions.text_range_to_utf8_range(token.text_range())
}

fn push_name_highlight<T>(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    current_file: FileId,
    highlights: &mut Vec<DocumentHighlight>,
    id: Option<AstId<T>>,
    range: fn(&position::PositionConverter<'_>, &SyntaxNode, &SyntaxNodePtr) -> Option<Utf8Range>,
) -> Result<(), AnalyzerError>
where
    T: AstNode,
{
    let content = context.queries().content(current_file)?;
    let positions = position::PositionConverter::new(&content, context.position_encoding());
    let (parsed, _) = context.queries().parsed(current_file)?;
    let root = parsed.syntax_node();
    let stabilized = context.queries().stabilized(current_file)?;

    highlights.extend(id.and_then(|id| {
        let ptr = stabilized.syntax_ptr(id)?;
        let range = range(&positions, &root, &ptr)?;
        document_highlight(&content, context.position_encoding(), range)
    }));

    Ok(())
}

trait DocumentHighlightRange: AstNode {
    fn annotation_syntax_range(&self) -> AnnotationSyntaxRange;
}

macro_rules! impl_document_highlight_range {
    ($($target:ty),+ $(,)?) => {
        $(
            impl DocumentHighlightRange for $target {
                fn annotation_syntax_range(&self) -> AnnotationSyntaxRange {
                    AnnotationSyntaxRange::from_node(self.syntax())
                }
            }
        )+
    };
}

impl_document_highlight_range!(
    cst::Binder,
    cst::Expression,
    cst::TermOperator,
    cst::Type,
    cst::TypeOperator,
);

impl DocumentHighlightRange for cst::RecordPun {
    fn annotation_syntax_range(&self) -> AnnotationSyntaxRange {
        self.name().map(|name| AnnotationSyntaxRange::from_node(name.syntax())).unwrap_or_default()
    }
}

fn highlight_id_range<T>(
    content: &str,
    parsed: &parsing::ParsedModule,
    stabilized: &stabilizing::StabilizedModule,
    item_id: AstId<T>,
) -> Option<Utf8Range>
where
    T: DocumentHighlightRange,
{
    let positions = position::PositionConverter::new(content, PositionEncoding::Utf8);
    let root = parsed.syntax_node();
    let ptr = stabilized.syntax_ptr(item_id)?;
    let node = ptr.try_to_node(&root)?;
    let target = T::cast(node)?;
    let range = target.annotation_syntax_range().syntax?;
    positions.text_range_to_utf8_range(range)
}

fn document_highlight(
    content: &str,
    encoding: PositionEncoding,
    range: Utf8Range,
) -> Option<DocumentHighlight> {
    let positions = position::PositionConverter::new(content, encoding);
    let range = positions.utf8_range_to_protocol(range)?;
    Some(DocumentHighlight { range, kind: None })
}

fn finish_highlights(mut highlights: Vec<DocumentHighlight>) -> Option<Vec<DocumentHighlight>> {
    highlights.sort_by_key(|DocumentHighlight { range, .. }| {
        (range.start.line, range.start.character, range.end.line, range.end.character)
    });
    highlights.dedup_by(|left, right| left.range == right.range);

    let has_highlights = !highlights.is_empty();
    has_highlights.then_some(highlights)
}
