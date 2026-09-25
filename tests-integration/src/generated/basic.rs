use std::fmt::Write;

use building::QueryEngine;
use checking::core::pretty;
use checking::{PrettyQueries, core};
use files::FileId;
use indexing::{ImportKind, IndexedTermItem, IndexedTypeItem, IndexedTypeItemKind, TypeItemId};
use iris_analysis::position;
use iris_diagnostics::{collect_diagnostics, format_rich_with_path};
use itertools::Itertools;
use line_index::LineIndex;
use lowering::{
    BinderKind, ExpressionKind, GraphNode, ImplicitTypeVariable, TermVariableResolution, TypeKind,
    TypeVariableResolution,
};
use syntax::ast::AstNode;
use syntax::cst;

macro_rules! pos {
    ($positions:expr, $stabilized:expr, $id:expr) => {{
        let cst = $stabilized.ast_ptr($id).unwrap();
        let range = cst.syntax_node_ptr().text_range();
        let p = $positions.offset_to_utf8_position(range.start()).unwrap();
        format!("{}:{}", p.line, p.column)
    }};
}

fn heading(out: &mut String, title: &str) {
    writeln!(out).unwrap();
    writeln!(out, "{title}").unwrap();
}

macro_rules! write_import_items {
    ($out:expr, $title:expr, $iter:expr) => {{
        writeln!($out).unwrap();
        writeln!($out, "{}:", $title).unwrap();
        for (item_name, _, _, kind) in $iter {
            if matches!(kind, ImportKind::Hidden) {
                continue;
            }
            writeln!($out, "  - {item_name} is {kind:?}").unwrap();
        }
    }};
}

pub fn report_resolved(engine: &QueryEngine, id: FileId, name: &str, path: &str) -> String {
    let resolved = engine.resolved(id).unwrap();

    let mut out = String::default();
    writeln!(out, "module {name}").unwrap();

    heading(&mut out, "Unqualified Imports:");
    for import in resolved.unqualified.values().flatten() {
        write_import_items!(out, "Terms", import.iter_terms());
        write_import_items!(out, "Types", import.iter_types());
        write_import_items!(out, "Classes", import.iter_classes());
    }

    heading(&mut out, "Qualified Imports:");
    for (qualifier, imports) in &resolved.qualified {
        for import in imports {
            write_import_items!(out, format!("{qualifier} Terms"), import.iter_terms());
            write_import_items!(out, format!("{qualifier} Types"), import.iter_types());
            write_import_items!(out, format!("{qualifier} Classes"), import.iter_classes());
        }
    }

    heading(&mut out, "Exported Terms:");
    for (name, _, _) in resolved.exports.iter_terms() {
        writeln!(out, "  - {name}").unwrap();
    }

    heading(&mut out, "Exported Types:");
    for (name, _, _) in resolved.exports.iter_types() {
        writeln!(out, "  - {name}").unwrap();
    }

    heading(&mut out, "Exported Classes:");
    for (name, _, _) in resolved.exports.iter_classes() {
        writeln!(out, "  - {name}").unwrap();
    }

    heading(&mut out, "Local Terms:");
    for (name, _, _) in resolved.locals.iter_terms() {
        writeln!(out, "  - {name}").unwrap();
    }

    heading(&mut out, "Local Types:");
    for (name, _, _) in resolved.locals.iter_types() {
        writeln!(out, "  - {name}").unwrap();
    }

    heading(&mut out, "Local Classes:");
    for (name, _, _) in resolved.locals.iter_classes() {
        writeln!(out, "  - {name}").unwrap();
    }

    heading(&mut out, "Class Members:");
    let indexed = engine.indexed(id).unwrap();
    let mut class_member_entries: Vec<_> = resolved.class.iter().collect();
    class_member_entries.sort_by_key(|(class_id, name, _, _)| (class_id.into_raw(), name.as_str()));
    for (class_id, member_name, member_file, _) in class_member_entries {
        let class_name = resolve_class_name(engine, &indexed, id, (member_file, class_id));
        let locality = if member_file == id { "" } else { " (imported)" };
        writeln!(out, "  - {class_name}.{member_name}{locality}").unwrap();
    }

    let mut collected = collect_diagnostics(engine, &[id]).unwrap();
    let collected = collected.pop().unwrap();
    let diagnostics = collected
        .checking_diagnostics()
        .iter()
        .filter(|diagnostic| diagnostic.source == "resolving");
    let diagnostics: Vec<_> = diagnostics.cloned().collect();
    if !diagnostics.is_empty() {
        heading(&mut out, "Diagnostics");
        let line_index = LineIndex::new(&collected.content);
        out.push_str(&format_rich_with_path(
            &diagnostics,
            &collected.content,
            &line_index,
            path,
            false,
        ));
    }

    out
}

