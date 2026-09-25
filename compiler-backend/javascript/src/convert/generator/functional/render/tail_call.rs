//! Tail-recursive function groups eligible for loop rendering.

use functional::tree::{
    EffectExpression, ExpressionId, ExpressionKind, GlobalId, LocalId, Module, PatternId,
    PatternKind, RecursiveGroupId,
};
use rustc_hash::{FxHashMap, FxHashSet};
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
    pub(super) effect_step: bool,
}

pub(super) struct TailCallGroup {
    pub(super) dispatcher_name: SmolStr,
    pub(super) profiles: Vec<TailCallProfile>,
    pub(super) maximum_arity: usize,
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
    pub(super) argument_names: Vec<SmolStr>,
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
        argument_names: Vec<SmolStr>,
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
        argument_names: Vec<SmolStr>,
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
        let (identity, arguments, uncurried) = application(module, expression)?;
        let target = self.targets.get(&identity)?;
        if target.uncurried != uncurried || target.arity != arguments.len() {
            return None;
        }
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

pub(super) fn tail_call_group(
    module: &Module,
    mut profiles: Vec<TailCallProfile>,
    dispatcher_name: SmolStr,
) -> Option<TailCallGroup> {
    if profiles.is_empty() {
        return None;
    }

    let targets = profiles.iter().enumerate().map(|(position, profile)| {
        (profile.identity, (position, profile.parameters.len(), profile.uncurried))
    });
    let targets = targets.collect::<FxHashMap<_, _>>();
    let mut edges = Vec::new();
    for (source, profile) in profiles.iter().enumerate() {
        collect_tail_edges(module, profile.body, TailPosition::Value, source, &targets, &mut edges);
    }
    if edges.is_empty() {
        return None;
    }

    // Effect-tail calls execute beneath the returned thunk, so their connected tail-call
    // component uses the boxed step protocol. Value-tail edges preserve the return type and
    // therefore propagate that requirement to both endpoints.
    let mut effect_steps = FxHashSet::default();
    for edge in &edges {
        if edge.position == TailPosition::Effect {
            effect_steps.insert(edge.source);
            effect_steps.insert(edge.target);
        }
    }
    loop {
        let previous_length = effect_steps.len();
        for edge in &edges {
            if effect_steps.contains(&edge.source) || effect_steps.contains(&edge.target) {
                effect_steps.insert(edge.source);
                effect_steps.insert(edge.target);
            }
        }
        if effect_steps.len() == previous_length {
            break;
        }
    }
    for (position, profile) in profiles.iter_mut().enumerate() {
        profile.effect_step = effect_steps.contains(&position);
    }

    let maximum_arity = profiles.iter().map(|profile| profile.parameters.len()).max().unwrap_or(0);
    Some(TailCallGroup { dispatcher_name, profiles, maximum_arity })
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
                effect_step: false,
            })
        }
        ExpressionKind::UncurriedAbstraction { parameters, body } => Some(TailCallProfile {
            identity,
            parameters: parameters.to_vec(),
            body: *body,
            uncurried: true,
            effect_step: false,
        }),
        ExpressionKind::Error
        | ExpressionKind::Literal { .. }
        | ExpressionKind::Array { .. }
        | ExpressionKind::Record { .. }
        | ExpressionKind::RecordUpdate { .. }
        | ExpressionKind::Project { .. }
        | ExpressionKind::Unary { .. }
        | ExpressionKind::Binary { .. }
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
    let mut pending = vec![(expression, position)];
    while let Some((expression, position)) = pending.pop() {
        if let Some((identity, arguments, uncurried)) = application(module, expression)
            && let Some(&(target, arity, target_uncurried)) = targets.get(&identity)
            && arguments.len() == arity
            && uncurried == target_uncurried
        {
            edges.push(TailEdge { source, target, position });
            continue;
        }

        match &module.storage[expression].kind {
            ExpressionKind::IfThenElse { then, else_, .. } => {
                pending.push((*else_, position));
                pending.push((*then, position));
            }
            ExpressionKind::Case { alternatives, .. } => {
                let alternatives = alternatives.iter().rev();
                pending.extend(alternatives.map(|alternative| (alternative.expression, position)));
            }
            ExpressionKind::Guarded { alternatives } => {
                let alternatives = alternatives.iter().rev();
                pending.extend(alternatives.map(|alternative| (alternative.expression, position)));
            }
            ExpressionKind::Let { body, .. } | ExpressionKind::LetPattern { body, .. } => {
                pending.push((*body, position));
            }
            ExpressionKind::Effect { effect } => match effect {
                EffectExpression::Bind { body, .. } => {
                    pending.push((*body, TailPosition::Effect));
                }
                EffectExpression::Pure(_)
                | EffectExpression::Map { .. }
                | EffectExpression::Apply { .. } => {}
            },
            ExpressionKind::Error
            | ExpressionKind::Literal { .. }
            | ExpressionKind::Array { .. }
            | ExpressionKind::Record { .. }
            | ExpressionKind::RecordUpdate { .. }
            | ExpressionKind::Project { .. }
            | ExpressionKind::Unary { .. }
            | ExpressionKind::Binary { .. }
            | ExpressionKind::Constructor { .. }
            | ExpressionKind::Global { .. }
            | ExpressionKind::Local { .. }
            | ExpressionKind::Abstraction { .. }
            | ExpressionKind::UncurriedAbstraction { .. }
            | ExpressionKind::Application { .. }
            | ExpressionKind::UncurriedApplication { .. }
            | ExpressionKind::StyleX(_)
            | ExpressionKind::SynthesizedEvidence { .. }
            | ExpressionKind::TrivialEvidence => {}
        }
    }
}

