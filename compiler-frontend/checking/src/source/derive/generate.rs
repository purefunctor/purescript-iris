use std::sync::Arc;

use building_types::QueryResult;

use crate::context::CheckContext;
use crate::core::{ApplicationArgument, TypeId, signature, toolkit};
use crate::evidence::Evidence;
use crate::source::derive::builder::DerivedTreeBuilder;
use crate::source::term_items::{
    emit_instance_superclass_constraints, freshen_instance_rigids, instantiate_class_member_type,
};
use crate::source::terms::{ElaboratedExpression, equations};
use crate::state::CheckState;
use crate::{ExternalQueries, tree};

use super::variance::VarianceRecipe;
use super::{DeriveDispatch, DeriveHeadResult, DeriveStrategy, derive_dispatch};

mod contravariant;
mod eq_ord;
mod foldable;
mod functor;
mod generic;
mod traversable;

pub(super) fn generate_instance<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    result: &DeriveHeadResult,
    recipe: Option<&VarianceRecipe>,
) -> QueryResult<Option<tree::TermDeclaration>>
where
    Q: ExternalQueries,
{
    if let DeriveStrategy::NewtypeDeriveConstraint { delegate_constraint } = result.strategy {
        return generate_delegate_instance(state, context, result, delegate_constraint);
    }

    let dispatch = derive_dispatch(context, result.class_file, result.class_id);
    match dispatch {
        DeriveDispatch::Eq
        | DeriveDispatch::Eq1
        | DeriveDispatch::Ord
        | DeriveDispatch::Ord1
        | DeriveDispatch::Functor
        | DeriveDispatch::Bifunctor
        | DeriveDispatch::Contravariant
        | DeriveDispatch::Profunctor
        | DeriveDispatch::Foldable
        | DeriveDispatch::Bifoldable
        | DeriveDispatch::Traversable
        | DeriveDispatch::Bitraversable
        | DeriveDispatch::Newtype
        | DeriveDispatch::Generic => {
            generate_known_instance(state, context, result, dispatch, recipe)
        }
        DeriveDispatch::Unsupported => {
            unreachable!("invariant violated: successful derive has unsupported dispatch")
        }
    }
}

fn generate_delegate_instance<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    result: &DeriveHeadResult,
    delegate_constraint: TypeId,
) -> QueryResult<Option<tree::TermDeclaration>>
where
    Q: ExternalQueries,
{
    let Some(instance) = toolkit::instance_info(
        state,
        context,
        result.signature,
        (result.class_file, result.class_id),
    )?
    else {
        return Ok(None);
    };

    let freshened = freshen_instance_rigids(state, context, &instance)?;
    state.with_source_type_renaming(&freshened.renaming, |state| {
        let mut evidences = Vec::with_capacity(freshened.constraints.len());
        for (&constraint, &signature_constraint) in
            std::iter::zip(&freshened.constraints, &instance.constraints)
        {
            let evidence = Evidence::Given(state.push_given(constraint));
            evidences.push(tree::InstanceEvidence { constraint: signature_constraint, evidence });
        }

        let delegate_constraint =
            freshened.renaming.substitute(state, context, delegate_constraint)?;
        let evidence = state.push_wanted(delegate_constraint);

        let instance = tree::InstanceDeclaration {
            class: (result.class_file, result.class_id),
            rigid_parameters: Arc::from(freshened.rigids),
            evidences: Arc::from(evidences),
            superclasses: Arc::from([]),
            implementation: tree::InstanceImplementation::Delegate {
                constraint: delegate_constraint,
                evidence,
            },
        };
        let declaration = tree::TermDeclaration {
            type_id: result.signature,
            kind: tree::TermDeclarationKind::Instance(instance),
        };
        Ok(Some(declaration))
    })
}

