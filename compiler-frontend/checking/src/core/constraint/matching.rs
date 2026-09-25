use std::iter;

use building_types::QueryResult;
use itertools::Itertools;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::ExternalQueries;
use crate::context::CheckContext;
use crate::core::constraint::instances::InstanceCandidate;
use crate::core::constraint::{CanonicalConstraintId, canonical};
use crate::core::fd::{
    Fd, compute_closure, get_all_determined, get_functional_dependencies, positions_cover_all,
};
use crate::core::substitute::SubstituteName;
use crate::core::walk::{TypeWalker, WalkAction, walk_type};
use crate::core::{
    ApplicationArgument, CheckedInstance, Name, RowField, RowTypeId, Type, TypeFlags, TypeId,
    normalise, toolkit,
};
use crate::source::types;
use crate::state::CheckState;

#[derive(PartialEq, Eq)]
pub enum MatchType {
    Match { bindings: Vec<(Name, TypeId)> },
    Apart,
    Stuck { stuck: Vec<u32>, skolem: bool },
}

impl MatchType {
    pub fn combine(self, other: MatchType) -> MatchType {
        match (self, other) {
            (MatchType::Match { bindings: mut left }, MatchType::Match { bindings: right }) => {
                left.extend(right);
                MatchType::Match { bindings: left }
            }

            (MatchType::Apart, _) | (_, MatchType::Apart) => MatchType::Apart,

            (
                MatchType::Stuck { stuck: mut left, skolem: left_skolem },
                MatchType::Stuck { stuck: right, skolem: right_skolem },
            ) => {
                left.extend(right);
                MatchType::Stuck { stuck: left, skolem: left_skolem || right_skolem }
            }

            (MatchType::Stuck { stuck, skolem }, _) | (_, MatchType::Stuck { stuck, skolem }) => {
                MatchType::Stuck { stuck, skolem }
            }
        }
    }

    /// Combines with the result of `other`, which is not computed when this
    /// result is already apart since combining with it would stay apart.
    pub fn and_then(
        self,
        other: impl FnOnce() -> QueryResult<MatchType>,
    ) -> QueryResult<MatchType> {
        if self.is_apart() { Ok(MatchType::Apart) } else { Ok(self.combine(other()?)) }
    }

    pub fn is_match(&self) -> bool {
        matches!(self, MatchType::Match { .. })
    }

    pub fn is_apart(&self) -> bool {
        matches!(self, MatchType::Apart)
    }

    pub fn is_unknown(&self) -> bool {
        matches!(self, MatchType::Stuck { .. })
    }
}

