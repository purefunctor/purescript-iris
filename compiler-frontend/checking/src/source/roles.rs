//! Implements role checking for source type declarations.

use std::sync::Arc;

use building_types::QueryResult;
use indexing::{TermItemId, TypeItemId};
use rustc_hash::FxHashMap;

use crate::ExternalQueries;
use crate::context::CheckContext;
use crate::core::{ForallBinder, Name, Role, Type, TypeId, normalise, signature};
use crate::error::{ErrorCrumb, ErrorKind};
use crate::state::CheckState;

pub fn infer_data_roles<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    parameters: &[ForallBinder],
    constructors: &[(TermItemId, Vec<TypeId>)],
) -> QueryResult<Vec<Role>>
where
    Q: ExternalQueries,
{
    let mut inference = RoleInference::new(state, context, parameters);

    for (_, arguments) in constructors {
        for &argument in arguments {
            infer_roles(&mut inference, argument, RoleInferencePosition::ROOT)?;
        }
    }

    Ok(inference.finish())
}

pub fn count_kind_arguments<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    kind: TypeId,
) -> QueryResult<usize>
where
    Q: ExternalQueries,
{
    let signature = signature::decompose_signature(
        state,
        context,
        kind,
        signature::DecomposeSignatureMode::Full,
    )?;
    Ok(signature.arguments().count())
}

pub fn check_declared_roles(
    state: &mut CheckState,
    item_id: TypeItemId,
    inferred: &[Role],
    declared: &[lowering::Role],
    is_foreign: bool,
) -> Arc<[Role]> {
    let mut validated = inferred.to_vec();

    for (index, (&inferred_role, declared_role)) in inferred.iter().zip(declared.iter()).enumerate()
    {
        let Some(declared_role) = lower_role(declared_role) else {
            continue;
        };

        if is_foreign || declared_role >= inferred_role {
            validated[index] = declared_role;
        } else {
            state.with_error_crumb(ErrorCrumb::TypeDeclaration(item_id), |state| {
                state.insert_error(ErrorKind::InvalidRoleDeclaration {
                    index,
                    declared: declared_role,
                    inferred: inferred_role,
                });
            });
        }
    }

    Arc::from(validated)
}

fn lower_role(role: &lowering::Role) -> Option<Role> {
    match role {
        lowering::Role::Nominal => Some(Role::Nominal),
        lowering::Role::Representational => Some(Role::Representational),
        lowering::Role::Phantom => Some(Role::Phantom),
        lowering::Role::Unknown => None,
    }
}

#[derive(Copy, Clone, Debug)]
struct RoleInferencePosition {
    under_constraint: bool,
    variable_argument: bool,
}

impl RoleInferencePosition {
    const ROOT: RoleInferencePosition =
        RoleInferencePosition { under_constraint: false, variable_argument: false };

    fn child(self) -> RoleInferencePosition {
        RoleInferencePosition { under_constraint: self.under_constraint, variable_argument: false }
    }

    fn constrained_child(self) -> RoleInferencePosition {
        RoleInferencePosition { under_constraint: true, variable_argument: false }
    }

    fn argument_child(self, is_variable_argument: bool) -> RoleInferencePosition {
        RoleInferencePosition {
            under_constraint: self.under_constraint,
            variable_argument: is_variable_argument,
        }
    }

    fn requires_nominal_role(self) -> bool {
        self.under_constraint || self.variable_argument
    }
}

struct RoleInference<'a, 'q, Q>
where
    Q: ExternalQueries,
{
    state: &'a mut CheckState,
    context: &'a CheckContext<'q, Q>,
    roles: Vec<Role>,
    parameters: FxHashMap<Name, usize>,
}

impl<'a, 'q, Q> RoleInference<'a, 'q, Q>
where
    Q: ExternalQueries,
{
    fn new(
        state: &'a mut CheckState,
        context: &'a CheckContext<'q, Q>,
        parameters: &[ForallBinder],
    ) -> RoleInference<'a, 'q, Q> {
        let parameter_count = parameters.len();
        let parameters: FxHashMap<Name, usize> = parameters
            .iter()
            .enumerate()
            .map(|(index, parameter)| (parameter.name, index))
            .collect();

        RoleInference { state, context, roles: vec![Role::Phantom; parameter_count], parameters }
    }

    fn finish(self) -> Vec<Role> {
        self.roles
    }
}

fn infer_roles<'a, 'q, Q>(
    inference: &mut RoleInference<'a, 'q, Q>,
    type_id: TypeId,
    mode: RoleInferencePosition,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let type_id = normalise::expand(inference.state, inference.context, type_id)?;

    match *inference.context.lookup_type(type_id) {
        Type::Rigid(name, _, kind) => {
            if let Some(index) = inference.parameters.get(&name) {
                let role = if mode.requires_nominal_role() {
                    Role::Nominal
                } else {
                    Role::Representational
                };
                inference.roles[*index] = inference.roles[*index].max(role);
            }

            infer_roles(inference, kind, mode.child())?;
        }

        Type::Application(function, argument) => {
            let function_id = normalise::expand(inference.state, inference.context, function)?;

            let is_type_variable =
                matches!(*inference.context.lookup_type(function_id), Type::Rigid(_, _, _));

            infer_roles(inference, function, mode.child())?;
            infer_roles(inference, argument, mode.argument_child(is_type_variable))?;
        }

        Type::Constrained(constraint, inner) => {
            let mode = mode.constrained_child();
            infer_roles(inference, constraint, mode)?;
            infer_roles(inference, inner, mode)?;
        }

        Type::Forall(binder_id, inner) => {
            let binder = inference.context.lookup_forall_binder(binder_id);

            infer_roles(inference, binder.kind, mode.child())?;
            infer_roles(inference, inner, mode.child())?;
        }

        Type::Function(argument, result) => {
            infer_roles(inference, argument, mode.child())?;
            infer_roles(inference, result, mode.child())?;
        }

        Type::KindApplication(function, argument) => {
            infer_roles(inference, function, mode.child())?;
            infer_roles(inference, argument, mode.child())?;
        }

        Type::Kinded(inner, kind) => {
            infer_roles(inference, inner, mode.child())?;
            infer_roles(inference, kind, mode.child())?;
        }

        Type::Row(row_id) => {
            let row = inference.context.lookup_row_type(row_id);

            for field in row.fields.iter() {
                infer_roles(inference, field.id, mode.child())?;
            }

            if let Some(tail) = row.tail {
                infer_roles(inference, tail, mode.child())?;
            }
        }

        Type::Constructor(_, _)
        | Type::Integer(_)
        | Type::String(_, _)
        | Type::Unification(_)
        | Type::Free(_)
        | Type::Unknown(_) => {}
    }

    Ok(())
}
