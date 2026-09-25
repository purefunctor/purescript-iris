//! Implements the constraint solver for PureScript.
//!
//! The [`solve_implication`] function is the entrypoint for solving the root
//! [`Implication`]; the [`solve_implication_id`] function solves a single
//! [`Implication`] with respect to inheritance; and the [`solve_constraints`]
//! function implements the equality-driven constraint solver.
//!
//! [`Implication`]: crate::implication::Implication

pub mod canonical;
pub mod compiler;
pub mod elaborate;
pub mod instances;
pub mod matching;

pub use canonical::{CanonicalConstraint, CanonicalConstraintId, Canonicals};
use itertools::Itertools;

use std::collections::VecDeque;
use std::collections::hash_map::Entry;
use std::mem;
use std::num::NonZeroU32;
use std::rc::Rc;

use building_types::QueryResult;
use indexmap::IndexMap;
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};

use crate::context::CheckContext;
use crate::core::fd::{compute_closure, get_functional_dependencies};
use crate::core::{ApplicationArgument, TypeId, unification};
use crate::error::{CheckingError, ErrorKind};
use crate::evidence::{Evidence, EvidenceId, EvidenceVarId};
use crate::implication::{GivenConstraint, ImplicationId, Patterns, WantedConstraint};
use crate::state::{CheckState, UnificationState};
use crate::{ExternalQueries, Type};

pub fn solve_implication<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
) -> QueryResult<Vec<ConstraintInScope>>
where
    Q: ExternalQueries,
{
    let implication = state.implications.current();
    let constraints = collect_constraints(state, context, implication)?;
    solve_constraints(state, context, constraints)
}

pub struct Work {
    unifications: Vec<(TypeId, TypeId)>,
    unclassified: VecDeque<ConstraintInScope>,
    constraints: VecDeque<ConstraintInScope>,
}

impl Work {
    fn new(unclassified: VecDeque<ConstraintInScope>) -> Work {
        Work { unifications: vec![], unclassified, constraints: VecDeque::default() }
    }

    fn extend_from_parts<Unifications, Constraints>(
        &mut self,
        unifications: Unifications,
        constraints: Constraints,
    ) where
        Unifications: IntoIterator<Item = (TypeId, TypeId)>,
        Constraints: IntoIterator<Item = ConstraintInScope>,
    {
        self.unifications.extend(unifications);
        self.extend_constraints(constraints);
    }

    fn extend_constraints<Constraints>(&mut self, constraints: Constraints)
    where
        Constraints: IntoIterator<Item = ConstraintInScope>,
    {
        self.unclassified.extend(constraints);
    }

    fn pop_next<Q>(
        &mut self,
        state: &mut CheckState,
        context: &CheckContext<Q>,
    ) -> QueryResult<Option<ConstraintInScope>>
    where
        Q: ExternalQueries,
    {
        while let Some(constraint) = self.unclassified.pop_front() {
            if is_improving_constraint(state, context, constraint.key.wanted)? {
                return Ok(Some(constraint));
            }

            self.constraints.push_back(constraint);
        }

        Ok(self.constraints.pop_front())
    }
}

type Stuck = FxHashMap<u32, Vec<ConstraintInScope>>;
type ConstraintSet = IndexMap<ConstraintKey, ConstraintEvidence, FxBuildHasher>;
type Skolem = Vec<ConstraintInScope>;

fn fresh_scoped_constraints(
    state: &mut CheckState,
    parent: &ConstraintInScope,
    constraints: Vec<CanonicalConstraintId>,
) -> Vec<ConstraintInScope> {
    let constraints = constraints.into_iter().map(|wanted| {
        let wanted_evidence = state.checked.evidence.fresh_variable();
        let given = Rc::clone(&parent.key.given);
        let given_evidence = Rc::clone(&parent.evidence.given);
        ConstraintInScope::new(parent.key.scope, given, wanted, given_evidence, wanted_evidence)
    });
    constraints.collect()
}

