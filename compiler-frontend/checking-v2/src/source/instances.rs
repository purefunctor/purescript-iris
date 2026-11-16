use std::mem;
use std::sync::Arc;

use building_types::QueryResult;
use indexing::{InstanceChainId, InstanceItemId};
use itertools::Itertools;
use rustc_hash::{FxHashMap, FxHashSet};
use smol_str::SmolStr;

use crate::context::{CheckContext, Item};
use crate::core::data::{self, NewtypeField};
use crate::core::entail::{self, ConstraintHead};
use crate::core::{fold, kind, skolem, subsume, unify};
use crate::error::{ErrorCrumb, ErrorKind};
use crate::source::term_items::{generalisable_unknowns, quantify_unknowns};
use crate::source::types::{self, TypeArgument};
use crate::source::{derive, equations, escape};
use crate::state::CheckState;
use crate::tree::{InstanceDeclaration, InstanceMember};
use crate::types::{ForallBinder, Name, TypeId};
use crate::{CheckedClass, CheckedInstance, ExternalQueries, InstanceOrigin};

struct InstanceHead<'a> {
    origin: InstanceOrigin,
    resolution: Option<Item>,
    arguments: &'a [lowering::TypeId],
    constraints: &'a [lowering::TypeId],
    chain: Option<(InstanceChainId, u32)>,
}

pub fn check_instance_heads<Q>(state: &mut CheckState, context: &CheckContext<Q>) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    for (instance_id, item) in context.indexed.items.iter_instances() {
        let Some(lowered) = context.lowered.tree.get_instance_item(instance_id) else {
            continue;
        };
        let head = InstanceHead {
            origin: InstanceOrigin::Instance(instance_id),
            resolution: lowered.resolution,
            arguments: &lowered.arguments,
            constraints: &lowered.constraints,
            chain: context.indexed.pairs.instance_chain_metadata(item.id),
        };
        state.with_crumb(ErrorCrumb::InstanceDeclaration(instance_id), |state| {
            check_instance_head(state, context, head)
        })?;
    }

    for (derive_id, _) in context.indexed.items.iter_derives() {
        let Some(lowered) = context.lowered.tree.get_derive_item(derive_id) else {
            continue;
        };
        let head = InstanceHead {
            origin: InstanceOrigin::Derive(derive_id),
            resolution: lowered.resolution,
            arguments: &lowered.arguments,
            constraints: &lowered.constraints,
            chain: None,
        };
        state.with_crumb(ErrorCrumb::DeriveDeclaration(derive_id), |state| {
            check_instance_head(state, context, head)
        })?;
    }

    Ok(())
}

fn check_instance_head<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    head: InstanceHead,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let Some(class) = head.resolution else {
        return Ok(());
    };

    let outer_implicit = mem::take(&mut state.implicit_binders);
    let (constraint, constraints) = state.with_level(|state| {
        let class_kind = kind::lookup_type_item_kind(state, context, class)?;
        let mut applied = (context.intern_constructor(class), class_kind);
        for &argument in head.arguments {
            applied =
                types::infer_application(state, context, applied, TypeArgument::Source(argument))?;
        }
        let (constraint, constraint_kind) = applied;
        unify::unify_kinds(state, context, context.types.constraint, constraint_kind)?;

        let constraints = head.constraints.iter().map(|&constraint| {
            types::check_type(state, context, constraint, context.types.constraint)
        });
        let constraints = constraints.collect::<QueryResult<Vec<_>>>()?;

        if matches!(head.origin, InstanceOrigin::Derive(_)) && context.known.newtype == Some(class)
        {
            improve_derived_newtype(state, context, constraint)?;
        }

        QueryResult::Ok((constraint, constraints))
    })?;
    let implicit = mem::replace(&mut state.implicit_binders, outer_implicit);

    let mut types = vec![constraint];
    types.extend(constraints.iter().copied());
    types.extend(implicit.iter().map(|&binder_id| context.lookup_forall_binder(binder_id).kind));
    let unknowns = generalisable_unknowns(state, context, &types);
    let kind_binders = quantify_unknowns(state, context, &unknowns);

    let implicit = implicit.iter().map(|&binder_id| {
        let binder = context.lookup_forall_binder(binder_id);
        let kind = fold::zonk(state, context, binder.kind);
        context.intern_forall_binder(ForallBinder { kind, ..binder })
    });
    let binders = kind_binders.into_iter().chain(implicit).collect::<Arc<[_]>>();

    let constraint = fold::zonk(state, context, constraint);
    let constraints = constraints.iter().map(|&constraint| fold::zonk(state, context, constraint));
    let constraints = constraints.collect::<Arc<[_]>>();

    let Some(constraint_head) = entail::constraint_head(state, context, constraint) else {
        return Ok(());
    };

    state.checked.instances.push(CheckedInstance {
        origin: head.origin,
        class: constraint_head.class,
        binders,
        constraints,
        kind_arguments: Arc::from(constraint_head.kind_arguments),
        arguments: Arc::from(constraint_head.arguments),
        chain: head.chain,
    });

    Ok(())
}

fn improve_derived_newtype<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    constraint: TypeId,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let Some(head) = entail::constraint_head(state, context, constraint) else {
        return Ok(());
    };
    let [wrapper, wrapped] = head.arguments.as_slice() else {
        return Ok(());
    };
    match data::newtype_field(state, context, *wrapper)? {
        Some(NewtypeField { field, .. }) => {
            unify::unify(state, context, *wrapped, field)?;
        }
        None => {
            let class = type_item_name(context, head.class)?;
            state.insert_error(ErrorKind::CannotDerive { class });
        }
    }
    Ok(())
}

