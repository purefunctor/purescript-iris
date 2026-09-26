//! Implements checking and inference rules for guarded and where expressions.

use building_types::QueryResult;

use crate::context::CheckContext;
use crate::core::TypeId;
use crate::source::terms::form_let;
use crate::source::{binder, terms};
use crate::state::CheckState;
use crate::{ExternalQueries, tree};

pub struct ElaboratedGuardedExpression {
    pub type_id: TypeId,
    pub guarded_expression: tree::GuardedExpression,
}

pub struct ElaboratedWhereExpression {
    pub type_id: TypeId,
    pub where_expression: tree::WhereExpression,
}

#[derive(Copy, Clone, Debug)]
enum GuardedExpressionMode {
    Infer,
    Subsume { expected: TypeId },
    Check { expected: TypeId },
}

#[derive(Copy, Clone, Debug)]
enum WhereExpressionMode {
    Infer,
    Check { expected: TypeId },
}

pub fn infer_guarded_expression<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    guarded: &lowering::GuardedExpression,
) -> QueryResult<ElaboratedGuardedExpression>
where
    Q: ExternalQueries,
{
    guarded_expression_core(state, context, guarded, GuardedExpressionMode::Infer)
}

pub fn check_guarded_expression<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    guarded: &lowering::GuardedExpression,
    expected: TypeId,
) -> QueryResult<ElaboratedGuardedExpression>
where
    Q: ExternalQueries,
{
    guarded_expression_core(state, context, guarded, GuardedExpressionMode::Check { expected })
}

/// Infers an unconditional guarded result before subsuming it against the
/// shared result type of an inferred case expression. Subsumption operates on
/// the elaborated expression rather than its type alone so that implicit
/// evidence applications introduced by the relation remain in the checked
/// tree.
pub fn subsume_guarded_expression<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    guarded: &lowering::GuardedExpression,
    expected: TypeId,
) -> QueryResult<ElaboratedGuardedExpression>
where
    Q: ExternalQueries,
{
    guarded_expression_core(state, context, guarded, GuardedExpressionMode::Subsume { expected })
}

fn guarded_expression_core<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    guarded: &lowering::GuardedExpression,
    mode: GuardedExpressionMode,
) -> QueryResult<ElaboratedGuardedExpression>
where
    Q: ExternalQueries,
{
    match guarded {
        lowering::GuardedExpression::Unconditional { where_expression } => {
            let Some(where_expression) = where_expression else {
                let type_id = match mode {
                    GuardedExpressionMode::Infer => context.unknown("missing guarded expression"),
                    GuardedExpressionMode::Subsume { expected }
                    | GuardedExpressionMode::Check { expected } => expected,
                };
                let expression = state.allocate_error_expression(type_id);
                let where_expression = tree::WhereExpression::new(expression);
                let guarded_expression = tree::GuardedExpression::unconditional(where_expression);
                return Ok(ElaboratedGuardedExpression { type_id, guarded_expression });
            };

            let where_expression = match mode {
                GuardedExpressionMode::Infer => {
                    infer_where_expression(state, context, where_expression)?
                }
                GuardedExpressionMode::Subsume { expected } => {
                    let where_expression =
                        infer_where_expression(state, context, where_expression)?;
                    subsume_where_expression(state, context, where_expression, expected)?
                }
                GuardedExpressionMode::Check { expected } => {
                    check_where_expression(state, context, where_expression, expected)?
                }
            };
            let type_id = where_expression.type_id;
            let guarded_expression =
                tree::GuardedExpression::unconditional(where_expression.where_expression);
            Ok(ElaboratedGuardedExpression { type_id, guarded_expression })
        }
        lowering::GuardedExpression::Conditionals { pattern_guarded } => {
            let expected_type = match mode {
                GuardedExpressionMode::Infer => {
                    state.fresh_unification(context.queries, context.prim.t)
                }
                GuardedExpressionMode::Subsume { expected }
                | GuardedExpressionMode::Check { expected } => expected,
            };

            let mut alternatives = Vec::new();
            for pattern_guarded in pattern_guarded.iter() {
                let mut pattern_guards = Vec::new();
                for pattern_guard in pattern_guarded.pattern_guards.iter() {
                    let pattern_guard = check_pattern_guard(state, context, pattern_guard)?;
                    pattern_guards.push(pattern_guard);
                }
                let where_expression =
                    if let Some(where_expression) = &pattern_guarded.where_expression {
                        check_where_expression(state, context, where_expression, expected_type)?
                            .where_expression
                    } else {
                        let expression = state.allocate_error_expression(expected_type);
                        tree::WhereExpression::new(expression)
                    };
                alternatives.push(tree::GuardedAlternative {
                    pattern_guards: pattern_guards.into(),
                    where_expression,
                });
            }

            let guarded_expression = tree::GuardedExpression { alternatives: alternatives.into() };
            Ok(ElaboratedGuardedExpression { type_id: expected_type, guarded_expression })
        }
    }
}

