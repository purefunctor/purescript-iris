//! Implements generalisation algorithms for the core representation.
//!
//! Simply put, generalisation is an operation that takes some inferred
//! type full of unsolved [unification variables] and replaces them with
//! [universally quantified] [rigid type variables]. For example:
//!
//! ```purescript
//! id :: ?0 -> ?0
//! ```
//!
//! this will generalise into the following:
//!
//! ```purescript
//! id :: forall (t0 :: Type). t0 -> t0
//! ```
//!
//! [unification variables]: crate::core::Type::Unification
//! [universally quantified]: crate::core::Type::Forall
//! [rigid type variables]: crate::core::Type::Rigid

use building_types::QueryResult;
use itertools::Itertools;
use petgraph::algo;
use petgraph::prelude::DiGraphMap;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::context::CheckContext;
use crate::core::constraint::{CanonicalConstraintId, ConstraintInScope, compiler, elaborate};
use crate::core::walk::{TypeWalker, WalkAction, walk_type};
use crate::core::{ForallBinder, Name, Type, TypeFlags, TypeId, normalise, zonk};
use crate::evidence::{Evidence, EvidenceBinderId};
use crate::state::{CheckState, UnificationEntry, UnificationState};
use crate::{ExternalQueries, safe_loop};

type UniGraph = DiGraphMap<u32, ()>;

fn collect_unification_into<Q>(
    graph: &mut UniGraph,
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: TypeId,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    fn aux<Q>(
        graph: &mut UniGraph,
        state: &mut CheckState,
        context: &CheckContext<Q>,
        id: TypeId,
        dependent: Option<u32>,
        visited_kinds: &mut FxHashSet<u32>,
    ) -> QueryResult<()>
    where
        Q: ExternalQueries,
    {
        if !context.lookup_type_flags(id).has_unification() {
            return Ok(());
        }

        let id = normalise::normalise(state, context, id);
        let t = context.lookup_type(id);

        match *t {
            Type::Application(function, argument) | Type::KindApplication(function, argument) => {
                aux(graph, state, context, function, dependent, visited_kinds)?;
                aux(graph, state, context, argument, dependent, visited_kinds)?;
            }
            Type::Forall(binder_id, inner) => {
                let binder = context.lookup_forall_binder(binder_id);
                aux(graph, state, context, binder.kind, dependent, visited_kinds)?;
                aux(graph, state, context, inner, dependent, visited_kinds)?;
            }
            Type::Constrained(constraint, inner) => {
                aux(graph, state, context, constraint, dependent, visited_kinds)?;
                aux(graph, state, context, inner, dependent, visited_kinds)?;
            }
            Type::Function(argument, result) => {
                aux(graph, state, context, argument, dependent, visited_kinds)?;
                aux(graph, state, context, result, dependent, visited_kinds)?;
            }
            Type::Kinded(inner, kind) => {
                aux(graph, state, context, inner, dependent, visited_kinds)?;
                aux(graph, state, context, kind, dependent, visited_kinds)?;
            }
            Type::Row(row_id) => {
                let row = context.lookup_row_type(row_id);
                for field in row.fields.iter() {
                    aux(graph, state, context, field.id, dependent, visited_kinds)?;
                }
                if let Some(tail) = row.tail {
                    aux(graph, state, context, tail, dependent, visited_kinds)?;
                }
            }
            Type::Rigid(_, _, kind) => {
                aux(graph, state, context, kind, dependent, visited_kinds)?;
            }
            Type::Unification(unification_id) => {
                graph.add_node(unification_id);

                if let Some(dependent_id) = dependent {
                    graph.add_edge(dependent_id, unification_id, ());
                }

                if visited_kinds.insert(unification_id) {
                    let entry = state.unifications.get(unification_id);
                    aux(graph, state, context, entry.kind, Some(unification_id), visited_kinds)?;
                }
            }
            Type::Constructor(_, _)
            | Type::Integer(_)
            | Type::String(_, _)
            | Type::Free(_)
            | Type::Unknown(_) => {}
        }

        Ok(())
    }

    let mut visited_kinds = FxHashSet::default();
    aux(graph, state, context, id, None, &mut visited_kinds)
}

/// Collect the unsolved unification variables in a type.
///
/// This function returns the unification variables topologically sorted
/// based on their dependencies, such as when unification variables appear
/// in another unification variable's kind.
pub fn unsolved_unifications<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: TypeId,
) -> QueryResult<Vec<u32>>
where
    Q: ExternalQueries,
{
    let mut graph = UniGraph::new();
    collect_unification_into(&mut graph, state, context, id)?;

    if graph.node_count() == 0 {
        return Ok(vec![]);
    }

    let Ok(unsolved) = algo::toposort(&graph, None) else {
        return Ok(vec![]);
    };

    Ok(unsolved)
}

