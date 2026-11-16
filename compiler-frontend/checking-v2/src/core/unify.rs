use std::cmp::Ordering;
use std::sync::Arc;

use building_types::QueryResult;
use itertools::{EitherOrBoth, Itertools};
use smol_str::SmolStr;

use crate::ExternalQueries;
use crate::context::CheckContext;
use crate::core::{kind, normalise, skolem};
use crate::error::ErrorKind;
use crate::state::CheckState;
use crate::types::{Level, RowField, RowType, Type, TypeId, UnificationId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mismatch {
    Types(TypeId, TypeId),
    Kinds(TypeId, TypeId),
    InfiniteType(TypeId, TypeId),
    InfiniteKind(TypeId, TypeId),
}

impl Mismatch {
    fn in_kind(self) -> Mismatch {
        match self {
            Mismatch::Types(expected, actual) => Mismatch::Kinds(expected, actual),
            Mismatch::InfiniteType(unification, solution) => {
                Mismatch::InfiniteKind(unification, solution)
            }
            mismatch => mismatch,
        }
    }

    pub fn into_error(self) -> ErrorKind {
        match self {
            Mismatch::Types(expected, actual) => ErrorKind::TypesDoNotUnify { expected, actual },
            Mismatch::Kinds(expected, actual) => ErrorKind::KindsDoNotUnify { expected, actual },
            Mismatch::InfiniteType(unification, solution) => {
                ErrorKind::InfiniteType { unification, solution }
            }
            Mismatch::InfiniteKind(unification, solution) => {
                ErrorKind::InfiniteKind { unification, solution }
            }
        }
    }
}

pub type Unified = QueryResult<Result<(), Mismatch>>;

pub fn unify<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    expected: TypeId,
    actual: TypeId,
) -> QueryResult<bool>
where
    Q: ExternalQueries,
{
    match unify_core(state, context, expected, actual)? {
        Ok(()) => Ok(true),
        Err(mismatch) => {
            state.insert_error(mismatch.into_error());
            Ok(false)
        }
    }
}

pub fn unify_kinds<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    expected: TypeId,
    actual: TypeId,
) -> QueryResult<bool>
where
    Q: ExternalQueries,
{
    match unify_core(state, context, expected, actual)? {
        Ok(()) => Ok(true),
        Err(mismatch) => {
            state.insert_error(mismatch.in_kind().into_error());
            Ok(false)
        }
    }
}

