use building_types::QueryResult;

use crate::ExternalQueries;
use crate::context::{CheckContext, Item};
use crate::core::{fold, normalise};
use crate::state::CheckState;
use crate::types::{Type, TypeId};

pub fn fresh_unification<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    kind: TypeId,
) -> TypeId
where
    Q: ExternalQueries,
{
    let unification_id = state.fresh_unification_id(kind);
    context.intern_type(Type::Unification(unification_id))
}

pub fn fresh_type<Q>(state: &mut CheckState, context: &CheckContext<Q>) -> TypeId
where
    Q: ExternalQueries,
{
    fresh_unification(state, context, context.types.type_)
}

pub fn lookup_type_item_kind<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    (file_id, type_id): Item,
) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    if file_id == context.id {
        if let Some(&kind) = state.pending_types.get(&type_id) {
            return Ok(kind);
        }
        let kind = state.checked.type_kinds.get(&type_id).copied();
        return Ok(kind.unwrap_or(context.types.unknown));
    }

    let checked = context.checked_dependency(file_id)?;
    let kind = checked.type_kinds.get(&type_id).copied();
    Ok(kind.unwrap_or(context.types.unknown))
}

pub fn kind_of<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: TypeId,
) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    let kind = match context.lookup_type(id) {
        Type::Application(function, _) => {
            let function_kind = kind_of(state, context, function)?;
            match normalise::function_parts(state, context, function_kind) {
                Some((_, result_kind)) => result_kind,
                None => context.types.unknown,
            }
        }
        Type::KindApplication(function, argument) => {
            let function_kind = kind_of(state, context, function)?;
            let function_kind = normalise::normalise(state, context, function_kind);
            match context.lookup_type(function_kind) {
                Type::Forall(binder_id, inner_kind) => {
                    let binder = context.lookup_forall_binder(binder_id);
                    fold::substitute_variable(context, binder.name, argument, inner_kind)
                }
                _ => context.types.unknown,
            }
        }
        Type::Function(..) | Type::Forall(..) | Type::Constrained(..) => context.types.type_,
        Type::Kinded(_, kind) => kind,
        Type::Constructor(file_id, type_id) => {
            lookup_type_item_kind(state, context, (file_id, type_id))?
        }
        Type::Synonym(application_id) => {
            let application = context.lookup_synonym_application(application_id);
            kind_of(state, context, application.expansion)?
        }
        Type::Integer(_) => context.types.int,
        Type::String(_) => context.types.symbol,
        Type::Row(row_id) => {
            let row = context.lookup_row_type(row_id);
            if let Some(field) = row.fields.first() {
                let field_kind = kind_of(state, context, field.id)?;
                context.intern_row_kind(field_kind)
            } else if let Some(tail) = row.tail {
                kind_of(state, context, tail)?
            } else {
                let element = fresh_type(state, context);
                context.intern_row_kind(element)
            }
        }
        Type::Variable(binder_id) => context.lookup_forall_binder(binder_id).kind,
        Type::Rigid(rigid_id) => context.lookup_rigid(rigid_id).kind,
        Type::Unification(unification_id) => state.unification(unification_id).kind,
        Type::Unknown => context.types.unknown,
    };
    Ok(kind)
}
