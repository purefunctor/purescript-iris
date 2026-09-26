use std::cmp::Ordering;

use building_types::QueryResult;
use rustc_hash::FxHashSet;

use crate::ExternalQueries;
use crate::context::CheckContext;
use crate::core::{Name, RowTypeId, Type, TypeId, normalise};
use crate::state::CheckState;

pub fn collect_structural_improvements<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    left: TypeId,
    right: TypeId,
    seen: &mut FxHashSet<(TypeId, TypeId)>,
    improvements: &mut Vec<(Name, TypeId)>,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let left = normalise::expand(state, context, left)?;
    let right = normalise::expand(state, context, right)?;

    if left == right {
        return Ok(());
    }

    if !seen.insert((left, right)) {
        return Ok(());
    }

    match (context.lookup_type(left), context.lookup_type(right)) {
        (Type::Rigid(name, _, _), _) => {
            improvements.push((*name, right));
        }
        (_, Type::Rigid(name, _, _)) => {
            improvements.push((*name, left));
        }

        (
            Type::Application(left_function, left_argument),
            Type::Application(right_function, right_argument),
        )
        | (
            Type::KindApplication(left_function, left_argument),
            Type::KindApplication(right_function, right_argument),
        ) => {
            collect_structural_improvements(
                state,
                context,
                *left_function,
                *right_function,
                seen,
                improvements,
            )?;
            collect_structural_improvements(
                state,
                context,
                *left_argument,
                *right_argument,
                seen,
                improvements,
            )?;
        }

        (
            Type::Function(left_argument, left_result),
            Type::Function(right_argument, right_result),
        ) => {
            collect_structural_improvements(
                state,
                context,
                *left_argument,
                *right_argument,
                seen,
                improvements,
            )?;
            collect_structural_improvements(
                state,
                context,
                *left_result,
                *right_result,
                seen,
                improvements,
            )?;
        }

        (Type::Function(left_argument, left_result), Type::Application(_, _)) => {
            let left = context.intern_function_application(*left_argument, *left_result);
            collect_structural_improvements(state, context, left, right, seen, improvements)?;
        }

        (Type::Application(_, _), Type::Function(right_argument, right_result)) => {
            let right = context.intern_function_application(*right_argument, *right_result);
            collect_structural_improvements(state, context, left, right, seen, improvements)?;
        }

        (Type::Kinded(left_inner, left_kind), Type::Kinded(right_inner, right_kind)) => {
            collect_structural_improvements(
                state,
                context,
                *left_inner,
                *right_inner,
                seen,
                improvements,
            )?;
            collect_structural_improvements(
                state,
                context,
                *left_kind,
                *right_kind,
                seen,
                improvements,
            )?;
        }

        (Type::Row(left_row_id), Type::Row(right_row_id)) => {
            collect_row_improvements(
                state,
                context,
                *left_row_id,
                *right_row_id,
                seen,
                improvements,
            )?;
        }

        _ => {}
    }

    Ok(())
}

fn collect_row_improvements<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    left_row_id: RowTypeId,
    right_row_id: RowTypeId,
    seen: &mut FxHashSet<(TypeId, TypeId)>,
    improvements: &mut Vec<(Name, TypeId)>,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let left_row = context.lookup_row_type(left_row_id);
    let right_row = context.lookup_row_type(right_row_id);

    let mut left_index = 0;
    let mut left_only = Vec::new();

    let mut right_index = 0;
    let mut right_only = Vec::new();

    let left_fields = &left_row.fields;
    let right_fields = &right_row.fields;

    while left_index < left_fields.len() && right_index < right_fields.len() {
        let left_field = &left_fields[left_index];
        let right_field = &right_fields[right_index];

        match left_field.label.cmp(&right_field.label) {
            Ordering::Equal => {
                collect_structural_improvements(
                    state,
                    context,
                    left_field.id,
                    right_field.id,
                    seen,
                    improvements,
                )?;
                left_index += 1;
                right_index += 1;
            }
            Ordering::Less => {
                left_only.push(left_field.clone());
                left_index += 1;
            }
            Ordering::Greater => {
                right_only.push(right_field.clone());
                right_index += 1;
            }
        }
    }

    left_only.extend(left_fields[left_index..].iter().cloned());
    right_only.extend(right_fields[right_index..].iter().cloned());

    match (left_row.tail, right_row.tail) {
        (Some(left_tail), Some(right_tail)) if left_only.is_empty() && right_only.is_empty() => {
            collect_structural_improvements(
                state,
                context,
                left_tail,
                right_tail,
                seen,
                improvements,
            )?;
        }

        (Some(left_tail), right_tail) if right_only.is_empty() => {
            let right_remainder = context.intern_row(left_only, right_tail);
            collect_structural_improvements(
                state,
                context,
                left_tail,
                right_remainder,
                seen,
                improvements,
            )?;
        }

        (left_tail, Some(right_tail)) if left_only.is_empty() => {
            let left_remainder = context.intern_row(right_only, left_tail);
            collect_structural_improvements(
                state,
                context,
                left_remainder,
                right_tail,
                seen,
                improvements,
            )?;
        }

        _ => {}
    }

    Ok(())
}
