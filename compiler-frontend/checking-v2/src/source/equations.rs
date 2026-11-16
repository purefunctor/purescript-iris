use std::sync::Arc;

use building_types::QueryResult;
use itertools::Itertools;
use lowering::{
    CaseBranch, DoStatement, Equation, GuardedExpression, LetBindingChunk, PatternGuarded,
    TermVariableResolution, WhereExpression,
};
use smol_str::SmolStr;

use crate::ExternalQueries;
use crate::context::CheckContext;
use crate::core::{kind, normalise, skolem, subsume, unify};
use crate::error::{ErrorCrumb, ErrorKind};
use crate::source::terms::{self, Argument, Elaborated};
use crate::source::{binders, types};
use crate::state::{CheckState, GroupMember, LocalVariable, TermVariableKey};
use crate::tree::{self, TermReference};
use crate::types::{Type, TypeId};

pub fn check_value<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    signature: Option<TypeId>,
    pending: TypeId,
    equations: &[Equation],
) -> QueryResult<tree::ValueDeclaration>
where
    Q: ExternalQueries,
{
    let Some(signature) = signature else {
        let (parameters, equations) = check_equations(state, context, equations, pending)?;
        return Ok(tree::ValueDeclaration {
            type_id: pending,
            type_binders: Arc::from([]),
            evidence: Arc::from([]),
            parameters: Arc::from(parameters),
            equations: Arc::from(equations),
        });
    };

    state.with_level(|state| {
        let skolemised = skolem::skolemise(state, context, signature);
        let type_binders = skolemised.rigids.iter().map(|&(_, rigid)| rigid).collect_vec();
        let scoped =
            skolemised.rigids.iter().map(|&(binder, rigid)| (binder.name, rigid)).collect();
        skolem::scope_rigids(state, context, &scoped);
        let evidence = subsume::bind_givens(state, &skolemised.constraints);
        let givens = subsume::given_list(&evidence);
        let (parameters, equations) = state.with_givens(givens, |state| {
            let checked = check_equations(state, context, equations, skolemised.inner)?;
            crate::core::entail::solve_wanteds(state, context)?;
            QueryResult::Ok(checked)
        })?;
        Ok(tree::ValueDeclaration {
            type_id: signature,
            type_binders: Arc::from(type_binders),
            evidence: Arc::from(evidence),
            parameters: Arc::from(parameters),
            equations: Arc::from(equations),
        })
    })
}

pub fn check_equations<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    equations: &[Equation],
    expected: TypeId,
) -> QueryResult<(Vec<tree::Parameter>, Vec<tree::Equation>)>
where
    Q: ExternalQueries,
{
    let arity = equations.first().map_or(0, |equation| equation.binders.len());
    let mut arguments = Vec::with_capacity(arity);
    if arity == 0 {
        return check_parameters(state, context, equations, arity, &mut arguments, expected);
    }
    state.with_group_visible(|state| {
        check_parameters(state, context, equations, arity, &mut arguments, expected)
    })
}

fn check_parameters<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    equations: &[Equation],
    arity: usize,
    arguments: &mut Vec<TypeId>,
    expected: TypeId,
) -> QueryResult<(Vec<tree::Parameter>, Vec<tree::Equation>)>
where
    Q: ExternalQueries,
{
    if arguments.len() == arity {
        let equations = check_bodies(state, context, equations, arguments, expected)?;
        return Ok((Vec::new(), equations));
    }

    let expected_head = normalise::normalise(state, context, expected);
    if let Type::Forall(..) | Type::Constrained(..) = context.lookup_type(expected_head) {
        let (abstraction, (mut parameters, equations)) =
            subsume::abstract_over(state, context, expected_head, |state, inner| {
                check_parameters(state, context, equations, arity, arguments, inner)
            })?;
        parameters.insert(0, tree::Parameter::Abstract(abstraction));
        return Ok((parameters, equations));
    }

    let (argument, result) = terms::expect_function(state, context, expected_head)?;
    arguments.push(argument);
    let (mut parameters, equations) =
        check_parameters(state, context, equations, arity, arguments, result)?;
    parameters.insert(0, tree::Parameter::Argument(argument));
    Ok((parameters, equations))
}

