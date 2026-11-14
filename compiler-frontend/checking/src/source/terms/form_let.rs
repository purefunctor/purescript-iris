use std::sync::Arc;

use building_types::QueryResult;

use crate::context::CheckContext;
use crate::core::{Type, TypeId, exhaustive, normalise, skolem, unification};
use crate::error::ErrorCrumb;
use crate::source::terms::{ElaboratedExpression, application, equations, guarded};
use crate::source::{binder, types};
use crate::state::CheckState;
use crate::{ExternalQueries, tree};

#[derive(Copy, Clone, Debug)]
enum LetInMode {
    Infer,
    Check { expected: TypeId },
}

pub fn infer_let_in<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    bindings: &[lowering::LetBindingChunk],
    expression: Option<lowering::ExpressionId>,
) -> QueryResult<ElaboratedExpression>
where
    Q: ExternalQueries,
{
    let_in_core(state, context, bindings, expression, LetInMode::Infer)
}

pub fn check_let_in<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    bindings: &[lowering::LetBindingChunk],
    expression: Option<lowering::ExpressionId>,
    expected: TypeId,
) -> QueryResult<ElaboratedExpression>
where
    Q: ExternalQueries,
{
    let_in_core(state, context, bindings, expression, LetInMode::Check { expected })
}

fn let_in_core<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    bindings: &[lowering::LetBindingChunk],
    expression: Option<lowering::ExpressionId>,
    mode: LetInMode,
) -> QueryResult<ElaboratedExpression>
where
    Q: ExternalQueries,
{
    let bindings = check_let_chunks(state, context, bindings)?;

    let ElaboratedExpression { type_id, expression } = if let Some(expression) = expression {
        match mode {
            LetInMode::Infer => super::infer_expression(state, context, expression)?,
            LetInMode::Check { expected } => {
                super::check_expression(state, context, expression, expected)?
            }
        }
    } else {
        let type_id = context.unknown("missing let expression");
        let expression = state.allocate_error_expression(type_id);
        ElaboratedExpression { type_id, expression }
    };

    let kind = tree::ExpressionKind::Let { bindings, expression };
    Ok(super::allocate_expression(state, type_id, kind))
}

pub fn check_let_chunks<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    chunks: &[lowering::LetBindingChunk],
) -> QueryResult<tree::LetBindings>
where
    Q: ExternalQueries,
{
    let checked_chunks = chunks.iter().map(|chunk| match chunk {
        lowering::LetBindingChunk::Pattern { source, binder, where_expression } => {
            check_pattern_let_binding(state, context, *source, binder, where_expression)
        }
        lowering::LetBindingChunk::Names { bindings, scc } => {
            check_names_chunk(state, context, bindings, scc)
        }
    });
    let chunks = checked_chunks.collect::<QueryResult<Arc<[_]>>>()?;
    Ok(tree::LetBindings { chunks })
}

pub fn check_pattern_let_binding<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    source: lowering::LetBindingId,
    binder: &Option<lowering::BinderId>,
    where_expression: &Option<lowering::WhereExpression>,
) -> QueryResult<tree::LetBindingChunk>
where
    Q: ExternalQueries,
{
    let Some(where_expression) = where_expression else {
        return Ok(tree::LetBindingChunk::PatternError {
            source,
            binder_source: *binder,
            where_expression: None,
        });
    };

    let guarded::ElaboratedWhereExpression { type_id, where_expression } =
        guarded::infer_where_expression(state, context, where_expression)?;

    let Some(binder_id) = *binder else {
        return Ok(tree::LetBindingChunk::PatternError {
            source,
            binder_source: None,
            where_expression: Some(where_expression),
        });
    };

    let expression = ElaboratedExpression { type_id, expression: where_expression.expression };
    let expression = if binder::requires_instantiation(context, binder_id) {
        application::instantiate_expression(state, context, expression)?
    } else {
        application::collect_expression_wanteds(state, context, expression)?
    };
    let where_expression =
        tree::WhereExpression { expression: expression.expression, ..where_expression };

    let binder = binder::check_binder(state, context, binder_id, expression.type_id)?;

    let exhaustiveness =
        exhaustive::check_lambda_patterns(state, context, &[binder.type_id], &[binder_id])?;

    let has_missing = exhaustiveness.missing.is_some();
    state.report_exhaustiveness(exhaustiveness);

    if has_missing {
        state.push_wanted(context.prim.partial);
    }

    Ok(tree::LetBindingChunk::Pattern { source, binder: binder.binder, where_expression })
}

