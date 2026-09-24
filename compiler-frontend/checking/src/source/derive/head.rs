use building_types::QueryResult;
use files::FileId;
use indexing::{DeriveItemId, TypeItemId};
use itertools::Itertools;

use crate::ExternalQueries;
use crate::context::CheckContext;
use crate::core::{
    CheckedInstance, Type, constraint, generalise, signature, toolkit, unification, zonk,
};
use crate::error::{ErrorCrumb, ErrorKind};
use crate::source::types;
use crate::state::CheckState;

use super::{
    DeriveDispatch, DeriveHeadResult, contravariant, derive_dispatch, eq_ord, eq1_ord1, foldable,
    functor, generic, newtype, traversable,
};

pub fn check_derive_declarations<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
) -> QueryResult<Vec<DeriveHeadResult>>
where
    Q: ExternalQueries,
{
    let mut results = vec![];

    for &item_id in &context.grouped.derive_items {
        let Some(item) = context.lowered.tree.get_derive_item(item_id) else { continue };
        let item = CheckDeriveDeclaration {
            item_id,
            newtype: item.newtype,
            constraints: &item.constraints,
            resolution: item.resolution,
            arguments: &item.arguments,
        };
        if let Some(result) = check_derive_declaration(state, context, item)? {
            results.push(result);
        }
    }

    Ok(results)
}

struct CheckDeriveDeclaration<'a> {
    item_id: DeriveItemId,
    newtype: bool,
    constraints: &'a [lowering::TypeId],
    resolution: Option<(FileId, TypeItemId)>,
    arguments: &'a [lowering::TypeId],
}

struct CheckDeriveDeclarationCore<'a> {
    derive_id: indexing::DeriveId,
    item_id: DeriveItemId,
    newtype: bool,
    class_file: FileId,
    class_id: TypeItemId,
    constraints: &'a [lowering::TypeId],
    arguments: &'a [lowering::TypeId],
}

fn check_derive_declaration<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    item: CheckDeriveDeclaration,
) -> QueryResult<Option<DeriveHeadResult>>
where
    Q: ExternalQueries,
{
    let CheckDeriveDeclaration { item_id, newtype, constraints, resolution, arguments } = item;

    let Some((class_file, class_id)) = resolution else {
        return Ok(None);
    };

    let derive_id = context.indexed.items[item_id].id;

    state.with_error_crumb(ErrorCrumb::DeriveDeclaration(item_id), |state| {
        check_derive_declaration_core(
            state,
            context,
            CheckDeriveDeclarationCore {
                derive_id,
                item_id,
                newtype,
                class_file,
                class_id,
                constraints,
                arguments,
            },
        )
    })
}

