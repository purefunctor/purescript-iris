use building_types::QueryResult;
use itertools::Itertools;

use crate::ExternalQueries;
use crate::context::CheckContext;
use crate::core::entail::{ConstraintHead, Outcome};
use crate::core::normalise;
use crate::state::{CheckState, Wanted};
use crate::types::{RowField, Type};

use super::{equate, intern_constraint, no_instance, push_subgoal, solved, symbol};

pub fn solve_cons<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    wanted: &Wanted,
    head: &ConstraintHead,
) -> QueryResult<Outcome>
where
    Q: ExternalQueries,
{
    let &[label, value, tail, row] = head.arguments.as_slice() else {
        return Ok(no_instance(state, context, wanted));
    };

    let Some(label) = symbol(state, context, label) else {
        return Ok(Outcome::Stuck);
    };

    let result = context.intern_row(vec![RowField { label, id: value }], Some(tail));
    equate(state, context, wanted, head.class, &[(row, result)])
}

pub fn solve_lacks<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    wanted: &Wanted,
    head: &ConstraintHead,
) -> QueryResult<Outcome>
where
    Q: ExternalQueries,
{
    let &[label_type, row] = head.arguments.as_slice() else {
        return Ok(no_instance(state, context, wanted));
    };

    let Some(label) = symbol(state, context, label_type) else {
        return Ok(Outcome::Stuck);
    };

    let parts = normalise::row_parts(state, context, row);
    if parts.fields.iter().any(|field| field.label == label) {
        return Ok(no_instance(state, context, wanted));
    }

    let Some(tail) = parts.tail else {
        return Ok(solved(state, wanted, head.class));
    };
    let tail = normalise::normalise(state, context, tail);
    if parts.fields.is_empty() || matches!(context.lookup_type(tail), Type::Unification(_)) {
        return Ok(Outcome::Stuck);
    }

    let subgoal = intern_constraint(context, head, &[label_type, tail]);
    push_subgoal(state, wanted, subgoal);
    Ok(solved(state, wanted, head.class))
}

pub fn solve_union<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    wanted: &Wanted,
    head: &ConstraintHead,
) -> QueryResult<Outcome>
where
    Q: ExternalQueries,
{
    let &[left, right, union] = head.arguments.as_slice() else {
        return Ok(no_instance(state, context, wanted));
    };

    let left_parts = normalise::row_parts(state, context, left);
    if left_parts.tail.is_some() {
        return Ok(Outcome::Stuck);
    }

    let right_parts = normalise::row_parts(state, context, right);
    let fields = left_parts.fields.iter().chain(right_parts.fields.iter());
    let fields = fields.map(RowField::clone).collect_vec();

    let result = context.intern_row(fields, right_parts.tail);
    equate(state, context, wanted, head.class, &[(union, result)])
}

pub fn solve_nub<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    wanted: &Wanted,
    head: &ConstraintHead,
) -> QueryResult<Outcome>
where
    Q: ExternalQueries,
{
    let &[input, output] = head.arguments.as_slice() else {
        return Ok(no_instance(state, context, wanted));
    };

    let input_parts = normalise::row_parts(state, context, input);
    if input_parts.tail.is_some() {
        return Ok(Outcome::Stuck);
    }

    let fields = input_parts.fields.iter().dedup_by(|left, right| left.label == right.label);
    let fields = fields.map(RowField::clone).collect_vec();

    let result = context.intern_row(fields, None);
    equate(state, context, wanted, head.class, &[(output, result)])
}