fn collect_unification<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: TypeId,
) -> QueryResult<UniGraph>
where
    Q: ExternalQueries,
{
    let mut graph = UniGraph::new();
    collect_unification_into(&mut graph, state, context, id)?;
    Ok(graph)
}

/// Generalise a type with the given unification variables.
///
/// The `unsolved` parameter should be sourced from [`unsolved_unifications`].
/// This split is necessary for generalisation on mutually-recursive bindings.
/// Note that while this function expects unsolved unification variables, it
/// also handles solved ones gracefully in the event that they become solved
/// before being generalised.
pub fn generalise_unsolved<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: TypeId,
    unsolved: &[u32],
) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    let generalised = generalise_unsolved_with_parameters(state, context, id, unsolved)?;
    Ok(generalised.type_id)
}

pub struct GeneralisedType {
    pub type_id: TypeId,
    pub parameters: Vec<GeneralisedParameter>,
}

pub struct GeneralisedParameter {
    pub binder: crate::core::ForallBinderId,
    pub rigid: TypeId,
}

pub fn generalise_unsolved_with_parameters<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: TypeId,
    unsolved: &[u32],
) -> QueryResult<GeneralisedType>
where
    Q: ExternalQueries,
{
    if unsolved.is_empty() {
        return Ok(GeneralisedType { type_id: id, parameters: vec![] });
    }

    let mut quantified = id;
    let mut parameters = Vec::with_capacity(unsolved.len());

    // All rigid type variables in a single generalisation share the same
    // depth, one level deeper than the ambient scope. Note that the depth
    // refers to the nesting level of forall scopes with respect to higher
    // rank types, not the number of bindings introduced. For example,
    //
    //   forall a b. a -> b -> a
    //
    // has `a` and `b` on the same depth, whereas,
    //
    //   forall a. (forall r. ST r a) -> a
    //   forall a. a -> (forall b. b -> a)
    //
    // have `r` and `b` one level deeper. Note that the latter example is
    // actually still a Rank-1 type; a forall can be floated trivially when
    // it occurs to the right of the function arrow.
    //
    //   forall a. a -> (forall b. b -> a)
    //   forall a b. a -> b -> a
    //
    // See also: https://wiki.haskell.org/Rank-N_types
    let depth = state.depth.increment();

    for &unification_id in unsolved.iter() {
        let UnificationEntry { kind, state: unification_state, .. } =
            *state.unifications.get(unification_id);

        let (rigid, name, kind) = match unification_state {
            UnificationState::Unsolved => {
                let name = state.names.fresh();
                let rigid = context.intern_rigid(name, depth, kind);
                state.unifications.solve(unification_id, rigid);
                (rigid, name, kind)
            }
            UnificationState::Solved(solution) => {
                let solution = normalise::expand(state, context, solution)?;
                let Type::Rigid(name, _, kind) = *context.lookup_type(solution) else {
                    continue;
                };
                (solution, name, kind)
            }
        };

        let binder = ForallBinder { visible: false, name, kind, scope: None };
        let binder = context.intern_forall_binder(binder);
        quantified = context.intern_forall(binder, quantified);
        parameters.push(GeneralisedParameter { binder, rigid });
    }

    parameters.reverse();
    let type_id = zonk::zonk(state, context, quantified)?;
    Ok(GeneralisedType { type_id, parameters })
}

/// Generalises a given type. See also module-level documentation.
pub fn generalise<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: TypeId,
) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    let unsolved = unsolved_unifications(state, context, id)?;
    generalise_unsolved(state, context, id, &unsolved)
}

#[derive(Default)]
pub struct ConstraintErrors {
    pub ambiguous: Vec<CanonicalConstraintId>,
    pub unsatisfied: Vec<ConstraintInScope>,
}

pub struct ConstrainedByResiduals {
    pub type_id: TypeId,
    pub constraints: Vec<GeneralisedConstraint>,
}

pub fn constrain_using_residuals<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    unconstrained: TypeId,
    mut residuals: Vec<ConstraintInScope>,
    errors: &mut ConstraintErrors,
) -> QueryResult<ConstrainedByResiduals>
where
    Q: ExternalQueries,
{
    if residuals.is_empty() {
        return Ok(ConstrainedByResiduals { type_id: unconstrained, constraints: vec![] });
    }

    for residual in residuals.iter_mut() {
        *residual = residual.zonk(state, context)?;
    }

    let (residuals, partial) = prune_partial(state, context, residuals);
    let residuals = prune_unsatisfied(state, context, residuals, errors)?;
    let residuals = prune_ambiguous(state, context, unconstrained, residuals, errors)?;

    let residuals = partial.into_iter().chain(residuals);
    let generalised = residuals.sorted_by_key(|constraint| constraint.key.wanted).collect_vec();
    let constraints = finalise_generalised_constraints(state, context, generalised)?;
    let constrained = constraints.iter().rfold(unconstrained, |inner, constraint| {
        context.intern_constrained(constraint.constraint, inner)
    });

    Ok(ConstrainedByResiduals { type_id: constrained, constraints })
}

