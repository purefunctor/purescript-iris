use std::sync::Arc;

use crate::context::CheckContext;
use crate::state::CheckState;
use crate::types::{RowField, RowType, Type, TypeId};
use crate::{ExternalQueries, safe_loop};

pub fn normalise<Q>(state: &CheckState, context: &CheckContext<Q>, id: TypeId) -> TypeId
where
    Q: ExternalQueries,
{
    let mut id = id;
    safe_loop! {
        match context.lookup_type(id) {
            Type::Unification(unification_id) => {
                let Some(solution) = state.unification(unification_id).solution else {
                    break id;
                };
                id = solution;
            }
            Type::Synonym(application_id) => {
                id = context.lookup_synonym_application(application_id).expansion;
            }
            Type::Kinded(inner, _) => {
                id = inner;
            }
            Type::Row(row_id) => {
                let row = context.lookup_row_type(row_id);
                let flattened = flatten_row(state, context, row);
                if flattened.fields.is_empty()
                    && let Some(tail) = flattened.tail
                {
                    id = tail;
                    continue;
                }
                let row_id = context.intern_row_type(flattened);
                break context.intern_type(Type::Row(row_id));
            }
            _ => break id,
        }
    }
}

pub fn flatten_row<Q>(state: &CheckState, context: &CheckContext<Q>, row: RowType) -> RowType
where
    Q: ExternalQueries,
{
    let Some(tail) = row.tail else {
        return row;
    };

    let tail = normalise(state, context, tail);
    let Type::Row(tail_id) = context.lookup_type(tail) else {
        return RowType { fields: row.fields, tail: Some(tail) };
    };

    let tail_row = context.lookup_row_type(tail_id);
    let mut fields = row.fields.iter().map(RowField::clone).collect::<Vec<_>>();
    fields.extend(tail_row.fields.iter().map(RowField::clone));
    fields.sort_by(|left, right| left.label.cmp(&right.label));

    RowType { fields: Arc::from(fields), tail: tail_row.tail }
}

pub fn row_parts<Q>(state: &CheckState, context: &CheckContext<Q>, id: TypeId) -> RowType
where
    Q: ExternalQueries,
{
    let id = normalise(state, context, id);
    match context.lookup_type(id) {
        Type::Row(row_id) => context.lookup_row_type(row_id),
        _ => RowType { fields: Arc::from([]), tail: Some(id) },
    }
}

pub fn function_parts<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    id: TypeId,
) -> Option<(TypeId, TypeId)>
where
    Q: ExternalQueries,
{
    let id = normalise(state, context, id);
    match context.lookup_type(id) {
        Type::Function(argument, result) => Some((argument, result)),
        Type::Application(function, result) => {
            let function = normalise(state, context, function);
            let Type::Application(constructor, argument) = context.lookup_type(function) else {
                return None;
            };
            let constructor = normalise(state, context, constructor);
            (constructor == context.types.function).then_some((argument, result))
        }
        _ => None,
    }
}

pub fn application_spine<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    id: TypeId,
) -> (TypeId, Vec<TypeId>)
where
    Q: ExternalQueries,
{
    let mut arguments = Vec::new();
    let mut current = normalise(state, context, id);
    safe_loop! {
        match context.lookup_type(current) {
            Type::Application(function, argument) => {
                arguments.push(argument);
                current = normalise(state, context, function);
            }
            Type::KindApplication(function, _) => {
                current = normalise(state, context, function);
            }
            Type::Function(argument, result) => {
                arguments.push(result);
                arguments.push(argument);
                current = context.types.function;
            }
            _ => break,
        }
    }
    arguments.reverse();
    (current, arguments)
}
