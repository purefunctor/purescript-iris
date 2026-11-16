use building_types::QueryResult;

use crate::ExternalQueries;
use crate::context::CheckContext;
use crate::core::entail::{ConstraintHead, Outcome};
use crate::state::{CheckState, Wanted};
use crate::types::Type;

use super::{equate, integer, intern_ordering, intern_symbol, no_instance};

pub fn solve_add<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    wanted: &Wanted,
    head: &ConstraintHead,
) -> QueryResult<Outcome>
where
    Q: ExternalQueries,
{
    let &[left, right, sum] = head.arguments.as_slice() else {
        return Ok(no_instance(state, context, wanted));
    };

    let left_integer = integer(state, context, left);
    let right_integer = integer(state, context, right);
    let sum_integer = integer(state, context, sum);

    let (target, result) = match (left_integer, right_integer, sum_integer) {
        (Some(left_integer), Some(right_integer), _) => {
            (sum, left_integer.checked_add(right_integer))
        }
        (Some(left_integer), None, Some(sum_integer)) => {
            (right, sum_integer.checked_sub(left_integer))
        }
        (None, Some(right_integer), Some(sum_integer)) => {
            (left, sum_integer.checked_sub(right_integer))
        }
        _ => return Ok(Outcome::Stuck),
    };

    let Some(result) = result else {
        return Ok(no_instance(state, context, wanted));
    };
    let result = context.intern_type(Type::Integer(result));
    equate(state, context, wanted, head.class, &[(target, result)])
}

pub fn solve_mul<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    wanted: &Wanted,
    head: &ConstraintHead,
) -> QueryResult<Outcome>
where
    Q: ExternalQueries,
{
    let &[left, right, product] = head.arguments.as_slice() else {
        return Ok(no_instance(state, context, wanted));
    };

    let (Some(left_integer), Some(right_integer)) =
        (integer(state, context, left), integer(state, context, right))
    else {
        return Ok(Outcome::Stuck);
    };

    let Some(result) = left_integer.checked_mul(right_integer) else {
        return Ok(no_instance(state, context, wanted));
    };
    let result = context.intern_type(Type::Integer(result));
    equate(state, context, wanted, head.class, &[(product, result)])
}

pub fn solve_compare<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    wanted: &Wanted,
    head: &ConstraintHead,
) -> QueryResult<Outcome>
where
    Q: ExternalQueries,
{
    let &[left, right, ordering] = head.arguments.as_slice() else {
        return Ok(no_instance(state, context, wanted));
    };

    let (Some(left_integer), Some(right_integer)) =
        (integer(state, context, left), integer(state, context, right))
    else {
        return Ok(Outcome::Stuck);
    };

    let Some(result) = intern_ordering(context, left_integer.cmp(&right_integer)) else {
        return Ok(no_instance(state, context, wanted));
    };
    equate(state, context, wanted, head.class, &[(ordering, result)])
}

pub fn solve_to_string<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    wanted: &Wanted,
    head: &ConstraintHead,
) -> QueryResult<Outcome>
where
    Q: ExternalQueries,
{
    let &[value, rendered] = head.arguments.as_slice() else {
        return Ok(no_instance(state, context, wanted));
    };

    let Some(value_integer) = integer(state, context, value) else {
        return Ok(Outcome::Stuck);
    };

    let result = intern_symbol(context, &value_integer.to_string());
    equate(state, context, wanted, head.class, &[(rendered, result)])
}
