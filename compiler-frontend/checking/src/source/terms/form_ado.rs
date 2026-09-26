use building_types::QueryResult;

use crate::context::CheckContext;
use crate::core::{TypeId, unification};
use crate::error::{ErrorCrumb, ErrorKind};
use crate::source::binder;
use crate::source::terms::{ElaboratedExpression, application, form_do, form_let};
use crate::state::CheckState;
use crate::{ExternalQueries, tree};

enum AdoStep<'a> {
    Action {
        statement: lowering::DoStatementId,
        binder_type: TypeId,
        binder: tree::BinderId,
        expression: lowering::ExpressionId,
    },
    Let {
        statement: lowering::DoStatementId,
        statements: &'a [lowering::LetBindingChunk],
    },
}

enum CheckedAdoApplication {
    Application {
        function: ElaboratedExpression,
        first_implicit: Vec<application::ImplicitApplication>,
        first_result: TypeId,
        second_implicit: Vec<application::ImplicitApplication>,
        argument: ElaboratedExpression,
        result: TypeId,
    },
    Error {
        result: TypeId,
    },
}

impl CheckedAdoApplication {
    fn type_id(&self) -> TypeId {
        match self {
            CheckedAdoApplication::Application { result, .. }
            | CheckedAdoApplication::Error { result } => *result,
        }
    }

    fn materialize(
        self,
        state: &mut CheckState,
        first_argument: ElaboratedExpression,
    ) -> ElaboratedExpression {
        match self {
            CheckedAdoApplication::Application {
                function,
                first_implicit,
                first_result,
                second_implicit,
                argument,
                result,
            } => {
                let function = application::materialize_application(
                    state,
                    function,
                    first_implicit,
                    first_result,
                    first_argument,
                );
                application::materialize_application(
                    state,
                    function,
                    second_implicit,
                    result,
                    argument,
                )
            }
            CheckedAdoApplication::Error { result } => {
                super::allocate_error_expression(state, result)
            }
        }
    }
}

