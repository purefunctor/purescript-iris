//! Implements elabortion for given constraints.

pub mod improvements;

use std::collections::VecDeque;

use building_types::QueryResult;
use itertools::Itertools;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::context::CheckContext;
use crate::core::constraint::canonical::CanonicalConstraint;
use crate::core::constraint::{CanonicalConstraintId, canonical, compiler};
use crate::core::substitute::{NameToType, SubstituteName};
use crate::core::{ApplicationArgument, CheckedClass, Name, Type, TypeId, normalise, toolkit};
use crate::evidence::{Evidence, EvidenceId, Evidences, SuperclassId};
use crate::state::CheckState;
use crate::{ExternalQueries, safe_loop};

pub struct ElaboratedGiven {
    pub given: Vec<(CanonicalConstraintId, EvidenceId)>,
    pub substitution: NameToType,
}

/// Entrypoint for elaborating given [`CanonicalConstraint`].
pub fn elaborate_given<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    given: &[(CanonicalConstraintId, EvidenceId)],
) -> QueryResult<ElaboratedGiven>
where
    Q: ExternalQueries,
{
    let given = elaborate_superclasses_with_evidence(state, context, given)?;
    let given = elaborate_coercible(state, context, given);
    let (given, substitution) = extract_compiler_solved(state, context, given)?;
    Ok(ElaboratedGiven { given, substitution })
}

/// Elaborates superclasses while retaining the dictionary projection path.
pub fn elaborate_superclasses_with_evidence<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    given: &[(CanonicalConstraintId, EvidenceId)],
) -> QueryResult<Vec<(CanonicalConstraintId, EvidenceId)>>
where
    Q: ExternalQueries,
{
    let mut elaborated = Vec::with_capacity(given.len());
    let mut seen = FxHashSet::default();

    for &(constraint, evidence) in given {
        if seen.insert(constraint) {
            elaborated.push((constraint, evidence));
        }
    }

    let constraints = elaborated.iter().map(|&(constraint, _)| constraint);
    let constraints = constraints.collect_vec();
    let edges = superclass_edges(state, context, &constraints)?;

    let evidence_by_constraint = elaborated.iter().copied();
    let mut evidence_by_constraint = evidence_by_constraint.collect::<FxHashMap<_, _>>();

    for edge in edges {
        let parent = evidence_by_constraint[&edge.parent];

        let evidence = Evidence::Superclass { parent, superclass: edge.superclass_id };
        let projection = state.checked.evidence.allocate(evidence);

        evidence_by_constraint.insert(edge.child, projection);
        elaborated.push((edge.child, projection));
    }

    Ok(elaborated)
}

/// Elaborates superclasses from a given [`CanonicalConstraint`].
pub fn elaborate_superclasses<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    given: &[CanonicalConstraintId],
) -> QueryResult<Vec<CanonicalConstraintId>>
where
    Q: ExternalQueries,
{
    let mut elaborated = Vec::with_capacity(given.len());
    let mut seen = FxHashSet::default();

    for &given in given {
        if seen.insert(given) {
            elaborated.push(given);
        }
    }

    let edges = superclass_edges(state, context, &elaborated)?;
    let superclasses = edges.into_iter().map(|edge| edge.child);
    elaborated.extend(superclasses);

    Ok(elaborated)
}

struct SuperclassEdge {
    parent: CanonicalConstraintId,
    child: CanonicalConstraintId,
    superclass_id: SuperclassId,
}

fn superclass_edges<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    roots: &[CanonicalConstraintId],
) -> QueryResult<Vec<SuperclassEdge>>
where
    Q: ExternalQueries,
{
    let mut edges = vec![];
    let mut pending = VecDeque::with_capacity(roots.len());
    let mut seen = FxHashSet::default();

    for &root in roots {
        if seen.insert(root) {
            pending.push_back(root);
        }
    }

    while let Some(parent) = pending.pop_front() {
        let CanonicalConstraint { file_id, type_id, .. } = state.canonicals[parent];
        let Some(class) = toolkit::lookup_file_class(state, context, file_id, type_id)? else {
            continue;
        };

        if class.superclasses.is_empty() {
            continue;
        }

        let CanonicalConstraint { arguments, .. } = &state.canonicals[parent];
        let Some(substitutions) = superclass_substitutions(context, &class, arguments)? else {
            continue;
        };

        for &crate::core::CheckedSuperclass { source_id, constraint } in class.superclasses.iter() {
            let child = SubstituteName::many(state, context, &substitutions, constraint)?;
            if let Some(child) = canonical::canonicalise(state, context, child)?
                && seen.insert(child)
            {
                let superclass_id = SuperclassId { file_id, type_id, source_id };
                edges.push(SuperclassEdge { parent, child, superclass_id });
                pending.push_back(child);
            }
        }
    }

    Ok(edges)
}

