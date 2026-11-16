use std::mem;
use std::sync::Arc;

use building_types::QueryResult;
use files::FileId;
use indexing::InstanceChainId;
use itertools::Itertools;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::context::{CheckContext, Item};
use crate::core::{builtin, fold, kind, normalise, unify};
use crate::error::ErrorKind;
use crate::state::{CheckState, Given, Wanted};
use crate::tree::Evidence;
use crate::types::{Name, Type, TypeId};
use crate::{CheckedClass, CheckedInstance, ExternalQueries, FunctionalDependency};

pub const WORK_LIMIT: u32 = 1000;
pub const COERCIBLE_STEP_LIMIT: u32 = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Match {
    Match,
    Unknown,
    Apart,
}

impl Match {
    fn and(self, other: Match) -> Match {
        self.max(other)
    }
}

pub enum Outcome {
    Solved,
    Improved,
    Stuck,
    Failed(ErrorKind),
}

#[derive(Debug, Clone)]
pub struct ConstraintHead {
    pub class: Item,
    pub kind_arguments: Vec<TypeId>,
    pub arguments: Vec<TypeId>,
}

pub fn constraint_head<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    constraint: TypeId,
) -> Option<ConstraintHead>
where
    Q: ExternalQueries,
{
    let mut arguments = Vec::new();
    let mut kind_arguments = Vec::new();
    let mut current = normalise::normalise(state, context, constraint);
    loop {
        match context.lookup_type(current) {
            Type::Application(function, argument) => {
                arguments.push(argument);
                current = normalise::normalise(state, context, function);
            }
            Type::KindApplication(function, argument) => {
                kind_arguments.push(argument);
                current = normalise::normalise(state, context, function);
            }
            Type::Constructor(file_id, type_id) => {
                arguments.reverse();
                kind_arguments.reverse();
                let class = (file_id, type_id);
                return Some(ConstraintHead { class, kind_arguments, arguments });
            }
            _ => return None,
        }
    }
}

pub fn lookup_class<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    (file_id, type_id): Item,
) -> QueryResult<Option<CheckedClass>>
where
    Q: ExternalQueries,
{
    if file_id == context.id {
        return Ok(state.checked.classes.get(&type_id).cloned());
    }
    let checked = context.checked_dependency(file_id)?;
    Ok(checked.classes.get(&type_id).cloned())
}

pub fn solve_wanteds<Q>(state: &mut CheckState, context: &CheckContext<Q>) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let mut failed = FxHashSet::default();
    loop {
        let wanted = mem::take(&mut state.wanted);
        let mut remaining = Vec::with_capacity(wanted.len());
        let mut progress = false;

        for wanted in wanted {
            if failed.contains(&wanted.root) {
                continue;
            }
            let solutions = state.solutions;
            match solve_wanted(state, context, &wanted)? {
                Outcome::Solved => progress = true,
                Outcome::Improved => {
                    progress |= state.solutions != solutions;
                    remaining.push(wanted);
                }
                Outcome::Stuck => remaining.push(wanted),
                Outcome::Failed(kind) => {
                    progress = true;
                    failed.insert(wanted.root);
                    state.insert_error_at(kind, wanted.crumbs);
                }
            }
        }

        let subgoals = mem::take(&mut state.wanted);
        remaining.extend(subgoals);
        remaining.retain(|wanted| !failed.contains(&wanted.root));
        state.wanted = remaining;

        if !progress {
            return Ok(());
        }
    }
}

pub fn report_unsolved<Q>(state: &mut CheckState, context: &CheckContext<Q>)
where
    Q: ExternalQueries,
{
    for wanted in mem::take(&mut state.wanted) {
        let constraint = fold::zonk(state, context, wanted.constraint);
        state.insert_error_at(ErrorKind::NoInstanceFound { constraint }, wanted.crumbs);
    }
}

