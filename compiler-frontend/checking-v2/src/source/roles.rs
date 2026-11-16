use std::sync::Arc;

use building_types::QueryResult;
use indexing::TypeItemId;
use itertools::Itertools;
use lowering::TypeItemKind;
use rustc_hash::FxHashMap;

use crate::ExternalQueries;
use crate::context::{CheckContext, Item};
use crate::core::normalise;
use crate::error::{ErrorCrumb, ErrorKind};
use crate::state::CheckState;
use crate::types::{Name, Role, Type, TypeId};

fn compose(context_role: Role, role: Role) -> Role {
    if context_role == Role::Phantom || role == Role::Phantom {
        Role::Phantom
    } else {
        context_role.max(role)
    }
}

pub fn lookup_roles<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    (file_id, type_id): Item,
) -> QueryResult<Option<Arc<[Role]>>>
where
    Q: ExternalQueries,
{
    if file_id == context.id {
        return Ok(state.checked.roles.get(&type_id).map(Arc::clone));
    }
    let checked = context.checked_dependency(file_id)?;
    Ok(checked.roles.get(&type_id).map(Arc::clone))
}

struct RoleWalk<'a> {
    parameters: &'a FxHashMap<Name, usize>,
    group: &'a FxHashMap<TypeItemId, Vec<Role>>,
    roles: Vec<Role>,
}

fn walk<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    walker: &mut RoleWalk<'_>,
    id: TypeId,
    context_role: Role,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    if context_role == Role::Phantom {
        return Ok(());
    }

    let id = normalise::normalise(state, context, id);
    match context.lookup_type(id) {
        Type::Variable(binder_id) => {
            let binder = context.lookup_forall_binder(binder_id);
            if let Some(&index) = walker.parameters.get(&binder.name)
                && let Some(role) = walker.roles.get_mut(index)
            {
                *role = (*role).max(context_role);
            }
            Ok(())
        }

        Type::Function(argument, result) => {
            walk(state, context, walker, argument, context_role)?;
            walk(state, context, walker, result, context_role)
        }

        Type::Application(..) => {
            let (head, arguments) = normalise::application_spine(state, context, id);
            let head_roles = match context.lookup_type(head) {
                Type::Constructor(file_id, type_id) => {
                    if file_id == context.id
                        && let Some(roles) = walker.group.get(&type_id)
                    {
                        Some(roles.iter().copied().collect::<Arc<[_]>>())
                    } else {
                        lookup_roles(state, context, (file_id, type_id))?
                    }
                }
                _ => {
                    walk(state, context, walker, head, context_role)?;
                    None
                }
            };
            for (index, &argument) in arguments.iter().enumerate() {
                let role = head_roles.as_ref().and_then(|roles| roles.get(index).copied());
                let role = role.unwrap_or(Role::Nominal);
                walk(state, context, walker, argument, compose(context_role, role))?;
            }
            Ok(())
        }

        Type::KindApplication(function, _) => walk(state, context, walker, function, context_role),

        Type::Forall(_, inner) => walk(state, context, walker, inner, context_role),

        Type::Constrained(constraint, constrained) => {
            walk(state, context, walker, constraint, compose(context_role, Role::Nominal))?;
            walk(state, context, walker, constrained, context_role)
        }

        Type::Row(row_id) => {
            let row = context.lookup_row_type(row_id);
            for field in row.fields.iter() {
                walk(state, context, walker, field.id, context_role)?;
            }
            if let Some(tail) = row.tail {
                walk(state, context, walker, tail, context_role)?;
            }
            Ok(())
        }

        Type::Kinded(..)
        | Type::Synonym(_)
        | Type::Constructor(..)
        | Type::Integer(_)
        | Type::String(_)
        | Type::Rigid(_)
        | Type::Unification(_)
        | Type::Unknown => Ok(()),
    }
}

