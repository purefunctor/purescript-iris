pub mod application;
pub mod collections;
pub mod equations;
pub mod form_ado;
pub mod form_do;
pub mod form_let;
pub mod forms;
pub mod guarded;

use std::sync::Arc;

use building_types::QueryResult;
use files::FileId;
use indexing::{ImportKind, TermItemId};
use itertools::Itertools;
use lowering::GraphNode;
use rustc_hash::FxHashSet;
use smol_str::SmolStr;

use crate::context::CheckContext;
use crate::core::substitute::SubstituteName;
use crate::core::{Type, TypeId, normalise, toolkit, unification};
use crate::error::{ErrorCrumb, ErrorKind};
use crate::holes::{HoleBinding, TermHole};
use crate::source::{operator, types};
use crate::state::CheckState;
use crate::{ExternalQueries, tree};

#[derive(Copy, Clone, Debug)]
pub struct ElaboratedExpression {
    pub type_id: TypeId,
    pub expression: tree::ExpressionId,
}

pub(super) fn allocate_expression(
    state: &mut CheckState,
    type_id: TypeId,
    kind: tree::ExpressionKind,
) -> ElaboratedExpression {
    let expression = state.allocate_expression(type_id, kind);
    ElaboratedExpression { type_id, expression }
}

fn allocate_error_expression(state: &mut CheckState, type_id: TypeId) -> ElaboratedExpression {
    let expression = state.allocate_error_expression(type_id);
    ElaboratedExpression { type_id, expression }
}

fn retain_expression_judgment(
    state: &mut CheckState,
    expression: ElaboratedExpression,
) -> ElaboratedExpression {
    // Only source expression roots correspond to PureScript's `TypedValue`
    // judgments. Generated elaboration nodes can cache transient unification
    // solutions and must not become independent judgments.
    state.retain_expression_judgment(expression.expression);
    expression
}

pub(super) fn allocate_term_reference<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    file_id: FileId,
    term_id: TermItemId,
    type_id: TypeId,
) -> QueryResult<ElaboratedExpression>
where
    Q: ExternalQueries,
{
    let is_constructor = if file_id == context.id {
        let item = context.lowered.tree.get_term_item_kind(term_id);
        matches!(item, Some(lowering::TermItemKind::Constructor { .. }))
    } else {
        let lowered = context.queries.lowered(file_id)?;
        let item = lowered.tree.get_term_item_kind(term_id);
        matches!(item, Some(lowering::TermItemKind::Constructor { .. }))
    };

    let kind = if is_constructor {
        tree::ExpressionKind::Constructor { resolution: (file_id, term_id) }
    } else {
        let resolution = lowering::TermVariableResolution::Reference(file_id, term_id);
        let resolution = tree::VariableResolution::Source(resolution);
        tree::ExpressionKind::Variable { resolution }
    };
    Ok(allocate_expression(state, type_id, kind))
}

/// Checks the type of an expression.
pub fn check_expression<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    expression: lowering::ExpressionId,
    expected: TypeId,
) -> QueryResult<ElaboratedExpression>
where
    Q: ExternalQueries,
{
    state.with_error_crumb(ErrorCrumb::CheckingExpression(expression), |state| {
        let checked = check_expression_quiet(state, context, expression, expected)?;
        let checked = retain_expression_judgment(state, checked);
        state.checked.node_types.expressions.insert(expression, checked.type_id);
        Ok(checked)
    })
}

fn check_expression_quiet<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    expression: lowering::ExpressionId,
    expected: TypeId,
) -> QueryResult<ElaboratedExpression>
where
    Q: ExternalQueries,
{
    let expected = normalise::normalise(state, context, expected);
    check_expected_expression(state, context, expected, |state, expected| {
        if let Some(section_result) = context.sectioned.expressions.get(&expression) {
            check_sectioned_expression(state, context, expression, section_result, expected)
        } else {
            check_expression_core(state, context, expression, expected)
        }
    })
}

