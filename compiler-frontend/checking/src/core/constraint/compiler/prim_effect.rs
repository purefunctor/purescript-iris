use building_types::QueryResult;
use std::sync::Arc;

use crate::ExternalQueries;
use crate::context::CheckContext;
use crate::core::constraint::matching::{self, MatchInstance};
use crate::core::{Type, TypeId, normalise, toolkit};
use crate::error::{EffectOrigin, ErrorCrumb};
use crate::state::CheckState;

#[derive(Clone)]
struct EffectSetView {
    root: TypeId,
    members: Vec<TypeId>,
    tail: Option<TypeId>,
}

pub enum SubsetMatch {
    Instance(MatchInstance),
    Missing { effects: Vec<TypeId>, origins: Vec<EffectOrigin> },
}

pub fn seed_continuation_origin<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    constraint: TypeId,
    crumbs: Arc<[ErrorCrumb]>,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let (constructor, arguments) = toolkit::extract_type_application(state, context, constraint)?;
    let Type::Constructor(file_id, item_id) = *context.lookup_type(constructor) else {
        return Ok(());
    };
    if file_id == context.prim_effect.file_id
        && item_id == context.prim_effect.union
        && let [_, right, _] = arguments.as_slice()
    {
        let expanded = normalise::expand(state, context, *right)?;
        state.insert_effect_channel_origin(*right, Arc::clone(&crumbs));
        state.insert_effect_channel_origin(expanded, Arc::clone(&crumbs));
        let effects = extract_effect_set(state, context, expanded)?;
        for effect in effects.members {
            state.insert_effect_origin(*right, effect, Arc::clone(&crumbs));
            state.insert_effect_origin(expanded, effect, Arc::clone(&crumbs));
        }
    }
    Ok(())
}

fn extract_effect_set<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    effect_set: TypeId,
) -> QueryResult<EffectSetView>
where
    Q: ExternalQueries,
{
    let mut current = normalise::expand(state, context, effect_set)?;
    let root = current;
    let mut members = vec![];

    loop {
        if current == context.prim.effect_nil {
            members.sort_unstable();
            members.dedup();
            return Ok(EffectSetView { root, members, tail: None });
        }

        let Type::Application(function, tail) = *context.lookup_type(current) else {
            members.sort_unstable();
            members.dedup();
            return Ok(EffectSetView { root, members, tail: Some(current) });
        };
        let Type::Application(constructor, member) = *context.lookup_type(function) else {
            members.sort_unstable();
            members.dedup();
            return Ok(EffectSetView { root, members, tail: Some(current) });
        };
        if constructor != context.prim.effect_cons {
            members.sort_unstable();
            members.dedup();
            return Ok(EffectSetView { root, members, tail: Some(current) });
        }

        members.push(super::recursively_normalise(state, context, member)?);
        current = normalise::expand(state, context, tail)?;
    }
}

fn intern_effect_set<Q>(
    context: &CheckContext<Q>,
    members: impl IntoIterator<Item = TypeId>,
    tail: Option<TypeId>,
) -> TypeId
where
    Q: ExternalQueries,
{
    let mut members = members.into_iter().collect::<Vec<_>>();
    members.sort_unstable();
    members.dedup();

    members.into_iter().rev().fold(tail.unwrap_or(context.prim.effect_nil), |effects, effect| {
        let constructor = context.intern_application(context.prim.effect_cons, effect);
        context.intern_application(constructor, effects)
    })
}