pub fn infer_ado<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    map: Option<lowering::TermVariableResolution>,
    apply: Option<lowering::TermVariableResolution>,
    pure: Option<lowering::TermVariableResolution>,
    statement_ids: &[lowering::DoStatementId],
    expression: Option<lowering::ExpressionId>,
) -> QueryResult<ElaboratedExpression>
where
    Q: ExternalQueries,
{
    // First, perform a forward pass where variable bindings are bound
    // to unification variables. Let bindings are not checked here to
    // avoid premature solving of unification variables. Instead, they
    // are checked inline during the statement checking loop.
    let mut steps = Vec::new();
    let mut has_missing_action = false;
    for &statement_id in statement_ids.iter() {
        let Some(statement) = context.lowered.tree.get_do_statement(statement_id) else {
            continue;
        };
        match statement {
            lowering::DoStatement::Bind { binder, expression } => {
                let (binder_type, binder) = if let Some(binder) = binder {
                    let binder = binder::infer_binder(state, context, *binder)?;
                    (binder.type_id, binder.binder)
                } else {
                    let binder_type = state.fresh_unification(context.queries, context.prim.t);
                    let binder = state.allocate_generated_binder(
                        statement_id,
                        binder_type,
                        tree::BinderKind::Error,
                    );
                    (binder_type, binder)
                };
                let Some(expression) = *expression else {
                    has_missing_action = true;
                    continue;
                };
                steps.push(AdoStep::Action {
                    statement: statement_id,
                    binder_type,
                    binder,
                    expression,
                });
            }
            lowering::DoStatement::Let { statements } => {
                steps.push(AdoStep::Let { statement: statement_id, statements });
            }
            lowering::DoStatement::Discard { expression } => {
                let binder_type = state.fresh_unification(context.queries, context.prim.t);
                let binder = state.allocate_generated_binder(
                    statement_id,
                    binder_type,
                    tree::BinderKind::Wildcard,
                );
                let Some(expression) = *expression else {
                    has_missing_action = true;
                    continue;
                };
                steps.push(AdoStep::Action {
                    statement: statement_id,
                    binder_type,
                    binder,
                    expression,
                });
            }
        }
    }

    let binder_types = steps.iter().filter_map(|step| match step {
        AdoStep::Action { binder_type, .. } => Some(*binder_type),
        AdoStep::Let { .. } => None,
    });
    let binder_types = binder_types.collect::<Vec<_>>();

    let binders = steps.iter().filter_map(|step| match step {
        AdoStep::Action { binder, .. } => Some(*binder),
        AdoStep::Let { .. } => None,
    });
    let binders = binders.collect::<Vec<_>>();

    // For ado blocks with no bindings, we check let statements and then
    // apply pure to the expression.
    //
    //   pure_type  := a -> f a
    //   expression := t
    if binder_types.is_empty() {
        let mut checked_lets = Vec::new();
        for step in &steps {
            if let AdoStep::Let { statement, statements } = step {
                let bindings = state
                    .with_error_crumb(ErrorCrumb::CheckingAdoLet(*statement), |state| {
                        form_let::check_let_chunks(state, context, statements)
                    })?;
                checked_lets.push(bindings);
            }
        }
        return if let Some(expression) = expression {
            let function = form_do::lookup_or_synthesise_pure(state, context, pure)?;
            let Some(application::UnanchoredApplication { implicit, argument, result }) =
                application::check_unanchored_application(state, context, function.type_id())?
            else {
                let type_id = context.unknown("invalid function application");
                let expression = super::allocate_error_expression(state, type_id);
                return Ok(wrap_ado_lets(state, checked_lets, expression));
            };
            let argument = super::check_expression(state, context, expression, argument)?;
            let argument = if has_missing_action {
                super::allocate_error_expression(state, argument.type_id)
            } else {
                argument
            };
            let function = function.allocate_expression(state);
            let expression =
                application::materialize_application(state, function, implicit, result, argument);
            Ok(wrap_ado_lets(state, checked_lets, expression))
        } else {
            state.insert_error(ErrorKind::EmptyAdoBlock);
            let type_id = context.unknown("empty ado block");
            let expression = super::allocate_error_expression(state, type_id);
            Ok(wrap_ado_lets(state, checked_lets, expression))
        };
    }

    // Create a fresh unification variable for the in_expression.
    // Inferring expression directly may solve the unification variables
    // introduced in the first pass. This is undesirable, because the
    // errors would be attributed incorrectly to the ado statements
    // rather than the in-expression itself.
    //
    //   ado
    //     a <- pure "Hello!"
    //     _ <- pure 42
    //     in Message a
    //
    //   in_expression      :: Effect Message
    //   in_expression_type := ?in_expression
    //   lambda_type        := ?a -> ?b -> ?in_expression
    let in_expression_type = state.fresh_unification(context.queries, context.prim.t);
    let lambda_type = context.intern_function_list(&binder_types, in_expression_type);

    // The desugared form of an ado-expression is a forward applicative
    // pipeline, unlike do-notation which works inside-out. The example
    // above desugars to the following expression:
    //
    //   (\a _ -> Message a) <$> (pure "Hello!") <*> (pure 42)
    //
    // To emulate this, we process steps in source order. Let bindings
    // are checked inline between map/apply operations. The first action
    // uses infer_ado_map, and subsequent actions use infer_ado_apply.
    //
    //   map_type        :: (a -> b) -> f a -> f b
    //   lambda_type     := ?a -> ?b -> ?in_expression
    //
    //   expression_type         := Effect String
    //   map(lambda, expression) := Effect (?b -> ?in_expression)
    //                           >>
    //                           >> ?a := String
    //
    //   continuation_type := Effect (?b -> ?in_expression)

    // Lazily compute map_type and apply_type only when needed.
    // - 1 action: only map is needed
    // - 2+ actions: map and apply are needed
    let action_count = binder_types.len();

    let map_function = form_do::lookup_or_synthesise_map(state, context, map)?;

    let apply_function = if action_count > 1 {
        Some(form_do::lookup_or_synthesise_apply(state, context, apply)?)
    } else {
        None
    };

    let mut continuation_type = None;
    let mut checked_actions = Vec::new();
    let mut checked_lets = Vec::new();

    for step in &steps {
        match step {
            AdoStep::Let { statement, statements } => {
                let bindings = state
                    .with_error_crumb(ErrorCrumb::CheckingAdoLet(*statement), |state| {
                        form_let::check_let_chunks(state, context, statements)
                    })?;
                checked_lets.push(bindings);
            }
            AdoStep::Action { statement, expression, .. } => {
                let application = if let Some(continuation_type) = continuation_type {
                    // Then, the infer_ado_apply rule applies `apply` to the inferred
                    // expression type and the continuation type that is a function
                    // contained within some container, like Effect.
                    //
                    //   apply_type        := f (x -> y) -> f x -> f y
                    //   continuation_type := Effect (?b -> ?in_expression)
                    //
                    //   expression_type                 := Effect Int
                    //   apply(continuation, expression) := Effect ?in_expression
                    //                                   >>
                    //                                   >> ?b := Int
                    //
                    //   continuation_type := Effect ?in_expression
                    state.with_error_crumb(ErrorCrumb::InferringAdoApply(*statement), |state| {
                        infer_ado_application_core(
                            state,
                            context,
                            apply_function.as_ref().expect(
                                "invariant violated: desugared apply function was not initialised",
                            ),
                            continuation_type,
                            *expression,
                        )
                    })?
                } else {
                    state.with_error_crumb(ErrorCrumb::InferringAdoMap(*statement), |state| {
                        infer_ado_application_core(
                            state,
                            context,
                            &map_function,
                            lambda_type,
                            *expression,
                        )
                    })?
                };
                continuation_type = Some(application.type_id());
                checked_actions.push(application);
            }
        }
    }

    // Finally, check the in-expression against in_expression.
    // At this point the binder unification variables have been solved
    // to concrete types, so errors are attributed to the in-expression.
    //
    //   in_expression      :: Effect Message
    //   in_expression_type := Effect ?in_expression
    //                      >>
    //                      >> ?in_expression := Message
    let expression = if let Some(expression) = expression {
        super::check_expression(state, context, expression, in_expression_type)?
    } else {
        super::allocate_error_expression(state, in_expression_type)
    };
    let expression = if has_missing_action {
        super::allocate_error_expression(state, in_expression_type)
    } else {
        expression
    };
    let expression = ElaboratedExpression { type_id: in_expression_type, ..expression };
    let body = wrap_ado_lets(state, checked_lets, expression);

    let kind =
        tree::ExpressionKind::Lambda { binders: binders.into(), expression: body.expression };
    let lambda = super::allocate_expression(state, lambda_type, kind);

    let mut checked_actions = checked_actions.into_iter();
    let first_action =
        checked_actions.next().expect("invariant violated: ado has no checked map application");
    let mut expression = first_action.materialize(state, lambda);
    for action in checked_actions {
        expression = action.materialize(state, expression);
    }

    let Some(continuation_type) = continuation_type else {
        unreachable!("invariant violated: impossible empty steps");
    };

    Ok(ElaboratedExpression { type_id: continuation_type, ..expression })
}

