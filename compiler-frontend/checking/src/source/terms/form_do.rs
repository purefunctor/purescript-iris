use std::iter;
use std::sync::Arc;

use building_types::QueryResult;
use itertools::Itertools;

use crate::context::CheckContext;
use crate::core::constraint::compiler::prim_effect;
use crate::core::{TypeId, toolkit, unification};
use crate::error::{ErrorCrumb, ErrorKind};
use crate::source::binder;
use crate::source::terms::{ElaboratedExpression, application, form_let};
use crate::state::CheckState;
use crate::{ExternalQueries, tree};

pub struct DesugaredFunction {
    resolution: Option<lowering::TermVariableResolution>,
    type_id: TypeId,
}

impl DesugaredFunction {
    pub fn type_id(&self) -> TypeId {
        self.type_id
    }

    pub fn allocate_expression(&self, state: &mut CheckState) -> ElaboratedExpression {
        if let Some(resolution) = self.resolution {
            let resolution = tree::VariableResolution::Source(resolution);
            let kind = tree::ExpressionKind::Variable { resolution };
            super::allocate_expression(state, self.type_id, kind)
        } else {
            super::allocate_error_expression(state, self.type_id)
        }
    }
}

enum DoStep<'a> {
    Bind {
        statement: lowering::DoStatementId,
        binder_type: TypeId,
        binder: tree::BinderId,
        expression: Option<lowering::ExpressionId>,
    },
    Discard {
        statement: lowering::DoStatementId,
        expression: Option<lowering::ExpressionId>,
    },
    Let {
        statement: lowering::DoStatementId,
        statements: &'a [lowering::LetBindingChunk],
    },
}

impl DoStep<'_> {
    fn is_action(&self) -> bool {
        matches!(self, Self::Bind { .. } | Self::Discard { .. })
    }
}

struct CheckedDoApplication {
    function: ElaboratedExpression,
    implicit: Vec<application::ImplicitApplication>,
    constraints: Vec<TypeId>,
    result: TypeId,
    lambda_type: TypeId,
    crumbs: Arc<[ErrorCrumb]>,
}

enum CheckedDoStep {
    Application { application: CheckedDoApplication, binder: tree::BinderId },
    MissingApplication { binder: tree::BinderId, lambda_type: TypeId, result: TypeId },
    Let { bindings: tree::LetBindings },
}

enum DoBlockFinalStep {
    Empty,
    InvalidBind { statement: lowering::DoStatementId, expression: Option<lowering::ExpressionId> },
    Discard { expression: Option<lowering::ExpressionId> },
    InvalidLet { statement: lowering::DoStatementId },
}

impl DoBlockFinalStep {
    fn is_invalid_let(&self) -> bool {
        matches!(self, Self::InvalidLet { .. })
    }
}

/// Lookup `bind` from resolution, or synthesize `?m ?a -> (?a -> ?m ?b) -> ?m ?b`.
pub fn lookup_or_synthesise_bind<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    resolution: Option<lowering::TermVariableResolution>,
) -> QueryResult<DesugaredFunction>
where
    Q: ExternalQueries,
{
    let type_id = if let Some(resolution) = resolution {
        toolkit::lookup_term_variable(state, context, resolution)
    } else {
        let m = state.fresh_unification(context.queries, context.prim.type_to_type);
        let a = state.fresh_unification(context.queries, context.prim.t);
        let b = state.fresh_unification(context.queries, context.prim.t);
        let m_a = context.intern_application(m, a);
        let m_b = context.intern_application(m, b);
        let a_to_m_b = context.intern_function(a, m_b);
        Ok(context.intern_function_list(&[m_a, a_to_m_b], m_b))
    }?;
    Ok(DesugaredFunction { resolution, type_id })
}

/// Lookup `discard` from resolution, or synthesize `?m ?a -> (?a -> ?m ?b) -> ?m ?b`.
pub fn lookup_or_synthesise_discard<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    resolution: Option<lowering::TermVariableResolution>,
) -> QueryResult<DesugaredFunction>
where
    Q: ExternalQueries,
{
    // Same shape as bind
    lookup_or_synthesise_bind(state, context, resolution)
}