fn insert_unique_constraint(
    state: &mut CheckState,
    constraints: &mut ConstraintSet,
    constraint: ConstraintInScope,
) {
    let ConstraintInScope { key, evidence } = constraint;
    if let Some(existing) = constraints.get(&key) {
        state.checked.evidence.merge_duplicate(evidence.wanted, existing.wanted);
    } else {
        constraints.insert(key, evidence);
    }
}

fn is_improving_constraint<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    constraint: CanonicalConstraintId,
) -> QueryResult<bool>
where
    Q: ExternalQueries,
{
    let CanonicalConstraint { file_id, type_id, arguments } = state.canonicals[constraint].clone();
    let functional_dependencies = get_functional_dependencies(state, context, file_id, type_id)?;

    if functional_dependencies.is_empty() {
        return Ok(false);
    }

    let arguments = arguments.iter().filter_map(|argument| {
        if let ApplicationArgument::Type(argument) = argument { Some(*argument) } else { None }
    });

    let arguments = arguments.collect_vec();

    let mut known_positions = FxHashSet::default();
    let mut blocking_by_position = vec![];

    for (position, &argument) in arguments.iter().enumerate() {
        let blocking = matching::collect_blocking(state, context, &[argument])?;
        if blocking.is_empty() {
            known_positions.insert(position);
        }
        blocking_by_position.push(blocking);
    }

    let closure = compute_closure(&functional_dependencies, &known_positions);
    for position in closure.difference(&known_positions) {
        if blocking_by_position.get(*position).is_some_and(|blocking| !blocking.is_empty()) {
            return Ok(true);
        }
    }

    Ok(false)
}

fn wake_constraints(work: &mut Work, stuck: &mut Stuck, state: &mut CheckState) {
    let mut awake = ConstraintSet::default();

    stuck.retain(|&id, constraints| {
        if let UnificationState::Solved(_) = state.unifications.get(id).state {
            for constraint in constraints.iter().cloned() {
                insert_unique_constraint(state, &mut awake, constraint);
            }
            false
        } else {
            true
        }
    });

    if awake.is_empty() {
        return;
    }

    // For each constraint in the constraint set;
    for constraints in stuck.values_mut() {
        // keep only the constraints that are not awake;
        constraints.retain(|constraint| {
            if let Some(existing) = awake.get(&constraint.key) {
                state.checked.evidence.merge_duplicate(constraint.evidence.wanted, existing.wanted);
                false
            } else {
                true
            }
        });
    }

    // and keep only the non-empty constraint sets.
    stuck.retain(|_, constraints| !constraints.is_empty());

    let awake = awake.into_iter().map(ConstraintInScope::from_parts);
    work.extend_constraints(awake);
}