pub fn check_names_chunk<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    bindings: &[lowering::LetBindingNameGroupId],
    scc: &[lowering::Scc<lowering::LetBindingNameGroupId>],
) -> QueryResult<tree::LetBindingChunk>
where
    Q: ExternalQueries,
{
    for &id in bindings {
        let Some(name) = context.lowered.tree.get_let_binding(id) else {
            continue;
        };
        if let Some(signature_id) = name.signature {
            let (name_type, _) = types::check_kind(state, context, signature_id, context.prim.t)?;
            state.checked.node_types.lets.insert(id, name_type);
        } else {
            let name_type = state.fresh_unification(context.queries, context.prim.t);
            state.checked.node_types.lets.insert(id, name_type);
        }
    }

    for item in scc {
        match item {
            lowering::Scc::Base(id) | lowering::Scc::Recursive(id) => {
                let declaration = check_let_name_binding(state, context, *id)?;
                let declaration = state.checked.tree.insert_let(declaration);
                skolem::check_local(state, context, declaration)?;
            }
            lowering::Scc::Mutual(mutual) => {
                for &id in mutual {
                    let declaration = check_let_name_binding(state, context, id)?;
                    let declaration = state.checked.tree.insert_let(declaration);
                    skolem::check_local(state, context, declaration)?;
                }
            }
        }
    }

    let lookup = |source| {
        state
            .checked
            .tree
            .lookup_let(source)
            .expect("invariant violated: checked local declaration is missing")
    };

    let declarations = bindings.iter().copied().map(lookup);
    let declarations = declarations.collect();
    let groups = Arc::from(scc);

    Ok(tree::LetBindingChunk::Names { declarations, groups })
}

pub fn check_let_name_binding<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: lowering::LetBindingNameGroupId,
) -> QueryResult<tree::LocalDeclaration>
where
    Q: ExternalQueries,
{
    state.with_implication(|state| {
        state.with_error_crumb(ErrorCrumb::CheckingLetName(id), |state| {
            check_let_name_binding_core(state, context, id)
        })
    })
}

pub fn check_let_name_binding_core<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: lowering::LetBindingNameGroupId,
) -> QueryResult<tree::LocalDeclaration>
where
    Q: ExternalQueries,
{
    let group = context.lowered.tree.get_let_binding_group(id);
    let name = context
        .lowered
        .tree
        .get_let_binding(id)
        .expect("invariant violated: let binding group has no lowered binding");
    let name_type = state
        .checked
        .node_types
        .lookup_let(id)
        .expect("invariant violated: let binding has no preallocated type");

    let checked_equations = if let Some(signature_id) = name.signature {
        equations::check_value_equations(
            state,
            context,
            equations::EquationTypeOrigin::Explicit(signature_id),
            name_type,
            &name.equations,
        )?
    } else {
        if let [equation] = name.equations.as_ref()
            && let [equation_source] = group.equations.as_ref()
            && equation.binders.is_empty()
            && let Some(guarded) = &equation.guarded
        {
            let inferred = guarded::infer_guarded_expression(state, context, guarded)?;
            let inferred_type = inferred.type_id;
            // Keep simple let bindings e.g. `appendLocal = append` polymorphic.
            let expanded_name_type = normalise::expand(state, context, name_type)?;
            if matches!(context.lookup_type(expanded_name_type), Type::Unification(_)) {
                unification::unify(state, context, expanded_name_type, inferred_type)?;
            } else {
                unification::subtype(state, context, inferred_type, expanded_name_type)?;
            }

            let declaration = tree::LocalDeclaration::nullary(
                id,
                name_type,
                *equation_source,
                inferred.guarded_expression,
            );
            return Ok(declaration);
        } else {
            equations::infer_value_equations(state, context, name_type, &name.equations)?
        }
    };

    let exhaustiveness = exhaustive::check_equation_patterns(
        state,
        context,
        &checked_equations.patterns,
        &name.equations,
    )?;
    state.report_exhaustiveness(exhaustiveness);

    assert_eq!(
        group.equations.len(),
        checked_equations.equations.len(),
        "invariant violated: checked local equation count does not match lowering",
    );

    let equations = std::iter::zip(group.equations.iter().copied(), checked_equations.equations)
        .map(|(source, equation)| equation.into_local_tree(source));

    let equations = equations.collect();
    let abstractions = checked_equations.abstractions.into();

    Ok(tree::LocalDeclaration::new(id, name_type, abstractions, equations))
}