fn solve_wanted<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    wanted: &Wanted,
) -> QueryResult<Outcome>
where
    Q: ExternalQueries,
{
    let constraint = fold::zonk(state, context, wanted.constraint);

    let Some(head) = constraint_head(state, context, constraint) else {
        return Ok(Outcome::Failed(ErrorKind::NoInstanceFound { constraint }));
    };

    let coercible = context.known.coercible == Some(head.class);
    if coercible {
        *state.coercible_steps.entry(wanted.root).or_default() += 1;
    }
    let coercible_steps = state.coercible_steps.get(&wanted.root).copied().unwrap_or(0);

    if wanted.work > WORK_LIMIT || coercible_steps > COERCIBLE_STEP_LIMIT {
        let kind = if coercible {
            ErrorKind::PossiblyInfiniteCoercibleInstance { constraint }
        } else {
            ErrorKind::PossiblyInfiniteInstance { constraint }
        };
        return Ok(Outcome::Failed(kind));
    }

    let class = lookup_class(state, context, head.class)?;
    let functional_dependencies =
        class.as_ref().map(|class| Arc::clone(&class.functional_dependencies));
    let functional_dependencies = functional_dependencies.unwrap_or_else(|| Arc::from([]));

    let givens = given_closure(state, context, wanted)?;
    let mut given_improvement = None;
    for given in &givens {
        let Some(given_head) = constraint_head(state, context, given.constraint) else {
            continue;
        };
        if given_head.class != head.class {
            continue;
        }
        let positions = compare_arguments(state, context, &given_head.arguments, &head.arguments);
        if positions.iter().all(|&position| position == Match::Match) {
            state.checked.tree.set_evidence(wanted.slot, Evidence::clone(&given.evidence));
            return Ok(Outcome::Solved);
        }
        if given_improvement.is_none()
            && let Some(improved) = improvable_positions(&positions, &functional_dependencies)
        {
            given_improvement = Some((given_head, improved));
        }
    }

    if let Some((given_head, improved)) = given_improvement {
        let improved = improve(state, context, &given_head.arguments, &head.arguments, &improved)?;
        return Ok(if improved { Outcome::Improved } else { Outcome::Stuck });
    }

    if let Some(outcome) = builtin::solve(state, context, wanted, &head)? {
        return Ok(outcome);
    }

    solve_with_instances(state, context, wanted, constraint, &head, &functional_dependencies)
}

pub fn given_closure<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    wanted: &Wanted,
) -> QueryResult<Vec<Given>>
where
    Q: ExternalQueries,
{
    let mut givens = state.givens_in_scope(wanted.scope);
    let mut index = 0;
    while let Some(given) = givens.get(index) {
        let given = Given::clone(given);
        index += 1;
        if givens.len() > 512 {
            break;
        }
        let Some(head) = constraint_head(state, context, given.constraint) else {
            continue;
        };
        let Some(class) = lookup_class(state, context, head.class)? else {
            continue;
        };
        let substitution = class_substitution(context, &class, &head);
        for (superclass_index, &superclass) in class.superclasses.iter().enumerate() {
            let constraint = fold::substitute_variables(context, &substitution, superclass);
            let evidence = Evidence::Superclass {
                from: Arc::new(Evidence::clone(&given.evidence)),
                class: head.class,
                index: superclass_index,
            };
            givens.push(Given { constraint, evidence });
        }
    }
    Ok(givens)
}

pub fn class_substitution<Q>(
    context: &CheckContext<Q>,
    class: &CheckedClass,
    head: &ConstraintHead,
) -> FxHashMap<Name, TypeId>
where
    Q: ExternalQueries,
{
    let kinds = class.kind_binders.iter().zip(&head.kind_arguments);
    let types = class.parameters.iter().zip(&head.arguments);
    let substitution = kinds
        .chain(types)
        .map(|(&binder_id, &argument)| (context.lookup_forall_binder(binder_id).name, argument));
    substitution.collect()
}

pub fn improvable_positions(
    positions: &[Match],
    functional_dependencies: &[FunctionalDependency],
) -> Option<Vec<usize>> {
    if positions.contains(&Match::Apart) {
        return None;
    }

    let mut known = positions.iter().map(|&position| position == Match::Match).collect_vec();
    let improved = determine_positions(&mut known, functional_dependencies);

    let complete = known.iter().all(|&known| known);
    (complete && !improved.is_empty()).then_some(improved)
}