fn check_derive_declaration_core<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    item: CheckDeriveDeclarationCore,
) -> QueryResult<Option<DeriveHeadResult>>
where
    Q: ExternalQueries,
{
    let CheckDeriveDeclarationCore {
        derive_id,
        item_id,
        newtype,
        class_file,
        class_id,
        constraints,
        arguments,
    } = item;

    let class_kind = toolkit::lookup_file_type(state, context, class_file, class_id)?;

    let expected_kinds = {
        let signature = signature::decompose_signature(
            state,
            context,
            class_kind,
            signature::DecomposeSignatureMode::Full,
        )?;
        signature.arguments().collect_vec()
    };

    if expected_kinds.len() != arguments.len() {
        state.insert_error(ErrorKind::DeriveInvalidArity {
            class_file,
            class_id,
            expected: expected_kinds.len(),
            actual: arguments.len(),
        });
    }

    let mut class_type = context.queries.intern_type(Type::Constructor(class_file, class_id));
    let mut class_kind = class_kind;
    let mut checked_arguments = Vec::with_capacity(arguments.len());

    for &argument in arguments {
        (class_type, class_kind) =
            types::infer_application_kind(state, context, (class_type, class_kind), argument)?;
        let (_, extracted_arguments) =
            toolkit::extract_type_application(state, context, class_type)?;
        if let Some(&checked_argument) = extracted_arguments.last() {
            checked_arguments.push(checked_argument);
        }
    }

    unification::subtype(state, context, class_kind, context.prim.constraint)?;

    let mut checked_constraints = Vec::with_capacity(constraints.len());
    for &constraint in constraints {
        let (constraint_type, _) =
            types::check_kind(state, context, constraint, context.prim.constraint)?;
        checked_constraints.push(constraint_type);
    }

    let mut canonical = class_type;
    for &constraint in checked_constraints.iter().rev() {
        canonical = context.intern_constrained(constraint, canonical);
    }

    constraint::instances::validate_rows(state, context, class_file, class_id, &checked_arguments)?;

    let strategy = if newtype {
        newtype::check_derive_newtype(state, context, class_file, class_id, &checked_arguments)?
    } else {
        match derive_dispatch(context, class_file, class_id) {
            DeriveDispatch::Eq => {
                eq_ord::check_derive_eq(state, context, class_file, class_id, &checked_arguments)?
            }
            DeriveDispatch::Eq1 => eq1_ord1::check_derive_eq1(
                state,
                context,
                class_file,
                class_id,
                &checked_arguments,
            )?,
            DeriveDispatch::Functor => functor::check_derive_functor(
                state,
                context,
                class_file,
                class_id,
                &checked_arguments,
            )?,
            DeriveDispatch::Bifunctor => functor::check_derive_bifunctor(
                state,
                context,
                class_file,
                class_id,
                &checked_arguments,
            )?,
            DeriveDispatch::Contravariant => contravariant::check_derive_contravariant(
                state,
                context,
                class_file,
                class_id,
                &checked_arguments,
            )?,
            DeriveDispatch::Profunctor => contravariant::check_derive_profunctor(
                state,
                context,
                class_file,
                class_id,
                &checked_arguments,
            )?,
            DeriveDispatch::Foldable => foldable::check_derive_foldable(
                state,
                context,
                class_file,
                class_id,
                &checked_arguments,
            )?,
            DeriveDispatch::Bifoldable => foldable::check_derive_bifoldable(
                state,
                context,
                class_file,
                class_id,
                &checked_arguments,
            )?,
            DeriveDispatch::Traversable => traversable::check_derive_traversable(
                state,
                context,
                class_file,
                class_id,
                &checked_arguments,
            )?,
            DeriveDispatch::Bitraversable => traversable::check_derive_bitraversable(
                state,
                context,
                class_file,
                class_id,
                &checked_arguments,
            )?,
            DeriveDispatch::Newtype => newtype::check_derive_newtype_class(
                state,
                context,
                class_file,
                class_id,
                &checked_arguments,
            )?,
            DeriveDispatch::Generic => generic::check_derive_generic(
                state,
                context,
                class_file,
                class_id,
                &checked_arguments,
            )?,
            DeriveDispatch::Ord => {
                eq_ord::check_derive_ord(state, context, class_file, class_id, &checked_arguments)?
            }
            DeriveDispatch::Ord1 => eq1_ord1::check_derive_ord1(
                state,
                context,
                class_file,
                class_id,
                &checked_arguments,
            )?,
            DeriveDispatch::Unsupported => {
                state.insert_error(ErrorKind::CannotDeriveClass { class_file, class_id });
                None
            }
        }
    };

    let resolution = (class_file, class_id);
    let canonical = zonk::zonk(state, context, canonical)?;
    let signature = generalise::generalise_implicit(state, context, canonical)?;
    let matchable = toolkit::freshen_instance_signature(state, context, signature)?;

    let checked = CheckedInstance { resolution, signature, matchable };
    state.checked.derived_instances.insert(derive_id, checked);

    Ok(strategy.map(|strategy| DeriveHeadResult {
        derive_id,
        item_id,
        signature,
        constraints: checked_constraints,
        class_file,
        class_id,
        arguments: checked_arguments,
        strategy,
    }))
}