pub(super) fn check_elaborated_expression<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    inferred: ElaboratedExpression,
    expected: TypeId,
) -> QueryResult<ElaboratedExpression>
where
    Q: ExternalQueries,
{
    let expected = normalise::normalise(state, context, expected);
    let checked = check_expected_expression(state, context, expected, |state, expected| {
        check_elaborated_expression_quiet(state, context, inferred, expected)
    })?;
    Ok(retain_expression_judgment(state, checked))
}

fn check_expected_expression<Q, F>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    expected: TypeId,
    check: F,
) -> QueryResult<ElaboratedExpression>
where
    Q: ExternalQueries,
    F: FnOnce(&mut CheckState, TypeId) -> QueryResult<ElaboratedExpression>,
{
    let expected = normalise::expand(state, context, expected)?;
    match *context.lookup_type(expected) {
        Type::Forall(binder_id, inner) => {
            let binder = context.lookup_forall_binder(binder_id);

            let kind = normalise::normalise(state, context, binder.kind);
            let text = state.checked.lookup_name(binder.name);
            let (rigid, name, scope) = state.fresh_scoped_rigid_named(context.queries, kind, text);

            let inner = SubstituteName::one(state, context, binder.name, rigid, inner)?;
            let checked = check_expected_expression(state, context, inner, check)?;
            let binder = crate::core::ForallBinder { name, scope: Some(scope), ..binder };
            let binder = context.intern_forall_binder(binder);
            let inner = state.checked.tree[checked.expression].type_id;
            let scoped = context.intern_forall(binder, inner);
            state.checked.tree.set_expression_type(checked.expression, scoped);
            Ok(ElaboratedExpression { type_id: expected, expression: checked.expression })
        }
        Type::Constrained(constraint, constrained) => state.with_implication(|state| {
            let binder = state.push_given(constraint);
            let checked = check_expected_expression(state, context, constrained, check)?;

            let kind = tree::ExpressionKind::EvidenceAbstraction {
                binder,
                expression: checked.expression,
            };

            Ok(allocate_expression(state, expected, kind))
        }),
        _ => check(state, expected),
    }
}

fn check_elaborated_expression_quiet<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    inferred: ElaboratedExpression,
    expected: TypeId,
) -> QueryResult<ElaboratedExpression>
where
    Q: ExternalQueries,
{
    let inferred = application::instantiate_expression(state, context, inferred)?;
    unification::subtype(state, context, inferred.type_id, expected)?;
    Ok(inferred)
}

fn check_sectioned_expression<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    expression: lowering::ExpressionId,
    sections: &[lowering::ExpressionId],
    expected: TypeId,
) -> QueryResult<ElaboratedExpression>
where
    Q: ExternalQueries,
{
    let mut current = expected;
    let mut parameters = vec![];
    let mut binders = vec![];

    for &section_id in sections {
        if !parameters.is_empty() {
            let expanded = normalise::expand(state, context, current)?;
            let requires_abstraction = matches!(
                *context.lookup_type(expanded),
                Type::Forall(_, _) | Type::Constrained(_, _)
            );
            if requires_abstraction {
                break;
            }
        }

        let decomposed = toolkit::decompose_function(state, context, current)?;
        let parameter = if let Some((argument_type, result_type)) = decomposed {
            current = result_type;
            argument_type
        } else {
            let parameter = state.fresh_unification(context.queries, context.prim.t);
            let result = state.fresh_unification(context.queries, context.prim.t);

            let function = context.intern_function(parameter, result);
            if unification::subtype(state, context, function, current)? {
                current = result;
            } else {
                unification::unify(state, context, result, current)?;
            }

            parameter
        };
        let binder = state.allocate_section_binder(section_id, parameter);
        parameters.push(parameter);
        binders.push(binder);
    }

    let (_, remaining_sections) = sections.split_at(parameters.len());
    let ElaboratedExpression { type_id, expression } = if remaining_sections.is_empty() {
        check_expected_expression(state, context, current, |state, expected| {
            let inferred = infer_expression_core(state, context, expression)?;
            let inferred = application::instantiate_expression(state, context, inferred)?;
            unification::subtype(state, context, inferred.type_id, expected)?;
            Ok(inferred)
        })?
    } else {
        check_expected_expression(state, context, current, |state, expected| {
            check_sectioned_expression(state, context, expression, remaining_sections, expected)
        })?
    };

    let function_type = context.intern_function_list(&parameters, type_id);
    let kind = tree::ExpressionKind::Lambda { binders: Arc::from(binders), expression };
    Ok(allocate_expression(state, function_type, kind))
}