fn solve_constraints<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    constraints: VecDeque<ConstraintInScope>,
) -> QueryResult<Vec<ConstraintInScope>>
where
    Q: ExternalQueries,
{
    let mut work = Work::new(constraints);
    let mut stuck = Stuck::default();
    let mut skolem = Skolem::default();
    let mut residuals = vec![];

    'work: loop {
        let mut has_unification = false;
        for (t1, t2) in mem::take(&mut work.unifications) {
            has_unification |= unification::unify(state, context, t1, t2)?;
        }

        if has_unification {
            wake_constraints(&mut work, &mut stuck, state);
            work.extend_constraints(mem::take(&mut skolem));
        }

        let Some(constraint) = work.pop_next(state, context)? else {
            break 'work;
        };

        let mut blocked = FxHashSet::default();
        let mut blocked_on_skolem = false;

        for (provided, provided_evidence) in constraint.given() {
            match matching::match_provided(state, context, constraint.key.wanted, provided)? {
                matching::MatchInstance::Match { unifications, constraints } => {
                    state.checked.evidence.solve(constraint.evidence.wanted, provided_evidence);
                    let constraints = fresh_scoped_constraints(state, &constraint, constraints);
                    work.extend_from_parts(unifications, constraints);
                    continue 'work;
                }
                matching::MatchInstance::Stuck { stuck, skolem } => {
                    blocked.extend(stuck);
                    blocked_on_skolem |= skolem;
                }
                matching::MatchInstance::Apart => (),
            }
        }

        let given = constraint.key.given.iter().copied();
        match compiler::match_compiler_instance(state, context, constraint.key.wanted, given)? {
            Some(compiler::CompilerMatch::Match { unifications, constraints, resolution }) => {
                let solve_with_evidence = |state: &mut CheckState, evidence: Evidence| {
                    let evidence = state.checked.evidence.allocate(evidence);
                    state.checked.evidence.solve(constraint.evidence.wanted, evidence);
                };

                match resolution {
                    compiler::CompilerResolution::Trivial => {
                        solve_with_evidence(state, Evidence::Trivial);
                    }
                    compiler::CompilerResolution::Synthesized => {
                        let evidence = compiler::synthesized_evidence_for_constraint(
                            state,
                            context,
                            constraint.key.wanted,
                        )?;
                        solve_with_evidence(state, Evidence::Synthesized(evidence));
                    }
                    compiler::CompilerResolution::Warning { message_id } => {
                        state.insert_error(ErrorKind::CustomWarning { message_id });
                        solve_with_evidence(state, Evidence::Trivial);
                    }
                    compiler::CompilerResolution::Failure { message_id } => {
                        state.insert_error(ErrorKind::CustomFailure { message_id });
                        state.checked.evidence.mark_error(constraint.evidence.wanted);
                    }
                }

                let constraints = fresh_scoped_constraints(state, &constraint, constraints);
                work.extend_from_parts(unifications, constraints);
                continue 'work;
            }
            Some(compiler::CompilerMatch::Stuck { stuck, skolem }) => {
                blocked.extend(stuck);
                blocked_on_skolem |= skolem;
            }
            Some(compiler::CompilerMatch::Apart) | None => (),
        }

        let search = instances::collect_instance_chains(state, context, constraint.key.wanted)?;
        'chain: for chain in search.chains() {
            for &candidate in chain {
                match matching::match_declared(state, context, constraint.key.wanted, candidate)? {
                    matching::MatchInstance::Match { unifications, constraints } => {
                        let constraints = fresh_scoped_constraints(state, &constraint, constraints);
                        let subgoals = constraints
                            .iter()
                            .map(|constraint| constraint.evidence.wanted)
                            .collect();
                        let evidence = state
                            .checked
                            .evidence
                            .allocate(Evidence::Instance { origin: candidate.origin, subgoals });
                        state.checked.evidence.solve(constraint.evidence.wanted, evidence);
                        work.extend_from_parts(unifications, constraints);
                        continue 'work;
                    }
                    matching::MatchInstance::Stuck { stuck, skolem } => {
                        blocked.extend(stuck);
                        blocked_on_skolem |= skolem;
                        continue 'chain;
                    }
                    matching::MatchInstance::Apart => (),
                }
            }
        }

        // If no candidate matched, the candidate search itself may also be incomplete
        // due to unsolved unification variables; we will wait for them to be solved.
        blocked.extend(search.blocking);

        if blocked.is_empty() {
            if blocked_on_skolem {
                skolem.push(constraint);
            } else {
                residuals.push(constraint);
            }
        } else {
            for id in blocked {
                let constraint = ConstraintInScope::clone(&constraint);
                stuck.entry(id).or_default().push(constraint);
            }
        }
    }

    let mut unique_residuals = ConstraintSet::default();
    for residual in residuals {
        insert_unique_constraint(state, &mut unique_residuals, residual);
    }

    for (_, constraints) in stuck {
        for constraint in constraints {
            insert_unique_constraint(state, &mut unique_residuals, constraint);
        }
    }
    for constraint in skolem {
        insert_unique_constraint(state, &mut unique_residuals, constraint);
    }

    let unique_residuals = unique_residuals.into_iter().map(ConstraintInScope::from_parts);
    Ok(unique_residuals.collect())
}

#[derive(Clone)]
struct EvidenceInScope {
    constraints: Vec<(CanonicalConstraintId, EvidenceId)>,
    positions: FxHashMap<CanonicalConstraintId, usize>,
    evidence_scope: Option<NonZeroU32>,
}

