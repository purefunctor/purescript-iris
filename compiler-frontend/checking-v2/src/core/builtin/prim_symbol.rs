use building_types::QueryResult;
use smol_str::format_smolstr;

use crate::ExternalQueries;
use crate::context::CheckContext;
use crate::core::entail::{ConstraintHead, Outcome};
use crate::state::{CheckState, Wanted};

use super::{equate, intern_ordering, intern_symbol, no_instance, symbol};

pub fn solve_append<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    wanted: &Wanted,
    head: &ConstraintHead,
) -> QueryResult<Outcome>
where
    Q: ExternalQueries,
{
    let &[left, right, appended] = head.arguments.as_slice() else {
        return Ok(no_instance(state, context, wanted));
    };

    let left_symbol = symbol(state, context, left);
    let right_symbol = symbol(state, context, right);
    let appended_symbol = symbol(state, context, appended);

    match (left_symbol, right_symbol, appended_symbol) {
        (Some(left_symbol), Some(right_symbol), _) => {
            let result = intern_symbol(context, &format_smolstr!("{left_symbol}{right_symbol}"));
            equate(state, context, wanted, head.class, &[(appended, result)])
        }
        (Some(left_symbol), None, Some(appended_symbol)) => {
            let Some(suffix) = appended_symbol.strip_prefix(left_symbol.as_str()) else {
                return Ok(no_instance(state, context, wanted));
            };
            let result = intern_symbol(context, suffix);
            equate(state, context, wanted, head.class, &[(right, result)])
        }
        (None, Some(right_symbol), Some(appended_symbol)) => {
            let Some(prefix) = appended_symbol.strip_suffix(right_symbol.as_str()) else {
                return Ok(no_instance(state, context, wanted));
            };
            let result = intern_symbol(context, prefix);
            equate(state, context, wanted, head.class, &[(left, result)])
        }
        _ => Ok(Outcome::Stuck),
    }
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

    let (Some(left_symbol), Some(right_symbol)) =
        (symbol(state, context, left), symbol(state, context, right))
    else {
        return Ok(Outcome::Stuck);
    };

    let Some(result) = intern_ordering(context, left_symbol.cmp(&right_symbol)) else {
        return Ok(no_instance(state, context, wanted));
    };
    equate(state, context, wanted, head.class, &[(ordering, result)])
}

pub fn solve_cons<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    wanted: &Wanted,
    head: &ConstraintHead,
) -> QueryResult<Outcome>
where
    Q: ExternalQueries,
{
    let &[first, rest, whole] = head.arguments.as_slice() else {
        return Ok(no_instance(state, context, wanted));
    };

    if let Some(whole_symbol) = symbol(state, context, whole) {
        let mut characters = whole_symbol.chars();
        let Some(first_character) = characters.next() else {
            return Ok(no_instance(state, context, wanted));
        };
        let first_result = intern_symbol(context, first_character.encode_utf8(&mut [0; 4]));
        let rest_result = intern_symbol(context, characters.as_str());
        let equations = [(first, first_result), (rest, rest_result)];
        return equate(state, context, wanted, head.class, &equations);
    }

    let (Some(first_symbol), Some(rest_symbol)) =
        (symbol(state, context, first), symbol(state, context, rest))
    else {
        return Ok(Outcome::Stuck);
    };
    if first_symbol.chars().count() != 1 {
        return Ok(no_instance(state, context, wanted));
    }
    let result = intern_symbol(context, &format_smolstr!("{first_symbol}{rest_symbol}"));
    equate(state, context, wanted, head.class, &[(whole, result)])
}
