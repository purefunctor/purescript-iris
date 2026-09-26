use building_types::QueryResult;
use files::FileId;
use indexing::TermItemId;
use lowering::StringKind;
use smol_str::SmolStr;

use crate::context::{CheckContext, KnownGeneric};
use crate::core::substitute::SubstituteName;
use crate::core::{Type, TypeId, normalise, toolkit, unification};
use crate::error::ErrorKind;
use crate::state::CheckState;
use crate::{ExternalQueries, safe_loop};

use super::{DeriveStrategy, tools};

pub fn check_derive_generic<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    class_file: FileId,
    class_id: indexing::TypeItemId,
    arguments: &[TypeId],
) -> QueryResult<Option<DeriveStrategy>>
where
    Q: ExternalQueries,
{
    let [derived_type, wildcard_type] = arguments else {
        state.insert_error(ErrorKind::DeriveInvalidArity {
            class_file,
            class_id,
            expected: 2,
            actual: arguments.len(),
        });
        return Ok(None);
    };

    let Some((data_file, data_id)) =
        toolkit::extract_type_constructor(state, context, *derived_type)?
    else {
        state.insert_error(ErrorKind::CannotDeriveForType { type_id: *derived_type });
        return Ok(None);
    };

    let Some(ref known_generic) = context.known_generic else {
        state.insert_error(ErrorKind::CannotDeriveClass { class_file, class_id });
        return Ok(None);
    };

    let constructors = tools::lookup_data_constructors(context, data_file, data_id)?;
    let generic_rep =
        build_generic_rep(state, context, known_generic, data_file, *derived_type, &constructors)?;

    if !unification::unify(state, context, *wildcard_type, generic_rep)? {
        return Ok(None);
    }
    Ok(Some(DeriveStrategy::Generic { data_file, data_id }))
}

fn build_generic_rep<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    known_generic: &KnownGeneric,
    data_file: FileId,
    derived_type: TypeId,
    constructors: &[TermItemId],
) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    let [rest @ .., last] = constructors else {
        return Ok(known_generic.no_constructors);
    };

    let arguments = extract_all_applications(state, context, derived_type)?;
    let mut rep =
        build_generic_constructor(state, context, known_generic, data_file, &arguments, *last)?;

    for &constructor_id in rest.iter().rev() {
        let constructor = build_generic_constructor(
            state,
            context,
            known_generic,
            data_file,
            &arguments,
            constructor_id,
        )?;
        let applied = context.intern_application(known_generic.sum, constructor);
        rep = context.intern_application(applied, rep);
    }

    Ok(rep)
}

fn build_generic_constructor<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    known_generic: &KnownGeneric,
    data_file: FileId,
    arguments: &[TypeId],
    constructor_id: TermItemId,
) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    let constructor_name = if data_file == context.id {
        context.indexed.items[constructor_id]
            .name
            .clone()
            .unwrap_or_else(|| SmolStr::new_static("<Unknown>"))
    } else {
        context.queries.indexed(data_file)?.items[constructor_id]
            .name
            .clone()
            .unwrap_or_else(|| SmolStr::new_static("<Unknown>"))
    };

    let constructor_type = toolkit::lookup_file_term(state, context, data_file, constructor_id)?;
    let field_types = instantiate_constructor_fields(state, context, constructor_type, arguments)?;
    let fields_rep = build_fields_rep(context, known_generic, &field_types);

    let name =
        context.queries.intern_type(Type::String(StringKind::String, constructor_name.into()));
    let constructor = context.intern_application(known_generic.constructor, name);
    Ok(context.intern_application(constructor, fields_rep))
}

fn build_fields_rep(
    context: &CheckContext<impl ExternalQueries>,
    known_generic: &KnownGeneric,
    field_types: &[TypeId],
) -> TypeId {
    let [rest @ .., last] = field_types else {
        return known_generic.no_arguments;
    };

    let mut rep = context.intern_application(known_generic.argument, *last);
    for &field in rest.iter().rev() {
        let argument = context.intern_application(known_generic.argument, field);
        let product = context.intern_application(known_generic.product, argument);
        rep = context.intern_application(product, rep);
    }
    rep
}

fn extract_all_applications<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    mut applied_type: TypeId,
) -> QueryResult<Vec<TypeId>>
where
    Q: ExternalQueries,
{
    let mut arguments = Vec::new();
    safe_loop! {
        applied_type = normalise::expand(state, context, applied_type)?;
        match *context.lookup_type(applied_type) {
            Type::Application(function, argument) | Type::KindApplication(function, argument) => {
                arguments.push(argument);
                applied_type = function;
            }
            _ => break,
        }
    }
    arguments.reverse();
    Ok(arguments)
}

fn instantiate_constructor_fields<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    constructor_type: TypeId,
    arguments: &[TypeId],
) -> QueryResult<Vec<TypeId>>
where
    Q: ExternalQueries,
{
    let mut current = constructor_type;
    let mut arguments = arguments.iter().copied();

    safe_loop! {
        current = normalise::expand(state, context, current)?;
        let Type::Forall(binder_id, inner) = *context.lookup_type(current) else {
            break;
        };

        let binder = context.lookup_forall_binder(binder_id);
        let argument_type =
            arguments.next().unwrap_or_else(|| context.intern_rigid(binder.name, state.depth, binder.kind));
        current = SubstituteName::one(state, context, binder.name, argument_type, inner)?;
    }

    let toolkit::InspectFunction { arguments, .. } =
        toolkit::inspect_function(state, context, current)?;
    Ok(arguments)
}
