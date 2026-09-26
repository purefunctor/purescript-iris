use std::iter;

use building_types::QueryResult;
use indexing::TypeItemId;
use rustc_hash::{FxHashMap, FxHashSet};
use smol_str::SmolStr;

use crate::ExternalQueries;
use crate::context::CheckContext;
use crate::core::constraint::canonical;
use crate::core::constraint::matching::{self, MatchInstance};
use crate::core::{RowField, Type, TypeId, normalise};
use crate::source::types;
use crate::state::CheckState;

use super::{RowView, extract_row, extract_symbol, match_equality};

fn make_prim_row_constraint<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    class_id: TypeItemId,
    arguments: &[TypeId],
) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    let row_element_kind = infer_prim_row_element_kind(state, context, arguments)?;

    let constructor =
        context.queries.intern_type(Type::Constructor(context.prim_row.file_id, class_id));

    Ok(arguments.iter().copied().fold(
        context.intern_kind_application(constructor, row_element_kind),
        |application, argument| context.intern_application(application, argument),
    ))
}

fn infer_prim_row_element_kind<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    arguments: &[TypeId],
) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    for &argument in arguments {
        let argument_kind = types::elaborate_kind(state, context, argument)?;
        let argument_kind = normalise::expand(state, context, argument_kind)?;

        if let Type::Application(row_constructor, row_element_kind) =
            context.lookup_type(argument_kind)
        {
            let row_constructor = normalise::expand(state, context, *row_constructor)?;
            if row_constructor == context.prim.row {
                return Ok(*row_element_kind);
            }
        }
    }

    Ok(state.fresh_unification(context.queries, context.prim.t))
}

fn extract_closed_row<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: TypeId,
) -> QueryResult<Option<RowView>>
where
    Q: ExternalQueries,
{
    let Some(row) = extract_row(state, context, id)? else {
        return Ok(None);
    };
    match row {
        RowView::Closed { fields: _ } => Ok(Some(row)),
        RowView::EmptyOpen { .. } | RowView::Open { .. } => Ok(None),
    }
}

fn partition_union_output(
    output: &[RowField],
    right: &[RowField],
) -> Option<(Vec<RowField>, Vec<RowField>)> {
    let mut right_labels = FxHashMap::default();
    for field in right {
        let label = SmolStr::clone(&field.label);
        *right_labels.entry(label).or_insert(0usize) += 1;
    }

    let mut left_fields = Vec::new();
    let mut right_fields = Vec::new();

    // Union is left-biased, so when solving `Union left right output` from a closed
    // `right` and `output`, the right-hand fields are taken from the end of output.
    // Example: `Union ( | l ) ( x :: R ) ( x :: L, x :: R )` leaves
    // `( x :: L )` for `l`.
    for field in output.iter().rev() {
        if let Some(demand) = right_labels.get_mut(&field.label)
            && *demand > 0
        {
            *demand -= 1;
            let field = RowField::clone(field);
            right_fields.push(field);
        } else {
            let field = RowField::clone(field);
            left_fields.push(field);
        }
    }

    if right_labels.values().any(|&demand| demand > 0) {
        return None;
    }

    left_fields.reverse();
    right_fields.reverse();

    Some((left_fields, right_fields))
}

pub fn match_union<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    arguments: &[TypeId],
) -> QueryResult<Option<MatchInstance>>
where
    Q: ExternalQueries,
{
    let &[left, right, union] = arguments else {
        return Ok(None);
    };

    let left_row = extract_row(state, context, left)?;
    let right_row = extract_row(state, context, right)?;
    let union_row = extract_row(state, context, union)?;

    match (&left_row, &right_row, &union_row) {
        // Matches when the left row is closed and the right row is known.
        // The known fields from both sides are merged, and the right tail,
        // if any, stays as the tail of the union output.
        //
        // `Union ( a :: A ) ( | r ) ( | u )` solves `u ~ ( a :: A | r )`.
        // `Union ( a :: A ) ( b :: B ) ( | u )` solves `u ~ ( a :: A, b :: B )`.
        // `Union () ( b :: B | r ) ( | u )` solves `u ~ ( b :: B | r )`.
        (Some(RowView::Closed { fields: left_fields }), Some(right_row), _) => {
            let left = left_fields.iter();
            let right = right_row.fields().iter();

            let fields = iter::chain(left, right).cloned();
            let result = context.intern_row(fields, right_row.tail());

            Ok(Some(MatchInstance::from_unifications(vec![(union, result)])))
        }

        // Matches when the left row has a tail and both the right row and output
        // row are closed. We can solve the left row by subtracting the right
        // fields from the output.
        //
        // `Union ( | l ) ( b :: B ) ( a :: A, b :: B )` solves `l ~ ( a :: A )`.
        // `Union ( a :: A | l ) ( b :: B ) ( a :: A, b :: B, c :: C )` solves `l ~ ( c :: C )`.
        // `Union ( | l ) ( x :: R ) ( x :: L, x :: R )` solves `l ~ ( x :: L )`.
        (
            Some(RowView::EmptyOpen { .. } | RowView::Open { .. }),
            Some(RowView::Closed { fields: right_fields }),
            Some(RowView::Closed { fields: union_fields }),
        ) => {
            let Some((left_fields, right_fields)) =
                partition_union_output(union_fields, right_fields)
            else {
                return Ok(Some(MatchInstance::Apart));
            };

            let left_result = context.intern_row(left_fields, None);
            let right_result = context.intern_row(right_fields, None);

            Ok(Some(MatchInstance::from_unifications(vec![
                (left, left_result),
                (right, right_result),
            ])))
        }

        // Matches when the left row has a tail and at least one known field.
        // These known fields are moved into the output and we recurse on the
        // left tail. With no known fields, this would not make progress, so
        // the constraint must stay blocked.
        //
        // `Union ( a :: A | t ) ( | r ) ( | u )` solves `u ~ ( a :: A | f )`,
        // plus the remaining `Union ( | t ) ( | r ) ( | f )` constraint.
        //
        // `Union ( a :: A, b :: B | t ) ( c :: C ) ( | u )` solves `u ~ ( a :: A, b :: B | f )`,
        // plus the remaining `Union ( | t ) ( c :: C ) ( | f )` constraint.
        (Some(RowView::Open { fields: left_fields, tail }), Some(_), _) => {
            let row_element_kind = infer_prim_row_element_kind(state, context, arguments)?;
            let fresh_kind = context.intern_application(context.prim.row, row_element_kind);

            let fresh = state.fresh_unification(context.queries, fresh_kind);
            let result = context.intern_row(left_fields.iter().cloned(), Some(fresh));

            let constraint = make_prim_row_constraint(
                state,
                context,
                context.prim_row.union,
                &[*tail, right, fresh],
            )?;

            let constraints =
                canonical::canonicalise(state, context, constraint)?.into_iter().collect();

            Ok(Some(MatchInstance::from_parts(vec![(union, result)], constraints)))
        }

        _ => Ok(Some(matching::blocking_constraint(state, context, &[left, right, union])?)),
    }
}