fn generate_known_instance<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    result: &DeriveHeadResult,
    dispatch: DeriveDispatch,
    recipe: Option<&VarianceRecipe>,
) -> QueryResult<Option<tree::TermDeclaration>>
where
    Q: ExternalQueries,
{
    let Some(instance) = toolkit::instance_info(
        state,
        context,
        result.signature,
        (result.class_file, result.class_id),
    )?
    else {
        if matches!(result.strategy, DeriveStrategy::NewtypeClass | DeriveStrategy::Generic { .. })
        {
            unreachable!("invariant violated: runtime derive has no instance signature");
        }
        return Ok(None);
    };

    let freshened = freshen_instance_rigids(state, context, &instance)?;
    state.with_source_type_renaming(&freshened.renaming, |state| {
        let mut evidences = Vec::with_capacity(freshened.constraints.len());
        for (&constraint, &signature_constraint) in
            std::iter::zip(&freshened.constraints, &instance.constraints)
        {
            let evidence = Evidence::Given(state.push_given(constraint));
            evidences.push(tree::InstanceEvidence { constraint: signature_constraint, evidence });
        }

        let superclasses = emit_instance_superclass_constraints(
            state,
            context,
            result.class_file,
            result.class_id,
            &freshened.arguments,
        )?;

        let members = match dispatch {
            DeriveDispatch::Eq => {
                eq_ord::generate_eq_member(state, context, result, &freshened.arguments)?
                    .map(|member| vec![member])
            }
            DeriveDispatch::Eq1 => generate_delegated_member(
                state,
                context,
                result,
                &freshened.arguments,
                context.known_terms.eq,
            )?
            .map(|member| vec![member]),
            DeriveDispatch::Ord => {
                eq_ord::generate_ord_member(state, context, result, &freshened.arguments)?
                    .map(|member| vec![member])
            }
            DeriveDispatch::Ord1 => generate_delegated_member(
                state,
                context,
                result,
                &freshened.arguments,
                context.known_terms.compare,
            )?
            .map(|member| vec![member]),
            DeriveDispatch::Functor | DeriveDispatch::Bifunctor => {
                let Some(recipe) = recipe else {
                    return Ok(None);
                };
                let traversal = match dispatch {
                    DeriveDispatch::Functor => functor::TraversalKind::Functor,
                    DeriveDispatch::Bifunctor => functor::TraversalKind::Bifunctor,
                    _ => unreachable!(),
                };
                functor::generate_traversal_member(
                    state,
                    context,
                    result,
                    &freshened.arguments,
                    recipe,
                    traversal,
                )?
                .map(|member| vec![member])
            }
            DeriveDispatch::Contravariant | DeriveDispatch::Profunctor => {
                let Some(recipe) = recipe else {
                    return Ok(None);
                };
                let traversal = match dispatch {
                    DeriveDispatch::Contravariant => contravariant::TraversalKind::Contravariant,
                    DeriveDispatch::Profunctor => contravariant::TraversalKind::Profunctor,
                    _ => unreachable!(),
                };
                contravariant::generate_traversal_member(
                    state,
                    context,
                    result,
                    &freshened.arguments,
                    recipe,
                    traversal,
                )?
                .map(|member| vec![member])
            }
            DeriveDispatch::Foldable | DeriveDispatch::Bifoldable => {
                let Some(recipe) = recipe else {
                    return Ok(None);
                };
                let traversal = match dispatch {
                    DeriveDispatch::Foldable => foldable::TraversalKind::Foldable,
                    DeriveDispatch::Bifoldable => foldable::TraversalKind::Bifoldable,
                    _ => unreachable!(),
                };
                foldable::generate_fold_members(
                    state,
                    context,
                    result,
                    &freshened.arguments,
                    recipe,
                    traversal,
                )?
            }
            DeriveDispatch::Traversable | DeriveDispatch::Bitraversable => {
                let Some(recipe) = recipe else {
                    return Ok(None);
                };
                let traversal = match dispatch {
                    DeriveDispatch::Traversable => traversable::TraversalKind::Traversable,
                    DeriveDispatch::Bitraversable => traversable::TraversalKind::Bitraversable,
                    _ => unreachable!(),
                };
                traversable::generate_traversal_members(
                    state,
                    context,
                    result,
                    &freshened.arguments,
                    recipe,
                    traversal,
                )?
            }
            DeriveDispatch::Newtype => Some(vec![]),
            DeriveDispatch::Generic => {
                generic::generate_generic_members(state, context, result, &freshened.arguments)?
            }
            DeriveDispatch::Unsupported => {
                unreachable!("invariant violated: successful derive has unsupported dispatch")
            }
        };

        let Some(members) = members else {
            return Ok(None);
        };

        let instance = tree::InstanceDeclaration {
            class: (result.class_file, result.class_id),
            rigid_parameters: Arc::from(freshened.rigids),
            evidences: Arc::from(evidences),
            superclasses: Arc::from(superclasses),
            implementation: tree::InstanceImplementation::Members(Arc::from(members)),
        };
        let declaration = tree::TermDeclaration {
            type_id: result.signature,
            kind: tree::TermDeclarationKind::Instance(instance),
        };
        Ok(Some(declaration))
    })
}

