use std::sync::Arc;

use itertools::Itertools;
use rustc_hash::FxHashMap;

use crate::ExternalQueries;
use crate::context::CheckContext;
use crate::state::CheckState;
use crate::types::{
    ForallBinder, ForallBinderId, Name, Rigid, RowField, RowType, SynonymApplication, Type, TypeId,
};

pub fn map_children<Q>(
    context: &CheckContext<Q>,
    id: TypeId,
    mut transform: impl FnMut(TypeId) -> TypeId,
) -> TypeId
where
    Q: ExternalQueries,
{
    let transform_binder =
        |binder_id: ForallBinderId, transform: &mut dyn FnMut(TypeId) -> TypeId| {
            let binder = context.lookup_forall_binder(binder_id);
            let kind = transform(binder.kind);
            if kind == binder.kind {
                return binder_id;
            }
            context.intern_forall_binder(ForallBinder { kind, ..binder })
        };

    let mapped = match context.lookup_type(id) {
        Type::Application(function, argument) => {
            Type::Application(transform(function), transform(argument))
        }
        Type::KindApplication(function, argument) => {
            Type::KindApplication(transform(function), transform(argument))
        }
        Type::Function(argument, result) => Type::Function(transform(argument), transform(result)),
        Type::Forall(binder_id, inner) => {
            let binder_id = transform_binder(binder_id, &mut transform);
            Type::Forall(binder_id, transform(inner))
        }
        Type::Constrained(constraint, constrained) => {
            Type::Constrained(transform(constraint), transform(constrained))
        }
        Type::Kinded(inner, kind) => Type::Kinded(transform(inner), transform(kind)),
        Type::Synonym(application_id) => {
            let application = context.lookup_synonym_application(application_id);
            let arguments = application.arguments.iter().map(|&argument| transform(argument));
            let arguments = arguments.collect::<Arc<[_]>>();
            let expansion = transform(application.expansion);
            let application = SynonymApplication { arguments, expansion, ..application };
            Type::Synonym(context.intern_synonym_application(application))
        }
        Type::Row(row_id) => {
            let row = context.lookup_row_type(row_id);
            let fields = row.fields.iter().map(|field| {
                let id = transform(field.id);
                RowField { label: smol_str::SmolStr::clone(&field.label), id }
            });
            let fields = fields.collect::<Arc<[_]>>();
            let tail = row.tail.map(&mut transform);
            let row_id = context.intern_row_type(RowType { fields, tail });
            Type::Row(row_id)
        }
        Type::Variable(binder_id) => Type::Variable(transform_binder(binder_id, &mut transform)),
        Type::Rigid(rigid_id) => {
            let rigid = context.lookup_rigid(rigid_id);
            let kind = transform(rigid.kind);
            if kind == rigid.kind {
                return id;
            }
            Type::Rigid(context.intern_rigid(Rigid { kind, ..rigid }))
        }
        Type::Constructor(..)
        | Type::Integer(_)
        | Type::String(_)
        | Type::Unification(_)
        | Type::Unknown => return id,
    };

    context.intern_type(mapped)
}

pub fn zonk<Q>(state: &CheckState, context: &CheckContext<Q>, id: TypeId) -> TypeId
where
    Q: ExternalQueries,
{
    if !context.lookup_type_flags(id).may_zonk() {
        return id;
    }

    match context.lookup_type(id) {
        Type::Unification(unification_id) => match state.unification(unification_id).solution {
            Some(solution) => zonk(state, context, solution),
            None => id,
        },
        Type::Row(row_id) => {
            let row = context.lookup_row_type(row_id);
            let row = crate::core::normalise::flatten_row(state, context, row);
            let fields = row.fields.iter().map(|field| {
                let id = zonk(state, context, field.id);
                RowField { label: smol_str::SmolStr::clone(&field.label), id }
            });
            let fields = fields.collect_vec();
            let tail = row.tail.map(|tail| zonk(state, context, tail));
            if fields.is_empty()
                && let Some(tail) = tail
            {
                return tail;
            }
            context.intern_row(fields, tail)
        }
        _ => map_children(context, id, |child| zonk(state, context, child)),
    }
}

pub fn zonk_exported<Q>(state: &CheckState, context: &CheckContext<Q>, id: TypeId) -> TypeId
where
    Q: ExternalQueries,
{
    let id = zonk(state, context, id);
    if !context.lookup_type_flags(id).has_unification() {
        return id;
    }
    if let Type::Unification(_) = context.lookup_type(id) {
        return context.types.unknown;
    }
    map_children(context, id, |child| zonk_exported(state, context, child))
}

pub fn zonk_exported_binder<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    binder_id: ForallBinderId,
) -> ForallBinderId
where
    Q: ExternalQueries,
{
    let binder = context.lookup_forall_binder(binder_id);
    let kind = zonk_exported(state, context, binder.kind);
    if kind == binder.kind {
        return binder_id;
    }
    context.intern_forall_binder(ForallBinder { kind, ..binder })
}

pub fn substitute_variables<Q>(
    context: &CheckContext<Q>,
    substitution: &FxHashMap<Name, TypeId>,
    id: TypeId,
) -> TypeId
where
    Q: ExternalQueries,
{
    if substitution.is_empty() || !context.lookup_type_flags(id).has_variable() {
        return id;
    }

    if let Type::Variable(binder_id) = context.lookup_type(id) {
        let binder = context.lookup_forall_binder(binder_id);
        if let Some(&replacement) = substitution.get(&binder.name) {
            return replacement;
        }
    }

    map_children(context, id, |child| substitute_variables(context, substitution, child))
}

pub fn substitute_variable<Q>(
    context: &CheckContext<Q>,
    name: Name,
    replacement: TypeId,
    id: TypeId,
) -> TypeId
where
    Q: ExternalQueries,
{
    let substitution = FxHashMap::from_iter([(name, replacement)]);
    substitute_variables(context, &substitution, id)
}

pub fn contains_unknown<Q>(state: &CheckState, context: &CheckContext<Q>, id: TypeId) -> bool
where
    Q: ExternalQueries,
{
    let id = zonk(state, context, id);
    context.lookup_type_flags(id).has_unknown()
}
