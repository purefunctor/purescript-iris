use std::sync::Arc;

use building_types::QueryResult;
use itertools::{EitherOrBoth, Itertools};

use crate::ExternalQueries;
use crate::context::CheckContext;
use crate::core::{normalise, skolem, unify};
use crate::error::ErrorKind;
use crate::state::{CheckState, Given};
use crate::tree::{self, Evidence, ExpressionKind};
use crate::types::{RowField, Type, TypeId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Elaborate,
    NoElaborate,
}

pub fn subsume_expression<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    expression: tree::ExpressionId,
    inferred: TypeId,
    expected: TypeId,
) -> QueryResult<tree::ExpressionId>
where
    Q: ExternalQueries,
{
    let expected_head = normalise::normalise(state, context, expected);
    if let Type::Forall(..) | Type::Constrained(..) = context.lookup_type(expected_head) {
        return abstract_expression(state, context, expected_head, |state, inner| {
            subsume_expression(state, context, expression, inferred, inner)
        });
    }

    let inferred_head = normalise::normalise(state, context, inferred);
    if let Type::Forall(..) | Type::Constrained(..) = context.lookup_type(inferred_head) {
        let instantiated = skolem::instantiate(state, context, inferred_head)?;
        let evidence =
            instantiated.constraints.iter().map(|&constraint| state.emit_wanted(constraint));
        let evidence = evidence.collect::<Arc<[_]>>();
        let kind = ExpressionKind::Instantiate {
            expression,
            type_arguments: Arc::from(instantiated.arguments),
            evidence,
        };
        let instantiated_type = instantiated.inner;
        let expression = state.checked.tree.allocate_expression(None, instantiated_type, kind);
        return subsume_expression(state, context, expression, instantiated_type, expected_head);
    }

    subsume(state, context, Mode::NoElaborate, inferred_head, expected_head)?;
    Ok(expression)
}

pub fn abstract_over<Q, T>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    expected: TypeId,
    action: impl FnOnce(&mut CheckState, TypeId) -> QueryResult<T>,
) -> QueryResult<(tree::Abstraction, T)>
where
    Q: ExternalQueries,
{
    state.with_level(|state| {
        let skolemised = skolem::skolemise(state, context, expected);
        let type_binders = skolemised.rigids.iter().map(|&(_, rigid)| rigid);
        let type_binders = type_binders.collect::<Arc<[_]>>();
        let evidence = bind_givens(state, &skolemised.constraints);
        let givens = given_list(&evidence);
        let value = state.with_givens(givens, |state| action(state, skolemised.inner))?;
        let abstraction = tree::Abstraction { type_binders, evidence: Arc::from(evidence) };
        Ok((abstraction, value))
    })
}

pub fn abstract_expression<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    expected: TypeId,
    action: impl FnOnce(&mut CheckState, TypeId) -> QueryResult<tree::ExpressionId>,
) -> QueryResult<tree::ExpressionId>
where
    Q: ExternalQueries,
{
    let (abstraction, expression) = abstract_over(state, context, expected, action)?;
    let tree::Abstraction { type_binders, evidence } = abstraction;
    let kind = ExpressionKind::Abstract { type_binders, evidence, expression };
    Ok(state.checked.tree.allocate_expression(None, expected, kind))
}

pub fn bind_givens(
    state: &mut CheckState,
    constraints: &[TypeId],
) -> Vec<(TypeId, tree::EvidenceBinderId)> {
    let evidence =
        constraints.iter().map(|&constraint| (constraint, state.fresh_evidence_binder()));
    evidence.collect_vec()
}

pub fn given_list(evidence: &[(TypeId, tree::EvidenceBinderId)]) -> Vec<Given> {
    let givens = evidence
        .iter()
        .map(|&(constraint, binder)| Given { constraint, evidence: Evidence::Binder(binder) });
    givens.collect_vec()
}

