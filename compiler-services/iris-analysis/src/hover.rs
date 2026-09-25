use building_types::QueryProxy;
use checking::core::pretty::{Pretty, PrettyConfig};
use checking::evidence::InstanceCandidateOrigin;
use checking::tree::pretty::Pretty as TreePretty;
use files::FileId;
use indexing::{ImportItemId, TermItemId, TypeItemId};
use lowering::{
    BinderKind, ExpressionKind, TermVariableResolution, TypeKind, TypeVariableResolution,
};
use lsp_types::*;
use smol_str::ToSmolStr;
use syntax::ast::{AstNode, AstPtr};
use syntax::{SyntaxToken, TextRange, TextSize, cst};

use crate::extract::AnnotationSyntaxRange;
use crate::position::PositionConverter;
use crate::{AnalyzerContext, AnalyzerError, AnalyzerQueries, extract, locate, position};

const PRETTY_CONFIG: PrettyConfig = PrettyConfig::new().width(80);

pub fn implementation(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    uri: Uri,
    position: Position,
) -> Result<Option<Hover>, AnalyzerError> {
    let current_file = {
        let uri = uri.as_str();
        context.file_id(uri).ok_or(AnalyzerError::NonFatal)?
    };

    let engine = context.queries();
    let content = engine.content(current_file)?;
    let positions = PositionConverter::new(&content, context.position_encoding());
    let position = positions.protocol_position_to_utf8(position).ok_or(AnalyzerError::NonFatal)?;

    let offset = positions.utf8_position_to_offset(position).ok_or(AnalyzerError::NonFatal)?;
    let (located, token) = locate::locate_with_token(engine, current_file, &positions, position)?;
    let range =
        hover_name_range(token, offset).and_then(|range| positions.text_range_to_protocol(range));
    let format = &context.capabilities().hover_format();

    let hover = match located {
        locate::Located::ModuleName(module_name) => {
            hover_module_name(engine, current_file, module_name, format)
        }
        locate::Located::ImportItem(import_id) => {
            hover_import(engine, current_file, import_id, format)
        }
        locate::Located::Binder(binder_id) => hover_binder(engine, current_file, binder_id, format),
        locate::Located::Expression(expression_id) => {
            hover_expression(engine, current_file, expression_id, format)
        }
        locate::Located::RecordAccessLabel(label_id) => {
            hover_record_access_label(engine, current_file, label_id, format)
        }
        locate::Located::Type(type_id) => hover_type(engine, current_file, type_id, format),
        locate::Located::TypeVariableBinding(binding_id) => {
            hover_type_variable_binding(engine, current_file, binding_id, format)
        }
        locate::Located::BinderPun(pun_id) => hover_pun(engine, current_file, pun_id, format),
        locate::Located::ExpressionPun(pun_id) => hover_pun(engine, current_file, pun_id, format),
        locate::Located::TermOperator(operator_id) => {
            let lowered = engine.lowered(current_file)?;
            let (f_id, t_id) =
                lowered.tree.get_term_operator(operator_id).ok_or(AnalyzerError::NonFatal)?;
            hover_file_term(engine, f_id, t_id, format)
        }
        locate::Located::TypeOperator(operator_id) => {
            let lowered = engine.lowered(current_file)?;
            let (f_id, t_id) =
                lowered.tree.get_type_operator(operator_id).ok_or(AnalyzerError::NonFatal)?;
            hover_file_type(engine, f_id, t_id, format)
        }
        locate::Located::TermReference(file_id, term_id) => {
            hover_file_term(engine, file_id, term_id, format)
        }
        locate::Located::TypeReference(file_id, type_id) => {
            hover_file_type(engine, file_id, type_id, format)
        }
        locate::Located::InstanceHead(file_id, type_id) => {
            hover_file_type(engine, file_id, type_id, format)
        }
        locate::Located::InstanceMember(file_id, term_id) => {
            hover_file_term(engine, file_id, term_id, format)
        }
        locate::Located::TermItem(term_id) => {
            hover_file_term(engine, current_file, term_id, format)
        }
        locate::Located::TypeItem(type_id) => {
            hover_file_type(engine, current_file, type_id, format)
        }
        locate::Located::InstanceItem(item_id) => {
            let indexed = engine.indexed(current_file)?;
            let checked = engine.checked(current_file)?;
            let item = &indexed.items[item_id];
            let signature = checked.lookup_instance(item.id).map(|instance| instance.signature);
            let pretty = TreePretty::new(engine, &checked);
            let origin = InstanceCandidateOrigin::Instance(current_file, item.id);
            let name = pretty.render_instance_name(current_file, origin)?;
            hover_instance_signature(engine, &checked, Some(&name), signature, format)
        }
        locate::Located::DeriveItem(item_id) => {
            let indexed = engine.indexed(current_file)?;
            let checked = engine.checked(current_file)?;
            let item = &indexed.items[item_id];
            let signature =
                checked.lookup_derived_instance(item.id).map(|instance| instance.signature);
            let pretty = TreePretty::new(engine, &checked);
            let origin = InstanceCandidateOrigin::Derive(current_file, item.id);
            let name = pretty.render_instance_name(current_file, origin)?;
            hover_instance_signature(engine, &checked, Some(&name), signature, format)
        }
        locate::Located::LetBinding(let_id) => hover_let(engine, current_file, let_id, format),
        locate::Located::Nothing => Ok(None),
    }?;

    Ok(hover.map(|mut hover| {
        hover.range = range;
        hover
    }))
}