pub fn types_match<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    pattern: &FxHashSet<Name>,
    left: TypeId,
    right: TypeId,
) -> QueryResult<MatchType>
where
    Q: ExternalQueries,
{
    let left = normalise::expand(state, context, left)?;
    let right = normalise::expand(state, context, right)?;

    let left_core = context.lookup_type(left);
    let right_core = context.lookup_type(right);

    match (left_core, right_core) {
        (Type::Kinded(left, _), _) => types_match(state, context, pattern, *left, right),
        (_, Type::Kinded(right, _)) => types_match(state, context, pattern, left, *right),

        (Type::Unification(left), Type::Unification(right)) => {
            if left == right {
                Ok(MatchType::Match { bindings: vec![] })
            } else {
                Ok(MatchType::Stuck { stuck: vec![*left, *right], skolem: false })
            }
        }

        (Type::Rigid(left, _, _), Type::Rigid(right, _, _))
            if !pattern.contains(left) && !pattern.contains(right) && left == right =>
        {
            Ok(MatchType::Match { bindings: vec![] })
        }

        (_, Type::Rigid(right, _, _)) if pattern.contains(right) => {
            Ok(MatchType::Match { bindings: vec![(*right, left)] })
        }

        (Type::Unification(unification), _) | (_, Type::Unification(unification)) => {
            Ok(MatchType::Stuck { stuck: vec![*unification], skolem: false })
        }

        (Type::Rigid(name, _, _), _) | (_, Type::Rigid(name, _, _)) if !pattern.contains(name) => {
            Ok(MatchType::Stuck { stuck: vec![], skolem: true })
        }

        (Type::Constructor(left_file, left_item), Type::Constructor(right_file, right_item))
            if (left_file, left_item) == (right_file, right_item) =>
        {
            Ok(MatchType::Match { bindings: vec![] })
        }

        (Type::String(_, left), Type::String(_, right)) if left == right => {
            Ok(MatchType::Match { bindings: vec![] })
        }

        (Type::Integer(left), Type::Integer(right)) if left == right => {
            Ok(MatchType::Match { bindings: vec![] })
        }

        (
            Type::Application(left_function, left_argument),
            Type::Application(right_function, right_argument),
        ) => {
            let function = types_match(state, context, pattern, *left_function, *right_function)?;
            function
                .and_then(|| types_match(state, context, pattern, *left_argument, *right_argument))
        }

        (Type::Application(_, _), Type::Function(right_argument, right_result)) => {
            let right = context.intern_function_application(*right_argument, *right_result);
            types_match(state, context, pattern, left, right)
        }

        (Type::Function(left_argument, left_result), Type::Application(_, _)) => {
            let left = context.intern_function_application(*left_argument, *left_result);
            types_match(state, context, pattern, left, right)
        }

        (
            Type::KindApplication(left_function, left_argument),
            Type::KindApplication(right_function, right_argument),
        ) => {
            let function = types_match(state, context, pattern, *left_function, *right_function)?;
            function
                .and_then(|| types_match(state, context, pattern, *left_argument, *right_argument))
        }

        (
            Type::Function(left_argument, left_result),
            Type::Function(right_argument, right_result),
        ) => {
            let argument = types_match(state, context, pattern, *left_argument, *right_argument)?;
            argument.and_then(|| types_match(state, context, pattern, *left_result, *right_result))
        }

        (Type::Row(left), Type::Row(right)) => compare_row_types_with(
            state,
            context,
            *left,
            *right,
            &mut |state, context, left, right| types_match(state, context, pattern, left, right),
        ),

        (_, _) => Ok(MatchType::Apart),
    }
}

fn compare_row_types_with<Q, Compare>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    left: RowTypeId,
    right: RowTypeId,
    compare: &mut Compare,
) -> QueryResult<MatchType>
where
    Q: ExternalQueries,
    Compare: FnMut(&mut CheckState, &CheckContext<Q>, TypeId, TypeId) -> QueryResult<MatchType>,
{
    let left = context.lookup_row_type(left);
    let right = context.lookup_row_type(right);

    let mut row_result = MatchType::Match { bindings: vec![] };
    let mut left_fields = vec![];
    let mut right_fields = vec![];

    for field in itertools::merge_join_by(left.fields.iter(), right.fields.iter(), |left, right| {
        left.label.cmp(&right.label)
    }) {
        match field {
            itertools::EitherOrBoth::Left(left) => {
                left_fields.push(left.clone());
            }
            itertools::EitherOrBoth::Both(left, right) => {
                let field_result = compare(state, context, left.id, right.id)?;
                row_result = row_result.combine(field_result);
                if row_result.is_apart() {
                    return Ok(MatchType::Apart);
                }
            }
            itertools::EitherOrBoth::Right(right) => {
                right_fields.push(right.clone());
            }
        }
    }

    let tail_result = compare_row_tails_with(
        state,
        context,
        left_fields,
        left.tail,
        right_fields,
        right.tail,
        compare,
    )?;

    Ok(row_result.combine(tail_result))
}

