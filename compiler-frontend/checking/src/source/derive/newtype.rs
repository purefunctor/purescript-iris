use building_types::QueryResult;
use files::FileId;
use indexing::TypeItemId;

use crate::ExternalQueries;
use crate::context::CheckContext;
use crate::core::{Type, TypeId, normalise, toolkit, unification};
use crate::error::ErrorKind;
use crate::source::types;
use crate::source::types::application::{self, Argument, Options};
use crate::state::CheckState;

use super::DeriveStrategy;

pub fn check_derive_newtype<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    class_file: FileId,
    class_id: TypeItemId,
    arguments: &[TypeId],
) -> QueryResult<Option<DeriveStrategy>>
where
    Q: ExternalQueries,
{
    let [preceding_arguments @ .., newtype_type] = arguments else {
        return Ok(None);
    };

    let Some((newtype_file, newtype_id)) =
        toolkit::extract_type_constructor(state, context, *newtype_type)?
    else {
        state.insert_error(ErrorKind::CannotDeriveForType { type_id: *newtype_type });
        return Ok(None);
    };

    if newtype_file != context.id {
        state.insert_error(ErrorKind::NonLocalNewtype { type_id: *newtype_type });
        return Ok(None);
    }

    let Some(toolkit::NewtypeInner { inner, rigids }) =
        toolkit::get_newtype_inner(state, context, newtype_file, newtype_id, *newtype_type)?
    else {
        state.insert_error(ErrorKind::ExpectedNewtype { type_id: *newtype_type });
        return Ok(None);
    };

    let inner = if let Some(inner) = try_peel_trailing_rigids(state, context, inner, &rigids)? {
        inner
    } else {
        state.insert_error(ErrorKind::InvalidNewtypeDeriveSkolemArguments);
        return Ok(None);
    };

    let inner = normalise::normalise(state, context, inner);
    let class = context.queries.intern_type(Type::Constructor(class_file, class_id));
    let class_kind = toolkit::lookup_file_type(state, context, class_file, class_id)?;

    let (delegate_constraint, _) = preceding_arguments.iter().copied().chain([inner]).try_fold(
        (class, class_kind),
        |function, argument| {
            let argument_kind = types::elaborate_kind(state, context, argument)?;
            let (function, function_kind) = application::infer_application_kind(
                state,
                context,
                function,
                Argument::Core(argument, argument_kind),
                Options::TYPES,
            )?;
            Ok((function, function_kind))
        },
    )?;

    Ok(Some(DeriveStrategy::NewtypeDeriveConstraint { delegate_constraint }))
}

pub fn check_derive_newtype_class<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    _class_file: FileId,
    _class_id: TypeItemId,
    arguments: &[TypeId],
) -> QueryResult<Option<DeriveStrategy>>
where
    Q: ExternalQueries,
{
    let [newtype_type, inner_type] = arguments else {
        return Ok(None);
    };

    let Some((newtype_file, newtype_id)) =
        toolkit::extract_type_constructor(state, context, *newtype_type)?
    else {
        state.insert_error(ErrorKind::CannotDeriveForType { type_id: *newtype_type });
        return Ok(None);
    };

    if newtype_file != context.id {
        state.insert_error(ErrorKind::NonLocalNewtype { type_id: *newtype_type });
        return Ok(None);
    }

    let Some(newtype_inner) =
        toolkit::get_newtype_inner(state, context, newtype_file, newtype_id, *newtype_type)?
    else {
        state.insert_error(ErrorKind::ExpectedNewtype { type_id: *newtype_type });
        return Ok(None);
    };

    if !unification::unify(state, context, *inner_type, newtype_inner.inner)? {
        return Ok(None);
    }

    Ok(Some(DeriveStrategy::NewtypeClass))
}

fn try_peel_trailing_rigids<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    mut type_id: TypeId,
    rigids: &[TypeId],
) -> QueryResult<Option<TypeId>>
where
    Q: ExternalQueries,
{
    if rigids.is_empty() {
        return Ok(Some(type_id));
    }

    for &rigid in rigids.iter().rev() {
        type_id = normalise::expand(state, context, type_id)?;

        match *context.lookup_type(type_id) {
            Type::Application(function, argument) | Type::KindApplication(function, argument) => {
                let argument = normalise::expand(state, context, argument)?;
                if argument != rigid {
                    return Ok(None);
                }
                type_id = function;
            }

            Type::Function(argument, result) => {
                let result = normalise::expand(state, context, result)?;
                if result != rigid {
                    return Ok(None);
                }
                type_id = context.intern_application(context.prim.function, argument);
            }

            _ => return Ok(None),
        }
    }

    Ok(Some(type_id))
}