/// Lookup `map` from resolution, or synthesize `(?a -> ?b) -> ?f ?a -> ?f ?b`.
pub fn lookup_or_synthesise_map<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    resolution: Option<lowering::TermVariableResolution>,
) -> QueryResult<DesugaredFunction>
where
    Q: ExternalQueries,
{
    let type_id = if let Some(resolution) = resolution {
        toolkit::lookup_term_variable(state, context, resolution)
    } else {
        let f = state.fresh_unification(context.queries, context.prim.type_to_type);
        let a = state.fresh_unification(context.queries, context.prim.t);
        let b = state.fresh_unification(context.queries, context.prim.t);
        let f_a = context.intern_application(f, a);
        let f_b = context.intern_application(f, b);
        let a_to_b = context.intern_function(a, b);
        Ok(context.intern_function_list(&[a_to_b, f_a], f_b))
    }?;
    Ok(DesugaredFunction { resolution, type_id })
}

/// Lookup `apply` from resolution, or synthesize `?f (?a -> ?b) -> ?f ?a -> ?f ?b`.
pub fn lookup_or_synthesise_apply<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    resolution: Option<lowering::TermVariableResolution>,
) -> QueryResult<DesugaredFunction>
where
    Q: ExternalQueries,
{
    let type_id = if let Some(resolution) = resolution {
        toolkit::lookup_term_variable(state, context, resolution)
    } else {
        let f = state.fresh_unification(context.queries, context.prim.type_to_type);
        let a = state.fresh_unification(context.queries, context.prim.t);
        let b = state.fresh_unification(context.queries, context.prim.t);
        let a_to_b = context.intern_function(a, b);
        let f_a_to_b = context.intern_application(f, a_to_b);
        let f_a = context.intern_application(f, a);
        let f_b = context.intern_application(f, b);
        Ok(context.intern_function_list(&[f_a_to_b, f_a], f_b))
    }?;
    Ok(DesugaredFunction { resolution, type_id })
}

/// Lookup `pure` from resolution, or synthesize `?a -> ?f ?a`.
pub fn lookup_or_synthesise_pure<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    resolution: Option<lowering::TermVariableResolution>,
) -> QueryResult<DesugaredFunction>
where
    Q: ExternalQueries,
{
    let type_id = if let Some(resolution) = resolution {
        toolkit::lookup_term_variable(state, context, resolution)
    } else {
        let f = state.fresh_unification(context.queries, context.prim.type_to_type);
        let a = state.fresh_unification(context.queries, context.prim.t);
        let f_a = context.intern_application(f, a);
        Ok(context.intern_function(a, f_a))
    }?;
    Ok(DesugaredFunction { resolution, type_id })
}