fn compare_row_tails_with<Q, Compare>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    left_fields: Vec<RowField>,
    left_tail: Option<TypeId>,
    right_fields: Vec<RowField>,
    right_tail: Option<TypeId>,
    compare: &mut Compare,
) -> QueryResult<MatchType>
where
    Q: ExternalQueries,
    Compare: FnMut(&mut CheckState, &CheckContext<Q>, TypeId, TypeId) -> QueryResult<MatchType>,
{
    let left_tail = context.intern_row(left_fields, left_tail);
    let right_tail = context.intern_row(right_fields, right_tail);

    if let (Type::Row(left_row), Type::Row(right_row)) =
        (context.lookup_type(left_tail), context.lookup_type(right_tail))
    {
        let left_row = context.lookup_row_type(*left_row);
        let right_row = context.lookup_row_type(*right_row);

        if left_row.fields.is_empty()
            && left_row.tail.is_none()
            && right_row.fields.is_empty()
            && right_row.tail.is_none()
        {
            Ok(MatchType::Match { bindings: vec![] })
        } else {
            Ok(MatchType::Apart)
        }
    } else {
        compare(state, context, left_tail, right_tail)
    }
}

pub fn types_equal<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    left: TypeId,
    right: TypeId,
) -> QueryResult<MatchType>
where
    Q: ExternalQueries,
{
    let left = normalise::expand(state, context, left)?;
    let right = normalise::expand(state, context, right)?;

    let left_core = context.lookup_type(left);
    let right_core = context.lookup_type(right);

    match (left_core, right_core) {
        (Type::Kinded(left, _), _) => types_equal(state, context, *left, right),
        (_, Type::Kinded(right, _)) => types_equal(state, context, left, *right),

        (Type::Unification(left), Type::Unification(right)) => {
            if left == right {
                Ok(MatchType::Match { bindings: vec![] })
            } else {
                Ok(MatchType::Stuck { stuck: vec![*left, *right], skolem: false })
            }
        }

        (Type::Rigid(left, _, _), Type::Rigid(right, _, _)) => {
            if left == right {
                Ok(MatchType::Match { bindings: vec![] })
            } else {
                Ok(MatchType::Stuck { stuck: vec![], skolem: true })
            }
        }

        (Type::Unification(left), _) => {
            if toolkit::contains_unification(state, context, right, *left)? {
                Ok(MatchType::Apart)
            } else {
                Ok(MatchType::Stuck { stuck: vec![*left], skolem: false })
            }
        }

        (_, Type::Unification(right)) => {
            if toolkit::contains_unification(state, context, left, *right)? {
                Ok(MatchType::Apart)
            } else {
                Ok(MatchType::Stuck { stuck: vec![*right], skolem: false })
            }
        }

        (Type::Rigid(left, _, _), _) => {
            if toolkit::contains_rigid(state, context, right, *left)? {
                Ok(MatchType::Apart)
            } else {
                Ok(MatchType::Stuck { stuck: vec![], skolem: true })
            }
        }

        (_, Type::Rigid(right, _, _)) => {
            if toolkit::contains_rigid(state, context, left, *right)? {
                Ok(MatchType::Apart)
            } else {
                Ok(MatchType::Stuck { stuck: vec![], skolem: true })
            }
        }

        (Type::Constructor(left_file, left_item), Type::Constructor(right_file, right_item))
            if (left_file, left_item) == (right_file, right_item) =>
        {
            Ok(MatchType::Match { bindings: vec![] })
        }

        (Type::String(_, left), Type::String(_, right)) if left == right => {
            Ok(MatchType::Match { bindings: vec![] })
        }

        (Type::Integer(left), Type::Integer(right)) if left == right => {
            Ok(MatchType::Match { bindings: vec![] })
        }

        (
            Type::Application(left_function, left_argument),
            Type::Application(right_function, right_argument),
        ) => {
            let function = types_equal(state, context, *left_function, *right_function)?;
            function.and_then(|| types_equal(state, context, *left_argument, *right_argument))
        }

        (Type::Application(_, _), Type::Function(right_argument, right_result)) => {
            let right = context.intern_function_application(*right_argument, *right_result);
            types_equal(state, context, left, right)
        }

        (Type::Function(left_argument, left_result), Type::Application(_, _)) => {
            let left = context.intern_function_application(*left_argument, *left_result);
            types_equal(state, context, left, right)
        }

        (
            Type::KindApplication(left_function, left_argument),
            Type::KindApplication(right_function, right_argument),
        ) => {
            let function = types_equal(state, context, *left_function, *right_function)?;
            function.and_then(|| types_equal(state, context, *left_argument, *right_argument))
        }

        (
            Type::Function(left_argument, left_result),
            Type::Function(right_argument, right_result),
        ) => {
            let argument = types_equal(state, context, *left_argument, *right_argument)?;
            argument.and_then(|| types_equal(state, context, *left_result, *right_result))
        }

        (Type::Row(left), Type::Row(right)) => compare_row_types_with(
            state,
            context,
            *left,
            *right,
            &mut |state, context, left, right| types_equal(state, context, left, right),
        ),

        (_, _) => Ok(MatchType::Apart),
    }
}