pub(super) struct ResolvedMember {
    pub(super) file_id: files::FileId,
    pub(super) item_id: indexing::TermItemId,
    pub(super) implementation_type: TypeId,
}

pub(super) fn resolve_member<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    result: &DeriveHeadResult,
    instance_arguments: &[ApplicationArgument],
) -> QueryResult<Option<ResolvedMember>>
where
    Q: ExternalQueries,
{
    let Some(class) =
        toolkit::lookup_file_class(state, context, result.class_file, result.class_id)?
    else {
        return Ok(None);
    };

    let [member] = class.members[..] else {
        return Ok(None);
    };

    let file_id = result.class_file;
    let item_id = member.item_id;
    let Some(implementation_type) = instantiate_class_member_type(
        state,
        context,
        (file_id, item_id),
        (result.class_file, result.class_id),
        instance_arguments,
    )?
    else {
        return Ok(None);
    };

    Ok(Some(ResolvedMember { file_id, item_id, implementation_type }))
}

pub(super) fn resolve_known_member<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    result: &DeriveHeadResult,
    instance_arguments: &[ApplicationArgument],
    (file_id, item_id): (files::FileId, indexing::TermItemId),
) -> QueryResult<Option<ResolvedMember>>
where
    Q: ExternalQueries,
{
    let Some(implementation_type) = instantiate_class_member_type(
        state,
        context,
        (file_id, item_id),
        (result.class_file, result.class_id),
        instance_arguments,
    )?
    else {
        return Ok(None);
    };

    Ok(Some(ResolvedMember { file_id, item_id, implementation_type }))
}

fn generate_delegated_member<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    result: &DeriveHeadResult,
    instance_arguments: &[ApplicationArgument],
    operation: Option<(files::FileId, indexing::TermItemId)>,
) -> QueryResult<Option<tree::InstanceMember>>
where
    Q: ExternalQueries,
{
    state.with_implication(|state| {
        let DeriveStrategy::DelegateConstraint { .. } = result.strategy else {
            return Ok(None);
        };
        let Some(operation) = operation else {
            return Ok(None);
        };
        let Some(member) = resolve_member(state, context, result, instance_arguments)? else {
            return Ok(None);
        };

        let signature::SkolemisedSignature { renaming, abstractions, result: body_type, .. } =
            signature::expect_term_signature(state, context, member.implementation_type, 0)?;

        let abstractions = equations::bind_signature_abstractions(state, &abstractions);

        let body = state.with_source_type_renaming(&renaming, |state| {
            let mut builder = DerivedTreeBuilder::new(state, context, result.derive_id);
            let body = builder.term_reference(operation)?;
            builder.subtype(body, body_type)
        })?;

        let member = generated_member(
            result.derive_id,
            (member.file_id, member.item_id),
            member.implementation_type,
            abstractions,
            body,
        );
        Ok(Some(member))
    })
}

pub(super) fn generated_member(
    derive_id: indexing::DeriveId,
    resolution: (files::FileId, indexing::TermItemId),
    implementation_type: TypeId,
    abstractions: Vec<tree::DeclarationAbstraction>,
    body: ElaboratedExpression,
) -> tree::InstanceMember {
    let where_expression = tree::WhereExpression::new(body.expression);
    let guarded_expression = tree::GuardedExpression::unconditional(where_expression);
    let equation =
        tree::Equation::generated(derive_id, resolution.1, Arc::from([]), guarded_expression);
    tree::InstanceMember {
        resolution,
        implementation_type,
        abstractions: Arc::from(abstractions),
        equations: Arc::from([equation]),
    }
}
