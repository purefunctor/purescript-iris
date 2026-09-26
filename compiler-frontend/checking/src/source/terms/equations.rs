//! Implements checking and inference rules for value groups.
//!
//! See [`check_value_equations`] and [`infer_value_equations`].

use std::sync::Arc;

use building_types::QueryResult;
use itertools::Itertools;

use crate::context::CheckContext;
use crate::core::{TypeId, signature, toolkit, unification};
use crate::error::ErrorKind;
use crate::source::binder;
use crate::source::terms::guarded;
use crate::state::CheckState;
use crate::{ExternalQueries, tree};

/// The syntactic origin of an equation's expected type.
pub enum EquationTypeOrigin {
    /// There is a syntactic origin.
    Explicit(lowering::TypeId),
    /// This is no syntactic origin.
    Implicit,
}

struct ValueEquationSignature {
    signature: TypeId,
    arguments: Vec<TypeId>,
    result: TypeId,
}

/// See documentation for [`check_value_equations`].
pub type ValueEquationPatterns = Vec<TypeId>;

pub struct CheckedValueEquations {
    pub patterns: ValueEquationPatterns,
    pub abstractions: Vec<tree::DeclarationAbstraction>,
    pub equations: Vec<ElaboratedEquation>,
}

pub struct ElaboratedEquation {
    pub source: Option<indexing::EquationSourceId>,
    pub binders: Arc<[tree::BinderId]>,
    pub guarded: tree::GuardedExpression,
}

impl ElaboratedEquation {
    pub fn into_tree(self) -> Option<tree::Equation> {
        let source = self.source?;
        Some(tree::Equation::item(source, self.binders, self.guarded))
    }

    pub fn into_local_tree(self, source: lowering::LetBindingEquationId) -> tree::Equation {
        assert!(self.source.is_none(), "invariant violated: local equation has an item source");
        tree::Equation::local(source, self.binders, self.guarded)
    }
}

/// Checks a group of [`lowering::Equation`].
///
/// This function returns the instantiated types of the equation's
/// arguments for use in exhaustiveness checking by the callers.
pub fn check_value_equations<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    origin: EquationTypeOrigin,
    expected_type: TypeId,
    equations: &[lowering::Equation],
) -> QueryResult<CheckedValueEquations>
where
    Q: ExternalQueries,
{
    let required = equations.iter().map(|equation| equation.binders.len()).max().unwrap_or(0);

    let signature = signature::expect_term_signature(state, context, expected_type, required)?;
    let arguments = signature.arguments().collect_vec();
    let signature::SkolemisedSignature { renaming, abstractions, result } = signature;
    let abstractions = bind_signature_abstractions(state, &abstractions);

    let signature = context.intern_function_list(&arguments, result);
    let signature = ValueEquationSignature { signature, arguments, result };

    let mut arguments = ValueEquationPatterns::clone(&signature.arguments);
    instantiate_pattern_arguments(state, context, &mut arguments, equations)?;

    let equations = state.with_source_type_renaming(&renaming, |state| {
        check_equations(state, context, origin, &signature, &arguments, equations)
    })?;

    Ok(CheckedValueEquations { patterns: arguments, abstractions, equations })
}

/// Infers a group of [`lowering::Equation`].
///
/// The `group_type` is a placeholder unification variable for the
/// equation group. Each inferred equation type must be [`subtype`]
/// of `group_type`.
///
/// [`subtype`]: unification::subtype
pub fn infer_value_equations<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    group_type: TypeId,
    equations: &[lowering::Equation],
) -> QueryResult<CheckedValueEquations>
where
    Q: ExternalQueries,
{
    let minimum_equation_arity =
        equations.iter().map(|equation| equation.binders.len()).min().unwrap_or(0);
    let result_type = state.fresh_unification(context.queries, context.prim.t);
    let mut elaborated_equations = Vec::new();

    for equation in equations {
        let mut inferred_argument_types = Vec::new();
        let mut elaborated_binders = Vec::new();
        for &binder_id in equation.binders.iter() {
            let binder = binder::infer_binder(state, context, binder_id)?;
            inferred_argument_types.push(binder.type_id);
            elaborated_binders.push(binder.binder);
        }

        let inferred_argument_types = &inferred_argument_types[..minimum_equation_arity];
        let equation_type = context.intern_function_list(inferred_argument_types, result_type);
        unification::subtype(state, context, equation_type, group_type)?;

        let guarded = if let Some(guarded) = &equation.guarded {
            guarded::check_guarded_expression(state, context, guarded, result_type)?
                .guarded_expression
        } else {
            missing_guarded_expression(state, result_type)
        };
        elaborated_equations.push(ElaboratedEquation {
            source: equation.source,
            binders: elaborated_binders.into(),
            guarded,
        });
    }

    let toolkit::InspectFunction { arguments, .. } =
        toolkit::inspect_function(state, context, group_type)?;

    Ok(CheckedValueEquations {
        patterns: arguments,
        abstractions: Vec::new(),
        equations: elaborated_equations,
    })
}