pub fn verify_substitution<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    bindings: Vec<(Name, TypeId)>,
) -> QueryResult<MatchType>
where
    Q: ExternalQueries,
{
    let mut map: FxHashMap<_, Vec<_>> = FxHashMap::default();
    for &(name, substitution) in &bindings {
        map.entry(name).or_default().push(substitution);
    }

    let mut outcome = MatchType::Match { bindings };
    for substitution in map.values() {
        for [&left, &right] in substitution.iter().array_combinations() {
            outcome = outcome.combine(types_equal(state, context, left, right)?);
        }
    }

    Ok(outcome)
}

pub fn match_instance<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    patterns: &FxHashSet<Name>,
    functional_dependencies: &[Fd],
    wanted_arguments: &[TypeId],
    given_arguments: &[TypeId],
) -> QueryResult<MatchType>
where
    Q: ExternalQueries,
{
    let determined = get_all_determined(functional_dependencies);
    let mut arguments = vec![];

    for (index, (&wanted, &given)) in iter::zip(wanted_arguments, given_arguments).enumerate() {
        let argument = types_match(state, context, patterns, wanted, given)?;
        // An apart argument that is not determined by functional dependencies
        // is combined into the outcome whether or not the matches cover it.
        if argument.is_apart() && !determined.contains(&index) {
            return Ok(MatchType::Apart);
        }
        arguments.push(argument);
    }

    if !covers(functional_dependencies, &arguments)? {
        return Ok(combine_arguments(arguments));
    }

    let arguments = arguments.into_iter().enumerate().filter_map(|(index, argument)| {
        let non_determined = !determined.contains(&index);
        non_determined.then_some(argument)
    });

    let outcome = combine_arguments(arguments);
    if let MatchType::Match { bindings } = outcome {
        return verify_substitution(state, context, bindings);
    }

    Ok(outcome)
}

fn combine_arguments(arguments: impl IntoIterator<Item = MatchType>) -> MatchType {
    let seed = MatchType::Match { bindings: vec![] };
    arguments.into_iter().fold(seed, MatchType::combine)
}

fn covers(fd: &[Fd], types: &[MatchType]) -> QueryResult<bool> {
    let match_indices: FxHashSet<_> = types
        .iter()
        .enumerate()
        .filter_map(|(index, argument)| argument.is_match().then_some(index))
        .collect();

    let determined = compute_closure(fd, &match_indices);
    Ok(types.iter().enumerate().all(|(index, _)| determined.contains(&index)))
}

pub enum MatchInstance {
    Match { unifications: Vec<(TypeId, TypeId)>, constraints: Vec<CanonicalConstraintId> },
    Apart,
    Stuck { stuck: Vec<u32>, skolem: bool },
}

impl MatchInstance {
    pub fn empty() -> MatchInstance {
        MatchInstance::Match { unifications: vec![], constraints: vec![] }
    }

    pub fn from_unifications(unifications: Vec<(TypeId, TypeId)>) -> MatchInstance {
        MatchInstance::Match { unifications, constraints: vec![] }
    }

    pub fn from_constraints(constraints: Vec<CanonicalConstraintId>) -> MatchInstance {
        MatchInstance::Match { unifications: vec![], constraints }
    }

    pub fn from_parts(
        unifications: Vec<(TypeId, TypeId)>,
        constraints: Vec<CanonicalConstraintId>,
    ) -> MatchInstance {
        MatchInstance::Match { unifications, constraints }
    }
}

struct CollectBlocking {
    blocking: Vec<u32>,
}