fn check_expression_core<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    expression: lowering::ExpressionId,
    expected: TypeId,
) -> QueryResult<ElaboratedExpression>
where
    Q: ExternalQueries,
{
    let unknown = || context.unknown("missing expression");

    let Some(kind) = context.lowered.tree.get_expression_kind(expression) else {
        return Ok(allocate_error_expression(state, unknown()));
    };

    match kind {
        lowering::ExpressionKind::Lambda { binders, expression } => {
            forms::check_lambda(state, context, binders, *expression, expected)
        }
        lowering::ExpressionKind::IfThenElse { if_, then, else_ } => {
            forms::check_if_then_else(state, context, *if_, *then, *else_, expected)
        }
        lowering::ExpressionKind::CaseOf { trunk, branches } => {
            forms::check_case_of(state, context, trunk, branches, expected)
        }
        lowering::ExpressionKind::OperatorChain { .. } => {
            let (checked, checked_type) =
                operator::check_operator_chain(state, context, expression, expected)?;
            Ok(checked.unwrap_or_else(|| allocate_error_expression(state, checked_type)))
        }
        lowering::ExpressionKind::LetIn { bindings, expression } => {
            form_let::check_let_in(state, context, bindings, *expression, expected)
        }
        lowering::ExpressionKind::Parenthesized { parenthesized } => {
            let Some(parenthesized) = parenthesized else {
                return Ok(allocate_error_expression(state, unknown()));
            };
            check_expression(state, context, *parenthesized, expected)
        }
        lowering::ExpressionKind::Array { array } => {
            collections::check_array(state, context, array, expected)
        }
        lowering::ExpressionKind::Record { record } => {
            collections::check_record(state, context, record, expected)
        }
        _ => {
            let inferred = infer_expression_quiet(state, context, expression)?;
            check_elaborated_expression_quiet(state, context, inferred, expected)
        }
    }
}

/// Infers the type of an expression.
pub fn infer_expression<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    expression: lowering::ExpressionId,
) -> QueryResult<ElaboratedExpression>
where
    Q: ExternalQueries,
{
    state.with_error_crumb(ErrorCrumb::InferringExpression(expression), |state| {
        let inferred = infer_expression_quiet(state, context, expression)?;
        let inferred = retain_expression_judgment(state, inferred);
        state.checked.node_types.expressions.insert(expression, inferred.type_id);
        Ok(inferred)
    })
}

fn infer_expression_quiet<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    expression: lowering::ExpressionId,
) -> QueryResult<ElaboratedExpression>
where
    Q: ExternalQueries,
{
    if let Some(section_result) = context.sectioned.expressions.get(&expression) {
        infer_sectioned_expression(state, context, expression, section_result)
    } else {
        infer_expression_core(state, context, expression)
    }
}

fn infer_sectioned_expression<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    expression: lowering::ExpressionId,
    section_result: &sugar::SectionResult,
) -> QueryResult<ElaboratedExpression>
where
    Q: ExternalQueries,
{
    let parameters = section_result.iter().map(|&section_id| {
        let parameter = state.fresh_unification(context.queries, context.prim.t);
        let binder = state.allocate_section_binder(section_id, parameter);
        (parameter, binder)
    });

    let (parameters, binders): (Vec<_>, Vec<_>) = parameters.unzip();

    let result = infer_expression_core(state, context, expression)?;
    let ElaboratedExpression { type_id, expression } =
        application::instantiate_expression(state, context, result)?;

    let function_type = context.intern_function_list(&parameters, type_id);
    let kind = tree::ExpressionKind::Lambda { binders: Arc::from(binders), expression };
    Ok(allocate_expression(state, function_type, kind))
}

