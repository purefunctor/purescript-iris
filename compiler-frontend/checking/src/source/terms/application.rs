use building_types::QueryResult;

use crate::context::CheckContext;
use crate::core::substitute::{NameToType, SubstituteName};
use crate::core::{ForallBinder, Type, TypeId, normalise, unification};
use crate::error::ErrorKind;
use crate::evidence::EvidenceVarId;
use crate::source::types;
use crate::state::CheckState;
use crate::{ExternalQueries, safe_loop, tree};

use super::ElaboratedExpression;

pub struct UnanchoredApplication {
    pub implicit: Vec<ImplicitApplication>,
    pub argument: TypeId,
    pub result: TypeId,
}

pub enum ImplicitApplication {
    Type { result: TypeId },
    Evidence { evidence: EvidenceVarId, constraint: TypeId, result: TypeId },
}

enum PendingImplicitApplication {
    Type { result: TypeId },
    Constraint { constraint: TypeId, result: TypeId },
}

enum ApplicationStep {
    Applied(ElaboratedExpression),
    Error(TypeId),
}

pub enum CallableAnalysis {
    Forall { binder: ForallBinder, body: TypeId },
    Constraint { constraint: TypeId, result: TypeId },
    Function { argument: TypeId, result: TypeId },
    NotCallable,
}

pub fn analyse_callable_head<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    function: TypeId,
) -> QueryResult<CallableAnalysis>
where
    Q: ExternalQueries,
{
    let function = normalise::expand(state, context, function)?;

    match *context.lookup_type(function) {
        Type::Function(argument, result) => Ok(CallableAnalysis::Function { argument, result }),

        Type::Unification(unification_id) => {
            let argument = state.fresh_unification(context.queries, context.prim.t);
            let result = state.fresh_unification(context.queries, context.prim.t);
            let function = context.intern_function(argument, result);

            unification::solve(state, context, function, unification_id, function)?;

            Ok(CallableAnalysis::Function { argument, result })
        }

        Type::Forall(binder_id, inner) => {
            let binder = context.lookup_forall_binder(binder_id);
            Ok(CallableAnalysis::Forall { binder, body: inner })
        }

        Type::Constrained(constraint, result) => {
            Ok(CallableAnalysis::Constraint { constraint, result })
        }

        Type::Application(function_argument, result) => {
            let function_argument = normalise::expand(state, context, function_argument)?;

            let Type::Application(constructor, argument) = *context.lookup_type(function_argument)
            else {
                return Ok(CallableAnalysis::NotCallable);
            };

            let constructor = normalise::expand(state, context, constructor)?;
            if constructor == context.prim.function {
                return Ok(CallableAnalysis::Function { argument, result });
            }

            if let Type::Unification(unification_id) = *context.lookup_type(constructor) {
                unification::solve(
                    state,
                    context,
                    constructor,
                    unification_id,
                    context.prim.function,
                )?;

                return Ok(CallableAnalysis::Function { argument, result });
            }

            Ok(CallableAnalysis::NotCallable)
        }

        _ => Ok(CallableAnalysis::NotCallable),
    }
}

/// Instantiates `binder` and the invisible binders that immediately follow it
/// with fresh unification variables in a single substitution.
///
/// Instantiation stops at a visible binder so that a following type
/// application, such as `f @Int`, can still supply its argument.
pub fn instantiate_callable_foralls<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    binder: ForallBinder,
    mut body: TypeId,
) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    let binder_kind = normalise::expand(state, context, binder.kind)?;
    let argument = state.fresh_unification(context.queries, binder_kind);

    let mut bindings = NameToType::default();
    bindings.insert(binder.name, argument);

    safe_loop! {
        let expanded = normalise::expand(state, context, body)?;
        let Type::Forall(binder_id, inner) = *context.lookup_type(expanded) else {
            break;
        };
        let binder = context.lookup_forall_binder(binder_id);
        if binder.visible {
            break;
        }
        let binder_kind = SubstituteName::many(state, context, &bindings, binder.kind)?;
        let binder_kind = normalise::expand(state, context, binder_kind)?;
        let argument = state.fresh_unification(context.queries, binder_kind);
        bindings.insert(binder.name, argument);
        body = inner;
    }

    SubstituteName::many(state, context, &bindings, body)
}

pub fn instantiate_expression<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    mut expression: ElaboratedExpression,
) -> QueryResult<ElaboratedExpression>
where
    Q: ExternalQueries,
{
    safe_loop! {
        let type_id = normalise::expand(state, context, expression.type_id)?;
        match *context.lookup_type(type_id) {
            Type::Forall(binder_id, body) => {
                let binder = context.lookup_forall_binder(binder_id);
                expression.type_id = instantiate_callable_foralls(state, context, binder, body)?;
            }
            Type::Constrained(constraint, result) => {
                let evidence = state.push_wanted(constraint);
                let kind = tree::ExpressionKind::EvidenceApplication {
                    function: expression.expression,
                    evidence,
                    constraint,
                };
                expression = super::allocate_expression(state, result, kind);
            }
            _ => {
                break Ok(expression);
            }
        }
    }
}