pub fn bind_signature_abstractions(
    state: &mut CheckState,
    abstractions: &[signature::SkolemisedAbstraction],
) -> Vec<tree::DeclarationAbstraction> {
    let abstractions = abstractions.iter().map(|abstraction| match *abstraction {
        signature::SkolemisedAbstraction::Type { binder, rigid } => {
            tree::DeclarationAbstraction::Type { binder, rigid }
        }
        signature::SkolemisedAbstraction::Constraint { constraint } => {
            let binder = state.push_given(constraint);
            tree::DeclarationAbstraction::Evidence { constraint, binder }
        }
        signature::SkolemisedAbstraction::Argument { .. } => tree::DeclarationAbstraction::Argument,
    });
    abstractions.collect()
}

fn check_equations<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    origin: EquationTypeOrigin,
    signature: &ValueEquationSignature,
    arguments: &[TypeId],
    equations: &[lowering::Equation],
) -> QueryResult<Vec<ElaboratedEquation>>
where
    Q: ExternalQueries,
{
    let expected_arity = signature.arguments.len();
    let mut elaborated_equations = Vec::new();

    for equation in equations {
        let equation_arity = equation.binders.len();

        if equation_arity > expected_arity {
            state.insert_error(ErrorKind::TooManyBinders {
                signature: match origin {
                    EquationTypeOrigin::Explicit(signature_id) => Some(signature_id),
                    EquationTypeOrigin::Implicit => None,
                },
                expected: expected_arity as u32,
                actual: equation_arity as u32,
            });
        }

        let mut elaborated_binders = Vec::new();
        for (position, &binder_id) in equation.binders.iter().enumerate() {
            let binder = if let Some(&argument_type) = arguments.get(position) {
                binder::check_argument_binder(state, context, binder_id, argument_type)?
            } else {
                binder::infer_binder(state, context, binder_id)?
            };
            elaborated_binders.push(binder.binder);
        }

        let expected_type = expected_guarded_type(context, signature, equation_arity);
        let guarded = if let Some(guarded) = &equation.guarded {
            guarded::check_guarded_expression(state, context, guarded, expected_type)?
                .guarded_expression
        } else {
            missing_guarded_expression(state, expected_type)
        };
        elaborated_equations.push(ElaboratedEquation {
            source: equation.source,
            binders: elaborated_binders.into(),
            guarded,
        });
    }

    Ok(elaborated_equations)
}

fn missing_guarded_expression(state: &mut CheckState, type_id: TypeId) -> tree::GuardedExpression {
    let expression = state.allocate_error_expression(type_id);
    let where_expression = tree::WhereExpression::new(expression);
    tree::GuardedExpression::unconditional(where_expression)
}

fn instantiate_pattern_arguments<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    pattern_arguments: &mut [TypeId],
    equations: &[lowering::Equation],
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    binder::instantiate_pattern_column_types(
        state,
        context,
        pattern_arguments,
        equations.iter().flat_map(|equation| equation.binders.iter().copied().enumerate()),
    )
}

fn expected_guarded_type<Q>(
    context: &CheckContext<Q>,
    signature: &ValueEquationSignature,
    equation_arity: usize,
) -> TypeId
where
    Q: ExternalQueries,
{
    let expected_arity = signature.arguments.len();
    if equation_arity == 0 {
        signature.signature
    } else if equation_arity >= expected_arity {
        signature.result
    } else {
        let remaining = &signature.arguments[equation_arity..];
        context.intern_function_list(remaining, signature.result)
    }
}