fn check_bodies<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    equations: &[Equation],
    arguments: &[TypeId],
    result: TypeId,
) -> QueryResult<Vec<tree::Equation>>
where
    Q: ExternalQueries,
{
    let arity = arguments.len();
    let mut checked = Vec::with_capacity(equations.len());
    for equation in equations {
        if equation.binders.len() != arity {
            let expected = arity;
            let actual = equation.binders.len();
            state.insert_error(ErrorKind::ArgListLengthsDiffer { expected, actual });
            continue;
        }

        binders::check_overlapping_names(state, context, &equation.binders);

        let binders =
            equation.binders.iter().zip(arguments).map(|(&binder, &argument)| {
                binders::check_binder(state, context, binder, argument)
            });
        let binders = binders.collect::<QueryResult<Vec<_>>>()?;

        let guarded = match &equation.guarded {
            Some(guarded) => check_guarded(state, context, guarded, result)?,
            None => {
                tree::Guarded::Unconditional(terms::error_expression(state, context).expression)
            }
        };

        checked.push(tree::Equation { binders: Arc::from(binders), guarded });
    }

    Ok(checked)
}

pub fn check_guarded<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    guarded: &GuardedExpression,
    expected: TypeId,
) -> QueryResult<tree::Guarded>
where
    Q: ExternalQueries,
{
    match guarded {
        GuardedExpression::Unconditional { where_expression } => {
            let expression = check_where(state, context, where_expression.as_ref(), expected)?;
            Ok(tree::Guarded::Unconditional(expression))
        }
        GuardedExpression::Conditionals { pattern_guarded } => {
            let branches = pattern_guarded
                .iter()
                .map(|branch| check_pattern_guarded(state, context, branch, expected));
            let branches = branches.collect::<QueryResult<Vec<_>>>()?;
            Ok(tree::Guarded::Conditionals(Arc::from(branches)))
        }
    }
}

fn check_pattern_guarded<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    branch: &PatternGuarded,
    expected: TypeId,
) -> QueryResult<tree::GuardedExpression>
where
    Q: ExternalQueries,
{
    let pattern_binders = branch.pattern_guards.iter().filter_map(|guard| guard.binder);
    let pattern_binders = pattern_binders.collect_vec();
    binders::check_overlapping_names(state, context, &pattern_binders);

    let mut guards = Vec::with_capacity(branch.pattern_guards.len());
    for guard in branch.pattern_guards.iter() {
        let Some(expression) = guard.expression else {
            continue;
        };
        let Some(binder) = guard.binder else {
            let condition =
                terms::check_expression(state, context, expression, context.types.boolean)?;
            guards.push(tree::Guard::Boolean(condition));
            continue;
        };
        let scrutinee = terms::infer_expression(state, context, expression)?;
        let scrutinee = instantiate_for_binder(state, context, scrutinee, binder)?;
        let binder = binders::check_binder(state, context, binder, scrutinee.type_id)?;
        guards.push(tree::Guard::Pattern(binder, scrutinee.expression));
    }

    let expression = check_where(state, context, branch.where_expression.as_ref(), expected)?;
    Ok(tree::GuardedExpression { guards: Arc::from(guards), expression })
}

fn instantiate_for_binder<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    scrutinee: Elaborated,
    binder: lowering::BinderId,
) -> QueryResult<Elaborated>
where
    Q: ExternalQueries,
{
    if binders::binder_requires_monotype(context, binder) {
        terms::instantiate(state, context, scrutinee)
    } else {
        Ok(scrutinee)
    }
}

fn check_where<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    where_expression: Option<&WhereExpression>,
    expected: TypeId,
) -> QueryResult<tree::ExpressionId>
where
    Q: ExternalQueries,
{
    let Some(where_expression) = where_expression else {
        return Ok(terms::error_expression(state, context).expression);
    };

    let expected_head = normalise::normalise(state, context, expected);
    if let Type::Forall(..) | Type::Constrained(..) = context.lookup_type(expected_head) {
        return subsume::abstract_expression(state, context, expected_head, |state, inner| {
            check_where(state, context, Some(where_expression), inner)
        });
    }

    let bindings = check_let_chunks(state, context, &where_expression.bindings)?;
    let body = match where_expression.expression {
        Some(expression) => terms::check_expression(state, context, expression, expected)?,
        None => terms::error_expression(state, context).expression,
    };

    if bindings.is_empty() {
        return Ok(body);
    }
    let kind = tree::ExpressionKind::Let { bindings: Arc::from(bindings), body };
    Ok(state.checked.tree.allocate_expression(None, expected, kind))
}