pub fn collect_expression_wanteds<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    mut expression: ElaboratedExpression,
) -> QueryResult<ElaboratedExpression>
where
    Q: ExternalQueries,
{
    safe_loop! {
        let type_id = normalise::expand(state, context, expression.type_id)?;
        let Type::Constrained(constraint, result) = *context.lookup_type(type_id) else {
            break Ok(expression);
        };
        let evidence = state.push_wanted(constraint);
        let kind = tree::ExpressionKind::EvidenceApplication {
            function: expression.expression,
            evidence,
            constraint,
        };
        expression = super::allocate_expression(state, result, kind);
    }
}

pub fn check_unanchored_application<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    function: TypeId,
) -> QueryResult<Option<UnanchoredApplication>>
where
    Q: ExternalQueries,
{
    let mut function = function;
    let mut implicit = Vec::new();
    safe_loop! {
        match analyse_callable_head(state, context, function)? {
            CallableAnalysis::Forall { binder, body } => {
                let result = instantiate_callable_foralls(state, context, binder, body)?;
                implicit.push(PendingImplicitApplication::Type { result });
                function = result;
            }
            CallableAnalysis::Constraint { constraint, result } => {
                implicit.push(PendingImplicitApplication::Constraint { constraint, result });
                function = result;
            }
            CallableAnalysis::Function { argument, result } => {
                let implicit = implicit.into_iter().map(|application| match application {
                    PendingImplicitApplication::Type { result } => {
                        ImplicitApplication::Type { result }
                    }
                    PendingImplicitApplication::Constraint { constraint, result } => {
                        let evidence = state.push_wanted(constraint);
                        ImplicitApplication::Evidence { evidence, constraint, result }
                    }
                });
                let implicit = implicit.collect();
                break Ok(Some(UnanchoredApplication { implicit, argument, result }));
            }
            CallableAnalysis::NotCallable => break Ok(None),
        }
    }
}

/// Checks an expression against an expected type while retaining implicit
/// applications introduced on the inferred side.
pub fn subtype_expression<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    expression: ElaboratedExpression,
    expected: TypeId,
) -> QueryResult<ElaboratedExpression>
where
    Q: ExternalQueries,
{
    let applications =
        unification::subtype_with_applications(state, context, expression.type_id, expected)?;
    let applications = applications.into_iter().map(|application| match application {
        unification::SubtypeApplication::Type { result } => ImplicitApplication::Type { result },
        unification::SubtypeApplication::Evidence { evidence, constraint, result } => {
            ImplicitApplication::Evidence { evidence, constraint, result }
        }
    });
    Ok(materialize_implicit_applications(state, expression, applications))
}

fn materialize_implicit_applications(
    state: &mut CheckState,
    mut expression: ElaboratedExpression,
    implicit: impl IntoIterator<Item = ImplicitApplication>,
) -> ElaboratedExpression {
    for application in implicit {
        match application {
            ImplicitApplication::Type { result } => {
                expression.type_id = result;
            }
            ImplicitApplication::Evidence { evidence, constraint, result } => {
                let kind = tree::ExpressionKind::EvidenceApplication {
                    function: expression.expression,
                    evidence,
                    constraint,
                };
                expression = super::allocate_expression(state, result, kind);
            }
        }
    }
    expression
}

pub fn materialize_application(
    state: &mut CheckState,
    function: ElaboratedExpression,
    implicit: Vec<ImplicitApplication>,
    result: TypeId,
    argument: ElaboratedExpression,
) -> ElaboratedExpression {
    let function = materialize_implicit_applications(state, function, implicit);
    let kind = tree::ExpressionKind::TermApplication {
        function: function.expression,
        argument: argument.expression,
    };
    super::allocate_expression(state, result, kind)
}

pub fn check_expression_application<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    mut function: ElaboratedExpression,
    arguments: &[lowering::ExpressionArgument],
) -> QueryResult<ElaboratedExpression>
where
    Q: ExternalQueries,
{
    for argument in arguments {
        let step = match argument {
            lowering::ExpressionArgument::Type(Some(argument)) => {
                check_expression_type_application(state, context, function, *argument)?
            }
            lowering::ExpressionArgument::Type(None) => {
                ApplicationStep::Error(context.unknown("missing type argument"))
            }
            lowering::ExpressionArgument::Term(Some(argument)) => {
                check_expression_term_application(state, context, function, *argument)?
            }
            lowering::ExpressionArgument::Term(None) => {
                ApplicationStep::Error(context.unknown("missing term argument"))
            }
        };

        match step {
            ApplicationStep::Applied(expression) => {
                function = expression;
            }
            ApplicationStep::Error(type_id) => {
                return Ok(super::allocate_error_expression(state, type_id));
            }
        }
    }

    Ok(function)
}

