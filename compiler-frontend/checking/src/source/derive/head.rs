use building_types::QueryResult;
use files::FileId;
use indexing::{DeriveItemId, TypeItemId};
use itertools::Itertools;

use crate::ExternalQueries;
use crate::context::CheckContext;
use crate::core::{
    CheckedInstance, Type, TypeId, constraint, generalise, signature, toolkit, unification, zonk,
};
use crate::error::{ErrorCrumb, ErrorKind};
use crate::source::types;
use crate::state::CheckState;

use super::{
    DeriveDispatch, DeriveHeadResult, DeriveStrategy, DerivedData, contravariant, derive_dispatch,
    eq_ord, eq1_ord1, foldable, functor, generic, newtype, tools, traversable,
};

pub fn check_derive_declarations<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
) -> QueryResult<Vec<DeriveHeadResult>>
where
    Q: ExternalQueries,
{
    let mut results = Vec::new();

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

struct CheckKnownClassDerive<'a> {
    class_file: FileId,
    class_id: TypeItemId,
    class_arity_matches: bool,
    arguments: &'a [TypeId],
}

fn check_known_class_derive<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    item: CheckKnownClassDerive,
) -> QueryResult<Option<DeriveStrategy>>
where
    Q: ExternalQueries,
{
    let CheckKnownClassDerive { class_file, class_id, class_arity_matches, arguments } = item;

    let dispatch = derive_dispatch(context, class_file, class_id);
    let Some(expected) = dispatch.arity() else {
        state.insert_error(ErrorKind::CannotDeriveClass { class_file, class_id });
        return Ok(None);
    };

    if arguments.len() != expected {
        if class_arity_matches {
            state.insert_error(ErrorKind::DeriveInvalidArity {
                class_file,
                class_id,
                expected,
                actual: arguments.len(),
            });
        }
        return Ok(None);
    }

    let derived_type = arguments[0];
    let local_type = if dispatch.accepts_foreign_data() {
        tools::extract_local_type_constructor(state, context, derived_type)?
    } else {
        tools::extract_local_algebraic_data(state, context, derived_type)?
    };

    let Some((data_file, data_id)) = local_type else {
        state.insert_error(ErrorKind::CannotDeriveForType { type_id: derived_type });
        return Ok(None);
    };

    let derived = DerivedData { derived_type, data_file, data_id };

    let strategy = match dispatch {
        DeriveDispatch::Eq | DeriveDispatch::Ord => {
            Some(eq_ord::check_derive_field_constraints(class_file, class_id, derived))
        }
        DeriveDispatch::Eq1 => {
            eq1_ord1::check_derive_eq1(state, context, class_file, class_id, derived)
        }
        DeriveDispatch::Ord1 => {
            eq1_ord1::check_derive_ord1(state, context, class_file, class_id, derived)
        }
        DeriveDispatch::Functor => {
            Some(functor::check_derive_functor(class_file, class_id, derived))
        }
        DeriveDispatch::Bifunctor => {
            Some(functor::check_derive_bifunctor(context, class_file, class_id, derived))
        }
        DeriveDispatch::Contravariant => {
            Some(contravariant::check_derive_contravariant(class_file, class_id, derived))
        }
        DeriveDispatch::Profunctor => {
            Some(contravariant::check_derive_profunctor(context, derived))
        }
        DeriveDispatch::Foldable => {
            Some(foldable::check_derive_foldable(context, class_file, class_id, derived))
        }
        DeriveDispatch::Bifoldable => {
            Some(foldable::check_derive_bifoldable(context, class_file, class_id, derived))
        }
        DeriveDispatch::Traversable => {
            Some(traversable::check_derive_traversable(context, class_file, class_id, derived))
        }
        DeriveDispatch::Bitraversable => {
            Some(traversable::check_derive_bitraversable(context, class_file, class_id, derived))
        }
        DeriveDispatch::Newtype => {
            newtype::check_derive_newtype_class(state, context, derived, arguments[1])?
        }
        DeriveDispatch::Generic => generic::check_derive_generic(
            state,
            context,
            class_file,
            class_id,
            derived,
            arguments[1],
        )?,
        DeriveDispatch::Unsupported => {
            unreachable!("invariant violated: unsupported dispatch has no derive arity")
        }
    };

    Ok(strategy)
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

    let class_arity_matches = expected_kinds.len() == arguments.len();
    if !class_arity_matches {
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
        let item = CheckKnownClassDerive {
            class_file,
            class_id,
            class_arity_matches,
            arguments: &checked_arguments,
        };
        check_known_class_derive(state, context, item)?
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
