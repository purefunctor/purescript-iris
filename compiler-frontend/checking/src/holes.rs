use std::sync::Arc;

use building_types::QueryResult;
use rustc_hash::FxHashSet;
use smol_str::SmolStr;

use crate::ExternalQueries;
use crate::context::CheckContext;
use crate::core::constraint::matching;
use crate::core::constraint::matching::MatchType;
use crate::core::{TypeId, toolkit};
use crate::state::CheckState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoleBinding {
    pub name: SmolStr,
    pub type_id: TypeId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TermHole {
    pub type_id: TypeId,
    pub bindings: Arc<[HoleBinding]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeHole {
    pub type_id: TypeId,
    pub kind_id: TypeId,
    pub bindings: Arc<[HoleBinding]>,
}

pub fn refine_bindings<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    expected: TypeId,
    mut bindings: Vec<HoleBinding>,
) -> QueryResult<Arc<[HoleBinding]>>
where
    Q: ExternalQueries,
{
    bindings.sort_by(|left, right| left.name.cmp(&right.name));
    let expected = toolkit::without_constraints(state, context, expected)?;

    let mut relevant = Vec::new();
    for binding in &bindings {
        if binding_matches(state, context, binding.type_id, expected)? {
            relevant.push(HoleBinding::clone(binding));
        }
    }

    let selected = if relevant.is_empty() { bindings } else { relevant };
    Ok(Arc::from(selected))
}

fn binding_matches<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    candidate: TypeId,
    expected: TypeId,
) -> QueryResult<bool>
where
    Q: ExternalQueries,
{
    let inspected = toolkit::inspect_quantified(state, context, candidate)?;
    let candidate = toolkit::without_constraints(state, context, inspected.quantified)?;

    let pattern = inspected.binders.iter().map(|binder| binder.name);
    let pattern = pattern.collect::<FxHashSet<_>>();

    let matched = matching::types_match(state, context, &pattern, expected, candidate)?;
    let MatchType::Match { bindings } = matched else { return Ok(false) };

    Ok(matching::verify_substitution(state, context, bindings)?.is_match())
}