pub fn unify_core<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    t1: TypeId,
    t2: TypeId,
) -> Unified
where
    Q: ExternalQueries,
{
    let t1 = normalise::normalise(state, context, t1);
    let t2 = normalise::normalise(state, context, t2);

    if t1 == t2 {
        return Ok(Ok(()));
    }

    let mismatch = Ok(Err(Mismatch::Types(t1, t2)));

    match (context.lookup_type(t1), context.lookup_type(t2)) {
        (Type::Unknown, _) | (_, Type::Unknown) => Ok(Ok(())),

        (Type::Unification(t1_id), Type::Unification(t2_id)) => {
            let t1_level = state.unification(t1_id).level;
            let t2_level = state.unification(t2_id).level;
            if t1_level < t2_level {
                solve(state, context, t2_id, t1)
            } else {
                solve(state, context, t1_id, t2)
            }
        }

        (Type::Unification(t1_id), _) => solve(state, context, t1_id, t2),

        (_, Type::Unification(t2_id)) => solve(state, context, t2_id, t1),

        (Type::Forall(t1_binder_id, t1_inner), Type::Forall(t2_binder_id, t2_inner)) => {
            let t1_binder = context.lookup_forall_binder(t1_binder_id);
            let t2_binder = context.lookup_forall_binder(t2_binder_id);
            if let Err(mismatch) = unify_core(state, context, t1_binder.kind, t2_binder.kind)? {
                return Ok(Err(mismatch.in_kind()));
            }
            state.with_level(|state| {
                let rigid = skolem::fresh_rigid(state, context, t1_binder);
                let t1_inner = skolem::replace_binder(context, t1_binder, rigid, t1_inner);
                let t2_inner = skolem::replace_binder(context, t2_binder, rigid, t2_inner);
                unify_core(state, context, t1_inner, t2_inner)
            })
        }

        (Type::Forall(binder_id, inner), _) => state.with_level(|state| {
            let binder = context.lookup_forall_binder(binder_id);
            let rigid = skolem::fresh_rigid(state, context, binder);
            let inner = skolem::replace_binder(context, binder, rigid, inner);
            unify_core(state, context, inner, t2)
        }),

        (_, Type::Forall(binder_id, inner)) => state.with_level(|state| {
            let binder = context.lookup_forall_binder(binder_id);
            let rigid = skolem::fresh_rigid(state, context, binder);
            let inner = skolem::replace_binder(context, binder, rigid, inner);
            unify_core(state, context, t1, inner)
        }),

        (Type::Function(t1_argument, t1_result), Type::Function(t2_argument, t2_result)) => {
            if let Err(mismatch) = unify_core(state, context, t1_argument, t2_argument)? {
                return Ok(Err(mismatch));
            }
            unify_core(state, context, t1_result, t2_result)
        }

        (Type::Function(argument, result), Type::Application(..)) => {
            let function = context.intern_application(context.types.function, argument);
            let function = context.intern_application(function, result);
            unify_core(state, context, function, t2)
        }

        (Type::Application(..), Type::Function(argument, result)) => {
            let function = context.intern_application(context.types.function, argument);
            let function = context.intern_application(function, result);
            unify_core(state, context, t1, function)
        }

        (
            Type::Application(t1_function, t1_argument),
            Type::Application(t2_function, t2_argument),
        )
        | (
            Type::KindApplication(t1_function, t1_argument),
            Type::KindApplication(t2_function, t2_argument),
        ) => {
            if let Err(mismatch) = unify_core(state, context, t1_function, t2_function)? {
                return Ok(Err(mismatch));
            }
            unify_core(state, context, t1_argument, t2_argument)
        }

        (
            Type::Constrained(t1_constraint, t1_constrained),
            Type::Constrained(t2_constraint, t2_constrained),
        ) => {
            if let Err(mismatch) = unify_core(state, context, t1_constraint, t2_constraint)? {
                return Ok(Err(mismatch));
            }
            unify_core(state, context, t1_constrained, t2_constrained)
        }

        (Type::Row(_), _) | (_, Type::Row(_)) => unify_rows(state, context, t1, t2),

        (Type::Rigid(t1_rigid), Type::Rigid(t2_rigid)) => {
            let t1_rigid = context.lookup_rigid(t1_rigid);
            let t2_rigid = context.lookup_rigid(t2_rigid);
            if t1_rigid.name == t2_rigid.name { Ok(Ok(())) } else { mismatch }
        }

        (Type::Variable(t1_binder), Type::Variable(t2_binder)) => {
            let t1_binder = context.lookup_forall_binder(t1_binder);
            let t2_binder = context.lookup_forall_binder(t2_binder);
            if t1_binder.name == t2_binder.name { Ok(Ok(())) } else { mismatch }
        }

        (Type::String(t1_value), Type::String(t2_value)) => {
            if t1_value == t2_value {
                Ok(Ok(()))
            } else {
                mismatch
            }
        }

        _ => mismatch,
    }
}

