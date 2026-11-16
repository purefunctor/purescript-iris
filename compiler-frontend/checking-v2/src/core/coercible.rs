use std::sync::Arc;

use building_types::QueryResult;
use itertools::{EitherOrBoth, Itertools};
use smol_str::SmolStr;

use crate::ExternalQueries;
use crate::context::{CheckContext, Item};
use crate::core::builtin::push_subgoal;
use crate::core::data::{self, NewtypeField};
use crate::core::entail::{self, ConstraintHead, Outcome};
use crate::core::{fold, kind, normalise, skolem, unify};
use crate::error::ErrorKind;
use crate::source::roles;
use crate::state::{CheckState, Wanted};
use crate::tree::{BuiltinEvidence, Evidence};
use crate::types::{ForallBinder, Role, RowField, Type, TypeId};

const REWRITE_LIMIT: u32 = 64;

struct Givens {
    rewrites: Vec<(TypeId, TypeId)>,
    pairs: Vec<(TypeId, TypeId)>,
}

struct Spine {
    head: TypeId,
    kind_arguments: Vec<TypeId>,
    arguments: Vec<TypeId>,
}

enum Step {
    Subgoals(Vec<(TypeId, TypeId)>),
    Stuck,
    Failed,
}

pub fn solve<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    wanted: &Wanted,
    head: &ConstraintHead,
) -> QueryResult<Outcome>
where
    Q: ExternalQueries,
{
    let constraint = fold::zonk(state, context, wanted.constraint);
    let [left, right] = head.arguments.as_slice() else {
        return Ok(Outcome::Failed(ErrorKind::NoInstanceFound { constraint }));
    };

    let givens = coercible_givens(state, context, wanted)?;
    let mut budget = REWRITE_LIMIT;
    let left = rewrite(state, context, &givens, *left, &mut budget)?;
    let right = rewrite(state, context, &givens, *right, &mut budget)?;

    let step = coercible_step(state, context, &givens, left, right)?;
    match step {
        Step::Stuck => Ok(Outcome::Stuck),
        Step::Failed => Ok(Outcome::Failed(ErrorKind::NoInstanceFound { constraint })),
        Step::Subgoals(subgoals) => {
            for (subgoal_left, subgoal_right) in subgoals {
                let subgoal =
                    coercible_constraint(state, context, head.class, subgoal_left, subgoal_right)?;
                push_subgoal(state, wanted, subgoal);
            }
            let evidence = Evidence::Builtin { class: head.class, value: BuiltinEvidence::Empty };
            state.checked.tree.set_evidence(wanted.slot, evidence);
            Ok(Outcome::Solved)
        }
    }
}

fn coercible_constraint<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    class: Item,
    left: TypeId,
    right: TypeId,
) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    let argument_kind = kind::kind_of(state, context, left)?;
    let constructor = context.intern_constructor(class);
    let constructor = context.intern_kind_application(constructor, argument_kind);
    Ok(context.intern_application_list(constructor, &[left, right]))
}

fn coercible_givens<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    wanted: &Wanted,
) -> QueryResult<Givens>
where
    Q: ExternalQueries,
{
    let mut givens = Givens { rewrites: Vec::new(), pairs: Vec::new() };
    for given in entail::given_closure(state, context, wanted)? {
        let Some(head) = entail::constraint_head(state, context, given.constraint) else {
            continue;
        };
        if context.known.coercible != Some(head.class) {
            continue;
        }
        let [left, right] = head.arguments.as_slice() else {
            continue;
        };
        let left = fold::zonk(state, context, *left);
        let right = fold::zonk(state, context, *right);
        if is_rigid(context, left) && !mentions(context, right, left) {
            givens.rewrites.push((left, right));
        } else if is_rigid(context, right) && !mentions(context, left, right) {
            givens.rewrites.push((right, left));
        } else {
            givens.pairs.push((left, right));
        }
    }
    Ok(givens)
}

fn is_rigid<Q>(context: &CheckContext<Q>, id: TypeId) -> bool
where
    Q: ExternalQueries,
{
    matches!(context.lookup_type(id), Type::Rigid(_))
}

fn mentions<Q>(context: &CheckContext<Q>, id: TypeId, rigid: TypeId) -> bool
where
    Q: ExternalQueries,
{
    if id == rigid {
        return true;
    }
    if !context.lookup_type_flags(id).has_rigid() {
        return false;
    }
    let mut found = false;
    fold::map_children(context, id, |child| {
        found |= mentions(context, child, rigid);
        child
    });
    found
}