pub(crate) fn superclass_substitutions<Q>(
    context: &CheckContext<Q>,
    class: &CheckedClass,
    arguments: &[ApplicationArgument],
) -> QueryResult<Option<NameToType>>
where
    Q: ExternalQueries,
{
    let mut bindings = NameToType::default();
    let mut arguments = arguments.iter().copied();

    for &binder_id in class.kind_binders.iter() {
        let Some(ApplicationArgument::Kind(argument)) = arguments.next() else {
            return Ok(None);
        };
        let binder = context.lookup_forall_binder(binder_id);
        bindings.insert(binder.name, argument);
    }

    for &binder_id in class.type_parameters.iter() {
        let Some(ApplicationArgument::Type(argument)) = arguments.next() else {
            return Ok(None);
        };
        let binder = context.lookup_forall_binder(binder_id);
        bindings.insert(binder.name, argument);
    }

    if arguments.next().is_some() {
        return Ok(None);
    }

    Ok(Some(bindings))
}

/// Elaborates useful [`CanonicalConstraint`] for `Coercible`.
pub fn elaborate_coercible<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    mut given: Vec<(CanonicalConstraintId, EvidenceId)>,
) -> Vec<(CanonicalConstraintId, EvidenceId)>
where
    Q: ExternalQueries,
{
    let constraints = given.iter().map(|&(constraint, _)| constraint);
    let mut seen: FxHashSet<_> = constraints.collect();

    let symmetric = given.iter().filter_map(|&(given, _)| {
        let CanonicalConstraint { file_id, type_id, ref arguments } = state.canonicals[given];

        if (file_id, type_id) != (context.prim_coerce.file_id, context.prim_coerce.coercible) {
            return None;
        }

        let (
            kind @ ApplicationArgument::Kind(_),
            left @ ApplicationArgument::Type(_),
            right @ ApplicationArgument::Type(_),
        ) = arguments.iter().copied().collect_tuple()?
        else {
            return None;
        };

        let arguments = [kind, right, left].into();
        let constraint =
            state.canonicals.intern(CanonicalConstraint { file_id, type_id, arguments });

        if !seen.insert(constraint) {
            return None;
        }

        let evidence = state.checked.evidence.allocate(Evidence::Trivial);
        Some((constraint, evidence))
    });

    let symmetric = symmetric.collect_vec();
    given.extend(symmetric);

    given
}

fn extract_compiler_solved<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    given: Vec<(CanonicalConstraintId, EvidenceId)>,
) -> QueryResult<(Vec<(CanonicalConstraintId, EvidenceId)>, NameToType)>
where
    Q: ExternalQueries,
{
    let mut substitution = NameToType::default();
    let mut conflicts = FxHashSet::default();

    loop {
        let given = given.iter().map(|&(constraint, evidence)| {
            let constraint =
                canonical::substitute_canonical(state, context, &substitution, constraint)?;
            Ok((constraint, evidence))
        });

        let given_evidence = given.collect::<QueryResult<Vec<_>>>()?;

        let given_evidence =
            retain_innermost_given_evidence(&state.checked.evidence, given_evidence);

        let mut changed = false;

        for &(constraint, _) in &given_evidence {
            let given_constraints = given_evidence.iter().map(|&(constraint, _)| constraint);
            let Some(matched) =
                compiler::match_compiler_instance(state, context, constraint, given_constraints)?
            else {
                continue;
            };

            let compiler::CompilerMatch::Match { unifications, .. } = matched else {
                continue;
            };

            for (left, right) in unifications {
                let improvements =
                    extract_improvements(state, context, &substitution, left, right)?;
                for (name, replacement) in improvements {
                    if register_improvement(
                        state,
                        context,
                        &mut substitution,
                        &mut conflicts,
                        name,
                        replacement,
                    )? {
                        changed = true;
                    }
                }
            }
        }

        if !changed {
            return Ok((given_evidence, substitution));
        }
    }
}