fn unify_rows<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    t1: TypeId,
    t2: TypeId,
) -> Unified
where
    Q: ExternalQueries,
{
    let t1_row = normalise::row_parts(state, context, t1);
    let t2_row = normalise::row_parts(state, context, t2);

    let t1_groups = t1_row.fields.iter().chunk_by(|field| SmolStr::clone(&field.label));
    let t1_groups = t1_groups.into_iter().map(|(_, group)| group.collect_vec()).collect_vec();
    let t2_groups = t2_row.fields.iter().chunk_by(|field| SmolStr::clone(&field.label));
    let t2_groups = t2_groups.into_iter().map(|(_, group)| group.collect_vec()).collect_vec();

    let mut t1_rest = Vec::new();
    let mut t2_rest = Vec::new();
    let mut common = Vec::new();

    let groups = t1_groups.into_iter().merge_join_by(t2_groups, |t1_group, t2_group| {
        let t1_label = t1_group.first().map(|field| &field.label);
        let t2_label = t2_group.first().map(|field| &field.label);
        t1_label.cmp(&t2_label)
    });

    for group in groups {
        match group {
            EitherOrBoth::Both(t1_group, t2_group) => {
                for pair in t1_group.into_iter().zip_longest(t2_group) {
                    match pair {
                        EitherOrBoth::Both(t1_field, t2_field) => {
                            common.push((t1_field.id, t2_field.id));
                        }
                        EitherOrBoth::Left(field) => t1_rest.push(RowField::clone(field)),
                        EitherOrBoth::Right(field) => t2_rest.push(RowField::clone(field)),
                    }
                }
            }
            EitherOrBoth::Left(group) => {
                for field in group {
                    t1_rest.push(RowField::clone(field));
                }
            }
            EitherOrBoth::Right(group) => {
                for field in group {
                    t2_rest.push(RowField::clone(field));
                }
            }
        }
    }

    for (t1_field, t2_field) in common {
        if let Err(mismatch) = unify_core(state, context, t1_field, t2_field)? {
            return Ok(Err(mismatch));
        }
    }

    let t1_remaining = context.intern_row(t1_rest, t1_row.tail);
    let t2_remaining = context.intern_row(t2_rest, t2_row.tail);
    let t1_rest = normalise::row_parts(state, context, t1_remaining);
    let t2_rest = normalise::row_parts(state, context, t2_remaining);

    let tails_solved = t1_rest.tail != t1_row.tail || t2_rest.tail != t2_row.tail;
    if tails_solved {
        return unify_rows(state, context, t1_remaining, t2_remaining);
    }

    unify_row_tails(state, context, (t1, t1_rest), (t2, t2_rest))
}

fn unify_row_tails<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    (t1, t1_rest): (TypeId, RowType),
    (t2, t2_rest): (TypeId, RowType),
) -> Unified
where
    Q: ExternalQueries,
{
    let empty = || context.intern_empty_row();
    let t1_tail = t1_rest.tail.unwrap_or_else(empty);
    let t2_tail = t2_rest.tail.unwrap_or_else(empty);

    match (t1_rest.fields.is_empty(), t2_rest.fields.is_empty()) {
        (true, true) => {
            if t1_rest.tail.is_none() && t2_rest.tail.is_none() {
                return Ok(Ok(()));
            }
            unify_core(state, context, t1_tail, t2_tail)
        }
        (true, false) => {
            if t1_rest.tail.is_none() {
                return Ok(Err(Mismatch::Types(t1, t2)));
            }
            let t2_remaining = context.intern_type(Type::Row(context.intern_row_type(t2_rest)));
            unify_core(state, context, t1_tail, t2_remaining)
        }
        (false, true) => {
            if t2_rest.tail.is_none() {
                return Ok(Err(Mismatch::Types(t1, t2)));
            }
            let t1_remaining = context.intern_type(Type::Row(context.intern_row_type(t1_rest)));
            unify_core(state, context, t1_remaining, t2_tail)
        }
        (false, false) => {
            let t1_tail_type = context.lookup_type(t1_tail);
            let t2_tail_type = context.lookup_type(t2_tail);
            let (Type::Unification(t1_id), Type::Unification(t2_id)) = (t1_tail_type, t2_tail_type)
            else {
                return Ok(Err(Mismatch::Types(t1, t2)));
            };
            if t1_id == t2_id {
                return Ok(Err(Mismatch::Types(t1, t2)));
            }

            let t1_level = state.unification(t1_id).level;
            let t2_level = state.unification(t2_id).level;
            let level = t1_level.min(t2_level);
            let rest_kind = state.unification(t1_id).kind;
            let rest = fresh_unification_at(state, context, rest_kind, level);

            let t2_fields = Arc::clone(&t2_rest.fields);
            let t1_solution =
                context.intern_row(t2_fields.iter().map(RowField::clone).collect(), Some(rest));
            if let Err(mismatch) = unify_core(state, context, t1_tail, t1_solution)? {
                return Ok(Err(mismatch));
            }

            let t1_fields = Arc::clone(&t1_rest.fields);
            let t2_solution =
                context.intern_row(t1_fields.iter().map(RowField::clone).collect(), Some(rest));
            unify_core(state, context, t2_tail, t2_solution)
        }
    }
}

