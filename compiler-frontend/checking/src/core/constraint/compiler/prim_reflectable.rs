use building_types::QueryResult;

use crate::ExternalQueries;
use crate::context::CheckContext;
use crate::core::constraint::matching::MatchInstance;
use crate::core::unification::{CanUnify, can_unify};
use crate::core::{Type, TypeId, normalise};
use crate::error::ErrorKind;
use crate::state::CheckState;

use super::{CompilerMatch, CompilerResolution, extract_integer, extract_symbol};

pub fn match_reflectable<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    arguments: &[TypeId],
) -> QueryResult<Option<CompilerMatch>>
where
    Q: ExternalQueries,
{
    let &[v, t] = arguments else { return Ok(None) };

    let v = normalise::expand(state, context, v)?;

    if let Type::Integer(integer) = *context.lookup_type(v)
        && i32::try_from(context.queries.lookup_integer(integer)).is_err()
    {
        let instance = match_expected(state, context, t, context.prim.int)?;
        let resolution =
            CompilerResolution::Failure(ErrorKind::ReflectedIntegerOutOfRange { integer });
        return Ok(Some(CompilerMatch::from_instance(instance, resolution)));
    }

    let instance = match_reflectable_value(state, context, v, t)?;
    Ok(Some(CompilerMatch::from_instance(instance, CompilerResolution::Synthesized)))
}

fn match_reflectable_value<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    v: TypeId,
    t: TypeId,
) -> QueryResult<MatchInstance>
where
    Q: ExternalQueries,
{
    if extract_symbol(state, context, v)?.is_some() {
        return match_expected(state, context, t, context.prim.string);
    }

    if extract_integer(state, context, v)?.is_some() {
        return match_expected(state, context, t, context.prim.int);
    }

    if v == context.prim_boolean.true_ || v == context.prim_boolean.false_ {
        return match_expected(state, context, t, context.prim.boolean);
    }

    if v == context.prim_ordering.lt
        || v == context.prim_ordering.eq
        || v == context.prim_ordering.gt
    {
        let Some(expected) = context.known_reflectable.ordering else {
            return Ok(MatchInstance::Stuck { stuck: Vec::new(), skolem: false });
        };
        return match_expected(state, context, t, expected);
    }

    if let Type::Unification(id) = *context.lookup_type(v) {
        return Ok(MatchInstance::Stuck { stuck: vec![id], skolem: false });
    }

    Ok(MatchInstance::Apart)
}

fn match_expected<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    actual: TypeId,
    expected: TypeId,
) -> QueryResult<MatchInstance>
where
    Q: ExternalQueries,
{
    Ok(match can_unify(state, context, actual, expected)? {
        CanUnify::Apart => MatchInstance::Apart,
        CanUnify::Equal | CanUnify::Unify => {
            MatchInstance::from_unifications(vec![(actual, expected)])
        }
    })
}