pub fn check_let_chunks<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    chunks: &[LetBindingChunk],
) -> QueryResult<Vec<tree::LocalBinding>>
where
    Q: ExternalQueries,
{
    let mut bindings = Vec::new();
    for chunk in chunks {
        match chunk {
            LetBindingChunk::Pattern { binder, where_expression, .. } => {
                let Some(binder) = *binder else {
                    continue;
                };
                let value = match where_expression {
                    Some(where_expression) => {
                        let value_type = kind::fresh_type(state, context);
                        let value =
                            check_where(state, context, Some(where_expression), value_type)?;
                        Elaborated { expression: value, type_id: value_type }
                    }
                    None => terms::error_expression(state, context),
                };
                let value = instantiate_for_binder(state, context, value, binder)?;
                let binder = binders::check_binder(state, context, binder, value.type_id)?;
                bindings.push(tree::LocalBinding::Pattern { binder, expression: value.expression });
            }
            LetBindingChunk::Names { bindings: names, scc } => {
                check_let_names(state, context, names, scc, &mut bindings)?;
            }
        }
    }
    Ok(bindings)
}

fn check_let_names<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    names: &[lowering::LetBindingNameGroupId],
    scc: &[lowering::Scc<lowering::LetBindingNameGroupId>],
    bindings: &mut Vec<tree::LocalBinding>,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let mut signatures = Vec::with_capacity(names.len());
    for &group in names {
        let Some(binding) = context.lowered.tree.get_let_binding(group) else {
            continue;
        };
        let signature = match binding.signature {
            Some(signature) => state.with_crumb(ErrorCrumb::LetBinding(group), |state| {
                types::check_type(state, context, signature, context.types.type_).map(Some)
            })?,
            None => None,
        };
        let type_id = match signature {
            Some(signature) => signature,
            None => kind::fresh_type(state, context),
        };
        let reference = TermReference::Let(group);
        state
            .term_variables
            .insert(TermVariableKey::Let(group), LocalVariable { type_id, reference });
        signatures.push((group, signature, type_id));
    }

    for component in scc {
        let undefined = match component {
            lowering::Scc::Base(_) => Vec::new(),
            component => {
                component.as_slice().iter().map(|&group| GroupMember::Let(group)).collect_vec()
            }
        };
        for &group in component.as_slice() {
            let Some(&(_, signature, type_id)) = signatures.iter().find(|(id, _, _)| *id == group)
            else {
                continue;
            };
            let Some(binding) = context.lowered.tree.get_let_binding(group) else {
                continue;
            };
            let value = state.with_crumb(ErrorCrumb::LetBinding(group), |state| {
                state.with_undefined_members(&undefined, |state| {
                    check_value(state, context, signature, type_id, &binding.equations)
                })
            })?;
            let name = context.lowered.tree.get_let_binding_group(group).name.clone();
            let name = name.unwrap_or_else(|| SmolStr::new_static("_"));
            bindings.push(tree::LocalBinding::Value(tree::LocalValue {
                source: group,
                name,
                value,
            }));
        }
    }

    Ok(())
}

pub fn check_case<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: lowering::ExpressionId,
    trunk: &[lowering::ExpressionId],
    branches: &[CaseBranch],
    expected: TypeId,
) -> QueryResult<tree::ExpressionId>
where
    Q: ExternalQueries,
{
    let mut scrutinees = Vec::with_capacity(trunk.len());
    for (index, &scrutinee) in trunk.iter().enumerate() {
        let inferred = terms::infer_expression(state, context, scrutinee)?;
        let requires_monotype = branches.iter().any(|branch| {
            let binder = branch.binders.get(index).copied();
            binder.is_some_and(|binder| binders::binder_requires_monotype(context, binder))
        });
        let inferred = if requires_monotype {
            terms::instantiate(state, context, inferred)?
        } else {
            inferred
        };
        scrutinees.push(inferred);
    }

    let mut checked = Vec::with_capacity(branches.len());
    for branch in branches {
        if branch.binders.len() != scrutinees.len() {
            let expected = scrutinees.len();
            let actual = branch.binders.len();
            state.insert_error(ErrorKind::ArgListLengthsDiffer { expected, actual });
            continue;
        }

        binders::check_overlapping_names(state, context, &branch.binders);

        let binders = branch.binders.iter().zip(&scrutinees).map(|(&binder, scrutinee)| {
            binders::check_binder(state, context, binder, scrutinee.type_id)
        });
        let binders = binders.collect::<QueryResult<Vec<_>>>()?;

        let guarded = match &branch.guarded_expression {
            Some(guarded) => check_guarded(state, context, guarded, expected)?,
            None => {
                tree::Guarded::Unconditional(terms::error_expression(state, context).expression)
            }
        };

        checked.push(tree::CaseBranch { binders: Arc::from(binders), guarded });
    }

    let scrutinees = scrutinees.iter().map(|scrutinee| scrutinee.expression).collect::<Arc<[_]>>();
    let kind = tree::ExpressionKind::Case { scrutinees, branches: Arc::from(checked) };
    Ok(state.checked.tree.allocate_expression(Some(id), expected, kind))
}