fn rewrite<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    givens: &Givens,
    id: TypeId,
    budget: &mut u32,
) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    let id = normalise::normalise(state, context, id);
    if givens.rewrites.is_empty() || *budget == 0 {
        return Ok(id);
    }

    let rewritten = givens.rewrites.iter().find(|&&(rigid, _)| rigid == id);
    if let Some(&(_, replacement)) = rewritten {
        *budget -= 1;
        return rewrite(state, context, givens, replacement, budget);
    }

    match context.lookup_type(id) {
        Type::Function(argument, result) => {
            let argument = rewrite(state, context, givens, argument, budget)?;
            let result = rewrite(state, context, givens, result, budget)?;
            Ok(context.intern_function(argument, result))
        }
        Type::Application(..) | Type::KindApplication(..) => {
            let spine = spine(state, context, id);
            let head = normalise::normalise(state, context, spine.head);
            let arguments = match context.lookup_type(head) {
                Type::Constructor(file_id, type_id) => {
                    let roles = roles::lookup_roles(state, context, (file_id, type_id))?;
                    let roles = roles.unwrap_or_else(|| Arc::from([]));
                    let arguments = spine.arguments.iter().enumerate().map(|(index, &argument)| {
                        match roles.get(index) {
                            Some(Role::Representational) => {
                                rewrite(state, context, givens, argument, budget)
                            }
                            _ => Ok(argument),
                        }
                    });
                    arguments.collect::<QueryResult<Vec<_>>>()?
                }
                _ => spine.arguments,
            };
            let head = match context.lookup_type(head) {
                Type::Constructor(..) => head,
                _ => rewrite(state, context, givens, head, budget)?,
            };
            Ok(rebuild(context, head, &spine.kind_arguments, &arguments))
        }
        Type::Row(row_id) => {
            let row = context.lookup_row_type(row_id);
            let fields = row.fields.iter().map(|field| {
                let id = rewrite(state, context, givens, field.id, budget)?;
                Ok(RowField { label: smol_str::SmolStr::clone(&field.label), id })
            });
            let fields = fields.collect::<QueryResult<Vec<_>>>()?;
            Ok(context.intern_row(fields, row.tail))
        }
        _ => Ok(id),
    }
}

fn rebuild<Q>(
    context: &CheckContext<Q>,
    head: TypeId,
    kind_arguments: &[TypeId],
    arguments: &[TypeId],
) -> TypeId
where
    Q: ExternalQueries,
{
    let head = kind_arguments
        .iter()
        .fold(head, |function, &argument| context.intern_kind_application(function, argument));
    context.intern_application_list(head, arguments)
}

fn spine<Q>(state: &CheckState, context: &CheckContext<Q>, id: TypeId) -> Spine
where
    Q: ExternalQueries,
{
    let mut kind_arguments = Vec::new();
    let mut arguments = Vec::new();
    let mut current = normalise::normalise(state, context, id);
    loop {
        match context.lookup_type(current) {
            Type::Application(function, argument) => {
                arguments.push(argument);
                current = normalise::normalise(state, context, function);
            }
            Type::KindApplication(function, argument) => {
                kind_arguments.push(argument);
                current = normalise::normalise(state, context, function);
            }
            Type::Function(argument, result)
                if arguments.is_empty() && kind_arguments.is_empty() =>
            {
                arguments.push(result);
                arguments.push(argument);
                current = context.types.function;
            }
            _ => break,
        }
    }
    kind_arguments.reverse();
    arguments.reverse();
    Spine { head: current, kind_arguments, arguments }
}

fn coercible_step<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    givens: &Givens,
    left: TypeId,
    right: TypeId,
) -> QueryResult<Step>
where
    Q: ExternalQueries,
{
    let left = fold::zonk(state, context, left);
    let right = fold::zonk(state, context, right);
    if left == right {
        return Ok(Step::Subgoals(Vec::new()));
    }
    let given = givens.pairs.iter().any(|&(given_left, given_right)| {
        (given_left, given_right) == (left, right) || (given_left, given_right) == (right, left)
    });
    if given {
        return Ok(Step::Subgoals(Vec::new()));
    }

    let left_normalised = normalise::normalise(state, context, left);
    let right_normalised = normalise::normalise(state, context, right);
    if let (Type::Row(_), Type::Row(_)) =
        (context.lookup_type(left_normalised), context.lookup_type(right_normalised))
    {
        return coercible_rows(state, context, left_normalised, right_normalised);
    }

    let left_spine = spine(state, context, left);
    let right_spine = spine(state, context, right);
    let left_head = context.lookup_type(left_spine.head);
    let right_head = context.lookup_type(right_spine.head);
    if matches!(left_head, Type::Unification(_)) || matches!(right_head, Type::Unification(_)) {
        return Ok(Step::Stuck);
    }

    if let Type::Constructor(..) = left_head {
        let kind = kind::kind_of(state, context, left)?;
        let kind = normalise::normalise(state, context, kind);
        if let Type::Function(argument_kind, _) = context.lookup_type(kind) {
            let text = context.intern_text(SmolStr::new_static("t"));
            let name = state.fresh_name();
            let binder = ForallBinder { visible: false, name, text, kind: argument_kind };
            let saturating = skolem::fresh_rigid(state, context, binder);
            let left = context.intern_application(left, saturating);
            let right = context.intern_application(right, saturating);
            return Ok(Step::Subgoals(vec![(left, right)]));
        }
    }

    if let Some(unwrapped) = unwrap_newtype(state, context, left)? {
        return Ok(Step::Subgoals(vec![(unwrapped, right)]));
    }
    if let Some(unwrapped) = unwrap_newtype(state, context, right)? {
        return Ok(Step::Subgoals(vec![(left, unwrapped)]));
    }

    if left_spine.head == right_spine.head
        && left_spine.arguments.len() == right_spine.arguments.len()
        && let Type::Constructor(file_id, type_id) = left_head
    {
        let roles = roles::lookup_roles(state, context, (file_id, type_id))?;
        let roles = roles.unwrap_or_else(|| Arc::from([]));
        return decompose(state, context, &left_spine, &right_spine, &roles);
    }

    let variable_heads = !matches!(left_head, Type::Constructor(..))
        && !matches!(right_head, Type::Constructor(..))
        && !left_spine.arguments.is_empty()
        && left_spine.arguments.len() == right_spine.arguments.len();
    if variable_heads {
        let mut step = decompose(state, context, &left_spine, &right_spine, &[])?;
        if let Step::Subgoals(subgoals) = &mut step {
            subgoals.push((left_spine.head, right_spine.head));
        }
        return Ok(step);
    }

    let irreducible = matches!(left_head, Type::Rigid(_)) || matches!(right_head, Type::Rigid(_));
    Ok(if irreducible { Step::Stuck } else { Step::Failed })
}

