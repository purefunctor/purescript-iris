//! Implements type walking for the core representation.

use std::ops::ControlFlow;

use building_types::QueryResult;

use crate::ExternalQueries;
use crate::context::CheckContext;
use crate::core::{ForallBinder, Type, TypeFlags, TypeId, normalise};
use crate::state::CheckState;

pub enum WalkAction {
    /// Stops the entire walk, skipping every remaining type.
    Break,
    Continue,
}

pub trait TypeWalker {
    fn visit<Q: ExternalQueries>(
        &mut self,
        state: &mut CheckState,
        context: &CheckContext<Q>,
        id: TypeId,
        t: &Type,
    ) -> QueryResult<WalkAction>;

    fn visit_binder(&mut self, _binder: &ForallBinder) {}

    /// Whether the walker may be interested in a type with the given flags.
    ///
    /// Types for which this returns `false` are skipped along with their
    /// children, without being visited.
    fn may_visit(&self, _flags: TypeFlags) -> bool {
        true
    }
}

pub fn walk_type<Q, W>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: TypeId,
    walker: &mut W,
) -> QueryResult<()>
where
    Q: ExternalQueries,
    W: TypeWalker,
{
    // Walkers record their own results, so breaking only ends the walk early.
    walk(state, context, id, walker).map(|_| ())
}

fn walk<Q, W>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: TypeId,
    walker: &mut W,
) -> QueryResult<ControlFlow<()>>
where
    Q: ExternalQueries,
    W: TypeWalker,
{
    macro_rules! walk_child {
        ($id:expr) => {
            if walk(state, context, $id, walker)?.is_break() {
                return Ok(ControlFlow::Break(()));
            }
        };
    }

    if !walker.may_visit(context.lookup_type_flags(id)) {
        return Ok(ControlFlow::Continue(()));
    }

    let id = normalise::normalise(state, context, id);
    let t = context.lookup_type(id);

    if let WalkAction::Break = walker.visit(state, context, id, t)? {
        return Ok(ControlFlow::Break(()));
    }

    match *t {
        Type::Application(function, argument) | Type::KindApplication(function, argument) => {
            walk_child!(function);
            walk_child!(argument);
        }
        Type::Forall(binder_id, inner) => {
            let binder = context.lookup_forall_binder(binder_id);
            walker.visit_binder(&binder);
            walk_child!(binder.kind);
            walk_child!(inner);
        }
        Type::Constrained(constraint, inner) => {
            walk_child!(constraint);
            walk_child!(inner);
        }
        Type::Function(argument, result) => {
            walk_child!(argument);
            walk_child!(result);
        }
        Type::Kinded(inner, kind) => {
            walk_child!(inner);
            walk_child!(kind);
        }
        Type::Constructor(_, _) => {}
        Type::Integer(_) | Type::String(_, _) => {}
        Type::Row(row_id) => {
            let row = context.lookup_row_type(row_id);
            for field in row.fields.iter() {
                walk_child!(field.id);
            }
            if let Some(tail) = row.tail {
                walk_child!(tail);
            }
        }
        Type::Rigid(_, _, kind) => {
            walk_child!(kind);
        }
        Type::Unification(_) | Type::Free(_) | Type::Unknown(_) => {}
    }

    Ok(ControlFlow::Continue(()))
}