fn infer_expression_core<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    expression: lowering::ExpressionId,
) -> QueryResult<ElaboratedExpression>
where
    Q: ExternalQueries,
{
    let unknown = || context.unknown("missing expression");

    let Some(kind) = context.lowered.tree.get_expression_kind(expression) else {
        return Ok(allocate_error_expression(state, unknown()));
    };

    match kind {
        lowering::ExpressionKind::Typed { expression, type_ } => {
            let Some(e) = expression else {
                return Ok(allocate_error_expression(state, unknown()));
            };
            let Some(t) = type_ else {
                return Ok(allocate_error_expression(state, unknown()));
            };

            let (t, _) = types::infer_kind(state, context, *t)?;
            let ElaboratedExpression { expression, .. } = check_expression(state, context, *e, t)?;
            Ok(ElaboratedExpression { type_id: t, expression })
        }

        lowering::ExpressionKind::OperatorChain { .. } => {
            let (inferred, inferred_type) =
                operator::infer_operator_chain(state, context, expression)?;
            Ok(inferred.unwrap_or_else(|| allocate_error_expression(state, inferred_type)))
        }

        lowering::ExpressionKind::InfixChain { head, tail } => {
            let Some(head) = *head else {
                return Ok(allocate_error_expression(state, unknown()));
            };
            application::infer_infix_chain(state, context, head, tail)
        }

        lowering::ExpressionKind::Negate { negate, expression } => {
            let Some(negate) = negate else {
                return Ok(allocate_error_expression(state, unknown()));
            };
            let Some(expression) = expression else {
                return Ok(allocate_error_expression(state, unknown()));
            };

            let negate_type = toolkit::lookup_term_variable(state, context, *negate)?;
            let resolution = tree::VariableResolution::Source(*negate);
            let kind = tree::ExpressionKind::Variable { resolution };
            let negate = allocate_expression(state, negate_type, kind);
            let Some(application::UnanchoredApplication { implicit, argument, result }) =
                application::check_unanchored_application(state, context, negate_type)?
            else {
                return Ok(allocate_error_expression(state, unknown()));
            };
            let operand = check_expression(state, context, *expression, argument)?;
            Ok(application::materialize_application(state, negate, implicit, result, operand))
        }

        lowering::ExpressionKind::Application { function, arguments } => {
            let Some(function) = function else {
                return Ok(allocate_error_expression(state, unknown()));
            };

            let function = infer_expression(state, context, *function)?;
            application::check_expression_application(state, context, function, arguments)
        }

        lowering::ExpressionKind::IfThenElse { if_, then, else_ } => {
            forms::infer_if_then_else(state, context, *if_, *then, *else_)
        }

        lowering::ExpressionKind::LetIn { bindings, expression } => {
            form_let::infer_let_in(state, context, bindings, *expression)
        }

        lowering::ExpressionKind::Lambda { binders, expression } => {
            forms::infer_lambda(state, context, binders, *expression)
        }

        lowering::ExpressionKind::CaseOf { trunk, branches } => {
            forms::infer_case_of(state, context, trunk, branches)
        }

        lowering::ExpressionKind::Do { bind, discard, statements } => {
            form_do::infer_do(state, context, *bind, *discard, statements)
        }

        lowering::ExpressionKind::Ado { map, apply, pure, statements, expression } => {
            form_ado::infer_ado(state, context, *map, *apply, *pure, statements, *expression)
        }

        lowering::ExpressionKind::Constructor { resolution } => {
            let Some((file_id, term_id)) = resolution else {
                return Ok(allocate_error_expression(state, unknown()));
            };
            let type_id = toolkit::lookup_file_term(state, context, *file_id, *term_id)?;
            let kind = tree::ExpressionKind::Constructor { resolution: (*file_id, *term_id) };
            Ok(allocate_expression(state, type_id, kind))
        }

        lowering::ExpressionKind::Variable { resolution } => {
            let Some(resolution) = *resolution else {
                return Ok(allocate_error_expression(state, unknown()));
            };
            let type_id = toolkit::lookup_term_variable(state, context, resolution)?;
            let resolution = tree::VariableResolution::Source(resolution);
            let kind = tree::ExpressionKind::Variable { resolution };
            Ok(allocate_expression(state, type_id, kind))
        }

        lowering::ExpressionKind::OperatorName { resolution } => {
            let Some((file_id, term_id)) = resolution else {
                return Ok(allocate_error_expression(state, unknown()));
            };
            let type_id = toolkit::lookup_file_term(state, context, *file_id, *term_id)?;
            let Some((target_file_id, target_term_id)) =
                toolkit::resolve_term_operator_target(context, *file_id, *term_id)?
            else {
                return Ok(allocate_error_expression(state, type_id));
            };
            allocate_term_reference(state, context, target_file_id, target_term_id, type_id)
        }

        lowering::ExpressionKind::Section => {
            let type_id = state.checked.node_types.lookup_section(expression);
            let binder = state.checked.tree.lookup_section_binder(expression);
            match (type_id, binder) {
                (Some(type_id), Some(binder)) => {
                    let kind = tree::ExpressionKind::Section { binder };
                    Ok(allocate_expression(state, type_id, kind))
                }
                (None, None) => Ok(allocate_error_expression(state, unknown())),
                _ => {
                    unreachable!("invariant violated: incomplete checked section provenance")
                }
            }
        }

        lowering::ExpressionKind::Hole => {
            let kind = state.fresh_unification(context.queries, context.prim.t);
            let type_id = state.fresh_unification(context.queries, kind);

            let bindings = term_hole_bindings(state, context, expression)?;
            state.checked.holes.terms.insert(expression, TermHole { type_id, bindings });
            state.insert_error(ErrorKind::TermHole { source_term: expression });

            Ok(allocate_error_expression(state, type_id))
        }

        lowering::ExpressionKind::String { kind, value } => {
            let Some(value) = value else {
                return Ok(allocate_error_expression(state, context.prim.string));
            };
            let kind = tree::ExpressionKind::String { kind: *kind, value: value.clone() };
            Ok(allocate_expression(state, context.prim.string, kind))
        }

        lowering::ExpressionKind::Char { value } => {
            let Some(value) = value else {
                return Ok(allocate_error_expression(state, context.prim.char));
            };
            let kind = tree::ExpressionKind::Char { value: *value };
            Ok(allocate_expression(state, context.prim.char, kind))
        }

        lowering::ExpressionKind::Boolean { boolean } => {
            let kind = tree::ExpressionKind::Boolean { value: *boolean };
            Ok(allocate_expression(state, context.prim.boolean, kind))
        }

        lowering::ExpressionKind::Integer { value } => {
            let Some(value) = value else {
                return Ok(allocate_error_expression(state, context.prim.int));
            };
            let kind = tree::ExpressionKind::Integer { value: *value };
            Ok(allocate_expression(state, context.prim.int, kind))
        }

        lowering::ExpressionKind::Number { value } => {
            let Some(value) = value else {
                return Ok(allocate_error_expression(state, context.prim.number));
            };
            let kind = tree::ExpressionKind::Number { value: SmolStr::clone(value) };
            Ok(allocate_expression(state, context.prim.number, kind))
        }

        lowering::ExpressionKind::Array { array } => {
            collections::infer_array(state, context, array)
        }

        lowering::ExpressionKind::Record { record } => {
            collections::infer_record(state, context, record)
        }

        lowering::ExpressionKind::Parenthesized { parenthesized } => {
            let Some(parenthesized) = parenthesized else {
                return Ok(allocate_error_expression(state, unknown()));
            };
            infer_expression(state, context, *parenthesized)
        }

        lowering::ExpressionKind::RecordAccess { record, labels } => {
            let Some(record) = *record else {
                return Ok(allocate_error_expression(state, unknown()));
            };
            let Some(labels) = labels else {
                return Ok(allocate_error_expression(state, unknown()));
            };
            collections::infer_record_access(state, context, record, labels)
        }

        lowering::ExpressionKind::RecordUpdate { record, updates } => {
            let Some(record) = *record else {
                return Ok(allocate_error_expression(state, unknown()));
            };
            collections::infer_record_update(state, context, record, updates)
        }
    }
}