pub fn match_union<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    arguments: &[TypeId],
    crumbs: Option<&[ErrorCrumb]>,
) -> QueryResult<Option<MatchInstance>>
where
    Q: ExternalQueries,
{
    let &[left, right, union] = arguments else {
        return Ok(None);
    };

    let left_set = extract_effect_set(state, context, left)?;
    let right_set = extract_effect_set(state, context, right)?;

    let tail = match (left_set.tail, right_set.tail) {
        (None, tail) | (tail, None) => tail,
        (Some(left), Some(right)) if left == right => Some(left),
        (Some(_), Some(_)) => {
            return Ok(Some(matching::blocking_constraint(state, context, &[left, right, union])?));
        }
    };

    let left_members = left_set.members;
    let right_members = right_set.members;
    let result =
        intern_effect_set(context, left_members.iter().chain(&right_members).copied(), tail);

    for &effect in &left_members {
        if let Some(crumbs) = crumbs {
            let origin = state
                .effect_origin(left, effect, crumbs)
                .or_else(|| state.effect_origin(left_set.root, effect, crumbs))
                .unwrap_or_else(|| Arc::from(crumbs));
            state.insert_effect_origin(union, effect, Arc::clone(&origin));
            state.insert_effect_origin(result, effect, origin);
        }
    }
    for effect in right_members {
        if left_members.contains(&effect) {
            continue;
        }
        if crumbs.is_some() {
            let origin = state
                .effect_origin(right, effect, crumbs.unwrap_or_default())
                .or_else(|| state.effect_origin(right_set.root, effect, crumbs.unwrap_or_default()))
                .or_else(|| state.effect_channel_origin(right, crumbs.unwrap_or_default()));
            if let Some(origin) = origin {
                state.insert_effect_origin(union, effect, Arc::clone(&origin));
                state.insert_effect_origin(result, effect, origin);
            }
        }
    }

    Ok(Some(MatchInstance::from_unifications(vec![(union, result)])))
}

pub fn match_remove<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    arguments: &[TypeId],
    crumbs: Option<&[ErrorCrumb]>,
) -> QueryResult<Option<MatchInstance>>
where
    Q: ExternalQueries,
{
    let &[effect, input, output] = arguments else {
        return Ok(None);
    };

    let effect = super::recursively_normalise(state, context, effect)?;
    let mut input_set = extract_effect_set(state, context, input)?;
    let length = input_set.members.len();
    input_set.members.retain(|member| *member != effect);

    if input_set.members.len() == length && input_set.tail.is_some() {
        return Ok(Some(matching::blocking_constraint(state, context, &[effect, input, output])?));
    }

    let input_members = input_set.members;
    let result = intern_effect_set(context, input_members.iter().copied(), input_set.tail);
    for member in input_members {
        if crumbs.is_some() {
            let origin =
                state.effect_origin(input, member, crumbs.unwrap_or_default()).or_else(|| {
                    state.effect_origin(input_set.root, member, crumbs.unwrap_or_default())
                });
            if let Some(origin) = origin {
                state.insert_effect_origin(output, member, Arc::clone(&origin));
                state.insert_effect_origin(result, member, origin);
            }
        }
    }
    Ok(Some(MatchInstance::from_unifications(vec![(output, result)])))
}

pub fn match_subset<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    arguments: &[TypeId],
    crumbs: Option<&[ErrorCrumb]>,
) -> QueryResult<Option<SubsetMatch>>
where
    Q: ExternalQueries,
{
    let &[required, allowed] = arguments else {
        return Ok(None);
    };

    let required_set = extract_effect_set(state, context, required)?;
    let allowed_set = extract_effect_set(state, context, allowed)?;

    if required_set.tail.is_some() {
        let blocked = matching::blocking_constraint(state, context, &[required, allowed])?;
        return Ok(Some(SubsetMatch::Instance(blocked)));
    }

    let missing = required_set
        .members
        .iter()
        .filter(|member| !allowed_set.members.contains(member))
        .copied()
        .collect::<Vec<_>>();

    if missing.is_empty() {
        return Ok(Some(SubsetMatch::Instance(MatchInstance::from_unifications(vec![]))));
    }

    if allowed_set.tail.is_some() {
        let blocked = matching::blocking_constraint(state, context, &[allowed])?;
        return Ok(Some(SubsetMatch::Instance(blocked)));
    }

    let origins = missing
        .iter()
        .filter_map(|effect| {
            crumbs?;
            state
                .effect_origin(required, *effect, crumbs.unwrap_or_default())
                .or_else(|| {
                    state.effect_origin(required_set.root, *effect, crumbs.unwrap_or_default())
                })
                .map(|origin| EffectOrigin { effect: *effect, crumbs: origin })
        })
        .collect();

    Ok(Some(SubsetMatch::Missing { effects: missing, origins }))
}