pub fn report_lowered(engine: &QueryEngine, id: FileId, name: &str) -> String {
    let content = engine.content(id).unwrap();
    let positions = position::PositionConverter::new(&content, position::PositionEncoding::Utf8);
    let (parsed, _) = engine.parsed(id).unwrap();

    let stabilized = engine.stabilized(id).unwrap();
    let lowered = engine.lowered(id).unwrap();

    let module = parsed.cst();
    let tree = &lowered.tree;
    let graph = &lowered.graph;

    let mut out = String::default();
    writeln!(out, "module {name}").unwrap();

    writeln!(out).unwrap();
    writeln!(out, "Expressions:").unwrap();
    writeln!(out).unwrap();
    for (expression_id, _) in tree.iter_expression() {
        let Some(kind) = tree.get_expression_kind(expression_id) else {
            continue;
        };
        match kind {
            ExpressionKind::Variable { resolution, .. } => {
                write_term_resolution(
                    &positions,
                    &stabilized,
                    &module,
                    tree,
                    &mut out,
                    expression_id,
                    resolution,
                );
            }
            ExpressionKind::Record { record } => {
                for field in record.iter() {
                    if let lowering::ExpressionRecordItem::RecordPun { resolution, .. } = field {
                        write_term_resolution(
                            &positions,
                            &stabilized,
                            &module,
                            tree,
                            &mut out,
                            expression_id,
                            resolution,
                        );
                    }
                }
            }
            ExpressionKind::String { .. }
            | ExpressionKind::Char { .. }
            | ExpressionKind::Integer { .. }
            | ExpressionKind::Number { .. } => write_literal_expression(
                &positions,
                &stabilized,
                &module,
                &mut out,
                expression_id,
                kind,
            ),
            _ => continue,
        }
    }

    let number_binders =
        tree.iter_binder().filter(|(_, kind)| matches!(kind, BinderKind::Number { .. }));
    let mut number_binders = number_binders.peekable();
    if number_binders.peek().is_some() {
        writeln!(out, "\nNumber Binders:\n").unwrap();
        for (binder_id, kind) in number_binders {
            write_number_binder(&positions, &stabilized, &module, &mut out, binder_id, kind);
        }
    }

    writeln!(out, "\nTypes:\n").unwrap();

    for (type_id, _) in tree.iter_type() {
        let Some(TypeKind::Variable { resolution, .. }) = tree.get_type_kind(type_id) else {
            continue;
        };

        let cst = stabilized.ast_ptr(type_id).unwrap();
        let node = cst.syntax_node_ptr().to_node(module.syntax());
        let text = node.text(&content).to_string();

        writeln!(out, "{}@{}", text.trim(), pos!(&positions, &stabilized, type_id)).unwrap();
        match resolution {
            Some(TypeVariableResolution::Forall(id)) => {
                writeln!(out, "  -> forall@{}", pos!(&positions, &stabilized, *id)).unwrap();
            }
            Some(TypeVariableResolution::Implicit(ImplicitTypeVariable { binding, node, id })) => {
                let GraphNode::Implicit { bindings, .. } = &graph[*node] else {
                    writeln!(out, "  did not resolve to constraint variable!").unwrap();
                    continue;
                };
                let (name, type_ids) =
                    bindings.get_index(*id).expect("invariant violated: invalid index");
                if *binding {
                    writeln!(out, "  introduces a constraint variable {name:?}").unwrap();
                } else {
                    writeln!(out, "  -> constraint variable {name:?}").unwrap();
                    for &tid in type_ids {
                        writeln!(out, "    {}", pos!(&positions, &stabilized, tid)).unwrap();
                    }
                }
            }
            None => {
                writeln!(out, "  -> nothing").unwrap();
            }
        }
    }

    out
}