fn term_hole_bindings<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    expression: lowering::ExpressionId,
) -> QueryResult<Arc<[HoleBinding]>>
where
    Q: ExternalQueries,
{
    let mut seen = FxHashSet::default();
    let mut result = vec![];

    if let Some(scope_node) = context.lowered.nodes.expression_node(expression) {
        collect_graph_term_hole_bindings(state, context, scope_node, &mut seen, &mut result);
    }

    collect_resolved_term_hole_bindings(state, context, &mut seen, &mut result)?;
    Ok(Arc::from(result))
}

fn collect_graph_term_hole_bindings<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    scope_node: lowering::GraphNodeId,
    seen: &mut FxHashSet<SmolStr>,
    result: &mut Vec<HoleBinding>,
) where
    Q: ExternalQueries,
{
    for (_, node) in context.lowered.graph.traverse(scope_node) {
        match node {
            GraphNode::Binder { binders, puns, .. } => {
                let mut binders = binders.iter().collect_vec();
                binders.sort_by_key(|(name, _)| *name);

                for (name, binder_id) in binders {
                    if seen.insert(SmolStr::clone(name))
                        && let Some(type_id) = state.checked.node_types.lookup_binder(*binder_id)
                    {
                        let name = SmolStr::clone(name);
                        result.push(HoleBinding { name, type_id });
                    }
                }

                let mut puns = puns.iter().collect_vec();
                puns.sort_by_key(|(name, _)| *name);

                for (name, pun_id) in puns {
                    if seen.insert(SmolStr::clone(name))
                        && let Some(type_id) = state.checked.node_types.lookup_pun(*pun_id)
                    {
                        let name = SmolStr::clone(name);
                        result.push(HoleBinding { name, type_id });
                    }
                }
            }
            GraphNode::Let { bindings, .. } => {
                let mut bindings = bindings.iter().collect_vec();
                bindings.sort_by_key(|(name, _)| *name);

                for (name, let_id) in bindings {
                    if seen.insert(SmolStr::clone(name))
                        && let Some(type_id) = state.checked.node_types.lookup_let(*let_id)
                    {
                        let name = SmolStr::clone(name);
                        result.push(HoleBinding { name, type_id });
                    }
                }
            }
            GraphNode::Forall { .. } | GraphNode::Implicit { .. } => {}
        }
    }
}