pub fn infer_do<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    bind: Option<lowering::TermVariableResolution>,
    discard: Option<lowering::TermVariableResolution>,
    statement_id: &[lowering::DoStatementId],
) -> QueryResult<ElaboratedExpression>
where
    Q: ExternalQueries,
{
    // First, perform a forward pass where variable bindings are bound
    // to unification variables. Let bindings are not checked here to
    // avoid premature solving of unification variables. Instead, they
    // are checked inline during the statement checking loop.
    let mut steps = vec![];
    for &statement_id in statement_id.iter() {
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
                steps.push(DoStep::Bind {
                    statement: statement_id,
                    binder_type,
                    binder,
                    expression: *expression,
                });
            }
            lowering::DoStatement::Let { statements } => {
                steps.push(DoStep::Let { statement: statement_id, statements });
            }
            lowering::DoStatement::Discard { expression } => {
                steps.push(DoStep::Discard { statement: statement_id, expression: *expression });
            }
        }
    }

    let final_step = match steps.last() {
        Some(DoStep::Bind { statement, expression, .. }) => {
            DoBlockFinalStep::InvalidBind { statement: *statement, expression: *expression }
        }
        Some(DoStep::Discard { expression, .. }) => {
            DoBlockFinalStep::Discard { expression: *expression }
        }
        Some(DoStep::Let { statement, .. }) => {
            DoBlockFinalStep::InvalidLet { statement: *statement }
        }
        None => DoBlockFinalStep::Empty,
    };

    let (has_bind_step, has_discard_step) = {
        let mut has_bind = false;
        let mut has_discard = false;
        for (position, statement) in steps.iter().with_position() {
            let is_final = position.is_last();
            match statement {
                DoStep::Bind { .. } => has_bind = true,
                DoStep::Discard { .. } if !is_final => has_discard = true,
                _ => (),
            }
        }
        (has_bind, has_discard)
    };

    let bind_function =
        if has_bind_step { Some(lookup_or_synthesise_bind(state, context, bind)?) } else { None };

    let discard_function = if has_discard_step {
        Some(lookup_or_synthesise_discard(state, context, discard)?)
    } else {
        None
    };

    let final_expression = match final_step {
        DoBlockFinalStep::Empty => {
            state.insert_error(ErrorKind::EmptyDoBlock);
            let type_id = context.unknown("empty do block");
            return Ok(super::allocate_error_expression(state, type_id));
        }
        // Technically valid, syntactically disallowed. This allows
        // partially-written do expressions to infer, with a friendly
        // warning to nudge the user that `bind` is prohibited.
        DoBlockFinalStep::InvalidBind { statement, expression } => {
            state.with_error_crumb(ErrorCrumb::InferringDoBind(statement), |state| {
                state.insert_error(ErrorKind::InvalidFinalBind);
            });
            let Some(expression) = expression else {
                state.insert_error(ErrorKind::EmptyDoBlock);
                let type_id = context.unknown("empty do block");
                return Ok(super::allocate_error_expression(state, type_id));
            };
            Some(expression)
        }
        DoBlockFinalStep::Discard { expression } => {
            let Some(expression) = expression else {
                state.insert_error(ErrorKind::EmptyDoBlock);
                let type_id = context.unknown("empty do block");
                return Ok(super::allocate_error_expression(state, type_id));
            };
            Some(expression)
        }
        DoBlockFinalStep::InvalidLet { statement } => {
            state.with_error_crumb(ErrorCrumb::CheckingDoLet(statement), |state| {
                state.insert_error(ErrorKind::InvalidFinalLet);
            });
            None
        }
    };

    // Create unification variables that each statement in the do expression
    // will unify against. The next section will get into more detail how
    // these are used. These unification variables are used to enable GHC-like
    // left-to-right checking of do expressions while maintaining the same
    // semantics as rebindable `do` in PureScript. When there is an invalid
    // final let, we synthesise a placeholder continuation for the missing
    // final expression, such that checking and inference proceeds as normal.
    let mut continuation_count = steps.iter().filter(|step| step.is_action()).count();
    continuation_count += usize::from(final_step.is_invalid_let());

    let continuation_types =
        iter::repeat_with(|| state.fresh_unification(context.queries, context.prim.t))
            .take(continuation_count)
            .collect_vec();

    // Let's trace over the following example:
    //
    //   main = do
    //     a <- effect
    //     b <- aff
    //     pure { a, b }
    //
    // For the first statement, we know the following information. We
    // instantiate the `bind` function to prepare it for application.
    // The first argument is easy, it's just the expression_type; the
    // second argument involves synthesising a function type using the
    // `binder_type` and the `next` continuation. The application of
    // these arguments creates important unifications, listed below.
    // Additionally, we also create a unification to unify the `now`
    // type with the result of the `bind` application.
    //
    //   expression_type := Effect Int
    //   binder_type     := ?a
    //   now             := ?0
    //   next            := ?1
    //   lambda_type     := ?a -> ?1
    //
    //   bind_type       := m a -> (a -> m b) -> m b
    //                   |
    //                   := apply(expression_type)
    //                   := (Int -> Effect ?r1) -> Effect ?r1
    //                   |
    //                   := apply(lambda_type)
    //                   := Effect ?r1
    //                   |
    //                   >> ?a := Int
    //                   >> ?1 := Effect ?r1
    //                   >> ?0 := Effect ?r1
    //
    // For the second statement, we know the following information.
    // The `now` type was already solved by the previous statement,
    // and an error should surface once we check the inferred type
    // of the statement against it.
    //
    //   expression_type := Aff Int
    //   binder_type     := ?b
    //   now             := ?1 := Effect ?r1
    //   next            := ?2
    //   lambda_type     := ?b -> ?2
    //
    //   bind_type       := m a -> (a -> m b) -> m b
    //                   |
    //                   := apply(expression_type)
    //                   := (Int -> Aff ?r2) -> Aff ?r2
    //                   |
    //                   := apply(lambda_type)
    //                   := Aff ?r2
    //                   |
    //                   >> ?b := Int
    //                   >> ?2 := Aff ?r2
    //                   |
    //                   >> ?1         ~ Aff ?r2
    //                   >> Effect ?r1 ~ Aff ?r2
    //                   |
    //                   >> Oh no!
    //
    // This unification error is expected, but this left-to-right checking
    // approach significantly improves the reported error positions compared
    // to the previous approach that emulated desugared checking.

    let mut continuations = continuation_types.iter().tuple_windows::<(_, _)>();
    let mut checked_steps = vec![];

    for step in &steps {
        match step {
            DoStep::Let { statement, statements } => {
                let bindings = state
                    .with_error_crumb(ErrorCrumb::CheckingDoLet(*statement), |state| {
                        form_let::check_let_chunks(state, context, statements)
                    })?;
                checked_steps.push(CheckedDoStep::Let { bindings });
            }
            DoStep::Bind { statement, binder_type, binder, expression } => {
                let Some((&now_type, &next_type)) = continuations.next() else {
                    continue;
                };
                let Some(expression) = *expression else {
                    let lambda_type = context.intern_function(*binder_type, next_type);
                    checked_steps.push(CheckedDoStep::MissingApplication {
                        binder: *binder,
                        lambda_type,
                        result: now_type,
                    });
                    continue;
                };
                let function = bind_function
                    .as_ref()
                    .expect("invariant violated: desugared bind function was not initialised");
                let mut application =
                    state.with_error_crumb(ErrorCrumb::InferringDoBind(*statement), |state| {
                        let application = infer_do_bind_core(
                            state,
                            context,
                            function,
                            next_type,
                            expression,
                            *binder_type,
                        )?;
                        unification::subtype(state, context, application.result, now_type)?;
                        Ok(application)
                    })?;
                application.result = now_type;
                checked_steps.push(CheckedDoStep::Application { application, binder: *binder });
            }
            DoStep::Discard { statement, expression } => {
                let Some((&now_type, &next_type)) = continuations.next() else {
                    continue;
                };
                let Some(expression) = *expression else {
                    let binder_type = context.unknown("missing do discard binder");
                    let binder = state.allocate_generated_binder(
                        *statement,
                        binder_type,
                        tree::BinderKind::Wildcard,
                    );
                    let lambda_type = context.intern_function(binder_type, next_type);
                    checked_steps.push(CheckedDoStep::MissingApplication {
                        binder,
                        lambda_type,
                        result: now_type,
                    });
                    continue;
                };
                let function = discard_function
                    .as_ref()
                    .expect("invariant violated: desugared discard function was not initialised");
                let (binder, mut application) = state.with_error_crumb(
                    ErrorCrumb::InferringDoDiscard(*statement),
                    |state| {
                        let (binder, application) = infer_do_discard_core(
                            state, context, function, next_type, expression, *statement,
                        )?;
                        unification::subtype(state, context, application.result, now_type)?;
                        Ok((binder, application))
                    },
                )?;
                application.result = now_type;
                checked_steps.push(CheckedDoStep::Application { application, binder });
            }
        }
    }

    // The `first_continuation` is the overall type of the do expression,
    // built iteratively and through solving unification variables. On
    // the other hand, the `final_continuation` is the expected type for
    // the final statement in the do expression. If there is only a single
    // statement in the do expression, then these two bindings are equivalent.
    let first_continuation =
        *continuation_types.first().expect("invariant violated: empty continuation_types");
    let final_continuation =
        *continuation_types.last().expect("invariant violated: empty continuation_types");

    let final_expression = if let Some(final_expression) = final_expression {
        let checked =
            super::check_expression(state, context, final_expression, final_continuation)?;
        if let Some(CheckedDoStep::Application { application, .. }) = checked_steps
            .iter()
            .rev()
            .find(|step| matches!(step, CheckedDoStep::Application { .. }))
        {
            for constraint in &application.constraints {
                let mut crumbs = Vec::from(application.crumbs.as_ref());
                crumbs.push(ErrorCrumb::InferringExpression(final_expression));
                prim_effect::seed_continuation_origin(
                    state,
                    context,
                    *constraint,
                    Arc::from(crumbs),
                )?;
            }
        }
        checked
    } else {
        super::allocate_error_expression(state, final_continuation)
    };
    let mut continuation = ElaboratedExpression { type_id: final_continuation, ..final_expression };

    for step in checked_steps.into_iter().rev() {
        match step {
            CheckedDoStep::Application { application, binder } => {
                let kind = tree::ExpressionKind::Lambda {
                    binders: vec![binder].into(),
                    expression: continuation.expression,
                };
                let lambda = super::allocate_expression(state, application.lambda_type, kind);
                continuation = application::materialize_application(
                    state,
                    application.function,
                    application.implicit,
                    application.result,
                    lambda,
                );
            }
            CheckedDoStep::MissingApplication { binder, lambda_type, result } => {
                let kind = tree::ExpressionKind::Lambda {
                    binders: vec![binder].into(),
                    expression: continuation.expression,
                };
                let lambda = super::allocate_expression(state, lambda_type, kind);
                let function_type = context.intern_function(lambda_type, result);
                let function = super::allocate_error_expression(state, function_type);
                continuation =
                    application::materialize_application(state, function, vec![], result, lambda);
            }
            CheckedDoStep::Let { bindings } => {
                let kind =
                    tree::ExpressionKind::Let { bindings, expression: continuation.expression };
                continuation = super::allocate_expression(state, continuation.type_id, kind);
            }
        }
    }

    Ok(ElaboratedExpression { type_id: first_continuation, ..continuation })
}