pub fn covering_sets(
    arity: usize,
    functional_dependencies: &[FunctionalDependency],
) -> Vec<Vec<usize>> {
    let mut covering_sets: Vec<Vec<usize>> = Vec::new();
    for candidate in (0..arity).powerset() {
        let contains_covering_set = covering_sets
            .iter()
            .any(|covering_set| covering_set.iter().all(|index| candidate.contains(index)));
        if contains_covering_set {
            continue;
        }

        let mut known = (0..arity).map(|index| candidate.contains(&index)).collect_vec();
        determine_positions(&mut known, functional_dependencies);

        if known.iter().all(|&known| known) {
            covering_sets.push(candidate);
        }
    }
    covering_sets
}

fn determine_positions(
    known: &mut [bool],
    functional_dependencies: &[FunctionalDependency],
) -> Vec<usize> {
    let mut determined = Vec::new();
    loop {
        let mut progress = false;
        for dependency in functional_dependencies {
            let determiners_known = dependency
                .determiners
                .iter()
                .all(|&index| known.get(index).copied().unwrap_or(false));
            if !determiners_known {
                continue;
            }
            for &index in dependency.determined.iter() {
                if let Some(slot) = known.get_mut(index)
                    && !*slot
                {
                    *slot = true;
                    determined.push(index);
                    progress = true;
                }
            }
        }
        if !progress {
            return determined;
        }
    }
}

pub fn improve<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    source: &[TypeId],
    target: &[TypeId],
    positions: &[usize],
) -> QueryResult<bool>
where
    Q: ExternalQueries,
{
    for &position in positions {
        let (Some(&source), Some(&target)) = (source.get(position), target.get(position)) else {
            continue;
        };
        if !unify::unify(state, context, target, source)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn unify_arguments<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    source: &[TypeId],
    target: &[TypeId],
) -> QueryResult<bool>
where
    Q: ExternalQueries,
{
    for (&source, &target) in source.iter().zip(target) {
        if !unify::unify(state, context, target, source)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn compare_arguments<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    left: &[TypeId],
    right: &[TypeId],
) -> Vec<Match>
where
    Q: ExternalQueries,
{
    let no_variables = FxHashSet::default();
    let mut bindings = FxHashMap::default();
    let positions = left.iter().zip(right).map(|(&left, &right)| {
        match_type(state, context, &no_variables, &mut bindings, left, right)
    });
    positions.collect_vec()
}

fn instance_files<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    head: &ConstraintHead,
) -> Vec<FileId>
where
    Q: ExternalQueries,
{
    let (class_file, _) = head.class;
    let mut files = vec![context.id, class_file];
    for &argument in head.arguments.iter().chain(&head.kind_arguments) {
        collect_files(state, context, argument, &mut files);
    }
    files.into_iter().unique().collect_vec()
}

fn collect_files<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    id: TypeId,
    files: &mut Vec<FileId>,
) where
    Q: ExternalQueries,
{
    let id = fold::zonk(state, context, id);
    if let Type::Constructor(file_id, _) = context.lookup_type(id) {
        files.push(file_id);
        return;
    }
    if let Type::Synonym(application_id) = context.lookup_type(id) {
        let application = context.lookup_synonym_application(application_id);
        collect_files(state, context, application.expansion, files);
        return;
    }
    fold::map_children(context, id, |child| {
        collect_files(state, context, child, files);
        child
    });
}

pub fn class_instances<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    file_id: FileId,
    class: Item,
) -> QueryResult<Vec<CheckedInstance>>
where
    Q: ExternalQueries,
{
    let matching = |instance: &&CheckedInstance| instance.class == class;
    if file_id == context.id {
        let instances = state.checked.instances.iter().filter(matching);
        return Ok(instances.map(CheckedInstance::clone).collect_vec());
    }
    let checked = context.checked_dependency(file_id)?;
    let instances = checked.instances.iter().filter(matching);
    Ok(instances.map(CheckedInstance::clone).collect_vec())
}