fn check_expression_term_application<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    mut function: ElaboratedExpression,
    expression_id: lowering::ExpressionId,
) -> QueryResult<ApplicationStep>
where
    Q: ExternalQueries,
{
    safe_loop! {
        match analyse_callable_head(state, context, function.type_id)? {
            CallableAnalysis::Forall { binder, body } => {
                function.type_id = instantiate_callable_foralls(state, context, binder, body)?;
            }
            CallableAnalysis::Constraint { constraint, result } => {
                let evidence = state.push_wanted(constraint);
                let kind = tree::ExpressionKind::EvidenceApplication {
                    function: function.expression,
                    evidence,
                    constraint,
                };
                function = super::allocate_expression(state, result, kind);
            }
            CallableAnalysis::Function { argument, result } => {
                let argument = super::check_expression(state, context, expression_id, argument)?;
                let kind = tree::ExpressionKind::TermApplication {
                    function: function.expression,
                    argument: argument.expression,
                };
                let application = super::allocate_expression(state, result, kind);
                break Ok(ApplicationStep::Applied(application));
            }
            CallableAnalysis::NotCallable => {
                let type_id = context.unknown("invalid function application");
                break Ok(ApplicationStep::Error(type_id));
            }
        }
    }
}

fn check_expression_type_application<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    mut function: ElaboratedExpression,
    argument: lowering::TypeId,
) -> QueryResult<ApplicationStep>
where
    Q: ExternalQueries,
{
    let function_type = function.type_id;

    safe_loop! {
        let type_id = normalise::expand(state, context, function.type_id)?;
        match *context.lookup_type(type_id) {
            Type::Forall(binder_id, body) => {
                let binder = context.lookup_forall_binder(binder_id);
                if binder.visible {
                    let binder_kind = normalise::expand(state, context, binder.kind)?;
                    let (argument, _) = types::check_kind(state, context, argument, binder_kind)?;
                    let result =
                        SubstituteName::one(state, context, binder.name, argument, body)?;
                    let application =
                        ElaboratedExpression { type_id: result, expression: function.expression };
                    break Ok(ApplicationStep::Applied(application));
                }

                function.type_id = instantiate_callable_foralls(state, context, binder, body)?;
            }
            Type::Constrained(constraint, result) => {
                let evidence = state.push_wanted(constraint);
                let kind = tree::ExpressionKind::EvidenceApplication {
                    function: function.expression,
                    evidence,
                    constraint,
                };
                function = super::allocate_expression(state, result, kind);
            }
            _ => {
                state.insert_error(ErrorKind::NoVisibleTypeVariable { function_type });
                let type_id = context.unknown("invalid visible type application");
                break Ok(ApplicationStep::Error(type_id));
            }
        }
    }
}

pub fn check_function_term_application<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    function: TypeId,
    expression_id: lowering::ExpressionId,
) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    let Some(UnanchoredApplication { argument, result, .. }) =
        check_unanchored_application(state, context, function)?
    else {
        return Ok(context.unknown("invalid function application"));
    };
    super::check_expression(state, context, expression_id, argument)?;
    Ok(result)
}

pub fn infer_infix_chain<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    head: lowering::ExpressionId,
    tail: &[lowering::InfixPair<lowering::ExpressionId>],
) -> QueryResult<ElaboratedExpression>
where
    Q: ExternalQueries,
{
    let mut infix = super::infer_expression(state, context, head)?;

    for lowering::InfixPair { tick, element } in tail.iter() {
        let Some(tick) = tick else {
            let unknown = context.unknown("missing infix tick");
            return Ok(super::allocate_error_expression(state, unknown));
        };
        let Some(element) = element else {
            let unknown = context.unknown("missing infix element");
            return Ok(super::allocate_error_expression(state, unknown));
        };

        let tick = super::infer_expression(state, context, *tick)?;
        let Some(UnanchoredApplication { implicit, argument, result }) =
            check_unanchored_application(state, context, tick.type_id)?
        else {
            let unknown = context.unknown("invalid function application");
            return Ok(super::allocate_error_expression(state, unknown));
        };
        infix = subtype_expression(state, context, infix, argument)?;
        let applied_tick = materialize_application(state, tick, implicit, result, infix);

        let Some(UnanchoredApplication { implicit, argument, result }) =
            check_unanchored_application(state, context, applied_tick.type_id)?
        else {
            let unknown = context.unknown("invalid function application");
            return Ok(super::allocate_error_expression(state, unknown));
        };
        let element = super::check_expression(state, context, *element, argument)?;
        infix = materialize_application(state, applied_tick, implicit, result, element);
    }

    Ok(infix)
}
