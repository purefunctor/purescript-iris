use building_types::QueryResult;
use rustc_hash::FxHashSet;

use crate::ExternalQueries;
use crate::context::CheckContext;
use crate::core::fold::{FoldAction, TypeFold, fold_type};
use crate::core::{Type, TypeId, normalise, toolkit};
use crate::error::ErrorKind;
use crate::source::synonym;
use crate::state::CheckState;

fn bundle_members<Q>(context: &CheckContext<Q>, id: TypeId) -> Option<[TypeId; 2]>
where
    Q: ExternalQueries,
{
    let Type::Application(function, right) = *context.lookup_type(id) else {
        return None;
    };
    let Type::Application(constructor, left) = *context.lookup_type(function) else {
        return None;
    };
    (constructor == context.iris_effect.bundle).then_some([left, right])
}

fn written_application<Q>(context: &CheckContext<Q>, mut id: TypeId) -> (TypeId, Vec<TypeId>)
where
    Q: ExternalQueries,
{
    let mut arguments = vec![];
    loop {
        match *context.lookup_type(id) {
            Type::Application(function, argument) => {
                arguments.push(argument);
                id = function;
            }
            Type::KindApplication(function, _) => id = function,
            _ => break,
        }
    }
    arguments.reverse();
    (id, arguments)
}

fn is_has<Q>(context: &CheckContext<Q>, id: TypeId) -> bool
where
    Q: ExternalQueries,
{
    *context.lookup_type(id)
        == Type::Constructor(context.iris_effect.file_id, context.iris_effect.has)
}

fn is_variable<Q>(context: &CheckContext<Q>, id: TypeId) -> bool
where
    Q: ExternalQueries,
{
    matches!(context.lookup_type(id), Type::Rigid(..) | Type::Unification(_) | Type::Free(_))
}

fn expand<Q>(state: &mut CheckState, context: &CheckContext<Q>, id: TypeId) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    if let Type::Synonym(file_id, type_id) =
        *context.lookup_type(written_application(context, id).0)
        && synonym::is_recursive_synonym(context, file_id, type_id)?
    {
        return Ok(id);
    }
    normalise::expand(state, context, id)
}

pub fn expand_constraint<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    constraint: TypeId,
) -> QueryResult<Vec<TypeId>>
where
    Q: ExternalQueries,
{
    let (has, arguments) = toolkit::extract_type_application(state, context, constraint)?;
    let &[service, services] = arguments.as_slice() else {
        return Ok(vec![constraint]);
    };
    if !is_has(context, has) {
        return Ok(vec![constraint]);
    }
    let expanded = expand(state, context, service)?;
    if bundle_members(context, expanded).is_none() {
        return Ok(vec![constraint]);
    }

    let mut pending = vec![(service, vec![])];
    let mut seen = FxHashSet::default();
    let mut constraints = vec![];

    while let Some((service, mut origins)) = pending.pop() {
        let expanded = expand(state, context, service)?;
        if let Some([left, right]) = bundle_members(context, expanded) {
            if expanded != service {
                origins.push(written_application(context, service).0);
            }
            pending.push((right, origins.clone()));
            pending.push((left, origins));
            continue;
        }

        if is_variable(context, expanded) {
            state.insert_error(ErrorKind::InvalidCapabilityBundleMember { member: service });
        }

        if seen.insert(expanded) {
            let service = validate(state, context, service, false)?;
            let constraint = context.intern_application(has, service);
            let constraint = context.intern_application(constraint, services);
            let constraint = origins.iter().rev().fold(constraint, |constraint, &origin| {
                context.intern_constraint_origin(constraint, origin)
            });
            constraints.push(constraint);
        }
    }

    Ok(constraints)
}

pub fn validate<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: TypeId,
    allow_bundle: bool,
) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    let mut validation = Validation::default();
    validation.validate(state, context, id, allow_bundle)?;
    if validation.invalid.is_empty() {
        Ok(id)
    } else {
        fold_type(state, context, id, &mut validation)
    }
}

#[derive(Default)]
struct Validation {
    visited: FxHashSet<(TypeId, bool)>,
    invalid: FxHashSet<TypeId>,
}

impl Validation {
    fn validate<Q>(
        &mut self,
        state: &mut CheckState,
        context: &CheckContext<Q>,
        id: TypeId,
        allow_bundle: bool,
    ) -> QueryResult<()>
    where
        Q: ExternalQueries,
    {
        if !self.visited.insert((id, allow_bundle)) {
            return Ok(());
        }

        let expanded = expand(state, context, id)?;
        let (constructor, arguments) = written_application(context, expanded);

        if let Some(members) = bundle_members(context, expanded) {
            if !allow_bundle {
                state.insert_error(ErrorKind::InvalidCapabilityBundlePosition { bundle: id });
                self.invalid.insert(id);
                return Ok(());
            }
            for member in members {
                if is_variable(context, expand(state, context, member)?) {
                    state.insert_error(ErrorKind::InvalidCapabilityBundleMember { member });
                } else {
                    self.validate(state, context, member, true)?;
                }
            }
            return Ok(());
        }

        if constructor == context.iris_effect.bundle {
            state.insert_error(ErrorKind::InvalidCapabilityBundlePosition { bundle: id });
            self.invalid.insert(id);
            return Ok(());
        }

        if is_has(context, constructor)
            && let &[service, services] = arguments.as_slice()
        {
            self.validate(state, context, service, true)?;
            return self.validate(state, context, services, false);
        }

        match *context.lookup_type(id) {
            Type::Application(left, right)
            | Type::KindApplication(left, right)
            | Type::Constrained(left, right)
            | Type::Function(left, right) => {
                self.validate(state, context, left, false)?;
                self.validate(state, context, right, false)?;
            }
            Type::Kinded(inner, kind) => {
                self.validate(state, context, inner, allow_bundle)?;
                self.validate(state, context, kind, false)?;
            }
            Type::ConstraintOrigin(constraint, _) => {
                self.validate(state, context, constraint, false)?;
            }
            Type::Forall(binder_id, inner) => {
                let kind = context.lookup_forall_binder(binder_id).kind;
                self.validate(state, context, kind, false)?;
                self.validate(state, context, inner, false)?;
            }
            Type::Row(row_id) => {
                let row = context.lookup_row_type(row_id);
                for field in row.fields.iter() {
                    self.validate(state, context, field.id, false)?;
                }
                if let Some(tail) = row.tail {
                    self.validate(state, context, tail, false)?;
                }
            }
            Type::Constructor(..)
            | Type::Synonym(..)
            | Type::Rigid(..)
            | Type::Unification(_)
            | Type::Free(_)
            | Type::Unknown(_)
            | Type::Integer(_)
            | Type::String(..) => (),
        }

        if expanded != id {
            self.validate(state, context, expanded, allow_bundle)?;
        }

        Ok(())
    }
}

impl TypeFold for Validation {
    fn transform<Q>(
        &mut self,
        _state: &mut CheckState,
        context: &CheckContext<Q>,
        id: TypeId,
        _t: &Type,
    ) -> QueryResult<FoldAction>
    where
        Q: ExternalQueries,
    {
        if self.invalid.contains(&id) {
            Ok(FoldAction::Replace(context.unknown("invalid capability bundle position")))
        } else {
            Ok(FoldAction::Continue)
        }
    }
}