fn application(
    module: &Module,
    expression: ExpressionId,
) -> Option<(TailCallIdentity, Vec<ExpressionId>, bool)> {
    match &module.storage[expression].kind {
        ExpressionKind::Application { .. } => {
            let mut function = expression;
            let mut groups = Vec::new();
            while let ExpressionKind::Application { function: inner, arguments, .. } =
                &module.storage[function].kind
            {
                groups.push(arguments.as_ref());
                function = *inner;
            }
            let identity = function_identity(module, function)?;
            let arguments = groups.into_iter().rev().flatten().copied().collect();
            Some((identity, arguments, false))
        }
        ExpressionKind::UncurriedApplication { function, arguments, .. } => {
            let identity = function_identity(module, *function)?;
            Some((identity, arguments.to_vec(), true))
        }
        ExpressionKind::Error
        | ExpressionKind::Literal { .. }
        | ExpressionKind::Array { .. }
        | ExpressionKind::Record { .. }
        | ExpressionKind::RecordUpdate { .. }
        | ExpressionKind::Project { .. }
        | ExpressionKind::Unary { .. }
        | ExpressionKind::Binary { .. }
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
        | ExpressionKind::SynthesizedEvidence { .. }
        | ExpressionKind::TrivialEvidence => None,
    }
}

fn function_identity(module: &Module, expression: ExpressionId) -> Option<TailCallIdentity> {
    match &module.storage[expression].kind {
        ExpressionKind::Global { global } => Some(TailCallIdentity::Global(global.id)),
        ExpressionKind::Local { parameter } => Some(TailCallIdentity::Local(parameter.id)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use files::FileId;
    use functional::tree::{
        EffectExpression, Expression, ExpressionKind, LocalId, Module, ModuleSurface, Parameter,
        Storage,
    };
    use smol_str::SmolStr;

    use super::{TailCallIdentity, TailCallProfile, tail_call_group};

    /// Runs `test` on a thread whose stack is far too small for a recursive traversal of a long
    /// chain, so recursion fails the test instead of passing on the generous default stack.
    fn with_small_stack(test: impl FnOnce() + Send + 'static) {
        let thread = std::thread::Builder::new()
            .stack_size(64 * 1024)
            .spawn(test)
            .expect("failed to spawn small-stack thread");
        thread.join().expect("test panicked on small-stack thread");
    }

    #[test]
    fn tail_edges_beneath_long_effect_chains_do_not_use_the_call_stack() {
        with_small_stack(|| {
            let mut storage = Storage::default();
            let function = Parameter { id: LocalId(0), name: SmolStr::new("loop") };
            let callee = storage.allocate_expression(Expression {
                kind: ExpressionKind::Local { parameter: Parameter::clone(&function) },
            });
            let mut body = storage.allocate_expression(Expression {
                kind: ExpressionKind::Application {
                    function: callee,
                    arguments: Arc::from([]),
                    synthetic: false,
                },
            });
            for index in 1..10_000 {
                let action = storage.allocate_expression(Expression {
                    kind: ExpressionKind::Local { parameter: Parameter::clone(&function) },
                });
                let parameter = Parameter { id: LocalId(index), name: SmolStr::new("value") };
                let effect = EffectExpression::Bind { action, parameter, body };
                body = storage
                    .allocate_expression(Expression { kind: ExpressionKind::Effect { effect } });
            }
            let module = Module {
                file_id: FileId::new(0),
                name: SmolStr::new("Main"),
                dependencies: Arc::from([]),
                surface: ModuleSurface::default(),
                declarations: Arc::from([]),
                storage,
            };
            let profile = TailCallProfile {
                identity: TailCallIdentity::Local(function.id),
                parameters: Vec::new(),
                body,
                uncurried: false,
                effect_step: false,
            };

            let group = tail_call_group(&module, vec![profile], SmolStr::new("$tail"))
                .expect("the effect chain ends in a tail call");

            assert!(group.profiles[0].effect_step);
        });
    }
}