pub fn check_instance_bodies<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let instances = state.checked.instances.iter().map(CheckedInstance::clone).collect_vec();
    for instance in instances {
        let crumb = match instance.origin {
            InstanceOrigin::Instance(id) => ErrorCrumb::InstanceDeclaration(id),
            InstanceOrigin::Derive(id) => ErrorCrumb::DeriveDeclaration(id),
        };
        state.with_crumb(crumb, |state| check_instance_body(state, context, &instance))?;
    }
    Ok(())
}

struct SkolemisedInstance {
    rigids: Vec<TypeId>,
    constraints: Vec<TypeId>,
    head: ConstraintHead,
}

fn skolemise_instance<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    instance: &CheckedInstance,
) -> SkolemisedInstance
where
    Q: ExternalQueries,
{
    let mut substitution = FxHashMap::<Name, TypeId>::default();
    let mut rigids = Vec::with_capacity(instance.binders.len());
    for &binder_id in instance.binders.iter() {
        let binder = context.lookup_forall_binder(binder_id);
        let kind = fold::substitute_variables(context, &substitution, binder.kind);
        let rigid = skolem::fresh_rigid(state, context, ForallBinder { kind, ..binder });
        substitution.insert(binder.name, rigid);
        rigids.push(rigid);
    }

    skolem::scope_rigids(state, context, &substitution);

    let substitute = |types: &[TypeId]| {
        let types = types.iter().map(|&id| fold::substitute_variables(context, &substitution, id));
        types.collect_vec()
    };

    SkolemisedInstance {
        rigids,
        constraints: substitute(&instance.constraints),
        head: ConstraintHead {
            class: instance.class,
            kind_arguments: substitute(&instance.kind_arguments),
            arguments: substitute(&instance.arguments),
        },
    }
}

fn check_instance_body<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    instance: &CheckedInstance,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let Some(class) = entail::lookup_class(state, context, instance.class)? else {
        return Ok(());
    };

    let declaration = state.with_level(|state| {
        let skolemised = skolemise_instance(state, context, instance);
        let evidence = subsume::bind_givens(state, &skolemised.constraints);
        let givens = subsume::given_list(&evidence);

        state.with_givens(givens, |state| {
            let substitution = entail::class_substitution(context, &class, &skolemised.head);

            let superclasses = class.superclasses.iter().map(|&superclass| {
                let superclass = fold::substitute_variables(context, &substitution, superclass);
                state.emit_wanted(superclass)
            });
            let superclasses = superclasses.collect::<Arc<[_]>>();

            let (members, obligations) = match instance.origin {
                InstanceOrigin::Instance(id) => {
                    let members = check_members(state, context, id, &class, &substitution)?;
                    (members, Vec::new())
                }
                InstanceOrigin::Derive(id) => {
                    let obligations =
                        derive::derive_obligations(state, context, id, &skolemised.head)?;
                    (Vec::new(), obligations)
                }
            };

            entail::solve_wanteds(state, context)?;
            entail::report_unsolved(state, context);

            for member in &members {
                escape::check_value(state, context, &skolemised.rigids, &member.value);
            }

            QueryResult::Ok(InstanceDeclaration {
                evidence: Arc::from(evidence),
                superclasses,
                members: Arc::from(members),
                obligations: Arc::from(obligations),
            })
        })
    })?;

    state.checked.tree.instances.insert(instance.origin, declaration);
    Ok(())
}

fn check_members<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    instance_id: InstanceItemId,
    class: &CheckedClass,
    substitution: &FxHashMap<Name, TypeId>,
) -> QueryResult<Vec<InstanceMember>>
where
    Q: ExternalQueries,
{
    let Some(lowered) = context.lowered.tree.get_instance_item(instance_id) else {
        return Ok(Vec::new());
    };

    let mut defined = FxHashSet::default();
    let mut members = Vec::with_capacity(lowered.members.len());
    for group in lowered.members.iter() {
        let Some((file_id, item)) = group.resolution else {
            continue;
        };
        let member_type = class.members.iter().find(|&&(member, _)| member == item);
        let Some(&(_, member_type)) = member_type else {
            continue;
        };
        defined.insert(item);

        let expected = fold::substitute_variables(context, substitution, member_type);
        let signature = match group.signature {
            Some(signature) => {
                let signature = types::check_type(state, context, signature, context.types.type_)?;
                subsume::subsume(state, context, subsume::Mode::NoElaborate, signature, expected)?;
                signature
            }
            None => expected,
        };

        let value =
            equations::check_value(state, context, Some(signature), signature, &group.equations)?;
        members.push(InstanceMember { item: (file_id, item), value });
    }

    let Some((class_file, _)) = lowered.resolution else {
        return Ok(members);
    };
    for &(member, _) in class.members.iter() {
        if defined.contains(&member) {
            continue;
        }
        let indexed = context.queries.indexed(class_file)?;
        let name = indexed.items[member].name.as_ref().map(SmolStr::clone);
        let name = name.unwrap_or_else(|| SmolStr::new_static("<anonymous>"));
        state.insert_error(ErrorKind::MissingClassMember { name });
    }

    Ok(members)
}

pub fn type_item_name<Q>(
    context: &CheckContext<Q>,
    (file_id, type_id): Item,
) -> QueryResult<SmolStr>
where
    Q: ExternalQueries,
{
    let indexed = context.queries.indexed(file_id)?;
    let name = indexed.items[type_id].name.as_ref().map(SmolStr::clone);
    Ok(name.unwrap_or_else(|| SmolStr::new_static("<anonymous>")))
}
