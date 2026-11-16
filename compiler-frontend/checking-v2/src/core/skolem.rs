use building_types::QueryResult;
use rustc_hash::FxHashMap;

use crate::ExternalQueries;
use crate::context::CheckContext;
use crate::core::{fold, kind, normalise};
use crate::state::CheckState;
use crate::types::{ForallBinder, Name, Rigid, Type, TypeId};

pub fn fresh_rigid<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    binder: ForallBinder,
) -> TypeId
where
    Q: ExternalQueries,
{
    let name = state.fresh_name();
    let rigid = Rigid { name, text: binder.text, level: state.level, kind: binder.kind };
    let rigid_id = context.intern_rigid(rigid);
    context.intern_type(Type::Rigid(rigid_id))
}

pub fn replace_binder<Q>(
    context: &CheckContext<Q>,
    binder: ForallBinder,
    replacement: TypeId,
    inner: TypeId,
) -> TypeId
where
    Q: ExternalQueries,
{
    fold::substitute_variable(context, binder.name, replacement, inner)
}

pub fn scope_rigids<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    substitution: &FxHashMap<Name, TypeId>,
) where
    Q: ExternalQueries,
{
    if substitution.is_empty() {
        return;
    }
    for variable in state.type_variables.values_mut() {
        let Type::Variable(binder_id) = context.lookup_type(*variable) else {
            continue;
        };
        let name = context.lookup_forall_binder(binder_id).name;
        if let Some(&rigid) = substitution.get(&name) {
            *variable = rigid;
        }
    }
}

#[derive(Debug, Clone)]
pub struct Skolemised {
    pub rigids: Vec<(ForallBinder, TypeId)>,
    pub constraints: Vec<TypeId>,
    pub inner: TypeId,
}

pub fn skolemise<Q>(state: &mut CheckState, context: &CheckContext<Q>, id: TypeId) -> Skolemised
where
    Q: ExternalQueries,
{
    let mut rigids = Vec::new();
    let mut constraints = Vec::new();
    let mut substitution = FxHashMap::default();
    let mut current = id;

    loop {
        let head = normalise::normalise(state, context, current);
        match context.lookup_type(head) {
            Type::Forall(binder_id, inner) => {
                let binder = context.lookup_forall_binder(binder_id);
                let kind = fold::substitute_variables(context, &substitution, binder.kind);
                let binder = ForallBinder { kind, ..binder };
                let rigid = fresh_rigid(state, context, binder);
                substitution.insert(binder.name, rigid);
                rigids.push((binder, rigid));
                current = inner;
            }
            Type::Constrained(constraint, constrained) => {
                constraints.push(fold::substitute_variables(context, &substitution, constraint));
                current = constrained;
            }
            _ => break,
        }
    }

    let inner = fold::substitute_variables(context, &substitution, current);
    Skolemised { rigids, constraints, inner }
}

#[derive(Debug, Clone)]
pub struct Instantiated {
    pub arguments: Vec<TypeId>,
    pub constraints: Vec<TypeId>,
    pub inner: TypeId,
}

pub fn instantiate<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: TypeId,
) -> QueryResult<Instantiated>
where
    Q: ExternalQueries,
{
    let mut arguments = Vec::new();
    let mut constraints = Vec::new();
    let mut substitution = FxHashMap::default();
    let mut current = id;

    loop {
        let head = normalise::normalise(state, context, current);
        match context.lookup_type(head) {
            Type::Forall(binder_id, inner) => {
                let binder = context.lookup_forall_binder(binder_id);
                let kind = fold::substitute_variables(context, &substitution, binder.kind);
                let argument = kind::fresh_unification(state, context, kind);
                substitution.insert(binder.name, argument);
                arguments.push(argument);
                current = inner;
            }
            Type::Constrained(constraint, constrained) => {
                constraints.push(fold::substitute_variables(context, &substitution, constraint));
                current = constrained;
            }
            _ => break,
        }
    }

    let inner = fold::substitute_variables(context, &substitution, current);
    Ok(Instantiated { arguments, constraints, inner })
}
