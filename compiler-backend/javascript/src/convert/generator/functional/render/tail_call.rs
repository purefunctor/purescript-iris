//! Tail-recursive function groups eligible for loop rendering.

use std::rc::Rc;

use functional::initializers::initializer_components;
use functional::tree::{
    EffectExpression, ExpressionId, ExpressionKind, GlobalId, LocalId, Module, Origin, PatternId,
    PatternKind, RecursiveGroupId,
};
use itertools::Itertools;
use rustc_hash::FxHashMap;
use smol_str::SmolStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum TailCallIdentity {
    Global(GlobalId),
    Local(LocalId),
}

#[derive(Clone)]
pub(super) struct TailCallProfile {
    pub(super) identity: TailCallIdentity,
    pub(super) parameters: Vec<PatternId>,
    pub(super) body: ExpressionId,
    pub(super) uncurried: bool,
    pub(super) origin: Option<Origin>,
}

pub(super) struct TailCallGroup {
    pub(super) dispatcher_name: SmolStr,
    pub(super) profiles: Vec<TailCallProfile>,
    pub(super) maximum_arity: usize,
    pub(super) effect_step: bool,
}

#[derive(Clone)]
pub(super) struct TailCallTarget {
    pub(super) state: usize,
    pub(super) arity: usize,
    pub(super) uncurried: bool,
}

#[derive(Clone)]
pub(super) struct TailCall {
    pub(super) target: TailCallTarget,
    pub(super) arguments: Vec<ExpressionId>,
}

#[derive(Clone)]
pub(super) struct TailCallContext {
    pub(super) state_name: Option<SmolStr>,
    pub(super) argument_names: Rc<[SmolStr]>,
    targets: FxHashMap<TailCallIdentity, TailCallTarget>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TailPosition {
    Value,
    Effect,
}

struct TailEdge {
    source: usize,
    target: usize,
    position: TailPosition,
}

impl TailCallContext {
    pub(super) fn new(
        group: &TailCallGroup,
        state_name: SmolStr,
        argument_names: Rc<[SmolStr]>,
    ) -> TailCallContext {
        let targets = group.profiles.iter().enumerate().map(|(state, profile)| {
            let target = TailCallTarget {
                state,
                arity: profile.parameters.len(),
                uncurried: profile.uncurried,
            };
            (profile.identity, target)
        });
        let targets = targets.collect();
        TailCallContext { state_name: Some(state_name), argument_names, targets }
    }

    pub(super) fn singleton(
        group: &TailCallGroup,
        argument_names: Rc<[SmolStr]>,
    ) -> TailCallContext {
        assert!(group.is_singleton(), "invariant violated: inline tail-call group is mutual");
        let profile = &group.profiles[0];
        let target = TailCallTarget {
            state: 0,
            arity: profile.parameters.len(),
            uncurried: profile.uncurried,
        };
        let targets = FxHashMap::from_iter([(profile.identity, target)]);
        TailCallContext { state_name: None, argument_names, targets }
    }