fn hover_instance_signature(
    engine: &impl AnalyzerQueries,
    checked: &checking::CheckedModule,
    name: Option<&str>,
    signature: Option<checking::TypeId>,
    format: &MarkupKind,
) -> Result<Option<Hover>, AnalyzerError> {
    let signature = signature.ok_or(AnalyzerError::NonFatal)?;
    let pretty = Pretty::with_config(engine, checked, PRETTY_CONFIG);
    let value = pretty.render_signature(name.unwrap_or("<unknown>"), signature).to_string();
    Ok(Some(render_hover(format, Some(value), None)))
}

fn hover_name_range(token: Option<SyntaxToken>, offset: TextSize) -> Option<TextRange> {
    let token = token?;
    let mut ancestors = token.parent_ancestors();
    let range = ancestors.find_map(|node| {
        let kind = node.kind();
        if cst::ModuleName::can_cast(kind) {
            Some(node.text_range())
        } else if let Some(qualified) = cst::QualifiedName::cast(node) {
            position::qualified_name_text_range(&qualified)
        } else {
            None
        }
    });
    range.filter(|range| range.contains_inclusive(offset))
}

fn hover_module_name(
    engine: &impl AnalyzerQueries,
    current_file: FileId,
    module_name: AstPtr<cst::ModuleName>,
    format: &MarkupKind,
) -> Result<Option<Hover>, AnalyzerError> {
    let content = engine.content(current_file)?;
    let (parsed, _) = engine.parsed(current_file)?;

    let root = parsed.syntax_node();
    let module_name = module_name.try_to_node(&root).ok_or(AnalyzerError::NonFatal)?;

    let module_name = module_name.syntax().text(&content).to_smolstr();
    let module_id = engine.module_file(&module_name).ok_or(AnalyzerError::NonFatal)?;

    let content = engine.content(module_id)?;
    let range = AnnotationSyntaxRange::of_file(engine, module_id)?;

    let annotation = range.annotation.and_then(|range| render_annotation(&content, range));
    let syntax = range.syntax.and_then(|range| render_syntax(&content, range));

    Ok(Some(render_hover(format, syntax, annotation)))
}