pub fn subsume<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    mode: Mode,
    inferred: TypeId,
    expected: TypeId,
) -> QueryResult<bool>
where
    Q: ExternalQueries,
{
    let inferred = normalise::normalise(state, context, inferred);
    let expected = normalise::normalise(state, context, expected);

    if let Type::Forall(..) = context.lookup_type(expected) {
        return state.with_level(|state| {
            let skolemised = skolem::skolemise(state, context, expected);
            if !skolemised.constraints.is_empty() {
                return unify::unify(state, context, expected, inferred);
            }
            subsume(state, context, mode, inferred, skolemised.inner)
        });
    }

    if let Type::Forall(..) = context.lookup_type(inferred) {
        let instantiated = skolem::instantiate(state, context, inferred)?;
        if !instantiated.constraints.is_empty() {
            if mode == Mode::Elaborate {
                for &constraint in &instantiated.constraints {
                    state.emit_wanted(constraint);
                }
            } else {
                return unify::unify(state, context, expected, inferred);
            }
        }
        return subsume(state, context, mode, instantiated.inner, expected);
    }

    let inferred_function = normalise::function_parts(state, context, inferred);
    let expected_function = normalise::function_parts(state, context, expected);
    if let (
        Some((inferred_argument, inferred_result)),
        Some((expected_argument, expected_result)),
    ) = (inferred_function, expected_function)
    {
        if !subsume(state, context, Mode::NoElaborate, expected_argument, inferred_argument)? {
            return Ok(false);
        }
        return subsume(state, context, Mode::NoElaborate, inferred_result, expected_result);
    }

    if let (Some(inferred_row), Some(expected_row)) =
        (record_row(state, context, inferred), record_row(state, context, expected))
    {
        return subsume_records(state, context, inferred_row, expected_row);
    }

    unify::unify(state, context, expected, inferred)
}

fn record_row<Q>(state: &CheckState, context: &CheckContext<Q>, id: TypeId) -> Option<TypeId>
where
    Q: ExternalQueries,
{
    let Type::Application(function, row) = context.lookup_type(id) else {
        return None;
    };
    let function = normalise::normalise(state, context, function);
    (function == context.types.record).then_some(row)
}

fn subsume_records<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    inferred_row: TypeId,
    expected_row: TypeId,
) -> QueryResult<bool>
where
    Q: ExternalQueries,
{
    let inferred_parts = normalise::row_parts(state, context, inferred_row);
    let expected_parts = normalise::row_parts(state, context, expected_row);

    let mut inferred_rest = Vec::new();
    let mut expected_rest = Vec::new();
    let mut common = Vec::new();

    let fields = inferred_parts
        .fields
        .iter()
        .merge_join_by(expected_parts.fields.iter(), |inferred, expected| {
            inferred.label.cmp(&expected.label)
        });
    for field in fields {
        match field {
            EitherOrBoth::Both(inferred, expected) => common.push((inferred.id, expected.id)),
            EitherOrBoth::Left(inferred) => inferred_rest.push(RowField::clone(inferred)),
            EitherOrBoth::Right(expected) => expected_rest.push(RowField::clone(expected)),
        }
    }

    if inferred_parts.tail.is_none() && expected_parts.tail.is_none() {
        if let Some(missing) = expected_rest.first() {
            let label = smol_str::SmolStr::clone(&missing.label);
            state.insert_error(ErrorKind::PropertyIsMissing { label });
            return Ok(false);
        }
        if let Some(additional) = inferred_rest.first() {
            let label = smol_str::SmolStr::clone(&additional.label);
            state.insert_error(ErrorKind::AdditionalProperty { label });
            return Ok(false);
        }
    }

    for (inferred, expected) in common {
        if !subsume(state, context, Mode::NoElaborate, inferred, expected)? {
            return Ok(false);
        }
    }

    let inferred_rest = context.intern_row(inferred_rest, inferred_parts.tail);
    let expected_rest = context.intern_row(expected_rest, expected_parts.tail);

    let inferred_tail = normalise::row_parts(state, context, inferred_rest).tail;
    let expected_tail = normalise::row_parts(state, context, expected_rest).tail;
    let tails_solved = inferred_tail != inferred_parts.tail || expected_tail != expected_parts.tail;
    if tails_solved {
        return subsume_records(state, context, inferred_rest, expected_rest);
    }

    unify::unify(state, context, expected_rest, inferred_rest)
}
