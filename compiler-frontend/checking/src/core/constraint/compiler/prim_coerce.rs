use std::iter;

use building_types::QueryResult;
use files::FileId;
use indexing::TypeItemId;
use itertools::{Itertools, izip};

use crate::context::CheckContext;
use crate::core::constraint::canonical;
use crate::core::constraint::matching::MatchInstance;
use crate::core::substitute::SubstituteName;
use crate::core::unification::{CanUnify, can_unify, unify};
use crate::core::{Role, Type, TypeId, normalise, toolkit};
use crate::error::ErrorKind;
use crate::source::types;
use crate::state::CheckState;
use crate::{ExternalQueries, safe_loop};

enum NewtypeCoercionResult {
    Success(MatchInstance),
    ConstructorNotInScope { file_id: FileId, item_id: TypeItemId },
    Nothing,
}

pub fn match_coercible<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    wanted: canonical::CanonicalConstraintId,
    arguments: &[TypeId],
) -> QueryResult<Option<MatchInstance>>
where
    Q: ExternalQueries,
{
    let &[left, right] = arguments else {
        return Ok(None);
    };

    let left = normalise::expand(state, context, left)?;
    let right = normalise::expand(state, context, right)?;

    if left == right {
        return Ok(Some(MatchInstance::empty()));
    }

    if let Some(stuck) = unification_head(state, context, left)? {
        return Ok(Some(MatchInstance::Stuck { stuck: vec![stuck], skolem: false }));
    }

    if let Some(stuck) = unification_head(state, context, right)? {
        return Ok(Some(MatchInstance::Stuck { stuck: vec![stuck], skolem: false }));
    }

    if try_refl(state, context, left, right)? {
        return Ok(Some(MatchInstance::empty()));
    }

    let newtype_result = try_newtype_coercion(state, context, left, right)?;
    if let NewtypeCoercionResult::Success(result) = newtype_result {
        return Ok(Some(result));
    }

    if let Some(result) = try_application_coercion(state, context, left, right)? {
        return Ok(Some(result));
    }

    if let Some(result) = try_function_coercion(state, context, left, right)? {
        return Ok(Some(result));
    }

    if let Some(result) = try_higher_kinded_coercion(state, context, left, right)? {
        return Ok(Some(result));
    }

    if let Some(result) = try_row_coercion(state, context, left, right)? {
        return Ok(Some(result));
    }

    if let NewtypeCoercionResult::ConstructorNotInScope { file_id, item_id } = newtype_result {
        let error = ErrorKind::CoercibleConstructorNotInScope { file_id, item_id };
        state.canonical_errors.entry(wanted).or_default().push(error);
    }

    Ok(Some(MatchInstance::Apart))
}

fn unification_head<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    mut id: TypeId,
) -> QueryResult<Option<u32>>
where
    Q: ExternalQueries,
{
    safe_loop! {
        id = normalise::expand(state, context, id)?;
        match *context.lookup_type(id) {
            Type::Unification(u) => return Ok(Some(u)),
            Type::Application(function, _) | Type::KindApplication(function, _) => {
                id = function;
            }
            _ => return Ok(None),
        }
    }
}

fn has_type_kind<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: TypeId,
) -> QueryResult<bool>
where
    Q: ExternalQueries,
{
    let kind = types::elaborate_kind(state, context, id)?;
    let kind = normalise::expand(state, context, kind)?;
    Ok(kind == context.prim.t)
}

