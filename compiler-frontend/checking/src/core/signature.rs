use std::rc::Rc;

use building_types::QueryResult;
use lowering::TypeVariableBinding;

use crate::context::CheckContext;
use crate::core::substitute::RigidRenaming;
use crate::core::{ForallBinderId, Type, TypeId, normalise, toolkit, unification};
use crate::error::ErrorKind;
use crate::state::CheckState;
use crate::{ExternalQueries, safe_loop};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecomposedAbstraction {
    Type { binder: ForallBinderId },
    Constraint { constraint: TypeId },
    Argument { argument: TypeId },
}

pub struct DecomposedSignature {
    /// The type spine in source order, including explicit function arguments.
    pub abstractions: Vec<DecomposedAbstraction>,
    pub result: TypeId,
}

impl DecomposedSignature {
    pub fn arguments(&self) -> impl Iterator<Item = TypeId> {
        self.abstractions.iter().filter_map(|abstraction| match abstraction {
            DecomposedAbstraction::Argument { argument } => Some(*argument),
            _ => None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkolemisedAbstraction {
    Type { binder: ForallBinderId, rigid: TypeId },
    Constraint { constraint: TypeId },
    Argument { argument: TypeId },
}

pub struct SkolemisedSignature {
    pub renaming: Rc<RigidRenaming>,
    pub abstractions: Vec<SkolemisedAbstraction>,
    pub result: TypeId,
}

impl SkolemisedSignature {
    pub fn arguments(&self) -> impl Iterator<Item = TypeId> {
        self.abstractions.iter().filter_map(|abstraction| match abstraction {
            SkolemisedAbstraction::Argument { argument } => Some(*argument),
            _ => None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecomposeSignatureMode {
    Full,
    Patterns { required: usize },
}

pub fn decompose_signature<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    mut current: TypeId,
    mode: DecomposeSignatureMode,
) -> QueryResult<DecomposedSignature>
where
    Q: ExternalQueries,
{
    let mut abstractions = vec![];
    let mut argument_count = 0;

    safe_loop! {
        current = normalise::expand(state, context, current)?;

        match *context.lookup_type(current) {
            Type::Forall(binder_id, inner) => {
                abstractions.push(DecomposedAbstraction::Type { binder: binder_id });
                current = inner;
            }

            Type::Constrained(constraint, constrained) => {
                abstractions.push(DecomposedAbstraction::Constraint { constraint });
                current = constrained;
            }

            Type::Function(argument, result) => {
                if let DecomposeSignatureMode::Patterns { required } = mode
                    && argument_count >= required
                {
                    return Ok(DecomposedSignature { abstractions, result: current });
                }

                abstractions.push(DecomposedAbstraction::Argument { argument });
                argument_count += 1;
                current = result;
            }

            Type::Application(function_argument, result) => {
                if let DecomposeSignatureMode::Patterns { required } = mode
                    && argument_count >= required
                {
                    return Ok(DecomposedSignature { abstractions, result: current });
                }

                let function_argument =
                    normalise::expand(state, context, function_argument)?;

                let Type::Application(function, argument) = *context.lookup_type(function_argument)
                else {
                    return Ok(DecomposedSignature { abstractions, result: current });
                };

                let function = normalise::expand(state, context, function)?;
                if function == context.prim.function {
                    abstractions.push(DecomposedAbstraction::Argument { argument });
                    argument_count += 1;
                    current = result;
                } else {
                    return Ok(DecomposedSignature { abstractions, result: current });
                }
            }

            _ => return Ok(DecomposedSignature { abstractions, result: current }),
        }
    }
}

pub fn expect_type_signature<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    (signature_id, signature_type): (lowering::TypeId, TypeId),
    bindings: &[TypeVariableBinding],
) -> QueryResult<DecomposedSignature>
where
    Q: ExternalQueries,
{
    let signature =
        decompose_signature(state, context, signature_type, DecomposeSignatureMode::Full)?;

    let actual = bindings.len() as u32;
    let expected = signature.arguments().count() as u32;

    if actual > expected {
        state.insert_error(ErrorKind::TypeSignatureVariableMismatch {
            id: signature_id,
            expected,
            actual,
        });
    }

    let mut argument_count = expected as usize;
    let mut result = signature.result;
    let mut abstractions = vec![];
    for abstraction in signature.abstractions.into_iter().rev() {
        if let DecomposedAbstraction::Argument { argument } = abstraction {
            argument_count -= 1;
            if argument_count >= bindings.len() {
                result = context.intern_function(argument, result);
                continue;
            }
        }
        abstractions.push(abstraction);
    }
    abstractions.reverse();
    Ok(DecomposedSignature { abstractions, result })
}

pub fn expect_term_signature<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    signature_type: TypeId,
    required: usize,
) -> QueryResult<SkolemisedSignature>
where
    Q: ExternalQueries,
{
    let signature =
        decompose_signature(state, context, signature_type, DecomposeSignatureMode::Full)?;
    let signature = skolemise_decomposed_signature(state, context, signature)?;
    let mut argument_count = signature.arguments().count();
    let mut result = signature.result;
    let mut abstractions = vec![];

    // Skolemise the whole spine to preserve hidden forall scopes, but leave
    // evidence beyond unapplied arguments in the result. With `f = g`, the
    // signature `A -> C => B` requires a body of type `A -> C => B`, not `A -> B`.
    for abstraction in signature.abstractions.into_iter().rev() {
        match abstraction {
            SkolemisedAbstraction::Argument { argument } => {
                argument_count -= 1;
                if argument_count >= required {
                    result = context.intern_function(argument, result);
                    continue;
                }
            }
            SkolemisedAbstraction::Constraint { constraint } if argument_count > required => {
                result = context.intern_constrained(constraint, result);
                continue;
            }
            _ => {}
        }
        abstractions.push(abstraction);
    }
    abstractions.reverse();

    let mut signature = SkolemisedSignature { renaming: signature.renaming, abstractions, result };
    synthesise_functions(state, context, &mut signature, required)?;

    Ok(signature)
}

fn synthesise_functions<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    signature: &mut SkolemisedSignature,
    required: usize,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let mut argument_count = signature.arguments().count();
    while argument_count < required {
        let current = normalise::expand(state, context, signature.result)?;

        let Type::Unification(unification_id) = *context.lookup_type(current) else {
            break;
        };

        let argument = state.fresh_unification(context.queries, context.prim.t);
        let result = state.fresh_unification(context.queries, context.prim.t);
        let function = context.intern_function(argument, result);

        if !unification::solve(state, context, current, unification_id, function)? {
            break;
        }

        signature.abstractions.push(SkolemisedAbstraction::Argument { argument });
        signature.result = result;
        argument_count += 1;
    }

    Ok(())
}

fn skolemise_decomposed_signature<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    signature: DecomposedSignature,
) -> QueryResult<SkolemisedSignature>
where
    Q: ExternalQueries,
{
    let mut renaming = RigidRenaming::default();
    let mut abstractions = Vec::with_capacity(signature.abstractions.len());

    for abstraction in signature.abstractions {
        match abstraction {
            DecomposedAbstraction::Type { binder } => {
                let forall_binder = context.lookup_forall_binder(binder);
                let kind = renaming.substitute(state, context, forall_binder.kind)?;
                let text = toolkit::lookup_name(state, context, forall_binder.name)?;
                let rigid = state.fresh_rigid_named(context.queries, kind, text);
                renaming.insert(context, forall_binder.name, rigid);
                abstractions.push(SkolemisedAbstraction::Type { binder, rigid });
            }
            DecomposedAbstraction::Constraint { constraint } => {
                let constraint = renaming.substitute(state, context, constraint)?;
                abstractions.push(SkolemisedAbstraction::Constraint { constraint });
            }
            DecomposedAbstraction::Argument { argument } => {
                let argument = renaming.substitute(state, context, argument)?;
                abstractions.push(SkolemisedAbstraction::Argument { argument });
            }
        }
    }

    let result = renaming.substitute(state, context, signature.result)?;
    let renaming = Rc::new(renaming);

    Ok(SkolemisedSignature { renaming, abstractions, result })
}
