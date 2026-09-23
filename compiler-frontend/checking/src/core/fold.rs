//! Implements type folding for the core representation.

use building_types::QueryResult;

use crate::context::CheckContext;
use crate::core::{ForallBinder, RowField, RowTypeId, Type, TypeFlags, TypeId, normalise};
use crate::state::CheckState;
use crate::{ExternalQueries, safe_loop};

pub enum FoldAction {
    Replace(TypeId),
    ReplaceThen(TypeId),
    Continue,
}

pub trait TypeFold {
    fn transform<Q: ExternalQueries>(
        &mut self,
        state: &mut CheckState,
        context: &CheckContext<Q>,
        id: TypeId,
        t: &Type,
    ) -> QueryResult<FoldAction>;

    fn transform_binder(&mut self, _binder: &mut ForallBinder) {}

    /// Whether the folder may change a type with the given flags.
    fn may_change(&self, _flags: TypeFlags) -> bool {
        true
    }

    fn complete<Q: ExternalQueries>(
        &mut self,
        _state: &mut CheckState,
        _context: &CheckContext<Q>,
        _id: TypeId,
        result: TypeId,
    ) -> QueryResult<TypeId> {
        Ok(result)
    }
}

pub fn fold_type<Q, F>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: TypeId,
    folder: &mut F,
) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
    F: TypeFold,
{
    if !folder.may_change(context.lookup_type_flags(id)) {
        return Ok(id);
    }

    let mut id = normalise::normalise(state, context, id);

    let t = safe_loop! {
        let t = context.lookup_type(id);
        match folder.transform(state, context, id, t)? {
            FoldAction::Replace(id) => return Ok(id),
            FoldAction::ReplaceThen(then_id) => {
                let then_id = normalise::normalise(state, context, then_id);
                if then_id == id {
                    break context.lookup_type(id);
                }
                id = then_id;
                continue;
            }
            FoldAction::Continue => {
                break t;
            }
        }
    };

    macro_rules! fold_pair {
        ($first:ident, $second:ident, $intern:ident) => {{
            let folded_first = fold_type(state, context, $first, folder)?;
            let folded_second = fold_type(state, context, $second, folder)?;
            if folded_first == $first && folded_second == $second {
                id
            } else {
                context.$intern(folded_first, folded_second)
            }
        }};
    }

    let result = match *t {
        Type::Application(function, argument) => {
            fold_pair!(function, argument, intern_application)
        }
        Type::KindApplication(function, argument) => {
            fold_pair!(function, argument, intern_kind_application)
        }
        Type::Forall(binder_id, inner) => {
            let original_binder = context.lookup_forall_binder(binder_id);
            let mut binder = original_binder;
            folder.transform_binder(&mut binder);
            binder.kind = fold_type(state, context, binder.kind, folder)?;
            let folded_inner = fold_type(state, context, inner, folder)?;
            if binder == original_binder && folded_inner == inner {
                id
            } else {
                let binder_id = context.intern_forall_binder(binder);
                context.intern_forall(binder_id, folded_inner)
            }
        }
        Type::Constrained(constraint, inner) => {
            fold_pair!(constraint, inner, intern_constrained)
        }
        Type::Function(argument, result) => fold_pair!(argument, result, intern_function),
        Type::Kinded(inner, kind) => fold_pair!(inner, kind, intern_kinded),
        Type::Constructor(_, _) => id,
        Type::Integer(_) | Type::String(_, _) => id,
        Type::Row(row_id) => {
            let original_row = context.lookup_row_type(row_id);
            let (mut fields, tail) = flatten_row(state, context, row_id)?;
            for field in fields.iter_mut() {
                field.id = fold_type(state, context, field.id, folder)?;
            }
            let tail = tail.map(|tail| fold_type(state, context, tail, folder)).transpose()?;
            if fields.as_slice() == original_row.fields.as_ref() && tail == original_row.tail {
                id
            } else {
                context.intern_row(fields, tail)
            }
        }
        Type::Rigid(name, depth, kind) => {
            let folded_kind = fold_type(state, context, kind, folder)?;
            if folded_kind == kind { id } else { context.intern_rigid(name, depth, folded_kind) }
        }
        Type::Unification(_) | Type::Free(_) | Type::Unknown(_) => id,
    };

    folder.complete(state, context, id, result)
}

/// Flatten a row type into a single row.
///
/// In most cases, row fields are stored contiguously in the [`RowType`] struct.
/// However, the constraint solver or other parts of the compiler may produce
/// the following transient structure:
///
/// ```purescript
/// ?t9 = ( t9 :: Type | ?t8 )
/// ?t8 = ( t8 :: Type | ?t8 )
/// ?t7 = ( t7 :: Type | ?t6 )
/// ```
///
/// This is effectively a linked list of row fields, and in exceptional cases,
/// recursive traversal of this structure is ineffective and leads to stack overflow.
///
/// This function flattens a given [`RowType`] by traversing the [`RowType::tail`]
/// and collecting fields, building the ideal canonical representation for a row type.
fn flatten_row<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    row_id: RowTypeId,
) -> QueryResult<(Vec<RowField>, Option<TypeId>)>
where
    Q: ExternalQueries,
{
    let mut row = context.lookup_row_type(row_id);
    let mut fields = row.fields.to_vec();

    safe_loop! {
        let Some(tail) = row.tail else {
            break;
        };

        let tail = normalise::normalise(state, context, tail);
        let Type::Row(row_id) = *context.lookup_type(tail) else {
            break;
        };

        row = context.lookup_row_type(row_id);
        fields.extend(row.fields.iter().cloned());
    }

    Ok((fields, row.tail))
}