fn infer_ado_application_core<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    function: &form_do::DesugaredFunction,
    first_argument_type: TypeId,
    expression: lowering::ExpressionId,
) -> QueryResult<CheckedAdoApplication>
where
    Q: ExternalQueries,
{
    let expression = super::infer_expression(state, context, expression)?;

    let Some(application::UnanchoredApplication {
        implicit: first_implicit,
        argument,
        result: first_result,
    }) = application::check_unanchored_application(state, context, function.type_id())?
    else {
        let result = context.unknown("invalid function application");
        return Ok(CheckedAdoApplication::Error { result });
    };
    unification::subtype(state, context, first_argument_type, argument)?;

    let Some(application::UnanchoredApplication { implicit: second_implicit, argument, result }) =
        application::check_unanchored_application(state, context, first_result)?
    else {
        let result = context.unknown("invalid function application");
        return Ok(CheckedAdoApplication::Error { result });
    };
    let argument = application::subtype_expression(state, context, expression, argument)?;
    let function = function.allocate_expression(state);

    Ok(CheckedAdoApplication::Application {
        function,
        first_implicit,
        first_result,
        second_implicit,
        argument,
        result,
    })
}

fn wrap_ado_lets(
    state: &mut CheckState,
    bindings: Vec<tree::LetBindings>,
    mut expression: ElaboratedExpression,
) -> ElaboratedExpression {
    for bindings in bindings.into_iter().rev() {
        let kind = tree::ExpressionKind::Let { bindings, expression: expression.expression };
        expression = super::allocate_expression(state, expression.type_id, kind);
    }
    expression
}