pub fn report_checked(engine: &QueryEngine, id: FileId) -> String {
    let indexed = engine.indexed(id).unwrap();
    let checked = engine.checked(id).unwrap();
    let config = pretty::PrettyConfig::new().fully_qualified_names();
    let pretty = pretty::Pretty::with_config(engine, &checked, config);

    let mut out = String::default();

    writeln!(out, "Terms").unwrap();
    for (id, IndexedTermItem { name, .. }) in indexed.items.iter_terms() {
        let Some(name) = name else { continue };
        let Some(kind) = checked.lookup_term_item_type(id) else { continue };
        let mut state = pretty.state();
        let signature = state.render_signature(name, kind);
        writeln!(out, "{signature}").unwrap();
    }

    writeln!(out, "\nTypes").unwrap();
    for (id, IndexedTypeItem { name, .. }) in indexed.items.iter_types() {
        let Some(name) = name else { continue };
        let Some(kind) = checked.lookup_type_item_kind(id) else { continue };
        let mut state = pretty.state();
        let signature = state.render_signature(name, kind);
        writeln!(out, "{signature}").unwrap();
    }

    if !checked.synonyms.is_empty() {
        writeln!(out, "\nSynonyms").unwrap();
    }
    for (id, IndexedTypeItem { name, .. }) in indexed.items.iter_types() {
        let Some(name) = name else { continue };
        let Some(definition) = checked.lookup_synonym(id) else { continue };
        let mut state = pretty.state();
        let replacement = state.render(definition.expansion);
        let binders =
            definition.parameters.iter().map(|b| state.display_name(b.name)).collect_vec();
        let binders_formatted =
            if binders.is_empty() { String::new() } else { format!(" {}", binders.join(" ")) };
        writeln!(out, "type {name}{binders_formatted} = {replacement}").unwrap();
    }

    if !checked.classes.is_empty() {
        writeln!(out, "\nClasses").unwrap();
    }
    for (id, IndexedTypeItem { .. }) in indexed.items.iter_types() {
        let Some(class) = checked.lookup_class(id) else { continue };
        let mut state = pretty.state();

        let class_binders =
            class.kind_binders.iter().chain(class.type_parameters.iter()).copied().collect_vec();

        let mut class_head = class.canonical;
        while let core::Type::Forall(_, inner) = *engine.lookup_type(class_head) {
            class_head = inner;
        }

        let canonical = state.render(class_head);
        let forall_prefix = if class_binders.is_empty() {
            String::new()
        } else {
            let binders = class_binders
                .iter()
                .map(|&binder_id| {
                    let binder = engine.lookup_forall_binder(binder_id);
                    let text = state.display_name(binder.name);
                    let kind = state.render(binder.kind);
                    format!("({text} :: {kind})")
                })
                .join(" ");
            format!("forall {binders}. ")
        };

        if class.superclasses.is_empty() {
            writeln!(out, "class {forall_prefix}{canonical}").unwrap();
        } else {
            let superclasses = class
                .superclasses
                .iter()
                .map(|superclass| state.render(superclass.constraint))
                .join(", ");
            writeln!(out, "class {forall_prefix}{superclasses} <= {canonical}").unwrap();
        }

        for member in class.members.iter() {
            let member_id = member.item_id;
            let Some(member_name) = indexed.items[member_id].name.as_deref() else { continue };
            let Some(member_type) = checked.lookup_term_item_type(member_id) else { continue };
            let signature = state.render_signature(member_name, member_type);
            writeln!(out, "  {signature}").unwrap();
        }
    }

    if !checked.instances.is_empty() {
        writeln!(out, "\nInstances").unwrap();
    }
    let mut instance_entries: Vec<_> = checked.instances.iter().collect();
    instance_entries.sort_by_key(|(id, _)| *id);
    for (_instance_id, instance) in instance_entries {
        let mut state = pretty.state();
        let canonical = state.render(instance.signature);
        writeln!(out, "instance {canonical}").unwrap();
    }

    if !checked.derived_instances.is_empty() {
        writeln!(out, "\nDerived").unwrap();
    }
    let mut derived_entries: Vec<_> = checked.derived_instances.iter().collect();
    derived_entries.sort_by_key(|(id, _)| *id);
    for (_derive_id, instance) in derived_entries {
        let mut state = pretty.state();
        let canonical = state.render(instance.signature);
        writeln!(out, "derive {canonical}").unwrap();
    }

    if !checked.roles.is_empty() {
        writeln!(out, "\nRoles").unwrap();
    }
    for (id, IndexedTypeItem { name, kind, .. }) in indexed.items.iter_types() {
        let (IndexedTypeItemKind::Data { .. }
        | IndexedTypeItemKind::Newtype { .. }
        | IndexedTypeItemKind::Foreign { .. }) = kind
        else {
            continue;
        };
        let Some(name) = name else { continue };
        let Some(roles) = checked.lookup_roles(id) else { continue };
        let roles_str: Vec<_> = roles.iter().map(|r| format!("{r:?}")).collect();
        writeln!(out, "{name} = [{}]", roles_str.join(", ")).unwrap();
    }

    out
}