impl EvidenceInScope {
    fn new(root: ImplicationId) -> EvidenceInScope {
        let mut evidence = EvidenceInScope {
            constraints: vec![],
            positions: FxHashMap::default(),
            evidence_scope: None,
        };
        evidence.assign_scope(root);
        evidence
    }

    fn assign_scope(&mut self, scope: ImplicationId) {
        let scope = scope
            .checked_add(1)
            .and_then(NonZeroU32::new)
            .expect("invariant violated: evidence scope overflow");
        self.evidence_scope = Some(scope);
    }

    fn scope(&self) -> ImplicationId {
        let scope = self.evidence_scope.expect("invariant violated: evidence scope not assigned");
        scope.get() - 1
    }

    fn insert(&mut self, constraint: CanonicalConstraintId, evidence: EvidenceId) {
        match self.positions.entry(constraint) {
            Entry::Occupied(position) => {
                self.constraints[*position.get()].1 = evidence;
            }
            Entry::Vacant(position) => {
                position.insert(self.constraints.len());
                self.constraints.push((constraint, evidence));
            }
        }
    }

    fn contains(&self, constraint: CanonicalConstraintId) -> bool {
        self.positions.contains_key(&constraint)
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ConstraintKey {
    pub scope: ImplicationId,
    pub given: Rc<[CanonicalConstraintId]>,
    pub wanted: CanonicalConstraintId,
}

#[derive(Clone)]
pub struct ConstraintEvidence {
    pub given: Rc<[EvidenceId]>,
    pub wanted: EvidenceVarId,
}

#[derive(Clone)]
pub struct ConstraintInScope {
    pub key: ConstraintKey,
    pub evidence: ConstraintEvidence,
}

impl ConstraintInScope {
    fn new(
        scope: ImplicationId,
        given: Rc<[CanonicalConstraintId]>,
        wanted: CanonicalConstraintId,
        given_evidence: Rc<[EvidenceId]>,
        wanted_evidence: EvidenceVarId,
    ) -> ConstraintInScope {
        assert_eq!(
            given.len(),
            given_evidence.len(),
            "critical violation: given constraints and evidence must correspond",
        );
        let key = ConstraintKey { scope, given, wanted };
        let evidence = ConstraintEvidence { given: given_evidence, wanted: wanted_evidence };
        ConstraintInScope { key, evidence }
    }

    fn from_parts((key, evidence): (ConstraintKey, ConstraintEvidence)) -> ConstraintInScope {
        assert_eq!(
            key.given.len(),
            evidence.given.len(),
            "critical violation: given constraints and evidence must correspond",
        );
        ConstraintInScope { key, evidence }
    }

    pub fn given(&self) -> impl Iterator<Item = (CanonicalConstraintId, EvidenceId)> + '_ {
        assert_eq!(
            self.key.given.len(),
            self.evidence.given.len(),
            "critical violation: given constraints and evidence must correspond",
        );
        std::iter::zip(self.key.given.iter().copied(), self.evidence.given.iter().copied())
    }

    pub fn zonk<Q>(
        &self,
        state: &mut CheckState,
        context: &CheckContext<Q>,
    ) -> QueryResult<ConstraintInScope>
    where
        Q: ExternalQueries,
    {
        let given =
            self.key.given.iter().map(|&given| canonical::zonk_canonical(state, context, given));

        let given = given.collect::<QueryResult<Rc<[_]>>>()?;
        let wanted = canonical::zonk_canonical(state, context, self.key.wanted)?;

        let given_evidence = Rc::clone(&self.evidence.given);
        Ok(ConstraintInScope::new(
            self.key.scope,
            given,
            wanted,
            given_evidence,
            self.evidence.wanted,
        ))
    }

    pub fn canonical<Q>(&self, state: &CheckState, context: &CheckContext<Q>) -> TypeId
    where
        Q: ExternalQueries,
    {
        state.canonicals.type_id(context, self.key.wanted)
    }

    pub fn is_partial<Q>(&self, state: &CheckState, context: &CheckContext<Q>) -> bool
    where
        Q: ExternalQueries,
    {
        let Type::Constructor(file_id, type_id) = *context.lookup_type(context.prim.partial) else {
            unreachable!("critical violation: Partial is not Partial");
        };
        let constraint = &state.canonicals[self.key.wanted];
        constraint.file_id == file_id
            && constraint.type_id == type_id
            && constraint.arguments.is_empty()
    }
}

fn collect_constraints<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    root: ImplicationId,
) -> QueryResult<VecDeque<ConstraintInScope>>
where
    Q: ExternalQueries,
{
    let partial = canonical::canonicalise(state, context, context.prim.partial)?;

    let mut constraints = VecDeque::default();
    // Children share their parent's evidence until they introduce givens.
    let mut stack = vec![(root, Rc::new(EvidenceInScope::new(root)))];

    while let Some((implication_id, mut evidence_in_scope)) = stack.pop() {
        let (given, wanted, children, patterns) = {
            let implication = &mut state.implications[implication_id];
            (
                mem::take(&mut implication.given),
                mem::take(&mut implication.wanted),
                mem::take(&mut implication.children),
                mem::take(&mut implication.patterns),
            )
        };

        let mut introduced_binders = vec![];
        for GivenConstraint { constraint, evidence } in given {
            if let Some(given) = canonical::canonicalise(state, context, constraint)? {
                introduced_binders.push((given, evidence));

                let canonical = state.canonicals.type_id(context, given);
                state.checked.evidence.bind_binder(evidence, canonical);

                let proof = state.checked.evidence.allocate(Evidence::Given(evidence));
                Rc::make_mut(&mut evidence_in_scope).insert(given, proof);
            }
        }

        if !introduced_binders.is_empty() {
            Rc::make_mut(&mut evidence_in_scope).assign_scope(implication_id);
        }

        let elide_missing_patterns =
            partial.is_some_and(|partial| evidence_in_scope.contains(partial));

        if !elide_missing_patterns {
            for Patterns { patterns, crumbs } in patterns {
                let kind = ErrorKind::MissingPatterns { patterns };
                state.checked.errors.push(CheckingError { kind, crumbs });
            }
        }

        if !wanted.is_empty() {
            let elaborate::ElaboratedGiven { given, substitution } =
                elaborate::elaborate_given(state, context, &evidence_in_scope.constraints)?;

            for (constraint, binder) in introduced_binders {
                let constraint =
                    canonical::substitute_canonical(state, context, &substitution, constraint)?;
                let canonical = state.canonicals.type_id(context, constraint);
                state.checked.evidence.bind_binder(binder, canonical);
            }

            for &(constraint, evidence) in &given {
                let binder = match &state.checked.evidence[evidence] {
                    Evidence::Given(binder) => Some(*binder),
                    _ => None,
                };
                if let Some(binder) = binder {
                    let canonical = state.canonicals.type_id(context, constraint);
                    state.checked.evidence.bind_binder(binder, canonical);
                }
            }

            let (given, given_evidence): (Vec<_>, Vec<_>) = given.into_iter().unzip();
            let given: Rc<[CanonicalConstraintId]> = Rc::from(given);
            let given_evidence: Rc<[EvidenceId]> = Rc::from(given_evidence);

            for WantedConstraint { constraint, evidence: wanted_evidence } in wanted {
                if let Some(wanted) = canonical::canonicalise(state, context, constraint)? {
                    let given = Rc::clone(&given);
                    let given_evidence = Rc::clone(&given_evidence);
                    let wanted =
                        canonical::substitute_canonical(state, context, &substitution, wanted)?;
                    constraints.push_back(ConstraintInScope::new(
                        evidence_in_scope.scope(),
                        given,
                        wanted,
                        given_evidence,
                        wanted_evidence,
                    ));
                } else {
                    state.checked.evidence.mark_error(wanted_evidence);
                }
            }
        }

        let children =
            children.into_iter().rev().map(|child| (child, Rc::clone(&evidence_in_scope)));

        stack.extend(children)
    }

    Ok(constraints)
}