impl TypeWalker for CollectBlocking {
    fn visit<Q: ExternalQueries>(
        &mut self,
        _state: &mut CheckState,
        _context: &CheckContext<Q>,
        _id: TypeId,
        t: &Type,
    ) -> QueryResult<WalkAction> {
        if let Type::Unification(id) = t {
            self.blocking.push(*id);
        }
        Ok(WalkAction::Continue)
    }

    fn may_visit(&self, flags: TypeFlags) -> bool {
        flags.has_unification()
    }
}

pub fn collect_blocking<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: &[TypeId],
) -> QueryResult<Vec<u32>>
where
    Q: ExternalQueries,
{
    let mut walker = CollectBlocking { blocking: vec![] };

    for &id in id {
        walk_type(state, context, id, &mut walker)?;
    }

    Ok(walker.blocking)
}

/// Whether a type contains an unsolved unification variable.
pub fn is_blocked<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: TypeId,
) -> QueryResult<bool>
where
    Q: ExternalQueries,
{
    struct HasBlocking {
        blocked: bool,
    }

    impl TypeWalker for HasBlocking {
        fn visit<Q: ExternalQueries>(
            &mut self,
            _state: &mut CheckState,
            _context: &CheckContext<Q>,
            _id: TypeId,
            t: &Type,
        ) -> QueryResult<WalkAction> {
            if let Type::Unification(_) = t {
                self.blocked = true;
                Ok(WalkAction::Break)
            } else {
                Ok(WalkAction::Continue)
            }
        }

        fn may_visit(&self, flags: TypeFlags) -> bool {
            flags.has_unification()
        }
    }

    let mut walker = HasBlocking { blocked: false };
    walk_type(state, context, id, &mut walker)?;
    Ok(walker.blocked)
}

pub fn blocking_constraint<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: &[TypeId],
) -> QueryResult<MatchInstance>
where
    Q: ExternalQueries,
{
    let stuck = collect_blocking(state, context, id)?;
    if !stuck.is_empty() {
        Ok(MatchInstance::Stuck { stuck, skolem: false })
    } else {
        Ok(MatchInstance::Apart)
    }
}

pub fn match_provided<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    wanted: CanonicalConstraintId,
    provided: CanonicalConstraintId,
) -> QueryResult<MatchInstance>
where
    Q: ExternalQueries,
{
    let wanted = &state.canonicals[wanted];
    let provided = &state.canonicals[provided];

    if (wanted.file_id, wanted.type_id) != (provided.file_id, provided.type_id)
        || wanted.arguments.len() != provided.arguments.len()
    {
        return Ok(MatchInstance::Apart);
    }

    let wanted = wanted.clone();
    let provided = provided.clone();

    let pattern_variables = FxHashSet::default();

    let functional_dependencies =
        get_functional_dependencies(state, context, wanted.file_id, wanted.type_id)?;

    let wanted_arguments = wanted
        .arguments
        .iter()
        .filter_map(
            |argument| {
                if let ApplicationArgument::Type(id) = argument { Some(*id) } else { None }
            },
        )
        .collect_vec();

    let provided_arguments = provided
        .arguments
        .iter()
        .filter_map(
            |argument| {
                if let ApplicationArgument::Type(id) = argument { Some(*id) } else { None }
            },
        )
        .collect_vec();

    match match_instance(
        state,
        context,
        &pattern_variables,
        &functional_dependencies,
        &wanted_arguments,
        &provided_arguments,
    )? {
        MatchType::Match { .. } => {
            let unifications = iter::zip(wanted_arguments, provided_arguments).collect_vec();
            Ok(MatchInstance::Match { unifications, constraints: vec![] })
        }
        MatchType::Apart => Ok(MatchInstance::Apart),
        MatchType::Stuck { stuck, skolem } => Ok(MatchInstance::Stuck { stuck, skolem }),
    }
}

