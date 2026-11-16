mod prim_int;
mod prim_row;
mod prim_row_list;
mod prim_symbol;
mod prim_type_error;
mod reflection;

use std::cmp::Ordering;

use building_types::QueryResult;
use smol_str::SmolStr;

use crate::ExternalQueries;
use crate::context::{CheckContext, Item};
use crate::core::entail::{ConstraintHead, Outcome};
use crate::core::{coercible, fold, normalise, unify};
use crate::error::ErrorKind;
use crate::state::{CheckState, Wanted};
use crate::tree::{BuiltinEvidence, Evidence};
use crate::types::{Type, TypeId};

pub fn solve<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    wanted: &Wanted,
    head: &ConstraintHead,
) -> QueryResult<Option<Outcome>>
where
    Q: ExternalQueries,
{
    let known = &context.known;
    let class = Some(head.class);

    let outcome = if class == known.coercible {
        coercible::solve(state, context, wanted, head)?
    } else if class == known.row_cons {
        prim_row::solve_cons(state, context, wanted, head)?
    } else if class == known.row_lacks {
        prim_row::solve_lacks(state, context, wanted, head)?
    } else if class == known.row_union {
        prim_row::solve_union(state, context, wanted, head)?
    } else if class == known.row_nub {
        prim_row::solve_nub(state, context, wanted, head)?
    } else if class == known.row_to_list {
        prim_row_list::solve_row_to_list(state, context, wanted, head)?
    } else if class == known.symbol_append {
        prim_symbol::solve_append(state, context, wanted, head)?
    } else if class == known.symbol_compare {
        prim_symbol::solve_compare(state, context, wanted, head)?
    } else if class == known.symbol_cons {
        prim_symbol::solve_cons(state, context, wanted, head)?
    } else if class == known.int_add {
        prim_int::solve_add(state, context, wanted, head)?
    } else if class == known.int_mul {
        prim_int::solve_mul(state, context, wanted, head)?
    } else if class == known.int_compare {
        prim_int::solve_compare(state, context, wanted, head)?
    } else if class == known.int_to_string {
        prim_int::solve_to_string(state, context, wanted, head)?
    } else if class == known.type_error_fail {
        prim_type_error::solve_fail(state, context, wanted, head)
    } else if class == known.type_error_warn {
        prim_type_error::solve_warn(state, context, wanted, head)
    } else if class == known.is_symbol {
        reflection::solve_is_symbol(state, context, wanted, head)
    } else if class == known.reflectable {
        reflection::solve_reflectable(state, context, wanted, head)?
    } else {
        return Ok(None);
    };

    Ok(Some(outcome))
}

pub fn push_subgoal(state: &mut CheckState, wanted: &Wanted, constraint: TypeId) {
    let slot = state.checked.tree.allocate_evidence_slot();
    let crumbs = wanted.crumbs.clone();
    let work = wanted.work + 1;
    let root = wanted.root;
    state.wanted.push(Wanted { constraint, slot, scope: wanted.scope, crumbs, work, root });
}

fn intern_constraint<Q>(
    context: &CheckContext<Q>,
    head: &ConstraintHead,
    arguments: &[TypeId],
) -> TypeId
where
    Q: ExternalQueries,
{
    let constructor = context.intern_constructor(head.class);
    let constructor = head.kind_arguments.iter().fold(constructor, |function, &argument| {
        context.intern_kind_application(function, argument)
    });
    context.intern_application_list(constructor, arguments)
}

fn symbol<Q>(state: &CheckState, context: &CheckContext<Q>, id: TypeId) -> Option<SmolStr>
where
    Q: ExternalQueries,
{
    let id = normalise::normalise(state, context, id);
    match context.lookup_type(id) {
        Type::String(text) => Some(context.lookup_text(text)),
        _ => None,
    }
}

fn integer<Q>(state: &CheckState, context: &CheckContext<Q>, id: TypeId) -> Option<i32>
where
    Q: ExternalQueries,
{
    let id = normalise::normalise(state, context, id);
    match context.lookup_type(id) {
        Type::Integer(value) => Some(value),
        _ => None,
    }
}

fn intern_symbol<Q>(context: &CheckContext<Q>, text: &str) -> TypeId
where
    Q: ExternalQueries,
{
    let text = context.intern_text(SmolStr::new(text));
    context.intern_type(Type::String(text))
}

fn intern_ordering<Q>(context: &CheckContext<Q>, ordering: Ordering) -> Option<TypeId>
where
    Q: ExternalQueries,
{
    let item = match ordering {
        Ordering::Less => context.known.ordering_lt,
        Ordering::Equal => context.known.ordering_eq,
        Ordering::Greater => context.known.ordering_gt,
    };
    item.map(|item| context.intern_constructor(item))
}

fn no_instance<Q>(state: &CheckState, context: &CheckContext<Q>, wanted: &Wanted) -> Outcome
where
    Q: ExternalQueries,
{
    let constraint = fold::zonk(state, context, wanted.constraint);
    Outcome::Failed(ErrorKind::NoInstanceFound { constraint })
}

fn solved(state: &mut CheckState, wanted: &Wanted, class: Item) -> Outcome {
    solved_with(state, wanted, class, BuiltinEvidence::Empty)
}

fn solved_with(
    state: &mut CheckState,
    wanted: &Wanted,
    class: Item,
    value: BuiltinEvidence,
) -> Outcome {
    state.checked.tree.set_evidence(wanted.slot, Evidence::Builtin { class, value });
    Outcome::Solved
}

fn equate<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    wanted: &Wanted,
    class: Item,
    equations: &[(TypeId, TypeId)],
) -> QueryResult<Outcome>
where
    Q: ExternalQueries,
{
    equate_with(state, context, wanted, class, equations, BuiltinEvidence::Empty)
}

fn equate_with<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    wanted: &Wanted,
    class: Item,
    equations: &[(TypeId, TypeId)],
    value: BuiltinEvidence,
) -> QueryResult<Outcome>
where
    Q: ExternalQueries,
{
    for &(target, result) in equations {
        if unify::unify_core(state, context, target, result)?.is_err() {
            return Ok(no_instance(state, context, wanted));
        }
    }
    Ok(solved_with(state, wanted, class, value))
}