fn rebound_function<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    resolution: Option<TermVariableResolution>,
) -> QueryResult<Elaborated>
where
    Q: ExternalQueries,
{
    match resolution {
        Some(resolution) => terms::variable_reference(state, context, None, resolution),
        None => Ok(terms::error_expression(state, context)),
    }
}

fn lambda<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    binder: tree::BinderId,
    argument: TypeId,
    body: Elaborated,
) -> Elaborated
where
    Q: ExternalQueries,
{
    let function_type = context.intern_function(argument, body.type_id);
    let kind = tree::ExpressionKind::Lambda { binder, body: body.expression };
    terms::allocate(state, None, function_type, kind)
}

pub fn infer_do<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: lowering::ExpressionId,
    bind: Option<TermVariableResolution>,
    discard: Option<TermVariableResolution>,
    statements: &[lowering::DoStatementId],
) -> QueryResult<Elaborated>
where
    Q: ExternalQueries,
{
    let statements = statements.iter().filter_map(|&statement| {
        let kind = context.lowered.tree.get_do_statement(statement)?;
        Some((statement, kind))
    });
    let statements = statements.collect_vec();

    let Some(result) = infer_do_statements(state, context, bind, discard, &statements)? else {
        state.insert_error(ErrorKind::InvalidSyntax);
        return Ok(terms::error_expression(state, context));
    };
    state.checked.tree.expressions[result.expression].source = Some(id);
    Ok(result)
}

fn infer_do_statements<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    bind: Option<TermVariableResolution>,
    discard: Option<TermVariableResolution>,
    statements: &[(lowering::DoStatementId, &DoStatement)],
) -> QueryResult<Option<Elaborated>>
where
    Q: ExternalQueries,
{
    let [(statement_id, statement), rest @ ..] = statements else {
        return Ok(None);
    };

    let result =
        state.with_crumb(ErrorCrumb::DoStatement(*statement_id), |state| match statement {
            DoStatement::Discard { expression } if rest.is_empty() => {
                let Some(expression) = *expression else {
                    return Ok(Some(terms::error_expression(state, context)));
                };
                let inferred = terms::infer_expression(state, context, expression)?;
                terms::instantiate(state, context, inferred).map(Some)
            }

            DoStatement::Discard { expression } => {
                let function = rebound_function(state, context, discard)?;
                let argument = match expression {
                    Some(expression) => Argument::Expression(*expression),
                    None => Argument::Missing,
                };
                let function = terms::apply(state, context, function, argument)?;
                let (continuation, _) = terms::expect_function(state, context, function.type_id)?;
                let (argument_type, _) = terms::expect_function(state, context, continuation)?;
                let binder_kind = tree::BinderKind::Wildcard;
                let binder = state.checked.tree.allocate_binder(None, argument_type, binder_kind);
                let body = state.with_group_visible(|state| {
                    infer_do_statements(state, context, bind, discard, rest)
                })?;
                let Some(body) = body else {
                    return Ok(None);
                };
                let continuation = lambda(state, context, binder, argument_type, body);
                terms::apply_elaborated(state, context, function, continuation).map(Some)
            }

            DoStatement::Bind { binder, expression } => {
                let function = rebound_function(state, context, bind)?;
                let argument = match expression {
                    Some(expression) => Argument::Expression(*expression),
                    None => Argument::Missing,
                };
                let function = terms::apply(state, context, function, argument)?;
                let (continuation, _) = terms::expect_function(state, context, function.type_id)?;
                let (argument_type, _) = terms::expect_function(state, context, continuation)?;
                let binder = match binder {
                    Some(binder) => binders::check_binder(state, context, *binder, argument_type)?,
                    None => state.checked.tree.allocate_binder(
                        None,
                        argument_type,
                        tree::BinderKind::Error,
                    ),
                };
                let body = state.with_group_visible(|state| {
                    infer_do_statements(state, context, bind, discard, rest)
                })?;
                let Some(body) = body else {
                    return Ok(None);
                };
                let continuation = lambda(state, context, binder, argument_type, body);
                terms::apply_elaborated(state, context, function, continuation).map(Some)
            }

            DoStatement::Let { statements } => {
                let bindings = check_let_chunks(state, context, statements)?;
                let Some(body) = infer_do_statements(state, context, bind, discard, rest)? else {
                    return Ok(None);
                };
                let kind = tree::ExpressionKind::Let {
                    bindings: Arc::from(bindings),
                    body: body.expression,
                };
                Ok(Some(terms::allocate(state, None, body.type_id, kind)))
            }
        })?;

    Ok(result)
}

