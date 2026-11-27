use building_types::QueryResult;
use files::FileId;
use indexing::{DeriveItemId, TypeItemId};
use itertools::Itertools;
use smol_str::SmolStr;

use crate::ExternalQueries;
use crate::context::CheckContext;
use crate::core::{Type, TypeId, constraint, generalise, signature, toolkit, unification, zonk};
use crate::error::{DeriveErrorKind, ErrorCrumb};
use crate::source::types;
use crate::source::types::application;
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
        let Some((class_file, class_id)) = item.resolution else { continue };
        let item = CheckDeriveDeclaration {
            derive_id: context.indexed.items[item_id].id,
            item_id,
            class_file,
            class_id,
            kind: &item.kind,
        };
        let result = state.with_error_crumb(ErrorCrumb::DeriveDeclaration(item_id), |state| {
            check_derive_declaration(state, context, item)
        })?;
        if let Some(result) = result {
            results.push(result);
        }
    }

    Ok(results)
}

struct CheckDeriveDeclaration<'a> {
    derive_id: indexing::DeriveId,
    item_id: DeriveItemId,
    class_file: FileId,
    class_id: TypeItemId,
    kind: &'a lowering::DeriveItemKind,
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
        state.insert_error(DeriveErrorKind::CannotDeriveClass { class_file, class_id });
        return Ok(None);
    };

    if arguments.len() != expected {
        if class_arity_matches {
            state.insert_error(DeriveErrorKind::DeriveInvalidArity {
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
        state.insert_error(DeriveErrorKind::CannotDeriveForType { type_id: derived_type });
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

fn check_derive_declaration<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    item: CheckDeriveDeclaration,
) -> QueryResult<Option<DeriveHeadResult>>
where
    Q: ExternalQueries,
{
    let CheckDeriveDeclaration { derive_id, item_id, class_file, class_id, kind } = item;

    let (newtype, constraints, arguments) = match kind {
        lowering::DeriveItemKind::Declaration { newtype, constraints, arguments } => {
            let arguments = arguments.iter().copied().map(application::Argument::Syntax);
            (*newtype, &constraints[..], arguments.collect_vec())
        }
        lowering::DeriveItemKind::Clause { type_id } => {
            let arguments =
                derive_clause_arguments(state, context, class_file, class_id, *type_id)?;
            (false, &[][..], arguments)
        }
    };

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
        state.insert_error(DeriveErrorKind::DeriveInvalidArity {
            class_file,
            class_id,
            expected: expected_kinds.len(),
            actual: arguments.len(),
        });
    }

    let mut class_type = context.queries.intern_type(Type::Constructor(class_file, class_id));
    let mut class_kind = class_kind;
    let mut checked_arguments = Vec::with_capacity(arguments.len());

    for argument in arguments {
        (class_type, class_kind) = application::infer_application_kind(
            state,
            context,
            (class_type, class_kind),
            argument,
            application::Options::TYPES,
        )?;
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

    let checked =
        constraint::instances::checked_instance(state, context, resolution, signature, matchable)?;
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

fn derive_clause_arguments<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    class_file: FileId,
    class_id: TypeItemId,
    type_id: TypeItemId,
) -> QueryResult<Vec<application::Argument>>
where
    Q: ExternalQueries,
{
    let derived_type = context.queries.intern_type(Type::Constructor(context.id, type_id));
    let derived_kind = toolkit::lookup_file_type(state, context, context.id, type_id)?;

    let dispatch = derive_dispatch(context, class_file, class_id);
    if !matches!(dispatch, DeriveDispatch::Generic | DeriveDispatch::Newtype) {
        return Ok(vec![application::Argument::Core(derived_type, derived_kind)]);
    }

    let variables = match context.lowered.tree.get_type_item_kind(type_id) {
        Some(lowering::TypeItemKind::Data { declaration: Some(declaration), .. }) => {
            &declaration.variables[..]
        }
        Some(lowering::TypeItemKind::Newtype { declaration: Some(declaration), .. }) => {
            &declaration.variables[..]
        }
        _ => &[],
    };

    let mut derived = (derived_type, derived_kind);
    for variable in variables {
        let name = state.names.fresh();
        if let Some(text) = &variable.name {
            let text = context.queries.intern_smol_str(SmolStr::clone(text));
            state.checked.names.insert(name, text);
        }
        let kind = state.fresh_unification(context.queries, context.prim.t);
        let variable = context.intern_rigid(name, state.depth, kind);
        derived = application::infer_application_kind(
            state,
            context,
            derived,
            application::Argument::Core(variable, kind),
            application::Options::TYPES,
        )?;
    }

    let wildcard_kind = state.fresh_unification(context.queries, context.prim.t);
    let wildcard_type = state.fresh_unification(context.queries, wildcard_kind);

    let (derived_type, derived_kind) = derived;
    Ok(vec![
        application::Argument::Core(derived_type, derived_kind),
        application::Argument::Core(wildcard_type, wildcard_kind),
    ])
}