fn check_pattern_guard<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    guard: &lowering::PatternGuard,
) -> QueryResult<tree::PatternGuard>
where
    Q: ExternalQueries,
{
    let expression = guard.expression;

    if let Some(binder_id) = guard.binder {
        if let Some(expression) = expression {
            let expression = terms::infer_expression(state, context, expression)?;
            let expression =
                super::application::instantiate_expression(state, context, expression)?;
            let binder = binder::check_binder(state, context, binder_id, expression.type_id)?;
            Ok(tree::PatternGuard::Pattern {
                binder: binder.binder,
                expression: expression.expression,
            })
        } else {
            let binder = binder::infer_binder(state, context, binder_id)?;
            let expression = state.allocate_error_expression(binder.type_id);
            Ok(tree::PatternGuard::Pattern { binder: binder.binder, expression })
        }
    } else {
        let expression = if let Some(expression) = expression {
            terms::check_expression(state, context, expression, context.prim.boolean)?
        } else {
            let type_id = context.prim.boolean;
            let expression = state.allocate_error_expression(type_id);
            terms::ElaboratedExpression { type_id, expression }
        };
        Ok(tree::PatternGuard::Boolean { expression: expression.expression })
    }
}

pub fn infer_where_expression<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    where_expression: &lowering::WhereExpression,
) -> QueryResult<ElaboratedWhereExpression>
where
    Q: ExternalQueries,
{
    where_expression_core(state, context, where_expression, WhereExpressionMode::Infer)
}

fn subsume_where_expression<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    where_expression: ElaboratedWhereExpression,
    expected: TypeId,
) -> QueryResult<ElaboratedWhereExpression>
where
    Q: ExternalQueries,
{
    let expression = terms::ElaboratedExpression {
        type_id: where_expression.type_id,
        expression: where_expression.where_expression.expression,
    };
    let expression = terms::application::subtype_expression(state, context, expression, expected)?;
    let where_expression = tree::WhereExpression {
        expression: expression.expression,
        ..where_expression.where_expression
    };
    Ok(ElaboratedWhereExpression { type_id: expected, where_expression })
}

fn check_where_expression<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    where_expression: &lowering::WhereExpression,
    expected: TypeId,
) -> QueryResult<ElaboratedWhereExpression>
where
    Q: ExternalQueries,
{
    where_expression_core(state, context, where_expression, WhereExpressionMode::Check { expected })
}

fn where_expression_core<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    where_expression: &lowering::WhereExpression,
    mode: WhereExpressionMode,
) -> QueryResult<ElaboratedWhereExpression>
where
    Q: ExternalQueries,
{
    let bindings = form_let::check_let_chunks(state, context, &where_expression.bindings)?;

    let Some(expression) = where_expression.expression else {
        let type_id = match mode {
            WhereExpressionMode::Infer => context.unknown("missing where expression"),
            WhereExpressionMode::Check { expected } => expected,
        };
        let expression = state.allocate_error_expression(type_id);
        let where_expression = tree::WhereExpression { bindings, expression };
        return Ok(ElaboratedWhereExpression { type_id, where_expression });
    };

    let terms::ElaboratedExpression { type_id, expression } = match mode {
        WhereExpressionMode::Infer => terms::infer_expression(state, context, expression)?,
        WhereExpressionMode::Check { expected } => {
            terms::check_expression(state, context, expression, expected)?
        }
    };
    let where_expression = tree::WhereExpression { bindings, expression };
    Ok(ElaboratedWhereExpression { type_id, where_expression })
}