fn try_newtype_coercion<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    left: TypeId,
    right: TypeId,
) -> QueryResult<NewtypeCoercionResult>
where
    Q: ExternalQueries,
{
    let mut constructor: Option<(FileId, TypeItemId)> = None;

    for (side, other) in [(left, right), (right, left)] {
        if has_type_kind(state, context, side)?
            && let Some((file_id, item_id)) =
                toolkit::extract_type_constructor(state, context, side)?
            && toolkit::is_newtype(context, file_id, item_id)?
        {
            if !toolkit::is_constructor_in_scope(context, file_id, item_id)? {
                constructor.get_or_insert((file_id, item_id));
                continue;
            }
            if let Some(toolkit::NewtypeInner { inner, .. }) =
                toolkit::get_newtype_inner(state, context, file_id, item_id, side)?
            {
                let constraint = if side == left {
                    make_coercible_constraint(state, context, inner, other)?
                } else {
                    make_coercible_constraint(state, context, other, inner)?
                };

                let canonical = canonical::canonicalise(state, context, constraint)?;
                let constraints = canonical.into_iter().collect_vec();

                return Ok(NewtypeCoercionResult::Success(MatchInstance::from_constraints(
                    constraints,
                )));
            }
        }
    }

    if let Some((file_id, item_id)) = constructor {
        return Ok(NewtypeCoercionResult::ConstructorNotInScope { file_id, item_id });
    }

    Ok(NewtypeCoercionResult::Nothing)
}

fn try_application_coercion<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    left: TypeId,
    right: TypeId,
) -> QueryResult<Option<MatchInstance>>
where
    Q: ExternalQueries,
{
    let Some((left_file, left_id)) = toolkit::extract_type_constructor(state, context, left)?
    else {
        return Ok(None);
    };
    let Some((right_file, right_id)) = toolkit::extract_type_constructor(state, context, right)?
    else {
        return Ok(None);
    };

    if left_file != right_file || left_id != right_id {
        return Ok(None);
    }

    let (_, left_arguments) = toolkit::extract_type_application(state, context, left)?;
    let (_, right_arguments) = toolkit::extract_type_application(state, context, right)?;

    if left_arguments.len() != right_arguments.len() {
        return Ok(Some(MatchInstance::Apart));
    }

    let Some(roles) = toolkit::lookup_file_roles(state, context, left_file, left_id)? else {
        return Ok(Some(MatchInstance::Apart));
    };

    debug_assert_eq!(roles.len(), left_arguments.len(), "critical failure: mismatched lengths");
    debug_assert_eq!(roles.len(), right_arguments.len(), "critical failure: mismatched lengths");

    let mut unifications = Vec::new();
    let mut constraints = Vec::new();

    for (role, &left_argument, &right_argument) in izip!(&*roles, &left_arguments, &right_arguments)
    {
        match role {
            Role::Phantom => (),
            Role::Representational => {
                let constraint =
                    make_coercible_constraint(state, context, left_argument, right_argument)?;
                if let Some(canonical_id) = canonical::canonicalise(state, context, constraint)? {
                    constraints.push(canonical_id);
                }
            }
            Role::Nominal => {
                if left_argument != right_argument {
                    if can_unify(state, context, left_argument, right_argument)? == CanUnify::Apart
                    {
                        return Ok(Some(MatchInstance::Apart));
                    }
                    unifications.push((left_argument, right_argument));
                }
            }
        }
    }

    Ok(Some(MatchInstance::from_parts(unifications, constraints)))
}

fn try_function_coercion<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    left: TypeId,
    right: TypeId,
) -> QueryResult<Option<MatchInstance>>
where
    Q: ExternalQueries,
{
    let left_function = decompose_function_simple(state, context, left)?;
    let right_function = decompose_function_simple(state, context, right)?;

    let (Some((left_argument, left_result)), Some((right_argument, right_result))) =
        (left_function, right_function)
    else {
        return Ok(None);
    };

    let mut constraints = Vec::new();

    let left_constraint = make_coercible_constraint(state, context, left_argument, right_argument)?;
    if let Some(canonical_id) = canonical::canonicalise(state, context, left_constraint)? {
        constraints.push(canonical_id);
    }

    let right_constraint = make_coercible_constraint(state, context, left_result, right_result)?;
    if let Some(canonical_id) = canonical::canonicalise(state, context, right_constraint)? {
        constraints.push(canonical_id);
    }

    Ok(Some(MatchInstance::from_constraints(constraints)))
}