type AdoFunctions = (
    Option<TermVariableResolution>,
    Option<TermVariableResolution>,
    Option<TermVariableResolution>,
);

pub fn infer_ado<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: lowering::ExpressionId,
    (map, apply, pure): AdoFunctions,
    statements: &[lowering::DoStatementId],
    expression: Option<lowering::ExpressionId>,
) -> QueryResult<Elaborated>
where
    Q: ExternalQueries,
{
    let mut actions = Vec::new();
    let mut lets = Vec::new();
    for &statement_id in statements {
        let Some(statement) = context.lowered.tree.get_do_statement(statement_id) else {
            continue;
        };
        match statement {
            DoStatement::Bind { binder, expression } => {
                actions.push((statement_id, *binder, *expression))
            }
            DoStatement::Discard { expression } => actions.push((statement_id, None, *expression)),
            DoStatement::Let { statements } => lets.push((statement_id, actions.len(), statements)),
        }
    }

    let mut inferred_actions = Vec::with_capacity(actions.len());
    for &(statement_id, _, expression) in &actions {
        let inferred =
            state.with_crumb(ErrorCrumb::DoStatement(statement_id), |state| match expression {
                Some(expression) => terms::infer_expression(state, context, expression),
                None => Ok(terms::error_expression(state, context)),
            })?;
        let inferred = terms::instantiate(state, context, inferred)?;
        inferred_actions.push(inferred);
    }

    let mut parameters = Vec::with_capacity(actions.len());
    for (&(_, binder, _), action) in actions.iter().zip(&inferred_actions) {
        let element = kind::fresh_type(state, context);
        let functor = kind::fresh_unification(
            state,
            context,
            context.intern_function(context.types.type_, context.types.type_),
        );
        let expected_action = context.intern_application(functor, element);
        unify::unify(state, context, expected_action, action.type_id)?;
        let binder = match binder {
            Some(binder) => binders::check_binder(state, context, binder, element)?,
            None => state.checked.tree.allocate_binder(None, element, tree::BinderKind::Wildcard),
        };
        parameters.push((binder, element));
    }

    let mut let_bindings = Vec::new();
    for &(statement_id, position, statements) in &lets {
        let bindings = state.with_crumb(ErrorCrumb::DoStatement(statement_id), |state| {
            if position == 0 {
                return check_let_chunks(state, context, statements);
            }
            state.with_group_visible(|state| check_let_chunks(state, context, statements))
        })?;
        let_bindings.extend(bindings);
    }

    let infer_body = |state: &mut CheckState| match expression {
        Some(expression) => {
            let inferred = terms::infer_expression(state, context, expression)?;
            terms::instantiate(state, context, inferred)
        }
        None => Ok(terms::error_expression(state, context)),
    };
    let body = if actions.is_empty() {
        infer_body(state)?
    } else {
        state.with_group_visible(infer_body)?
    };
    let body = if let_bindings.is_empty() {
        body
    } else {
        let kind =
            tree::ExpressionKind::Let { bindings: Arc::from(let_bindings), body: body.expression };
        terms::allocate(state, None, body.type_id, kind)
    };

    if inferred_actions.is_empty() {
        let function = rebound_function(state, context, pure)?;
        let result = terms::apply_elaborated(state, context, function, body)?;
        state.checked.tree.expressions[result.expression].source = Some(id);
        return Ok(result);
    }

    let function = parameters
        .iter()
        .rev()
        .fold(body, |body, &(binder, argument)| lambda(state, context, binder, argument, body));

    let map_function = rebound_function(state, context, map)?;
    let mut actions = inferred_actions.into_iter();
    let first = actions.next().expect("invariant violated: ado without actions");
    let mapped = terms::apply_elaborated(state, context, map_function, function)?;
    let mut result = terms::apply_elaborated(state, context, mapped, first)?;
    for action in actions {
        let apply_function = rebound_function(state, context, apply)?;
        let applied = terms::apply_elaborated(state, context, apply_function, result)?;
        result = terms::apply_elaborated(state, context, applied, action)?;
    }

    state.checked.tree.expressions[result.expression].source = Some(id);
    Ok(result)
}