pub fn fresh_unification_at<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    kind: TypeId,
    level: Level,
) -> TypeId
where
    Q: ExternalQueries,
{
    let unification = kind::fresh_unification(state, context, kind);
    if let Type::Unification(unification_id) = context.lookup_type(unification) {
        state.unification_mut(unification_id).level = level;
    }
    unification
}

fn solve<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    unification_id: UnificationId,
    solution: TypeId,
) -> Unified
where
    Q: ExternalQueries,
{
    let entry = state.unification(unification_id);
    debug_assert!(entry.solution.is_none(), "invariant violated: solving a solved unification");

    let unification = context.intern_type(Type::Unification(unification_id));
    match promote(state, context, unification_id, entry.level, solution) {
        Promotion::Promoted => (),
        Promotion::Occurs => return Ok(Err(Mismatch::InfiniteType(unification, solution))),
    }

    state.unification_mut(unification_id).solution = Some(solution);
    state.solutions += 1;

    let solution_kind = kind::kind_of(state, context, solution)?;
    match unify_core(state, context, entry.kind, solution_kind)? {
        Ok(()) => Ok(Ok(())),
        Err(mismatch) => Ok(Err(mismatch.in_kind())),
    }
}

enum Promotion {
    Promoted,
    Occurs,
}

fn promote<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    unification_id: UnificationId,
    level: Level,
    id: TypeId,
) -> Promotion
where
    Q: ExternalQueries,
{
    let flags = context.lookup_type_flags(id);
    if !flags.has_unification() && !flags.has_rigid() {
        return Promotion::Promoted;
    }

    let mut children = Vec::new();
    match context.lookup_type(id) {
        Type::Unification(other_id) => {
            let entry = state.unification(other_id);
            if let Some(solution) = entry.solution {
                return promote(state, context, unification_id, level, solution);
            }
            if other_id == unification_id {
                return Promotion::Occurs;
            }
            if entry.level > level {
                state.unification_mut(other_id).level = level;
                return promote(state, context, unification_id, level, entry.kind);
            }
            return Promotion::Promoted;
        }
        Type::Rigid(rigid_id) => children.push(context.lookup_rigid(rigid_id).kind),
        Type::Application(function, argument) | Type::KindApplication(function, argument) => {
            children.extend([function, argument]);
        }
        Type::Function(argument, result) => children.extend([argument, result]),
        Type::Constrained(constraint, constrained) => children.extend([constraint, constrained]),
        Type::Kinded(inner, kind) => children.extend([inner, kind]),
        Type::Forall(binder_id, inner) => {
            let binder = context.lookup_forall_binder(binder_id);
            children.extend([binder.kind, inner]);
        }
        Type::Synonym(application_id) => {
            let application = context.lookup_synonym_application(application_id);
            children.push(application.expansion);
        }
        Type::Row(row_id) => {
            let row = context.lookup_row_type(row_id);
            children.extend(row.fields.iter().map(|field| field.id));
            children.extend(row.tail);
        }
        Type::Variable(binder_id) => children.push(context.lookup_forall_binder(binder_id).kind),
        Type::Constructor(..) | Type::Integer(_) | Type::String(_) | Type::Unknown => (),
    }

    for child in children {
        match promote(state, context, unification_id, level, child) {
            Promotion::Promoted => (),
            failure => return failure,
        }
    }
    Promotion::Promoted
}

pub fn compare_levels(left: Level, right: Level) -> Ordering {
    left.cmp(&right)
}