fn decompose<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    left: &Spine,
    right: &Spine,
    roles: &[Role],
) -> QueryResult<Step>
where
    Q: ExternalQueries,
{
    let kinds = left.kind_arguments.iter().zip_longest(&right.kind_arguments);
    for pair in kinds {
        let EitherOrBoth::Both(&left_kind, &right_kind) = pair else {
            return Ok(Step::Failed);
        };
        if unify::unify_core(state, context, left_kind, right_kind)?.is_err() {
            return Ok(Step::Failed);
        }
    }

    let mut subgoals = Vec::new();
    let arguments = left.arguments.iter().zip(&right.arguments).enumerate();
    for (index, (&left_argument, &right_argument)) in arguments {
        match roles.get(index).copied().unwrap_or(Role::Nominal) {
            Role::Phantom => (),
            Role::Representational => subgoals.push((left_argument, right_argument)),
            Role::Nominal => {
                if unify::unify_core(state, context, left_argument, right_argument)?.is_err() {
                    return Ok(Step::Failed);
                }
            }
        }
    }
    Ok(Step::Subgoals(subgoals))
}

fn coercible_rows<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    left: TypeId,
    right: TypeId,
) -> QueryResult<Step>
where
    Q: ExternalQueries,
{
    let left_row = normalise::row_parts(state, context, left);
    let right_row = normalise::row_parts(state, context, right);

    let mut subgoals = Vec::new();
    let mut extra = false;
    let fields = left_row
        .fields
        .iter()
        .merge_join_by(right_row.fields.iter(), |left, right| left.label.cmp(&right.label));
    for field in fields {
        match field {
            EitherOrBoth::Both(left_field, right_field) => {
                subgoals.push((left_field.id, right_field.id))
            }
            EitherOrBoth::Left(_) | EitherOrBoth::Right(_) => extra = true,
        }
    }

    let tail_unknown = |tail: Option<TypeId>| {
        tail.is_some_and(|tail| matches!(context.lookup_type(tail), Type::Unification(_)))
    };
    if tail_unknown(left_row.tail) || tail_unknown(right_row.tail) {
        if left_row.tail == right_row.tail && !extra {
            return Ok(Step::Subgoals(subgoals));
        }
        return Ok(Step::Stuck);
    }

    if extra {
        return Ok(Step::Failed);
    }

    match (left_row.tail, right_row.tail) {
        (None, None) => Ok(Step::Subgoals(subgoals)),
        (Some(left_tail), Some(right_tail)) if left_tail == right_tail => {
            Ok(Step::Subgoals(subgoals))
        }
        _ => Ok(Step::Failed),
    }
}

fn unwrap_newtype<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    id: TypeId,
) -> QueryResult<Option<TypeId>>
where
    Q: ExternalQueries,
{
    let Some(NewtypeField { constructor: (file_id, item_id), field }) =
        data::newtype_field(state, context, id)?
    else {
        return Ok(None);
    };
    if file_id == context.id {
        return Ok(Some(field));
    }
    let Some(prim) = context.queries.module_file("Prim") else {
        return Ok(None);
    };
    let prim = context.queries.resolved(prim)?;
    let in_scope = context.resolved.is_term_in_scope(&prim, file_id, item_id);
    Ok(in_scope.then_some(field))
}