pub fn match_declared<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    wanted: CanonicalConstraintId,
    candidate: InstanceCandidate,
) -> QueryResult<MatchInstance>
where
    Q: ExternalQueries,
{
    let Some(declared) = toolkit::instance_info(
        state,
        context,
        candidate.instance.matchable,
        candidate.instance.resolution,
    )?
    else {
        return Ok(MatchInstance::Apart);
    };

    let wanted = state.canonicals[wanted].clone();

    let pattern_variables: FxHashSet<Name> =
        declared.binders.iter().map(|binder| binder.name).collect();

    let functional_dependencies =
        get_functional_dependencies(state, context, wanted.file_id, wanted.type_id)?;

    let wanted_arguments = type_arguments(&wanted.arguments);
    let declared_arguments = type_arguments(&declared.arguments);

    match match_instance(
        state,
        context,
        &pattern_variables,
        &functional_dependencies,
        &wanted_arguments,
        &declared_arguments,
    )? {
        MatchType::Match { bindings } => {
            let matched_names: FxHashSet<_> = bindings.iter().map(|(name, _)| *name).collect();

            let mut substitution = FxHashMap::default();
            for (name, bound) in bindings {
                substitution.entry(name).or_insert(bound);
            }

            for binder in &declared.binders {
                if substitution.contains_key(&binder.name) {
                    continue;
                }
                let binder_kind = SubstituteName::many(state, context, &substitution, binder.kind)?;
                let binder_type = state.fresh_unification(context.queries, binder_kind);
                substitution.insert(binder.name, binder_type);
            }

            let mut unifications = vec![];
            for binder in &declared.binders {
                if !matched_names.contains(&binder.name) {
                    continue;
                }
                let expected_kind =
                    SubstituteName::many(state, context, &substitution, binder.kind)?;
                let actual_kind =
                    types::elaborate_kind(state, context, substitution[&binder.name])?;
                unifications.push((actual_kind, expected_kind));
            }

            for (wanted, declared) in iter::zip(wanted_arguments, declared_arguments) {
                let wanted = SubstituteName::many(state, context, &substitution, wanted)?;
                let declared = SubstituteName::many(state, context, &substitution, declared)?;
                unifications.push((wanted, declared));
            }

            let mut constraints = vec![];
            for constraint in declared.constraints {
                let constraint = SubstituteName::many(state, context, &substitution, constraint)?;
                if let Some(constraint) = canonical::canonicalise(state, context, constraint)? {
                    constraints.push(constraint);
                }
            }

            Ok(MatchInstance::Match { unifications, constraints })
        }
        MatchType::Apart => Ok(MatchInstance::Apart),
        MatchType::Stuck { stuck, skolem } => Ok(MatchInstance::Stuck { stuck, skolem }),
    }
}

pub fn declared_instances_overlap<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    left: CheckedInstance,
    left_arguments: &[TypeId],
    right: CheckedInstance,
    right_arguments: &[TypeId],
) -> QueryResult<bool>
where
    Q: ExternalQueries,
{
    if left.resolution != right.resolution {
        return Ok(false);
    }

    let (file_id, type_id) = left.resolution;

    if left_arguments.len() != right_arguments.len() {
        return Ok(false);
    }

    let functional_dependencies = get_functional_dependencies(state, context, file_id, type_id)?;
    instances_overlap(state, context, &functional_dependencies, left_arguments, right_arguments)
}

pub(crate) fn type_arguments(arguments: &[ApplicationArgument]) -> Vec<TypeId> {
    arguments
        .iter()
        .filter_map(
            |argument| {
                if let ApplicationArgument::Type(id) = argument { Some(*id) } else { None }
            },
        )
        .collect_vec()
}

fn instances_overlap<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    functional_dependencies: &[Fd],
    left_arguments: &[TypeId],
    right_arguments: &[TypeId],
) -> QueryResult<bool>
where
    Q: ExternalQueries,
{
    let all_positions = FxHashSet::from_iter(0..left_arguments.len());
    let mut known_non_apart = FxHashSet::default();
    let mut possibly_non_apart = all_positions.clone();

    // Closure monotonicity lets these bounds decide whether the final non-apart positions contain
    // a covering set before every argument has been compared.
    if positions_cover_all(functional_dependencies, &known_non_apart, &all_positions) {
        return Ok(true);
    }

    for position in 0..left_arguments.len() {
        let apart = types_apart(
            state,
            context,
            left_arguments[position],
            right_arguments[position],
            false,
        )?
        .is_apart();

        if apart {
            possibly_non_apart.remove(&position);
            if !positions_cover_all(functional_dependencies, &possibly_non_apart, &all_positions) {
                return Ok(false);
            }
        } else {
            known_non_apart.insert(position);
            if positions_cover_all(functional_dependencies, &known_non_apart, &all_positions) {
                return Ok(true);
            }
        }
    }

    Ok(positions_cover_all(functional_dependencies, &known_non_apart, &all_positions))
}