    pub(super) fn call(&self, module: &Module, expression: ExpressionId) -> Option<TailCall> {
        let head = application_head(module, expression)?;
        let target = self.targets.get(&head.identity)?;
        if target.uncurried != head.uncurried || target.arity != head.arity {
            return None;
        }
        let arguments = application_arguments(module, expression);
        Some(TailCall { target: target.clone(), arguments })
    }
}

impl TailCallGroup {
    pub(super) fn is_singleton(&self) -> bool {
        self.profiles.len() == 1
    }
}

pub(super) fn global_profiles(module: &Module) -> Vec<Vec<TailCallProfile>> {
    let mut positions = FxHashMap::default();
    let mut groups = Vec::<(RecursiveGroupId, Vec<TailCallProfile>)>::new();
    for declaration in module.declarations.iter() {
        let Some(recursive_group) = declaration.recursive_group else { continue };
        let functional::tree::DeclarationKind::Value(expression) = declaration.kind else {
            continue;
        };
        let identity = TailCallIdentity::Global(declaration.global.id);
        let Some(profile) = function_profile(module, identity, expression) else { continue };
        let position = *positions.entry(recursive_group).or_insert_with(|| {
            let position = groups.len();
            groups.push((recursive_group, Vec::new()));
            position
        });
        groups[position].1.push(profile);
    }
    groups.into_iter().map(|(_, profiles)| profiles).collect()
}

pub(super) fn local_profiles(
    module: &Module,
    bindings: &[functional::tree::Binding],
) -> Vec<TailCallProfile> {
    let profiles = bindings.iter().filter_map(|binding| {
        let identity = TailCallIdentity::Local(binding.parameter.id);
        function_profile(module, identity, binding.expression)
    });
    profiles.collect()
}

pub(super) fn tail_call_groups(
    module: &Module,
    profiles: &[TailCallProfile],
    mut dispatcher_name: impl FnMut(&[TailCallProfile]) -> SmolStr,
) -> Vec<TailCallGroup> {
    let targets = profiles.iter().enumerate().map(|(position, profile)| {
        (profile.identity, (position, profile.parameters.len(), profile.uncurried))
    });
    let targets = targets.collect::<FxHashMap<_, _>>();
    let mut edges = Vec::new();
    for (source, profile) in profiles.iter().enumerate() {
        collect_tail_edges(module, profile.body, TailPosition::Value, source, &targets, &mut edges);
    }

    let mut successors = vec![Vec::new(); profiles.len()];
    for edge in &edges {
        successors[edge.source].push(edge.target);
    }
    let mut components = initializer_components(&successors);
    components.retain(|component| {
        let [position] = component[..] else { return true };
        successors[position].contains(&position)
    });
    for component in &mut components {
        component.sort_unstable();
    }
    components.sort_unstable();

    let groups = components.into_iter().map(|component| {
        // Effect-tail calls execute beneath the returned thunk, so their connected tail-call
        // component uses the boxed step protocol.
        let effect_step = edges.iter().any(|edge| {
            edge.position == TailPosition::Effect
                && component.contains(&edge.source)
                && component.contains(&edge.target)
        });
        let profiles = component.iter().map(|&position| profiles[position].clone()).collect_vec();
        let maximum_arity =
            profiles.iter().map(|profile| profile.parameters.len()).max().unwrap_or(0);
        let dispatcher_name = dispatcher_name(&profiles);
        TailCallGroup { dispatcher_name, profiles, maximum_arity, effect_step }
    });
    groups.collect()
}

fn function_profile(
    module: &Module,
    identity: TailCallIdentity,
    expression: ExpressionId,
) -> Option<TailCallProfile> {
    match &module.storage[expression].kind {
        ExpressionKind::Abstraction { .. } => {
            let mut parameters = Vec::new();
            let mut body = expression;
            while let ExpressionKind::Abstraction {
                parameters: abstraction_parameters,
                body: abstraction_body,
            } = &module.storage[body].kind
            {
                parameters.extend(abstraction_parameters.iter().copied());
                body = *abstraction_body;
            }
            if matches!(module.storage[body].kind, ExpressionKind::UncurriedAbstraction { .. }) {
                return None;
            }
            let (_, partially_applied) = parameters.split_last()?;
            if !partially_applied
                .iter()
                .all(|pattern| pattern_is_partial_application_safe(module, *pattern))
            {
                return None;
            }
            Some(TailCallProfile {
                identity,
                parameters,
                body,
                uncurried: false,
                origin: module.storage[expression].origin,
            })
        }
        ExpressionKind::UncurriedAbstraction { parameters, body } => Some(TailCallProfile {
            identity,
            parameters: parameters.to_vec(),
            body: *body,
            uncurried: true,
            origin: module.storage[expression].origin,
        }),
        ExpressionKind::Literal { .. }
        | ExpressionKind::Array { .. }
        | ExpressionKind::ArrayConcatenation { .. }
        | ExpressionKind::Record { .. }
        | ExpressionKind::RecordUpdate { .. }
        | ExpressionKind::Project { .. }
        | ExpressionKind::Unary { .. }
        | ExpressionKind::Binary { .. }
        | ExpressionKind::Index { .. }
        | ExpressionKind::Constructor { .. }
        | ExpressionKind::Global { .. }
        | ExpressionKind::Local { .. }
        | ExpressionKind::Application { .. }
        | ExpressionKind::UncurriedApplication { .. }
        | ExpressionKind::StyleX(_)
        | ExpressionKind::IfThenElse { .. }
        | ExpressionKind::Case { .. }
        | ExpressionKind::Guarded { .. }
        | ExpressionKind::Let { .. }
        | ExpressionKind::LetPattern { .. }
        | ExpressionKind::Effect { .. }
        | ExpressionKind::Async(_)
        | ExpressionKind::SynthesizedEvidence { .. }
        | ExpressionKind::TrivialEvidence => None,
    }
}

fn pattern_is_partial_application_safe(module: &Module, pattern: PatternId) -> bool {
    // Curried wrappers defer body entry until every argument has reached the dispatcher. Moving a
    // check or destructuring read there would change when partial application evaluates it.
    match &module.storage[pattern].kind {
        PatternKind::Variable(_) | PatternKind::Wildcard => true,
        PatternKind::Named { pattern, .. } => pattern_is_partial_application_safe(module, *pattern),
        PatternKind::Literal(_)
        | PatternKind::Array(_)
        | PatternKind::Record(_)
        | PatternKind::Constructor { .. } => false,
    }
}

fn collect_tail_edges(
    module: &Module,
    expression: ExpressionId,
    position: TailPosition,
    source: usize,
    targets: &FxHashMap<TailCallIdentity, (usize, usize, bool)>,
    edges: &mut Vec<TailEdge>,
) {
    if let Some(head) = application_head(module, expression)
        && let Some(&(target, arity, uncurried)) = targets.get(&head.identity)
        && head.arity == arity
        && head.uncurried == uncurried
    {
        edges.push(TailEdge { source, target, position });
        return;
    }

    match &module.storage[expression].kind {
        ExpressionKind::IfThenElse { then, else_, .. } => {
            collect_tail_edges(module, *then, position, source, targets, edges);
            collect_tail_edges(module, *else_, position, source, targets, edges);
        }
        ExpressionKind::Case { alternatives, .. } => {
            for alternative in alternatives.iter() {
                collect_tail_edges(
                    module,
                    alternative.expression,
                    position,
                    source,
                    targets,
                    edges,
                );
            }
        }
        ExpressionKind::Guarded { alternatives } => {
            for alternative in alternatives.iter() {
                collect_tail_edges(
                    module,
                    alternative.expression,
                    position,
                    source,
                    targets,
                    edges,
                );
            }
        }
        ExpressionKind::Let { body, .. } | ExpressionKind::LetPattern { body, .. } => {
            collect_tail_edges(module, *body, position, source, targets, edges);
        }
        ExpressionKind::Effect { effect } => match effect {
            EffectExpression::Bind { body, .. } => {
                collect_tail_edges(module, *body, TailPosition::Effect, source, targets, edges);
            }
            EffectExpression::Pure(_)
            | EffectExpression::Map { .. }
            | EffectExpression::Apply { .. }
            | EffectExpression::ReferenceNew(_)
            | EffectExpression::ReferenceRead(_)
            | EffectExpression::ReferenceWrite { .. }
            | EffectExpression::UncurriedApplication { .. }
            | EffectExpression::Statement { .. } => {}
        },
        ExpressionKind::Literal { .. }
        | ExpressionKind::Array { .. }
        | ExpressionKind::ArrayConcatenation { .. }
        | ExpressionKind::Record { .. }
        | ExpressionKind::RecordUpdate { .. }
        | ExpressionKind::Project { .. }
        | ExpressionKind::Unary { .. }
        | ExpressionKind::Binary { .. }
        | ExpressionKind::Index { .. }
        | ExpressionKind::Constructor { .. }
        | ExpressionKind::Global { .. }
        | ExpressionKind::Local { .. }
        | ExpressionKind::Abstraction { .. }
        | ExpressionKind::UncurriedAbstraction { .. }
        | ExpressionKind::Application { .. }
        | ExpressionKind::UncurriedApplication { .. }
        | ExpressionKind::StyleX(_)
        | ExpressionKind::Async(_)
        | ExpressionKind::SynthesizedEvidence { .. }
        | ExpressionKind::TrivialEvidence => {}
    }
}

struct ApplicationHead {
    identity: TailCallIdentity,
    arity: usize,
    uncurried: bool,
}

fn application_head(module: &Module, expression: ExpressionId) -> Option<ApplicationHead> {
    match &module.storage[expression].kind {
        ExpressionKind::Application { .. } => {
            let mut function = expression;
            let mut arity = 0;
            while let ExpressionKind::Application { function: inner, arguments, .. } =
                &module.storage[function].kind
            {
                arity += arguments.len();
                function = *inner;
            }
            let identity = function_identity(module, function)?;
            Some(ApplicationHead { identity, arity, uncurried: false })
        }
        ExpressionKind::UncurriedApplication { function, arguments, .. } => {
            let identity = function_identity(module, *function)?;
            Some(ApplicationHead { identity, arity: arguments.len(), uncurried: true })
        }
        ExpressionKind::Literal { .. }
        | ExpressionKind::Array { .. }
        | ExpressionKind::ArrayConcatenation { .. }
        | ExpressionKind::Record { .. }
        | ExpressionKind::RecordUpdate { .. }
        | ExpressionKind::Project { .. }
        | ExpressionKind::Unary { .. }
        | ExpressionKind::Binary { .. }
        | ExpressionKind::Index { .. }
        | ExpressionKind::Constructor { .. }
        | ExpressionKind::Global { .. }
        | ExpressionKind::Local { .. }
        | ExpressionKind::Abstraction { .. }
        | ExpressionKind::UncurriedAbstraction { .. }
        | ExpressionKind::StyleX(_)
        | ExpressionKind::IfThenElse { .. }
        | ExpressionKind::Case { .. }
        | ExpressionKind::Guarded { .. }
        | ExpressionKind::Let { .. }
        | ExpressionKind::LetPattern { .. }
        | ExpressionKind::Effect { .. }
        | ExpressionKind::Async(_)
        | ExpressionKind::SynthesizedEvidence { .. }
        | ExpressionKind::TrivialEvidence => None,
    }
}

/// Collects the arguments of an application already recognised by [`application_head`].
fn application_arguments(module: &Module, expression: ExpressionId) -> Vec<ExpressionId> {
    if let ExpressionKind::UncurriedApplication { arguments, .. } = &module.storage[expression].kind
    {
        return arguments.to_vec();
    }
    let mut function = expression;
    let mut groups = Vec::new();
    while let ExpressionKind::Application { function: inner, arguments, .. } =
        &module.storage[function].kind
    {
        groups.push(arguments.as_ref());
        function = *inner;
    }
    groups.into_iter().rev().flatten().copied().collect()
}

fn function_identity(module: &Module, expression: ExpressionId) -> Option<TailCallIdentity> {
    match &module.storage[expression].kind {
        ExpressionKind::Global { global } => Some(TailCallIdentity::Global(global.id)),
        ExpressionKind::Local { parameter } => Some(TailCallIdentity::Local(parameter.id)),
        _ => None,
    }
}