type PrunedPartial = (Vec<ConstraintInScope>, Option<ConstraintInScope>);

fn prune_partial<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    pending: Vec<ConstraintInScope>,
) -> PrunedPartial
where
    Q: ExternalQueries,
{
    let mut residuals = vec![];
    let mut partial: Option<ConstraintInScope> = None;

    for constraint in pending {
        if constraint.is_partial(state, context) {
            if let Some(existing) = &partial {
                let residual = constraint.evidence.wanted;
                let existing = existing.evidence.wanted;
                state.checked.evidence.merge_duplicate(residual, existing);
            } else {
                partial = Some(constraint);
            }
        } else {
            residuals.push(constraint);
        }
    }

    (residuals, partial)
}

type PrunedUnsatisfied = Vec<(ConstraintInScope, FxHashSet<u32>)>;

fn prune_unsatisfied<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    constraints: Vec<ConstraintInScope>,
    errors: &mut ConstraintErrors,
) -> QueryResult<PrunedUnsatisfied>
where
    Q: ExternalQueries,
{
    let mut residuals = vec![];

    for constraint in constraints {
        let canonical = state.canonicals.type_id(context, constraint.key.wanted);

        let unification: FxHashSet<u32> =
            collect_unification(state, context, canonical)?.nodes().collect();

        // A residual without unification variables is unsatisfied.
        // Mark its evidence as an error and queue a diagnostic.
        if unification.is_empty() {
            state.checked.evidence.mark_error(constraint.evidence.wanted);
            errors.unsatisfied.push(constraint);
        } else {
            residuals.push((constraint, unification));
        }
    }

    Ok(residuals)
}

type PrunedAmbiguous = Vec<ConstraintInScope>;

fn prune_ambiguous<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    unconstrained: TypeId,
    pending: Vec<(ConstraintInScope, FxHashSet<u32>)>,
    errors: &mut ConstraintErrors,
) -> QueryResult<PrunedAmbiguous>
where
    Q: ExternalQueries,
{
    type ConstraintMap = FxHashMap<CanonicalConstraintId, ConstraintInScope>;

    let mut reachable_variables: FxHashSet<u32> =
        collect_unification(state, context, unconstrained)?.nodes().collect();

    let mut constraints = ConstraintMap::default();
    let mut current = pending;

    safe_loop! {
        // A constraint is reachable if it contains unfiication variables
        // in the type being constrained, otherwise, it is unreachable.
        let (reachable, unreachable): (Vec<_>, Vec<_>) =
            current.into_iter().partition(|(_, unification)| {
                unification.iter().any(|variable| reachable_variables.contains(variable))
            });

        // If there are no more reachable constraints, mark the evidence
        // of unreachable constraints errors and queue the diagnostics.
        if reachable.is_empty() {
            for (constraint, _) in unreachable {
                state.checked.evidence.mark_error(constraint.evidence.wanted);
                errors.ambiguous.push(constraint.key.wanted);
            }
            break;
        }

        // Reachable constraints are recorded, and duplicate evidences
        // are merged. Their unification variables extend the current
        // unification variable graph. This extension enables transitive
        // solving such as in `F ?a ?b => G ?b ?c => ?a`, where `?b`
        // becomes reachable via `?a`.
        for (constraint, unification) in reachable {
            if let Some(existing) = constraints.get(&constraint.key.wanted) {
                let constraint = constraint.evidence.wanted;
                let existing = existing.evidence.wanted;
                state.checked.evidence.merge_duplicate(constraint, existing);
            } else {
                constraints.insert(constraint.key.wanted, constraint);
            }
            reachable_variables.extend(unification);
        }

        current = unreachable;
    }

    Ok(constraints.into_values().collect())
}

pub struct GeneralisedConstraint {
    pub constraint: TypeId,
    pub binder: EvidenceBinderId,
}