fn infer_do_bind_core<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    function: &DesugaredFunction,
    continuation_type: TypeId,
    expression: lowering::ExpressionId,
    binder_type: TypeId,
) -> QueryResult<CheckedDoApplication>
where
    Q: ExternalQueries,
{
    let crumbs = Arc::from(state.crumbs.as_slice());
    let expression = super::infer_expression(state, context, expression)?;
    let lambda_type = context.intern_function(binder_type, continuation_type);

    let Some(application::UnanchoredApplication { implicit: first_implicit, argument, result }) =
        application::check_unanchored_application(state, context, function.type_id())?
    else {
        return Ok(invalid_do_application(state, context, lambda_type));
    };
    let mut constraints = first_implicit
        .iter()
        .filter_map(|implicit| match implicit {
            application::ImplicitApplication::Evidence { constraint, .. } => Some(*constraint),
            application::ImplicitApplication::Type { .. } => None,
        })
        .collect::<Vec<_>>();
    let expression = application::subtype_expression(state, context, expression, argument)?;
    let function = function.allocate_expression(state);
    let function =
        application::materialize_application(state, function, first_implicit, result, expression);

    let Some(application::UnanchoredApplication { implicit, argument, result }) =
        application::check_unanchored_application(state, context, function.type_id)?
    else {
        return Ok(invalid_do_application(state, context, lambda_type));
    };
    constraints.extend(implicit.iter().filter_map(|implicit| match implicit {
        application::ImplicitApplication::Evidence { constraint, .. } => Some(*constraint),
        application::ImplicitApplication::Type { .. } => None,
    }));
    unification::subtype(state, context, lambda_type, argument)?;

    Ok(CheckedDoApplication { function, implicit, constraints, result, lambda_type, crumbs })
}

fn infer_do_discard_core<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    function: &DesugaredFunction,
    continuation_type: TypeId,
    expression: lowering::ExpressionId,
    statement: lowering::DoStatementId,
) -> QueryResult<(tree::BinderId, CheckedDoApplication)>
where
    Q: ExternalQueries,
{
    let binder_type = state.fresh_unification(context.queries, context.prim.t);
    let binder =
        state.allocate_generated_binder(statement, binder_type, tree::BinderKind::Wildcard);
    let application =
        infer_do_bind_core(state, context, function, continuation_type, expression, binder_type)?;
    Ok((binder, application))
}

fn invalid_do_application(
    state: &mut CheckState,
    context: &CheckContext<impl ExternalQueries>,
    lambda_type: TypeId,
) -> CheckedDoApplication {
    let result = context.unknown("invalid function application");
    let function_type = context.intern_function(lambda_type, result);
    let function = super::allocate_error_expression(state, function_type);
    let crumbs = Arc::from(state.crumbs.as_slice());
    CheckedDoApplication {
        function,
        implicit: vec![],
        constraints: vec![],
        result,
        lambda_type,
        crumbs,
    }
}