fn declared_roles<Q>(context: &CheckContext<Q>, item: TypeItemId) -> Option<Arc<[lowering::Role]>>
where
    Q: ExternalQueries,
{
    match context.lowered.tree.get_type_item_kind(item)? {
        TypeItemKind::Data { roles, .. }
        | TypeItemKind::Newtype { roles, .. }
        | TypeItemKind::Foreign { roles, .. } => {
            if roles.is_empty() {
                None
            } else {
                Some(Arc::clone(roles))
            }
        }
        _ => None,
    }
}

fn foreign_arity<Q>(state: &CheckState, context: &CheckContext<Q>, kind: TypeId) -> usize
where
    Q: ExternalQueries,
{
    let mut arity = 0;
    let mut current = kind;
    loop {
        let head = normalise::normalise(state, context, current);
        match context.lookup_type(head) {
            Type::Forall(_, inner) => current = inner,
            _ => match normalise::function_parts(state, context, head) {
                Some((_, result)) => {
                    arity += 1;
                    current = result;
                }
                None => break arity,
            },
        }
    }
}

pub fn infer_group_roles<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    items: &[TypeItemId],
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let data_items = items.iter().filter_map(|&item| {
        let data = state.checked.data.get(&item)?;
        Some((item, crate::CheckedData::clone(data)))
    });
    let data_items = data_items.collect_vec();

    let initial =
        data_items.iter().map(|(item, data)| (*item, vec![Role::Phantom; data.parameters.len()]));
    let mut group = initial.collect::<FxHashMap<_, _>>();

    loop {
        let mut changed = false;
        for (item, data) in &data_items {
            let parameters =
                data.parameters.iter().enumerate().map(|(index, &binder_id)| {
                    (context.lookup_forall_binder(binder_id).name, index)
                });
            let parameters = parameters.collect::<FxHashMap<_, _>>();

            let current = group.get(item).expect("invariant violated: data item without roles");
            let roles = Vec::clone(current);
            let mut walker = RoleWalk { parameters: &parameters, group: &group, roles };
            for constructor in data.constructors.iter() {
                for &field in constructor.fields.iter() {
                    walk(state, context, &mut walker, field, Role::Representational)?;
                }
            }

            let roles = walker.roles;
            if group.get(item) != Some(&roles) {
                group.insert(*item, roles);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    for &item in items {
        let is_foreign = matches!(
            context.lowered.tree.get_type_item_kind(item),
            Some(TypeItemKind::Foreign { .. })
        );
        let inferred = match group.remove(&item) {
            Some(inferred) => inferred,
            None => {
                if !is_foreign {
                    continue;
                }
                let kind = state.checked.type_kinds.get(&item).copied();
                let kind = kind.unwrap_or(context.types.unknown);
                vec![Role::Nominal; foreign_arity(state, context, kind)]
            }
        };

        let roles = match declared_roles(context, item) {
            None => inferred,
            Some(declared) => state.with_crumb(ErrorCrumb::TypeDeclaration(item), |state| {
                check_declared_roles(state, &declared, inferred, is_foreign)
            }),
        };
        state.checked.roles.insert(item, Arc::from(roles));
    }

    Ok(())
}

fn check_declared_roles(
    state: &mut CheckState,
    declared: &[lowering::Role],
    inferred: Vec<Role>,
    is_foreign: bool,
) -> Vec<Role> {
    if declared.len() != inferred.len() {
        let expected = inferred.len();
        let actual = declared.len();
        state.insert_error(ErrorKind::RoleDeclarationArityMismatch { expected, actual });
        return inferred;
    }

    let roles = declared.iter().zip(inferred).map(|(declared, inferred)| {
        let declared = match declared {
            lowering::Role::Nominal => Role::Nominal,
            lowering::Role::Representational => Role::Representational,
            lowering::Role::Phantom => Role::Phantom,
            lowering::Role::Unknown => return inferred,
        };
        if !is_foreign && declared < inferred {
            state.insert_error(ErrorKind::RoleMismatch { declared, inferred });
            inferred
        } else {
            declared
        }
    });
    roles.collect_vec()
}