struct Candidate {
    file_id: FileId,
    instance: CheckedInstance,
    bindings: FxHashMap<Name, TypeId>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct ChainKey {
    file_id: FileId,
    chain: Option<InstanceChainId>,
    index: usize,
}

fn solve_with_instances<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    wanted: &Wanted,
    constraint: TypeId,
    head: &ConstraintHead,
    functional_dependencies: &[FunctionalDependency],
) -> QueryResult<Outcome>
where
    Q: ExternalQueries,
{
    let mut chains: Vec<(ChainKey, Vec<CheckedInstance>)> = Vec::new();
    for file_id in instance_files(state, context, head) {
        let instances = class_instances(state, context, file_id, head.class)?;
        for (index, instance) in instances.into_iter().enumerate() {
            let key = match instance.chain {
                Some((chain_id, _)) => ChainKey { file_id, chain: Some(chain_id), index: 0 },
                None => ChainKey { file_id, chain: None, index },
            };
            match chains.iter_mut().find(|(existing, _)| *existing == key) {
                Some((_, chain)) => chain.push(instance),
                None => chains.push((key, vec![instance])),
            }
        }
    }

    let total = chains.len();
    let mut matches = Vec::new();
    let mut improvements = Vec::new();
    let mut stuck = false;

    for (ChainKey { file_id, .. }, mut chain) in chains {
        chain.sort_by_key(|instance| instance.chain.map(|(_, position)| position));
        for instance in chain {
            let variables = instance
                .binders
                .iter()
                .map(|&binder_id| context.lookup_forall_binder(binder_id).name);
            let variables = variables.collect::<FxHashSet<_>>();
            let mut bindings = FxHashMap::default();
            let positions =
                instance.arguments.iter().zip(&head.arguments).map(|(&pattern, &target)| {
                    match_type(state, context, &variables, &mut bindings, pattern, target)
                });
            let positions = positions.collect_vec();
            let result = positions.iter().copied().fold(Match::Match, Match::and);
            match result {
                Match::Apart => continue,
                Match::Match => {
                    matches.push(Candidate { file_id, instance, bindings });
                }
                Match::Unknown => {
                    if let Some(improved) =
                        improvable_positions(&positions, functional_dependencies)
                    {
                        improvements.push((Candidate { file_id, instance, bindings }, improved));
                    } else {
                        stuck = true;
                    }
                }
            }
            break;
        }
    }

    if matches.len() > 1 {
        let instances = matches.len();
        return Ok(Outcome::Failed(ErrorKind::OverlappingInstances { constraint, instances }));
    }

    if !stuck
        && improvements.is_empty()
        && let Some(candidate) = matches.pop()
    {
        return apply_instance(state, context, wanted, head, candidate);
    }

    if matches.is_empty()
        && !stuck
        && improvements.len() == 1
        && let Some((candidate, improved)) = improvements.pop()
    {
        let instantiated = instantiate_instance(state, context, &candidate)?;
        let improved =
            improve(state, context, &instantiated.arguments, &head.arguments, &improved)?;
        return Ok(if improved { Outcome::Improved } else { Outcome::Stuck });
    }

    if total > 0 && matches.is_empty() && improvements.is_empty() && !stuck {
        return Ok(Outcome::Failed(ErrorKind::NoInstanceFound { constraint }));
    }

    Ok(Outcome::Stuck)
}

struct InstantiatedInstance {
    type_arguments: Vec<TypeId>,
    arguments: Vec<TypeId>,
    constraints: Vec<TypeId>,
}

