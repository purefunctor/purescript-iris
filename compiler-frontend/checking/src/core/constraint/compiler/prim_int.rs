use std::cmp::Ordering;

use building_types::QueryResult;
use petgraph::algo::has_path_connecting;
use petgraph::prelude::DiGraphMap;

use crate::ExternalQueries;
use crate::context::CheckContext;
use crate::core::constraint::CanonicalConstraintId;
use crate::core::constraint::matching::{self, MatchInstance};
use crate::core::{TypeId, normalise, toolkit};
use crate::state::CheckState;

use super::{extract_integer, intern_integer, intern_symbol, match_equality};

pub fn match_add<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    arguments: &[TypeId],
) -> QueryResult<Option<MatchInstance>>
where
    Q: ExternalQueries,
{
    let &[left, right, sum] = arguments else {
        return Ok(None);
    };

    let left_int = extract_integer(state, context, left)?;
    let right_int = extract_integer(state, context, right)?;
    let sum_int = extract_integer(state, context, sum)?;

    let matched = match (left_int, right_int, sum_int) {
        (Some(left), Some(right), _) => {
            let result = intern_integer(context, left + right);
            match_equality(state, context, sum, result)?
        }
        (Some(left), _, Some(sum_value)) => {
            let result = intern_integer(context, sum_value - left);
            match_equality(state, context, right, result)?
        }
        (_, Some(right), Some(sum_value)) => {
            let result = intern_integer(context, sum_value - right);
            match_equality(state, context, left, result)?
        }
        _ => matching::blocking_constraint(state, context, &[left, right, sum])?,
    };

    Ok(Some(matched))
}

pub fn match_mul<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    arguments: &[TypeId],
) -> QueryResult<Option<MatchInstance>>
where
    Q: ExternalQueries,
{
    let &[left, right, product] = arguments else {
        return Ok(None);
    };

    let Some(left_int) = extract_integer(state, context, left)? else {
        return Ok(Some(matching::blocking_constraint(state, context, &[left])?));
    };
    let Some(right_int) = extract_integer(state, context, right)? else {
        return Ok(Some(matching::blocking_constraint(state, context, &[right])?));
    };

    let result = intern_integer(context, left_int * right_int);
    Ok(Some(match_equality(state, context, product, result)?))
}

pub fn match_compare<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    arguments: &[TypeId],
    given: impl IntoIterator<Item = CanonicalConstraintId>,
) -> QueryResult<Option<MatchInstance>>
where
    Q: ExternalQueries,
{
    let &[left, right, ordering] = arguments else {
        return Ok(None);
    };

    let left = normalise::expand(state, context, left)?;
    let right = normalise::expand(state, context, right)?;
    let ordering = normalise::expand(state, context, ordering)?;

    let left_int = extract_integer(state, context, left)?;
    let right_int = extract_integer(state, context, right)?;

    if let (Some(left_int), Some(right_int)) = (left_int, right_int) {
        let result = match left_int.cmp(&right_int) {
            Ordering::Less => context.prim_ordering.lt,
            Ordering::Equal => context.prim_ordering.eq,
            Ordering::Greater => context.prim_ordering.gt,
        };

        return Ok(Some(match_equality(state, context, ordering, result)?));
    }

    if let Some(result) = match_compare_transitive(state, context, left, right, ordering, given)? {
        return Ok(Some(result));
    }

    Ok(Some(matching::blocking_constraint(state, context, &[left, right])?))
}

fn match_compare_transitive<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    left: TypeId,
    right: TypeId,
    ordering: TypeId,
    given: impl IntoIterator<Item = CanonicalConstraintId>,
) -> QueryResult<Option<MatchInstance>>
where
    Q: ExternalQueries,
{
    let mut graph: DiGraphMap<TypeId, ()> = DiGraphMap::new();

    for constraint in given {
        let canonical = &state.canonicals[constraint];
        if canonical.file_id != context.prim_int.file_id
            || canonical.type_id != context.prim_int.compare
        {
            continue;
        }

        let Some([a, b, relation]) = canonical.expect_type_arguments::<3>() else {
            continue;
        };

        // A given that contains an unknown type doesn't apply; see [`Type::Unknown`].
        let constraint = state.canonicals.type_id(context, constraint);
        if toolkit::find_unknown(state, context, constraint)?.is_some() {
            continue;
        }

        let a = normalise::expand(state, context, a)?;
        let b = normalise::expand(state, context, b)?;
        let relation = normalise::expand(state, context, relation)?;

        if relation == context.prim_ordering.lt {
            graph.add_edge(a, b, ());
        } else if relation == context.prim_ordering.eq {
            graph.add_edge(a, b, ());
            graph.add_edge(b, a, ());
        } else if relation == context.prim_ordering.gt {
            graph.add_edge(b, a, ());
        }
    }

    graph.add_node(left);
    graph.add_node(right);

    let integers = graph.nodes().map(|node| {
        let value = extract_integer(state, context, node)?;
        Ok(value.map(|value| (value, node)))
    });

    let mut integers =
        integers.filter_map(|result| result.transpose()).collect::<QueryResult<Vec<_>>>()?;

    integers.sort_by_key(|&(value, _)| value);

    // Create edges between concrete integer nodes such that the
    // reachability algorithm considers them too; the direction
    // is ascending, as set by the prim_ordering.lt case above.
    for window in integers.windows(2) {
        let &[(_, lower), (_, upper)] = window else { continue };
        graph.add_edge(lower, upper, ());
    }

    let left_to_right = has_path_connecting(&graph, left, right, None);
    let right_to_left = has_path_connecting(&graph, right, left, None);

    let result = match (left_to_right, right_to_left) {
        (true, true) => context.prim_ordering.eq,
        (true, false) => context.prim_ordering.lt,
        (false, true) => context.prim_ordering.gt,
        (false, false) => return Ok(None),
    };

    match_equality(state, context, ordering, result).map(Some)
}

pub fn match_to_string<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    arguments: &[TypeId],
) -> QueryResult<Option<MatchInstance>>
where
    Q: ExternalQueries,
{
    let &[int, symbol] = arguments else {
        return Ok(None);
    };

    let Some(value) = extract_integer(state, context, int)? else {
        return Ok(Some(matching::blocking_constraint(state, context, &[int])?));
    };

    let result = intern_symbol(context, &value.to_string());
    Ok(Some(match_equality(state, context, symbol, result)?))
}
