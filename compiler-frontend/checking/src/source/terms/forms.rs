use building_types::QueryResult;

use crate::context::CheckContext;
use crate::core::{Type, TypeId, exhaustive, normalise, toolkit, unification};
use crate::source::binder;
use crate::source::terms::{ElaboratedExpression, application, guarded};
use crate::state::CheckState;
use crate::{ExternalQueries, tree};

#[derive(Copy, Clone, Debug)]
enum IfThenElseMode {
    Infer,
    Check { expected: TypeId },
}

#[derive(Copy, Clone, Debug)]
enum CaseOfMode {
    Infer,
    Check { expected: TypeId },
}

pub fn infer_if_then_else<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    if_: Option<lowering::ExpressionId>,
    then: Option<lowering::ExpressionId>,
    else_: Option<lowering::ExpressionId>,
) -> QueryResult<ElaboratedExpression>
where
    Q: ExternalQueries,
{
    if_then_else_core(state, context, if_, then, else_, IfThenElseMode::Infer)
}

pub fn check_if_then_else<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    if_: Option<lowering::ExpressionId>,
    then: Option<lowering::ExpressionId>,
    else_: Option<lowering::ExpressionId>,
    expected: TypeId,
) -> QueryResult<ElaboratedExpression>
where
    Q: ExternalQueries,
{
    if_then_else_core(state, context, if_, then, else_, IfThenElseMode::Check { expected })
}

fn if_then_else_core<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    if_: Option<lowering::ExpressionId>,
    then: Option<lowering::ExpressionId>,
    else_: Option<lowering::ExpressionId>,
    mode: IfThenElseMode,
) -> QueryResult<ElaboratedExpression>
where
    Q: ExternalQueries,
{
    let condition = if let Some(condition) = if_ {
        super::check_expression(state, context, condition, context.prim.boolean)?.expression
    } else {
        state.allocate_error_expression(context.prim.boolean)
    };

    let result_type = match mode {
        IfThenElseMode::Infer => state.fresh_unification(context.queries, context.prim.t),
        IfThenElseMode::Check { expected } => expected,
    };

    let then = if let Some(then) = then {
        super::check_expression(state, context, then, result_type)?.expression
    } else {
        state.allocate_error_expression(result_type)
    };

    let else_ = if let Some(else_) = else_ {
        super::check_expression(state, context, else_, result_type)?.expression
    } else {
        state.allocate_error_expression(result_type)
    };

    let kind = tree::ExpressionKind::IfThenElse { condition, then, else_ };
    Ok(super::allocate_expression(state, result_type, kind))
}

pub fn infer_lambda<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    binders: &[lowering::BinderId],
    expression: Option<lowering::ExpressionId>,
) -> QueryResult<ElaboratedExpression>
where
    Q: ExternalQueries,
{
    let mut argument_types = Vec::new();
    let mut checked_binders = Vec::new();

    for &binder_id in binders.iter() {
        let argument_type = state.fresh_unification(context.queries, context.prim.t);
        let checked_binder = binder::check_binder(state, context, binder_id, argument_type)?;
        argument_types.push(argument_type);
        checked_binders.push(checked_binder.binder);
    }

    let body = if let Some(body) = expression {
        let body = super::infer_expression(state, context, body)?;
        application::instantiate_expression(state, context, body)?
    } else {
        let type_id = state.fresh_unification(context.queries, context.prim.t);
        let expression = state.allocate_error_expression(type_id);
        ElaboratedExpression { type_id, expression }
    };

    let function_type = context.intern_function_list(&argument_types, body.type_id);

    let exhaustiveness =
        exhaustive::check_lambda_patterns(state, context, &argument_types, binders)?;

    let has_missing = exhaustiveness.missing.is_some();
    state.report_exhaustiveness(exhaustiveness);

    let kind = tree::ExpressionKind::Lambda {
        binders: checked_binders.into(),
        expression: body.expression,
    };

    if has_missing {
        let type_id = context.intern_constrained(context.prim.partial, function_type);
        let expression = state.allocate_expression(function_type, kind);
        let binder = state.checked.evidence.fresh_binder(context.prim.partial);
        let kind = tree::ExpressionKind::EvidenceAbstraction { binder, expression };
        Ok(super::allocate_expression(state, type_id, kind))
    } else {
        Ok(super::allocate_expression(state, function_type, kind))
    }
}