fn decompose_function_simple<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: TypeId,
) -> QueryResult<Option<(TypeId, TypeId)>>
where
    Q: ExternalQueries,
{
    let id = normalise::expand(state, context, id)?;
    match *context.lookup_type(id) {
        Type::Function(argument, result) => Ok(Some((argument, result))),
        Type::Application(partial, result) => {
            let partial = normalise::expand(state, context, partial)?;
            if let Type::Application(constructor, argument) = *context.lookup_type(partial) {
                let constructor = normalise::expand(state, context, constructor)?;
                if constructor == context.prim.function {
                    return Ok(Some((argument, result)));
                }
            }
            Ok(None)
        }
        _ => Ok(None),
    }
}

fn try_row_coercion<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    left: TypeId,
    right: TypeId,
) -> QueryResult<Option<MatchInstance>>
where
    Q: ExternalQueries,
{
    let left = normalise::expand(state, context, left)?;
    let right = normalise::expand(state, context, right)?;

    let Type::Row(left_row_id) = *context.lookup_type(left) else { return Ok(None) };
    let Type::Row(right_row_id) = *context.lookup_type(right) else { return Ok(None) };

    let left_row = context.lookup_row_type(left_row_id);
    let right_row = context.lookup_row_type(right_row_id);

    if left_row.fields.len() != right_row.fields.len() {
        return Ok(Some(MatchInstance::Apart));
    }

    let mut constraints = Vec::new();

    for (left_field, right_field) in izip!(left_row.fields.iter(), right_row.fields.iter()) {
        if left_field.label != right_field.label {
            return Ok(Some(MatchInstance::Apart));
        }
        let constraint = make_coercible_constraint(state, context, left_field.id, right_field.id)?;
        if let Some(canonical_id) = canonical::canonicalise(state, context, constraint)? {
            constraints.push(canonical_id);
        }
    }

    match (left_row.tail, right_row.tail) {
        (None, None) => (),
        (Some(left_tail), Some(right_tail)) => {
            let constraint = make_coercible_constraint(state, context, left_tail, right_tail)?;
            if let Some(canonical_id) = canonical::canonicalise(state, context, constraint)? {
                constraints.push(canonical_id);
            }
        }
        (None, Some(_)) | (Some(_), None) => {
            return Ok(Some(MatchInstance::Apart));
        }
    }

    Ok(Some(MatchInstance::from_constraints(constraints)))
}

fn try_higher_kinded_coercion<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    left: TypeId,
    right: TypeId,
) -> QueryResult<Option<MatchInstance>>
where
    Q: ExternalQueries,
{
    let left_kind = types::elaborate_kind(state, context, left)?;
    let right_kind = types::elaborate_kind(state, context, right)?;

    let Some((left_applied, left_domain)) =
        decompose_kind_for_coercion(state, context, left, left_kind)?
    else {
        return Ok(None);
    };

    let Some((right_applied, right_domain)) =
        decompose_kind_for_coercion(state, context, right, right_kind)?
    else {
        return Ok(None);
    };

    if can_unify(state, context, left_domain, right_domain)? == CanUnify::Apart {
        return Ok(Some(MatchInstance::Apart));
    }

    let argument = state.fresh_rigid(context.queries, left_domain);
    let left_saturated = context.intern_application(left_applied, argument);
    let right_saturated = context.intern_application(right_applied, argument);
    let constraint = make_coercible_constraint(state, context, left_saturated, right_saturated)?;

    let constraints = canonical::canonicalise(state, context, constraint)?.into_iter().collect();

    Ok(Some(MatchInstance::from_constraints(constraints)))
}

fn decompose_kind_for_coercion<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    mut type_id: TypeId,
    mut kind_id: TypeId,
) -> QueryResult<Option<(TypeId, TypeId)>>
where
    Q: ExternalQueries,
{
    safe_loop! {
        kind_id = normalise::expand(state, context, kind_id)?;
        match *context.lookup_type(kind_id) {
            Type::Forall(binder_id, inner_kind) => {
                let binder = context.lookup_forall_binder(binder_id);
                let fresh = state.fresh_rigid(context.queries, binder.kind);
                type_id = context.intern_kind_application(type_id, fresh);
                kind_id = SubstituteName::one(state, context, binder.name, fresh, inner_kind)?;
            }
            Type::Function(domain, _) => return Ok(Some((type_id, domain))),
            _ => return Ok(None),
        }
    }
}

