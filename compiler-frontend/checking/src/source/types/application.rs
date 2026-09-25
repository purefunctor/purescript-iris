use building_types::QueryResult;

use crate::context::CheckContext;
use crate::core::substitute::SubstituteName;
use crate::core::{Type, TypeId, normalise, unification};
use crate::error::ErrorKind;
use crate::state::CheckState;
use crate::{ExternalQueries, safe_loop};

use super::{check_kind, infer_kind};

pub type FnTypeKind = (TypeId, TypeId);

#[derive(Debug, Clone, Copy)]
pub enum Argument {
    Syntax(lowering::TypeId),
    Core(TypeId, TypeId),
}

#[derive(Debug, Clone, Copy)]
pub struct Options {
    pub message: &'static str,
    pub expand: bool,
}

impl Options {
    pub const TYPES: Options = Options { message: "cannot apply function type", expand: true };
    pub const SYNONYM: Options = Options { message: "cannot apply synonym type", expand: false };
    pub const OPERATOR: Options = Options { message: "cannot apply operator type", expand: true };

    fn normalise<Q>(
        self,
        state: &mut CheckState,
        context: &CheckContext<Q>,
        id: TypeId,
    ) -> QueryResult<TypeId>
    where
        Q: ExternalQueries,
    {
        if self.expand {
            normalise::expand(state, context, id)
        } else {
            Ok(normalise::normalise(state, context, id))
        }
    }
}

pub fn infer_application_kind<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    (mut function_type, mut function_kind): FnTypeKind,
    argument: Argument,
    options: Options,
) -> QueryResult<FnTypeKind>
where
    Q: ExternalQueries,
{
    safe_loop! {
        function_kind = options.normalise(state, context, function_kind)?;

        match *context.lookup_type(function_kind) {
            Type::Function(expected_kind, result_kind) => {
                let argument_type = check_application_argument_kind(
                    state,
                    context,
                    argument,
                    expected_kind,
                )?;

                let result_type = context.intern_application(function_type, argument_type);
                let result_kind = options.normalise(state, context, result_kind)?;

                break Ok((result_type, result_kind));
            }

            Type::Unification(unification_id) => {
                let argument_kind = state.fresh_unification(context.queries, context.prim.t);
                let result_kind = state.fresh_unification(context.queries, context.prim.t);

                let function = context.intern_function(argument_kind, result_kind);
                unification::solve(state, context, function_kind, unification_id, function)?;

                let argument_type = check_application_argument_kind(
                    state,
                    context,
                    argument,
                    argument_kind,
                )?;

                let result_type = context.intern_application(function_type, argument_type);
                let result_kind = options.normalise(state, context, result_kind)?;

                break Ok((result_type, result_kind));
            }

            Type::Forall(binder_id, inner_kind) => {
                let binder = context.lookup_forall_binder(binder_id);
                let binder_kind = options.normalise(state, context, binder.kind)?;

                let kind_argument = state.fresh_unification(context.queries, binder_kind);

                function_type = context.intern_kind_application(function_type, kind_argument);
                function_kind =
                    SubstituteName::one(state, context, binder.name, kind_argument, inner_kind)?;

            }

            _ => {
                // The argument type must still be inferred even with an
                // invalid function type. This is important for implicit
                // type variables as checking them also binds them.
                let argument_type = infer_invalid_application_argument(state, context, argument)?;

                let invalid_type = context.intern_application(function_type, argument_type);
                let unknown_kind = context.unknown(options.message);

                state.insert_error(ErrorKind::InvalidTypeApplication {
                    function_type,
                    function_kind,
                    argument_type,
                });

                break Ok((invalid_type, unknown_kind));
            }
        }
    }
}

pub fn infer_application_arguments<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    mut function: FnTypeKind,
    arguments: &[Argument],
    options: Options,
) -> QueryResult<FnTypeKind>
where
    Q: ExternalQueries,
{
    for &argument in arguments {
        function = infer_application_kind(state, context, function, argument, options)?;
    }

    Ok(function)
}

fn check_application_argument_kind<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    argument: Argument,
    expected_kind: TypeId,
) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    match argument {
        Argument::Syntax(argument_id) => {
            let (argument_type, _) = check_kind(state, context, argument_id, expected_kind)?;
            Ok(argument_type)
        }
        Argument::Core(argument_type, argument_kind) => {
            unification::subtype(state, context, argument_kind, expected_kind)?;
            Ok(argument_type)
        }
    }
}

fn infer_invalid_application_argument<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    argument: Argument,
) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    match argument {
        Argument::Syntax(argument_id) => {
            let (argument_type, _) = infer_kind(state, context, argument_id)?;
            Ok(argument_type)
        }
        Argument::Core(argument_type, _) => Ok(argument_type),
    }
}