pub fn check_lambda<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    binders: &[lowering::BinderId],
    expression: Option<lowering::ExpressionId>,
    expected: TypeId,
) -> QueryResult<ElaboratedExpression>
where
    Q: ExternalQueries,
{
    let mut arguments = Vec::new();
    let mut checked_binders = Vec::new();
    let mut remaining = expected;

    for &binder_id in binders.iter() {
        if !arguments.is_empty() {
            let expanded = normalise::expand(state, context, remaining)?;
            let requires_abstraction = matches!(
                *context.lookup_type(expanded),
                Type::Forall(_, _) | Type::Constrained(_, _)
            );
            if requires_abstraction {
                break;
            }
        }

        let decomposed = toolkit::decompose_function(state, context, remaining)?;
        let argument = if let Some((argument, result)) = decomposed {
            let argument = if binder::requires_instantiation(context, binder_id) {
                toolkit::instantiate_unifications(state, context, argument)?
            } else {
                argument
            };
            remaining = result;
            argument
        } else {
            let argument = state.fresh_unification(context.queries, context.prim.t);
            let result = state.fresh_unification(context.queries, context.prim.t);
            let function = context.intern_function(argument, result);

            unification::subtype(state, context, function, remaining)?;
            remaining = result;
            argument
        };
        let checked_binder = binder::check_binder(state, context, binder_id, argument)?;
        arguments.push(argument);
        checked_binders.push(checked_binder.binder);
    }

    let (current_binders, remaining_binders) = binders.split_at(arguments.len());
    let body = if !remaining_binders.is_empty() {
        super::check_expected_expression(state, context, remaining, |state, expected| {
            check_lambda(state, context, remaining_binders, expression, expected)
        })?
    } else if let Some(body) = expression {
        super::check_expression(state, context, body, remaining)?
    } else {
        let type_id = state.fresh_unification(context.queries, context.prim.t);
        let expression = state.allocate_error_expression(type_id);
        ElaboratedExpression { type_id, expression }
    };

    let function_type = context.intern_function_list(&arguments, body.type_id);

    let exhaustiveness =
        exhaustive::check_lambda_patterns(state, context, &arguments, current_binders)?;

    let has_missing = exhaustiveness.missing.is_some();
    state.report_exhaustiveness(exhaustiveness);

    let kind = tree::ExpressionKind::Lambda {
        binders: checked_binders.into(),
        expression: body.expression,
    };

    if has_missing {
        state.push_wanted(context.prim.partial);
        Ok(super::allocate_expression(state, function_type, kind))
    } else {
        Ok(super::allocate_expression(state, function_type, kind))
    }
}

pub fn instantiate_trunk_types<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    trunk_types: &mut [TypeId],
    branches: &[lowering::CaseBranch],
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    binder::instantiate_pattern_column_types(
        state,
        context,
        trunk_types,
        branches.iter().flat_map(|branch| branch.binders.iter().copied().enumerate()),
    )
}

pub fn infer_case_of<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    trunk: &[lowering::ExpressionId],
    branches: &[lowering::CaseBranch],
) -> QueryResult<ElaboratedExpression>
where
    Q: ExternalQueries,
{
    case_of_core(state, context, trunk, branches, CaseOfMode::Infer)
}

pub fn check_case_of<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    trunk: &[lowering::ExpressionId],
    branches: &[lowering::CaseBranch],
    expected: TypeId,
) -> QueryResult<ElaboratedExpression>
where
    Q: ExternalQueries,
{
    case_of_core(state, context, trunk, branches, CaseOfMode::Check { expected })
}

fn case_of_core<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    trunk: &[lowering::ExpressionId],
    branches: &[lowering::CaseBranch],
    mode: CaseOfMode,
) -> QueryResult<ElaboratedExpression>
where
    Q: ExternalQueries,
{
    let expected = match mode {
        CaseOfMode::Infer => state.fresh_unification(context.queries, context.prim.t),
        CaseOfMode::Check { expected } => expected,
    };

    let mut scrutinees = Vec::new();
    let mut trunk_types = Vec::new();
    for &scrutinee_id in trunk.iter() {
        let scrutinee = super::infer_expression(state, context, scrutinee_id)?;
        let scrutinee = application::instantiate_expression(state, context, scrutinee)?;
        trunk_types.push(scrutinee.type_id);
        scrutinees.push(scrutinee.expression);
    }

    instantiate_trunk_types(state, context, &mut trunk_types, branches)?;

    let mut alternatives = Vec::new();
    for branch in branches.iter() {
        let mut binders = Vec::new();
        for (&binder_id, &trunk_type) in branch.binders.iter().zip(&trunk_types) {
            let checked_binder = binder::check_binder(state, context, binder_id, trunk_type)?;
            binders.push(checked_binder.binder);
        }

        let guarded_expression = if let Some(guarded_source) = &branch.guarded_expression {
            match mode {
                CaseOfMode::Infer => {
                    guarded::subsume_guarded_expression(state, context, guarded_source, expected)?
                        .guarded_expression
                }
                CaseOfMode::Check { .. } => {
                    guarded::check_guarded_expression(state, context, guarded_source, expected)?
                        .guarded_expression
                }
            }
        } else {
            let expression = state.allocate_error_expression(expected);
            let where_expression = tree::WhereExpression::new(expression);
            tree::GuardedExpression::unconditional(where_expression)
        };
        alternatives.push(tree::CaseAlternative { binders: binders.into(), guarded_expression });
    }

    let exhaustiveness = exhaustive::check_case_patterns(state, context, &trunk_types, branches)?;

    let has_missing = exhaustiveness.missing.is_some();
    state.report_exhaustiveness(exhaustiveness);

    let kind = tree::ExpressionKind::Case {
        scrutinees: scrutinees.into(),
        alternatives: alternatives.into(),
    };

    if has_missing {
        if let CaseOfMode::Infer = mode {
            let result_type = context.intern_constrained(context.prim.partial, expected);
            let expression = state.allocate_expression(expected, kind);
            let binder = state.checked.evidence.fresh_binder(context.prim.partial);
            let kind = tree::ExpressionKind::EvidenceAbstraction { binder, expression };
            Ok(super::allocate_expression(state, result_type, kind))
        } else {
            state.push_wanted(context.prim.partial);
            Ok(super::allocate_expression(state, expected, kind))
        }
    } else {
        Ok(super::allocate_expression(state, expected, kind))
    }
}