fn collect_resolved_term_hole_bindings<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    seen: &mut FxHashSet<SmolStr>,
    result: &mut Vec<HoleBinding>,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    for (name, file_id, term_id) in context.resolved.locals.iter_terms() {
        collect_resolved_term_hole_binding(state, context, name, file_id, term_id, seen, result)?;
    }

    for import in context.resolved.unqualified.values().flatten() {
        let visible_terms =
            import.iter_terms().filter(|(_, _, _, kind)| !matches!(kind, ImportKind::Hidden));

        for (name, file_id, term_id, _) in visible_terms {
            collect_resolved_term_hole_binding(
                state, context, name, file_id, term_id, seen, result,
            )?;
        }
    }

    if !context.resolved.unqualified.contains_key("Prim") {
        for (name, file_id, term_id) in context.prim_resolved.exports.iter_terms() {
            collect_resolved_term_hole_binding(
                state, context, name, file_id, term_id, seen, result,
            )?;
        }
    }

    Ok(())
}

fn collect_resolved_term_hole_binding<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    name: &SmolStr,
    file_id: FileId,
    term_id: TermItemId,
    seen: &mut FxHashSet<SmolStr>,
    result: &mut Vec<HoleBinding>,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    if seen.contains(name) {
        return Ok(());
    }

    let Some((resolved_file_id, resolved_term_id)) =
        context.resolved.lookup_term(&context.prim_resolved, None, name)
    else {
        return Ok(());
    };

    if (resolved_file_id, resolved_term_id) != (file_id, term_id) {
        return Ok(());
    }

    let type_id = toolkit::lookup_file_term(state, context, resolved_file_id, resolved_term_id)?;
    let name = SmolStr::clone(name);
    seen.insert(SmolStr::clone(&name));
    result.push(HoleBinding { name, type_id });

    Ok(())
}