fn hover_import(
    engine: &impl AnalyzerQueries,
    current_file: FileId,
    import_id: ImportItemId,
    format: &MarkupKind,
) -> Result<Option<Hover>, AnalyzerError> {
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

    let import_id = engine.module_file(&module_name).ok_or(AnalyzerError::NonFatal)?;
    let import_resolved = engine.resolved(import_id)?;

    let hover_term_import = |engine, name: &str| {
        let name = name.trim_start_matches("(").trim_end_matches(")");
        let (f_id, t_id) =
            import_resolved.exports.lookup_term(name).ok_or(AnalyzerError::NonFatal)?;
        hover_file_term(engine, f_id, t_id, format)
    };

    let hover_type_import = |engine, name: &str| {
        let name = name.trim_start_matches("(").trim_end_matches(")");
        let (f_id, t_id) = import_resolved
            .exports
            .lookup_type(name)
            .or_else(|| import_resolved.exports.lookup_class(name))
            .ok_or(AnalyzerError::NonFatal)?;
        hover_file_type(engine, f_id, t_id, format)
    };

    let hover_class_import = |engine, name: &str| {
        let name = name.trim_start_matches("(").trim_end_matches(")");
        let (f_id, t_id) = import_resolved
            .exports
            .lookup_class(name)
            .or_else(|| import_resolved.exports.lookup_type(name))
            .ok_or(AnalyzerError::NonFatal)?;
        hover_file_type(engine, f_id, t_id, format)
    };

    match node {
        cst::ImportItem::ImportValue(cst) => {
            let token = cst.name_token().ok_or(AnalyzerError::NonFatal)?;
            let name = token.text(&content);
            hover_term_import(engine, name)
        }
        cst::ImportItem::ImportClass(cst) => {
            let token = cst.name_token().ok_or(AnalyzerError::NonFatal)?;
            let name = token.text(&content);
            hover_class_import(engine, name)
        }
        cst::ImportItem::ImportType(cst) => {
            let token = cst.name_token().ok_or(AnalyzerError::NonFatal)?;
            let name = token.text(&content);
            hover_type_import(engine, name)
        }
        cst::ImportItem::ImportOperator(cst) => {
            let token = cst.name_token().ok_or(AnalyzerError::NonFatal)?;
            let name = token.text(&content);
            hover_term_import(engine, name)
        }
        cst::ImportItem::ImportTypeOperator(cst) => {
            let token = cst.name_token().ok_or(AnalyzerError::NonFatal)?;
            let name = token.text(&content);
            hover_type_import(engine, name)
        }
    }
}

fn hover_binder(
    engine: &impl AnalyzerQueries,
    current_file: FileId,
    binder_id: lowering::BinderId,
    format: &MarkupKind,
) -> Result<Option<Hover>, AnalyzerError> {
    let lowered = engine.lowered(current_file)?;
    let kind = lowered.tree.get_binder_kind(binder_id).ok_or(AnalyzerError::NonFatal)?;
    match kind {
        BinderKind::Constructor { resolution, .. } => {
            let (f_id, t_id) = resolution.as_ref().ok_or(AnalyzerError::NonFatal)?;
            hover_file_term(engine, *f_id, *t_id, format)
        }
        _ => {
            let checked = engine.checked(current_file)?;

            let binder_type = checked.node_types.lookup_binder(binder_id);
            let binder_type = binder_type.ok_or(AnalyzerError::NonFatal)?;

            hover_checked_type(engine, current_file, binder_type, format)
        }
    }
}

fn hover_expression(
    engine: &impl AnalyzerQueries,
    current_file: FileId,
    expression_id: lowering::ExpressionId,
    format: &MarkupKind,
) -> Result<Option<Hover>, AnalyzerError> {
    let lowered = engine.lowered(current_file)?;
    let kind = lowered.tree.get_expression_kind(expression_id).ok_or(AnalyzerError::NonFatal)?;

    match kind {
        ExpressionKind::Constructor { resolution, .. } => {
            let (f_id, t_id) = resolution.as_ref().ok_or(AnalyzerError::NonFatal)?;
            hover_file_term(engine, *f_id, *t_id, format)
        }
        ExpressionKind::Variable { resolution, .. } => {
            let resolution = resolution.as_ref().ok_or(AnalyzerError::NonFatal)?;
            match resolution {
                TermVariableResolution::Binder(binder_id) => {
                    hover_binder(engine, current_file, *binder_id, format)
                }
                TermVariableResolution::Let(let_binding_id) => {
                    hover_let(engine, current_file, *let_binding_id, format)
                }
                TermVariableResolution::RecordPun(pun_id) => {
                    hover_pun(engine, current_file, *pun_id, format)
                }
                TermVariableResolution::Reference(f_id, t_id) => {
                    hover_file_term(engine, *f_id, *t_id, format)
                }
            }
        }
        ExpressionKind::OperatorName { resolution, .. } => {
            let (f_id, t_id) = resolution.as_ref().ok_or(AnalyzerError::NonFatal)?;
            hover_file_term(engine, *f_id, *t_id, format)
        }
        _ => {
            let checked = engine.checked(current_file)?;

            let expression_type = checked.node_types.lookup_expression(expression_id);
            let expression_type = expression_type.ok_or(AnalyzerError::NonFatal)?;

            hover_checked_type(engine, current_file, expression_type, format)
        }
    }
}