fn instantiate_instance<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    candidate: &Candidate,
) -> QueryResult<InstantiatedInstance>
where
    Q: ExternalQueries,
{
    let mut substitution = FxHashMap::default();
    let mut type_arguments = Vec::with_capacity(candidate.instance.binders.len());
    for &binder_id in candidate.instance.binders.iter() {
        let binder = context.lookup_forall_binder(binder_id);
        let argument = match candidate.bindings.get(&binder.name) {
            Some(&argument) => argument,
            None => {
                let kind = fold::substitute_variables(context, &substitution, binder.kind);
                kind::fresh_unification(state, context, kind)
            }
        };
        substitution.insert(binder.name, argument);
        type_arguments.push(argument);
    }

    let substitute = |id: TypeId| fold::substitute_variables(context, &substitution, id);
    let arguments = candidate.instance.arguments.iter().map(|&argument| substitute(argument));
    let constraints =
        candidate.instance.constraints.iter().map(|&constraint| substitute(constraint));

    Ok(InstantiatedInstance {
        type_arguments,
        arguments: arguments.collect_vec(),
        constraints: constraints.collect_vec(),
    })
}

fn apply_instance<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    wanted: &Wanted,
    head: &ConstraintHead,
    candidate: Candidate,
) -> QueryResult<Outcome>
where
    Q: ExternalQueries,
{
    let instantiated = instantiate_instance(state, context, &candidate)?;
    if !unify_arguments(state, context, &instantiated.arguments, &head.arguments)? {
        return Ok(Outcome::Stuck);
    }

    let scope = wanted.scope;
    let crumbs = wanted.crumbs.clone();
    let subgoals = instantiated.constraints.iter().map(|&constraint| {
        let slot = state.checked.tree.allocate_evidence_slot();
        let work = wanted.work + 1;
        let crumbs = crumbs.clone();
        let root = wanted.root;
        state.wanted.push(Wanted { constraint, slot, scope, crumbs, work, root });
        slot
    });
    let arguments = subgoals.collect::<Arc<[_]>>();

    let evidence = Evidence::Instance {
        file_id: candidate.file_id,
        origin: candidate.instance.origin,
        type_arguments: Arc::from(instantiated.type_arguments),
        arguments,
    };
    state.checked.tree.set_evidence(wanted.slot, evidence);
    Ok(Outcome::Solved)
}

pub fn match_type<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    variables: &FxHashSet<Name>,
    bindings: &mut FxHashMap<Name, TypeId>,
    pattern: TypeId,
    target: TypeId,
) -> Match
where
    Q: ExternalQueries,
{
    let pattern = normalise::normalise(state, context, pattern);
    let target = normalise::normalise(state, context, target);

    if let Type::Variable(binder_id) = context.lookup_type(pattern) {
        let name = context.lookup_forall_binder(binder_id).name;
        if variables.contains(&name) {
            return match bindings.get(&name) {
                Some(&bound) => {
                    let no_variables = FxHashSet::default();
                    let mut no_bindings = FxHashMap::default();
                    match_type(state, context, &no_variables, &mut no_bindings, bound, target)
                }
                None => {
                    bindings.insert(name, target);
                    Match::Match
                }
            };
        }
    }

    if pattern == target {
        return Match::Match;
    }

    let pattern_type = as_application(context, pattern);
    let target_type = as_application(context, target);

    match (pattern_type, target_type) {
        (_, Type::Unification(_)) | (Type::Unification(_), _) => Match::Unknown,
        (_, Type::Unknown) | (Type::Unknown, _) => Match::Unknown,

        (
            Type::Application(pattern_function, pattern_argument),
            Type::Application(target_function, target_argument),
        )
        | (
            Type::KindApplication(pattern_function, pattern_argument),
            Type::KindApplication(target_function, target_argument),
        ) => {
            let function =
                match_type(state, context, variables, bindings, pattern_function, target_function);
            if function == Match::Apart {
                return Match::Apart;
            }
            let argument =
                match_type(state, context, variables, bindings, pattern_argument, target_argument);
            function.and(argument)
        }

        (Type::KindApplication(pattern_function, _), _) => {
            match_type(state, context, variables, bindings, pattern_function, target)
        }

        (_, Type::KindApplication(target_function, _)) => {
            match_type(state, context, variables, bindings, pattern, target_function)
        }

        (Type::Row(pattern_row), Type::Row(target_row)) => {
            let pattern_row = context.lookup_row_type(pattern_row);
            let target_row = context.lookup_row_type(target_row);
            let pattern_labels = pattern_row.fields.iter().map(|field| &field.label).collect_vec();
            let target_labels = target_row.fields.iter().map(|field| &field.label).collect_vec();
            if pattern_labels != target_labels {
                let closed = pattern_row.tail.is_none() && target_row.tail.is_none();
                return if closed { Match::Apart } else { Match::Unknown };
            }
            let mut result = Match::Match;
            for (pattern_field, target_field) in
                pattern_row.fields.iter().zip(target_row.fields.iter())
            {
                let field = match_type(
                    state,
                    context,
                    variables,
                    bindings,
                    pattern_field.id,
                    target_field.id,
                );
                result = result.and(field);
            }
            match (pattern_row.tail, target_row.tail) {
                (None, None) => result,
                (Some(pattern_tail), Some(target_tail)) => {
                    let tail =
                        match_type(state, context, variables, bindings, pattern_tail, target_tail);
                    result.and(tail)
                }
                _ => result.and(Match::Unknown),
            }
        }

        (Type::Rigid(pattern_rigid), Type::Rigid(target_rigid)) => {
            let pattern_rigid = context.lookup_rigid(pattern_rigid);
            let target_rigid = context.lookup_rigid(target_rigid);
            if pattern_rigid.name == target_rigid.name { Match::Match } else { Match::Apart }
        }

        (Type::Variable(pattern_binder), Type::Variable(target_binder)) => {
            let pattern_binder = context.lookup_forall_binder(pattern_binder);
            let target_binder = context.lookup_forall_binder(target_binder);
            if pattern_binder.name == target_binder.name { Match::Match } else { Match::Apart }
        }

        (Type::Forall(..), _) | (_, Type::Forall(..)) => Match::Unknown,

        _ => Match::Apart,
    }
}