fn types_apart<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    left: TypeId,
    right: TypeId,
    comparing_kind_argument: bool,
) -> QueryResult<MatchType>
where
    Q: ExternalQueries,
{
    // Declaration-time overlap checking follows PureScript's conservative
    // head-apartness approximation rather than solver unification. In
    // particular, kinded applications may be separated by their explicit
    // kind arguments even when their value-level arguments could still unify;
    // any remaining ambiguity is reported at instance-search sites.
    let left = normalise::expand(state, context, left)?;
    let right = normalise::expand(state, context, right)?;

    let left_core = context.lookup_type(left);
    let right_core = context.lookup_type(right);

    match (left_core, right_core) {
        (Type::Kinded(left, _), _) => {
            types_apart(state, context, *left, right, comparing_kind_argument)
        }
        (_, Type::Kinded(right, _)) => {
            types_apart(state, context, left, *right, comparing_kind_argument)
        }

        (Type::Rigid(_, _, _), Type::Rigid(_, _, _))
        | (Type::Unification(_), Type::Unification(_)) => Ok(MatchType::Match { bindings: vec![] }),

        (Type::Rigid(_, _, _) | Type::Unification(_), _)
        | (_, Type::Rigid(_, _, _) | Type::Unification(_)) => Ok(if comparing_kind_argument {
            MatchType::Apart
        } else {
            MatchType::Match { bindings: vec![] }
        }),

        (Type::Constructor(left_file, left_item), Type::Constructor(right_file, right_item)) => {
            Ok(if (left_file, left_item) != (right_file, right_item) {
                MatchType::Apart
            } else {
                MatchType::Match { bindings: vec![] }
            })
        }

        (Type::String(_, left), Type::String(_, right)) => {
            Ok(if left != right { MatchType::Apart } else { MatchType::Match { bindings: vec![] } })
        }
        (Type::Integer(left), Type::Integer(right)) => {
            Ok(if left != right { MatchType::Apart } else { MatchType::Match { bindings: vec![] } })
        }

        (
            Type::Application(left_function, left_argument),
            Type::Application(right_function, right_argument),
        ) => types_apart(state, context, *left_function, *right_function, false)?
            .and_then(|| types_apart(state, context, *left_argument, *right_argument, false)),

        (Type::Application(_, _), Type::Function(right_argument, right_result)) => {
            let right = context.intern_function_application(*right_argument, *right_result);
            types_apart(state, context, left, right, comparing_kind_argument)
        }

        (Type::Function(left_argument, left_result), Type::Application(_, _)) => {
            let left = context.intern_function_application(*left_argument, *left_result);
            types_apart(state, context, left, right, comparing_kind_argument)
        }

        (
            Type::KindApplication(left_function, left_argument),
            Type::KindApplication(right_function, right_argument),
        ) => types_apart(state, context, *left_function, *right_function, false)?
            .and_then(|| types_apart(state, context, *left_argument, *right_argument, true)),

        (
            Type::Function(left_argument, left_result),
            Type::Function(right_argument, right_result),
        ) => types_apart(state, context, *left_argument, *right_argument, false)?
            .and_then(|| types_apart(state, context, *left_result, *right_result, false)),

        (Type::Row(left), Type::Row(right)) => compare_row_types_with(
            state,
            context,
            *left,
            *right,
            &mut |state, context, left, right| types_apart(state, context, left, right, false),
        ),

        (_, _) => Ok(MatchType::Apart),
    }
}