fn hover_record_access_label(
    engine: &impl AnalyzerQueries,
    current_file: FileId,
    label_id: lowering::RecordAccessLabelId,
    format: &MarkupKind,
) -> Result<Option<Hover>, AnalyzerError> {
    let checked = engine.checked(current_file)?;
    let label_type = checked.node_types.lookup_record_access_label(label_id);
    let label_type = label_type.ok_or(AnalyzerError::NonFatal)?;
    hover_checked_type(engine, current_file, label_type, format)
}

fn hover_let(
    engine: &impl AnalyzerQueries,
    current_file: FileId,
    let_binding_id: lowering::LetBindingNameGroupId,
    format: &MarkupKind,
) -> Result<Option<Hover>, AnalyzerError> {
    let checked = engine.checked(current_file)?;

    let let_type = checked.node_types.lookup_let(let_binding_id);
    let let_type = let_type.ok_or(AnalyzerError::NonFatal)?;

    hover_checked_type(engine, current_file, let_type, format)
}

fn hover_type(
    engine: &impl AnalyzerQueries,
    current_file: FileId,
    type_id: lowering::TypeId,
    format: &MarkupKind,
) -> Result<Option<Hover>, AnalyzerError> {
    let lowered = engine.lowered(current_file)?;
    let kind = lowered.tree.get_type_kind(type_id).ok_or(AnalyzerError::NonFatal)?;

    match kind {
        TypeKind::Constructor { resolution, .. } => {
            let (f_id, t_id) = resolution.as_ref().ok_or(AnalyzerError::NonFatal)?;
            hover_file_type(engine, *f_id, *t_id, format)
        }
        TypeKind::Variable {
            resolution: Some(TypeVariableResolution::Forall(binding_id)), ..
        } => hover_type_variable_binding(engine, current_file, *binding_id, format),
        _ => {
            let checked = engine.checked(current_file)?;

            let type_kind = checked.node_types.lookup_type_kind(type_id);
            let type_kind = type_kind.ok_or(AnalyzerError::NonFatal)?;

            hover_checked_kind(engine, current_file, type_kind, format)
        }
    }
}

fn hover_type_variable_binding(
    engine: &impl AnalyzerQueries,
    current_file: FileId,
    binding_id: lowering::TypeVariableBindingId,
    format: &MarkupKind,
) -> Result<Option<Hover>, AnalyzerError> {
    let checked = engine.checked(current_file)?;
    let binding_kind = checked.node_types.lookup_forall_binding(binding_id);
    let binding_kind = binding_kind.ok_or(AnalyzerError::NonFatal)?;

    hover_checked_kind(engine, current_file, binding_kind, format)
}

fn hover_checked_type(
    engine: &impl AnalyzerQueries,
    current_file: FileId,
    type_id: checking::TypeId,
    format: &MarkupKind,
) -> Result<Option<Hover>, AnalyzerError> {
    let checked = engine.checked(current_file)?;

    let pretty = Pretty::with_config(engine, &checked, PRETTY_CONFIG);
    let value = pretty.render(type_id).to_string();
    Ok(Some(render_hover(format, Some(value), None)))
}

fn hover_checked_kind(
    engine: &impl AnalyzerQueries,
    current_file: FileId,
    type_id: checking::TypeId,
    format: &MarkupKind,
) -> Result<Option<Hover>, AnalyzerError> {
    let checked = engine.checked(current_file)?;

    let pretty = Pretty::with_config(engine, &checked, PRETTY_CONFIG);
    let value = pretty.render_kind(type_id).to_string();
    Ok(Some(render_hover(format, Some(value), None)))
}