fn try_refl<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    t1_type: TypeId,
    t2_type: TypeId,
) -> QueryResult<bool>
where
    Q: ExternalQueries,
{
    let t1_type = normalise::expand(state, context, t1_type)?;
    let t2_type = normalise::expand(state, context, t2_type)?;

    if t1_type == t2_type {
        return Ok(true);
    }

    match (context.lookup_type(t1_type), context.lookup_type(t2_type)) {
        (
            Type::Application(t1_function, t1_argument),
            Type::Application(t2_function, t2_argument),
        )
        | (
            Type::Constrained(t1_function, t1_argument),
            Type::Constrained(t2_function, t2_argument),
        )
        | (
            Type::KindApplication(t1_function, t1_argument),
            Type::KindApplication(t2_function, t2_argument),
        )
        | (Type::Kinded(t1_function, t1_argument), Type::Kinded(t2_function, t2_argument)) => {
            Ok(try_refl(state, context, *t1_function, *t2_function)?
                && try_refl(state, context, *t1_argument, *t2_argument)?)
        }

        (Type::Function(t1_argument, t1_result), Type::Function(t2_argument, t2_result)) => {
            Ok(try_refl(state, context, *t1_argument, *t2_argument)?
                && try_refl(state, context, *t1_result, *t2_result)?)
        }

        (Type::Forall(t1_binder_id, t1_inner), Type::Forall(t2_binder_id, t2_inner)) => {
            let t1_binder = context.lookup_forall_binder(*t1_binder_id);
            let t2_binder = context.lookup_forall_binder(*t2_binder_id);
            Ok(try_refl(state, context, t1_binder.kind, t2_binder.kind)?
                && try_refl(state, context, *t1_inner, *t2_inner)?)
        }

        (Type::Row(t1_row_id), Type::Row(t2_row_id)) => {
            let t1_row = context.lookup_row_type(*t1_row_id);
            let t2_row = context.lookup_row_type(*t2_row_id);
            if t1_row.fields.len() != t2_row.fields.len() {
                return Ok(false);
            }
            for (t1_field, t2_field) in iter::zip(t1_row.fields.iter(), t2_row.fields.iter()) {
                if t1_field.label != t2_field.label
                    || !try_refl(state, context, t1_field.id, t2_field.id)?
                {
                    return Ok(false);
                }
            }
            match (t1_row.tail, t2_row.tail) {
                (Some(t1_tail), Some(t2_tail)) => try_refl(state, context, t1_tail, t2_tail),
                (None, None) => Ok(true),
                _ => Ok(false),
            }
        }

        (Type::Rigid(t1_name, _, t1_kind), Type::Rigid(t2_name, _, t2_kind)) => {
            Ok(t1_name == t2_name && try_refl(state, context, *t1_kind, *t2_kind)?)
        }

        _ => Ok(false),
    }
}

fn make_coercible_constraint<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    left: TypeId,
    right: TypeId,
) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    let kind = state.fresh_unification(context.queries, context.prim.t);

    let left_kind = types::elaborate_kind(state, context, left)?;
    let right_kind = types::elaborate_kind(state, context, right)?;

    unify(state, context, kind, left_kind)?;
    unify(state, context, kind, right_kind)?;

    let prim_coerce = &context.prim_coerce;

    let coercible = Type::Constructor(prim_coerce.file_id, prim_coerce.coercible);
    let coercible = context.queries.intern_type(coercible);

    let coercible = context.intern_kind_application(coercible, kind);
    let coercible = context.intern_application(coercible, left);

    Ok(context.intern_application(coercible, right))
}