pub fn as_application<Q>(context: &CheckContext<Q>, id: TypeId) -> Type
where
    Q: ExternalQueries,
{
    match context.lookup_type(id) {
        Type::Function(argument, result) => {
            let function = context.intern_application(context.types.function, argument);
            Type::Application(function, result)
        }
        other => other,
    }
}

pub fn improve_generalised<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    constraints: &[TypeId],
) -> QueryResult<bool>
where
    Q: ExternalQueries,
{
    let solutions = state.solutions;
    for (index, &given) in constraints.iter().enumerate() {
        for &wanted in constraints.iter().skip(index + 1) {
            let Some(given_head) = constraint_head(state, context, given) else {
                continue;
            };
            let Some(wanted_head) = constraint_head(state, context, wanted) else {
                continue;
            };
            if given_head.class != wanted_head.class {
                continue;
            }
            let Some(class) = lookup_class(state, context, given_head.class)? else {
                continue;
            };
            let positions =
                compare_arguments(state, context, &given_head.arguments, &wanted_head.arguments);
            let Some(improved) = improvable_positions(&positions, &class.functional_dependencies)
            else {
                continue;
            };
            improve(state, context, &given_head.arguments, &wanted_head.arguments, &improved)?;
        }
    }
    Ok(state.solutions != solutions)
}

pub fn can_be_generalised<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    head: &ConstraintHead,
) -> bool
where
    Q: ExternalQueries,
{
    if context.known.coercible == Some(head.class) {
        return false;
    }
    if head.kind_arguments.is_empty() && head.arguments.is_empty() {
        return true;
    }
    let mut arguments = head.kind_arguments.iter().chain(&head.arguments);
    arguments.any(|&argument| is_bare_unknown(state, context, argument))
}

fn is_bare_unknown<Q>(state: &CheckState, context: &CheckContext<Q>, id: TypeId) -> bool
where
    Q: ExternalQueries,
{
    let id = normalise::normalise(state, context, id);
    match context.lookup_type(id) {
        Type::Unification(_) => true,
        Type::Kinded(inner, _) => is_bare_unknown(state, context, inner),
        _ => false,
    }
}