fn hover_file_term(
    engine: &impl AnalyzerQueries,
    file_id: FileId,
    term_id: TermItemId,
    format: &MarkupKind,
) -> Result<Option<Hover>, AnalyzerError> {
    let content = engine.content(file_id)?;
    let indexed = engine.indexed(file_id)?;
    let checked = engine.checked(file_id)?;

    let range = AnnotationSyntaxRange::of_file_term(engine, file_id, term_id)?;
    let annotation = range.annotation.and_then(|range| render_annotation(&content, range));

    let name = if let Some(name) = &indexed.items[term_id].name { name } else { "<unknown>" };
    let signature = checked.lookup_term_item_type(term_id).ok_or(AnalyzerError::NonFatal)?;

    let pretty = Pretty::with_config(engine, &checked, PRETTY_CONFIG);
    let value = pretty.render_signature(name, signature).to_string();
    Ok(Some(render_hover(format, Some(value), annotation)))
}

fn hover_file_type(
    engine: &impl AnalyzerQueries,
    file_id: FileId,
    type_id: TypeItemId,
    format: &MarkupKind,
) -> Result<Option<Hover>, AnalyzerError> {
    let content = engine.content(file_id)?;
    let indexed = engine.indexed(file_id)?;
    let checked = engine.checked(file_id)?;

    let range = AnnotationSyntaxRange::of_file_type(engine, file_id, type_id)?;
    let annotation = range.annotation.and_then(|range| render_annotation(&content, range));

    let name = if let Some(name) = &indexed.items[type_id].name { name } else { "<unknown>" };
    let signature = checked.lookup_type_item_kind(type_id).ok_or(AnalyzerError::NonFatal)?;

    let pretty = Pretty::with_config(engine, &checked, PRETTY_CONFIG);
    let value = pretty.render_signature(name, signature).to_string();
    Ok(Some(render_hover(format, Some(value), annotation)))
}

fn render_annotation(source: &str, range: TextRange) -> Option<String> {
    let cleaned = extract::extract_annotation(source, range);
    if cleaned.is_empty() { None } else { Some(cleaned) }
}

fn render_syntax(source: &str, range: TextRange) -> Option<String> {
    Some(extract::extract_syntax(source, range))
}

fn render_hover(format: &MarkupKind, code: Option<String>, annotation: Option<String>) -> Hover {
    let value = match format {
        MarkupKind::Markdown => {
            let code = code.map(|value| {
                let longest_run = value
                    .split(|character| character != '`')
                    .map(str::len)
                    .max()
                    .unwrap_or_default();
                let fence = "`".repeat(longest_run.max(2) + 1);
                format!("{fence}purescript\n{value}\n{fence}")
            });
            [code, annotation].into_iter().flatten().collect::<Vec<_>>().join("\n\n---\n\n")
        }
        MarkupKind::PlainText | MarkupKind::Custom(_) => {
            [code, annotation].into_iter().flatten().collect::<Vec<_>>().join("\n\n")
        }
    };
    Hover {
        contents: Contents::MarkupContent(MarkupContent { kind: format.clone(), value }),
        range: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_hover_keeps_source_backticks_inside_the_code_block() {
        let source = "module Main {-\n```\n````\n-} where".to_string();
        let hover = render_hover(&MarkupKind::Markdown, Some(source.clone()), None);
        let Contents::MarkupContent(markup) = hover.contents else { panic!("expected markup") };
        assert_eq!(markup.kind, MarkupKind::Markdown);
        assert_eq!(markup.value, format!("`````purescript\n{source}\n`````"));
    }
}

fn hover_pun(
    engine: &impl AnalyzerQueries,
    current_file: FileId,
    pun_id: lowering::RecordPunId,
    format: &MarkupKind,
) -> Result<Option<Hover>, AnalyzerError> {
    let checked = engine.checked(current_file)?;

    let pun_type = checked.node_types.lookup_pun(pun_id);
    let pun_type = pun_type.ok_or(AnalyzerError::NonFatal)?;

    hover_checked_type(engine, current_file, pun_type, format)
}