pub fn match_cons<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    arguments: &[TypeId],
) -> QueryResult<Option<MatchInstance>>
where
    Q: ExternalQueries,
{
    let &[label, a, tail, row] = arguments else {
        return Ok(None);
    };

    let label_symbol = extract_symbol(state, context, label)?
        .and_then(|value| value.to_utf8().ok())
        .map(SmolStr::from);
    let tail_row = extract_row(state, context, tail)?;
    let row_row = extract_row(state, context, row)?;

    match (label_symbol, tail_row, row_row) {
        (Some(label_value), Some(tail_row), _) => {
            let mut fields = vec![RowField { label: label_value, id: a }];
            fields.extend(tail_row.fields().iter().cloned());

            let result = context.intern_row(fields, tail_row.tail());

            Ok(Some(MatchInstance::from_unifications(vec![(row, result)])))
        }
        (Some(label_value), _, Some(row_row)) => {
            let mut remaining = Vec::new();
            let mut found_type = None;

            for field in row_row.fields().iter() {
                if field.label == label_value && found_type.is_none() {
                    found_type = Some(field.id);
                } else {
                    remaining.push(field.clone());
                }
            }

            if let Some(field_type) = found_type {
                let tail_result = context.intern_row(remaining, row_row.tail());
                Ok(Some(MatchInstance::from_unifications(vec![
                    (a, field_type),
                    (tail, tail_result),
                ])))
            } else {
                Ok(Some(MatchInstance::Apart))
            }
        }
        _ => Ok(Some(matching::blocking_constraint(state, context, &[label, tail, row])?)),
    }
}

pub fn match_lacks<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    arguments: &[TypeId],
) -> QueryResult<Option<MatchInstance>>
where
    Q: ExternalQueries,
{
    let &[label, row] = arguments else {
        return Ok(None);
    };

    let Some(row_row) = extract_row(state, context, row)? else {
        return Ok(Some(matching::blocking_constraint(state, context, &[row])?));
    };

    if let RowView::Closed { fields } = &row_row
        && fields.is_empty()
    {
        return Ok(Some(MatchInstance::empty()));
    }

    let Some(label_value) = extract_symbol(state, context, label)?
        .and_then(|value| value.to_utf8().ok())
        .map(SmolStr::from)
    else {
        return Ok(Some(matching::blocking_constraint(state, context, &[label])?));
    };

    match row_row {
        RowView::Closed { fields } => {
            let has_label = fields.iter().any(|field| field.label == label_value);
            if has_label {
                Ok(Some(MatchInstance::Apart))
            } else {
                Ok(Some(MatchInstance::empty()))
            }
        }
        RowView::EmptyOpen { tail } => {
            Ok(Some(matching::blocking_constraint(state, context, &[tail])?))
        }
        RowView::Open { fields, tail } => {
            let has_label = fields.iter().any(|field| field.label == label_value);
            if has_label {
                return Ok(Some(MatchInstance::Apart));
            }

            let constraint =
                make_prim_row_constraint(state, context, context.prim_row.lacks, &[label, tail])?;

            let constraints =
                canonical::canonicalise(state, context, constraint)?.into_iter().collect();

            Ok(Some(MatchInstance::from_constraints(constraints)))
        }
    }
}

pub fn match_nub<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    arguments: &[TypeId],
) -> QueryResult<Option<MatchInstance>>
where
    Q: ExternalQueries,
{
    let &[original, nubbed] = arguments else {
        return Ok(None);
    };

    let Some(original_row) = extract_closed_row(state, context, original)? else {
        return Ok(Some(matching::blocking_constraint(state, context, &[original])?));
    };

    let mut seen = FxHashSet::default();
    let mut fields = Vec::new();

    for field in original_row.fields().iter() {
        if seen.insert(field.label.clone()) {
            fields.push(field.clone());
        }
    }

    let result = context.intern_row(fields, None);
    Ok(Some(match_equality(state, context, nubbed, result)?))
}