fn retain_innermost_given_evidence(
    evidences: &Evidences,
    given: Vec<(CanonicalConstraintId, EvidenceId)>,
) -> Vec<(CanonicalConstraintId, EvidenceId)> {
    let mut retained = Vec::with_capacity(given.len());
    for (constraint, evidence) in given {
        if let Some((_, retained_evidence)) =
            retained.iter_mut().find(|(retained, _)| *retained == constraint)
        {
            let retained_status = &evidences[*retained_evidence];
            let replacement_status = &evidences[evidence];

            let retained_is_trivial = matches!(retained_status, Evidence::Trivial);
            let replacement_is_trivial = matches!(replacement_status, Evidence::Trivial);

            if retained_is_trivial || !replacement_is_trivial {
                *retained_evidence = evidence;
            }
        } else {
            retained.push((constraint, evidence));
        }
    }
    retained
}

fn extract_improvements<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    substitution: &NameToType,
    left: TypeId,
    right: TypeId,
) -> QueryResult<Vec<(Name, TypeId)>>
where
    Q: ExternalQueries,
{
    let left = substitute_type(state, context, substitution, left)?;
    let right = substitute_type(state, context, substitution, right)?;

    let mut improvements = vec![];
    let mut seen = FxHashSet::default();
    improvements::collect_structural_improvements(
        state,
        context,
        left,
        right,
        &mut seen,
        &mut improvements,
    )?;

    Ok(improvements)
}

fn register_improvement<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    substitution: &mut NameToType,
    conflicts: &mut FxHashSet<Name>,
    name: Name,
    replacement: TypeId,
) -> QueryResult<bool>
where
    Q: ExternalQueries,
{
    if conflicts.contains(&name) {
        return Ok(false);
    }

    let replacement = substitute_type(state, context, substitution, replacement)?;
    let replacement = normalise::expand(state, context, replacement)?;

    match *context.lookup_type(replacement) {
        Type::Unification(_) | Type::Unknown(_) => return Ok(false),
        Type::Rigid(replacement, _, _) if replacement == name => return Ok(false),
        _ => {}
    }

    if toolkit::contains_rigid(state, context, replacement, name)? {
        return Ok(false);
    }

    if let Some(current) = substitution.get(&name).copied() {
        let current = substitute_type(state, context, substitution, current)?;
        let current = normalise::expand(state, context, current)?;
        if current == replacement {
            return Ok(false);
        }

        substitution.remove(&name);
        conflicts.insert(name);
        return Ok(true);
    }

    substitution.insert(name, replacement);
    Ok(true)
}

fn substitute_type<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    substitution: &NameToType,
    mut type_id: TypeId,
) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    if substitution.is_empty() {
        return Ok(type_id);
    }

    safe_loop! {
        let substituted = SubstituteName::many(state, context, substitution, type_id)?;
        if substituted == type_id {
            return Ok(type_id);
        }
        type_id = substituted;
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use interner::Id;

    use super::retain_innermost_given_evidence;
    use crate::core::constraint::canonical::CanonicalConstraint;
    use crate::evidence::{Evidence, EvidenceBinderId, Evidences};

    #[test]
    fn deduplicated_givens_retain_innermost_evidence() {
        let first = Id::<CanonicalConstraint>::new(NonZeroU32::new(1).unwrap());
        let second = Id::<CanonicalConstraint>::new(NonZeroU32::new(2).unwrap());

        let mut evidences = Evidences::default();
        let outer = evidences.allocate(Evidence::Given(EvidenceBinderId(0)));
        let unrelated = evidences.allocate(Evidence::Given(EvidenceBinderId(1)));
        let inner = evidences.allocate(Evidence::Given(EvidenceBinderId(2)));

        let given = vec![(first, outer), (second, unrelated), (first, inner)];
        let retained = retain_innermost_given_evidence(&evidences, given);

        assert_eq!(retained, vec![(first, inner), (second, unrelated)]);
    }

    #[test]
    fn symmetric_coercible_evidence_does_not_replace_explicit_given() {
        let constraint = Id::<CanonicalConstraint>::new(NonZeroU32::new(1).unwrap());

        let mut evidences = Evidences::default();
        let explicit = evidences.allocate(Evidence::Given(EvidenceBinderId(0)));
        let generated = evidences.allocate(Evidence::Trivial);

        let given = vec![(constraint, explicit), (constraint, generated)];
        let retained = retain_innermost_given_evidence(&evidences, given);

        assert_eq!(retained, vec![(constraint, explicit)]);
    }
}