fn write_literal_expression(
    positions: &position::PositionConverter<'_>,
    stabilized: &stabilizing::StabilizedModule,
    module: &cst::Module,
    out: &mut String,
    expression_id: lowering::ExpressionId,
    kind: &ExpressionKind,
) {
    let content = positions.content();
    let cst = stabilized.ast_ptr(expression_id).unwrap();
    let node = cst.syntax_node_ptr().to_node(module.syntax());
    let text = node.text(content).to_string();
    let position = positions.offset_to_utf8_position(node.text_range().start()).unwrap();

    writeln!(out, "{}@{}:{}", text.trim(), position.line, position.column).unwrap();

    match kind {
        ExpressionKind::String { kind, value } => match value {
            Some(value) => writeln!(out, "  -> string {kind:?} {value:?}").unwrap(),
            None => writeln!(out, "  -> string {kind:?} (missing)").unwrap(),
        },
        ExpressionKind::Char { value } => match value {
            Some(value) => writeln!(out, "  -> char {value:?}").unwrap(),
            None => writeln!(out, "  -> char (missing)").unwrap(),
        },
        ExpressionKind::Integer { value } => match value {
            Some(value) => writeln!(out, "  -> integer {value}").unwrap(),
            None => writeln!(out, "  -> integer (missing)").unwrap(),
        },
        ExpressionKind::Number { value } => match value {
            Some(value) => writeln!(out, "  -> number {value}").unwrap(),
            None => writeln!(out, "  -> number (missing)").unwrap(),
        },
        _ => unreachable!("invariant violated: expected literal expression"),
    }
}

fn write_number_binder(
    positions: &position::PositionConverter<'_>,
    stabilized: &stabilizing::StabilizedModule,
    module: &cst::Module,
    out: &mut String,
    binder_id: lowering::BinderId,
    kind: &BinderKind,
) {
    let content = positions.content();
    let cst = stabilized.ast_ptr(binder_id).unwrap();
    let node = cst.syntax_node_ptr().to_node(module.syntax());
    let text = node.text(content).to_string();
    let position = positions.offset_to_utf8_position(node.text_range().start()).unwrap();

    writeln!(out, "{}@{}:{}", text.trim(), position.line, position.column).unwrap();

    let BinderKind::Number { negative, value } = kind else {
        unreachable!("invariant violated: expected number binder")
    };
    match (negative, value) {
        (true, Some(value)) => writeln!(out, "  -> number -{value}").unwrap(),
        (false, Some(value)) => writeln!(out, "  -> number {value}").unwrap(),
        (_, None) => writeln!(out, "  -> number (missing)").unwrap(),
    }
}

fn write_term_resolution(
    positions: &position::PositionConverter<'_>,
    stabilized: &stabilizing::StabilizedModule,
    module: &cst::Module,
    tree: &lowering::LoweredTree,
    out: &mut String,
    expression_id: lowering::ExpressionId,
    resolution: &Option<TermVariableResolution>,
) {
    let content = positions.content();
    let cst = stabilized.ast_ptr(expression_id).unwrap();
    let node = cst.syntax_node_ptr().to_node(module.syntax());
    let text = node.text(content).to_string();
    let position = positions.offset_to_utf8_position(node.text_range().start()).unwrap();

    writeln!(out, "{}@{}:{}", text.trim(), position.line, position.column).unwrap();

    match resolution {
        Some(TermVariableResolution::Binder(id)) => {
            writeln!(out, "  -> binder@{}", pos!(positions, stabilized, *id)).unwrap();
        }
        Some(TermVariableResolution::Let(let_binding_id)) => {
            let let_binding = tree.get_let_binding_group(*let_binding_id);
            if let Some(sig) = let_binding.signature {
                writeln!(out, "  -> signature@{}", pos!(positions, stabilized, sig)).unwrap();
            }
            for eq in let_binding.equations.iter() {
                writeln!(out, "  -> equation@{}", pos!(positions, stabilized, *eq)).unwrap();
            }
        }
        Some(TermVariableResolution::RecordPun(id)) => {
            writeln!(out, "  -> record pun@{}", pos!(positions, stabilized, *id)).unwrap();
        }
        Some(TermVariableResolution::Reference(..)) => {
            writeln!(out, "  -> top-level").unwrap();
        }
        None => {
            writeln!(out, "  -> nothing").unwrap();
        }
    }
}

fn resolve_class_name(
    engine: &QueryEngine,
    indexed: &indexing::IndexedModule,
    current_file: FileId,
    resolution: (FileId, TypeItemId),
) -> String {
    let (class_file, class_type_id) = resolution;
    if class_file == current_file {
        indexed.items[class_type_id].name.as_deref().unwrap_or("<unknown>").to_string()
    } else {
        engine
            .indexed(class_file)
            .ok()
            .and_then(|idx| idx.items[class_type_id].name.as_deref().map(str::to_string))
            .unwrap_or_else(|| "<imported>".to_string())
    }
}