fn finalise_generalised_constraints<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    constraints: Vec<ConstraintInScope>,
) -> QueryResult<Vec<GeneralisedConstraint>>
where
    Q: ExternalQueries,
{
    let mut generalisable = vec![];

    for constraint in constraints {
        if compiler::is_fail_constraint(state, context, constraint.key.wanted) {
            state.checked.evidence.mark_error(constraint.evidence.wanted);
        } else {
            generalisable.push(constraint)
        }
    }

    let MinimisedBySuperclasses { retained, dropped } =
        minimise_by_superclasses(state, context, generalisable)?;

    let mut evidences = vec![];
    let mut generalised = vec![];
    for constraint in &retained {
        let ConstraintInScope { key, evidence } = constraint;
        let canonical = state.canonicals.type_id(context, key.wanted);
        // These binders belong to the declaration, not the surrounding implication.
        let binder = state.checked.evidence.fresh_binder(canonical);
        let proof = state.checked.evidence.allocate(Evidence::Given(binder));
        state.checked.evidence.solve(evidence.wanted, proof);

        evidences.push((key.wanted, proof));
        generalised.push(GeneralisedConstraint { constraint: canonical, binder });
    }

    let projections = elaborate::elaborate_superclasses_with_evidence(state, context, &evidences)?;
    let projections = projections.into_iter().collect::<FxHashMap<_, _>>();

    for constraint in dropped {
        if let Some(evidence) = projections.get(&constraint.key.wanted) {
            state.checked.evidence.solve(constraint.evidence.wanted, *evidence);
        } else {
            state.checked.evidence.mark_error(constraint.evidence.wanted);
        }
    }

    Ok(generalised)
}

struct MinimisedBySuperclasses {
    retained: Vec<ConstraintInScope>,
    dropped: Vec<ConstraintInScope>,
}

fn minimise_by_superclasses<Q>(
    state: &mut CheckState,
    context: &CheckContext<'_, Q>,
    constraints: Vec<ConstraintInScope>,
) -> QueryResult<MinimisedBySuperclasses>
where
    Q: ExternalQueries,
{
    let mut superclasses = FxHashSet::default();

    for constraint in &constraints {
        for superclass in
            elaborate::elaborate_superclasses(state, context, &[constraint.key.wanted])?
        {
            if superclass != constraint.key.wanted {
                superclasses.insert(superclass);
            }
        }
    }

    let (retained, dropped): (Vec<_>, Vec<_>) = constraints
        .into_iter()
        .partition(|constraint| !superclasses.contains(&constraint.key.wanted));

    Ok(MinimisedBySuperclasses { retained, dropped })
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum ImplicitOrUnification {
    Implicit(Name, TypeId),
    Unification(u32, TypeId),
}

#[derive(Default)]
struct GeneraliseImplicit {
    owner: Option<ImplicitOrUnification>,
    graph: DiGraphMap<ImplicitOrUnification, ()>,
    bound: FxHashSet<Name>,
}

impl TypeWalker for GeneraliseImplicit {
    fn visit<Q>(
        &mut self,
        state: &mut CheckState,
        context: &CheckContext<Q>,
        _id: TypeId,
        t: &Type,
    ) -> QueryResult<WalkAction>
    where
        Q: ExternalQueries,
    {
        match t {
            Type::Rigid(name, _, kind) => {
                let next_owner = ImplicitOrUnification::Implicit(*name, *kind);
                let prev_owner = self.owner.replace(next_owner);

                self.graph.add_node(next_owner);
                if let Some(prev_owner) = prev_owner {
                    self.graph.add_edge(prev_owner, next_owner, ());
                }

                walk_type(state, context, *kind, self)?;
                self.owner = prev_owner;
            }
            Type::Unification(id) => {
                let UnificationEntry { kind, .. } = state.unifications.get(*id);

                let next_owner = ImplicitOrUnification::Unification(*id, *kind);
                let prev_owner = self.owner.replace(next_owner);

                self.graph.add_node(next_owner);
                if let Some(prev_owner) = prev_owner {
                    self.graph.add_edge(prev_owner, next_owner, ());
                }

                walk_type(state, context, *kind, self)?;
                self.owner = prev_owner;
            }
            _ => {}
        }
        Ok(WalkAction::Continue)
    }

    fn visit_binder(&mut self, binder: &ForallBinder) {
        self.bound.insert(binder.name);
    }

    fn may_visit(&self, flags: TypeFlags) -> bool {
        flags.has_variables()
    }
}

pub fn generalise_implicit<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: TypeId,
) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    let mut walker = GeneraliseImplicit::default();
    walk_type(state, context, id, &mut walker)?;

    let Ok(implicits_unifications) = algo::toposort(&walker.graph, None) else {
        return Ok(context.unknown("invalid recursive graph"));
    };

    let depth = state.depth.increment();

    let mut binders = vec![];
    for implicit_unification in implicits_unifications {
        match implicit_unification {
            ImplicitOrUnification::Implicit(name, kind) => {
                binders.push(ForallBinder { visible: false, name, kind, scope: None })
            }
            ImplicitOrUnification::Unification(id, kind) => {
                let name = state.names.fresh();
                let rigid = context.intern_rigid(name, depth, kind);
                state.unifications.solve(id, rigid);
                binders.push(ForallBinder { visible: false, name, kind, scope: None })
            }
        }
    }

    let id = binders.into_iter().fold(id, |inner, binder| {
        let binder = context.intern_forall_binder(binder);
        context.intern_forall(binder, inner)
    });

    zonk::zonk(state, context, id)
}
