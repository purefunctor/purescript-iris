use building_types::QueryResult;

use crate::context::CheckContext;
use crate::core::{ApplicationArgument, RowType, Type, TypeId, normalise, toolkit};
use crate::source::derive::builder::DerivedTreeBuilder;
use crate::source::derive::variance::TraversalParameter;
use crate::source::terms::ElaboratedExpression;
use crate::state::CheckState;
use crate::{ExternalQueries, tree};

pub(super) struct InstantiatedDataType {
    pub(super) type_id: TypeId,
    pub(super) constructor_arguments: Vec<ApplicationArgument>,
}

impl InstantiatedDataType {
    pub(super) fn new<Q>(
        state: &mut CheckState,
        context: &CheckContext<Q>,
        type_id: TypeId,
    ) -> QueryResult<InstantiatedDataType>
    where
        Q: ExternalQueries,
    {
        let (_, constructor_arguments) =
            toolkit::extract_all_applications(state, context, type_id)?;
        Ok(InstantiatedDataType { type_id, constructor_arguments })
    }
}

#[derive(Clone, Copy)]
pub(super) enum TraversalKind {
    Unary,
    Binary,
}

impl TraversalKind {
    pub(super) fn mapping_count(self) -> usize {
        match self {
            TraversalKind::Unary => 1,
            TraversalKind::Binary => 2,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum Mappings<T> {
    Unary(T),
    Binary { first: T, second: T },
}

impl Mappings<TypeId> {
    pub(super) fn split(
        traversal: TraversalKind,
        arguments: &[TypeId],
    ) -> Option<(Mappings<TypeId>, &[TypeId])> {
        match (traversal, arguments) {
            (TraversalKind::Unary, [mapping, rest @ ..]) => Some((Mappings::Unary(*mapping), rest)),
            (TraversalKind::Binary, [first, second, rest @ ..]) => {
                Some((Mappings::Binary { first: *first, second: *second }, rest))
            }
            _ => None,
        }
    }

    pub(super) fn bind<Q>(
        self,
        builder: &mut DerivedTreeBuilder<'_, '_, '_, Q>,
    ) -> (Vec<tree::BinderId>, Mappings<ElaboratedExpression>)
    where
        Q: ExternalQueries,
    {
        match self {
            Mappings::Unary(mapping) => {
                let function = builder.variable_binder("function", mapping);
                let expression = builder.variable(function);
                (vec![function], Mappings::Unary(expression))
            }
            Mappings::Binary { first, second } => {
                let first = builder.variable_binder("firstFunction", first);
                let second = builder.variable_binder("secondFunction", second);
                let expressions = Mappings::Binary {
                    first: builder.variable(first),
                    second: builder.variable(second),
                };
                (vec![first, second], expressions)
            }
        }
    }
}

impl Mappings<ElaboratedExpression> {
    pub(super) fn mapping_for(self, parameter: TraversalParameter) -> Option<ElaboratedExpression> {
        match (self, parameter) {
            (Mappings::Unary(function), TraversalParameter::First) => Some(function),
            (Mappings::Binary { first, .. }, TraversalParameter::First) => Some(first),
            (Mappings::Binary { second, .. }, TraversalParameter::Second) => Some(second),
            (Mappings::Unary(_), TraversalParameter::Second) => None,
        }
    }
}

pub(super) fn extract_record_row<'q, Q>(
    state: &mut CheckState,
    context: &CheckContext<'q, Q>,
    type_id: TypeId,
) -> QueryResult<Option<&'q RowType>>
where
    Q: ExternalQueries,
{
    let Some((_, row)) = toolkit::decompose_type_application(state, context, type_id)? else {
        return Ok(None);
    };
    let row = normalise::expand(state, context, row)?;
    let Type::Row(row) = *context.lookup_type(row) else {
        return Ok(None);
    };
    Ok(Some(context.lookup_row_type(row)))
}
