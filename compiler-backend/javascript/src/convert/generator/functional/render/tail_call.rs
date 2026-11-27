//! Tail-recursive function groups eligible for loop rendering.

use std::rc::Rc;

use functional::initializers::initializer_components;
use functional::tree::{
    EffectExpression, ExpressionId, ExpressionKind, GlobalId, LocalId, Module, Origin, PatternId,
    PatternKind, RecursiveGroupId,
};
use itertools::Itertools;
use rustc_hash::FxHashMap;
use smol_str::{SmolStr, format_smolstr};

use crate::error::ModuleResult;
use crate::tree::{Tree, UnaryOperator};
use crate::writer::Writer;

use super::{
    Destination, FunctionContext, Generator, LocalBinding, OutputFile, TailCallWrapper,
    declaration_source_name,
};

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

struct CurriedTailCallWrapper<'a> {
    group: &'a TailCallGroup,
    state: usize,
    arguments: &'a [SmolStr],
    remaining: &'a [SmolStr],
}

struct TailCallWrapperResult<'a> {
    group: &'a TailCallGroup,
    state: usize,
    arguments: &'a [SmolStr],
}

pub(super) fn render_global_tail_call_group<'t>(
    generator: &Generator<'_>,
    tree: &mut Tree<'t>,
    writer: &mut Writer<'t>,
    file: OutputFile<'_>,
    group: &TailCallGroup,
) -> ModuleResult<()> {
    let mut context = FunctionContext::new(&generator.reserved_module_names);
    if !group.is_singleton() {
        let state_name = context.allocate("$state");
        let argument_names = (0..group.maximum_arity)
            .map(|position| context.allocate(format_smolstr!("$argument{position}")))
            .collect::<Rc<[_]>>();
        let tail_calls =
            TailCallContext::new(group, state_name.clone(), Rc::clone(&argument_names));
        let mut dispatcher_parameters = vec![state_name];
        dispatcher_parameters.extend(argument_names.iter().cloned());
        context.tail_calls = Some(tail_calls);
        writer.function(&group.dispatcher_name, None, dispatcher_parameters, false, |writer| {
            generator.render_tail_call_dispatcher(tree, writer, group, &mut context)
        })?;
        context.tail_calls = None;
    }

    for (state, profile) in group.profiles.iter().enumerate() {
        let TailCallIdentity::Global(global) = profile.identity else {
            unreachable!("invariant violated: global tail-call group contains a local")
        };
        let declaration = generator
            .module
            .declarations
            .iter()
            .find(|declaration| declaration.global.id == global)
            .expect("invariant violated: tail-call profile has no declaration");
        let _origin = tree.enter_origin(declaration.origin);
        let name = generator.global_name(global);
        let source_name = declaration_source_name(writer, declaration);
        let exported = file.declaration_is_inline_exported(generator, declaration);
        generator.render_global_tail_call_wrapper(
            tree,
            writer,
            TailCallWrapper { name, source_name, group, profile, state },
            exported,
            &mut context,
        )?;
    }
    Ok(())
}

impl Generator<'_> {
    pub(super) fn render_tail_call_dispatcher<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        group: &TailCallGroup,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        let condition = tree.boolean(true);
        writer.while_loop(tree, condition, |tree, writer| {
            if group.is_singleton() {
                return self.render_tail_call_profile(tree, writer, group, 0, context);
            }
            let tail_calls = context
                .tail_calls
                .as_ref()
                .expect("invariant violated: tail-call dispatcher has no context");
            let state_name = tail_calls
                .state_name
                .as_ref()
                .expect("invariant violated: mutual tail-call dispatcher has no state");
            let current_state = tree.identifier(state_name);
            let cases = group.profiles.iter().enumerate().map(|(state, profile)| {
                let state = tree.number(state.to_string());
                let name = self.tail_call_profile_name(profile, context);
                (state, name)
            });
            let cases = cases.collect_vec();
            writer.switch(tree, current_state, cases, |position, tree, writer| {
                self.render_tail_call_profile(tree, writer, group, position, context)
            })
        })
    }

    fn tail_call_profile_name(
        &self,
        profile: &TailCallProfile,
        context: &FunctionContext,
    ) -> SmolStr {
        match profile.identity {
            TailCallIdentity::Global(global) => self.global_names[&global].clone(),
            TailCallIdentity::Local(local) => match context.locals.get(&local) {
                Some(LocalBinding::Direct(name)) => name.clone(),
                Some(
                    LocalBinding::Inline(_) | LocalBinding::Lazy(_) | LocalBinding::Reference(_),
                )
                | None => {
                    unreachable!("invariant violated: tail-call profile has no direct local name")
                }
            },
        }
    }

    fn render_tail_call_profile<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        group: &TailCallGroup,
        state: usize,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        let profile = &group.profiles[state];
        let _origin = tree.enter_origin(profile.origin);
        let destination = if group.effect_step {
            Destination::TailEffectThunkReturn
        } else {
            Destination::Return
        };
        self.render_tail_call_parameters(tree, writer, profile, 0, destination, context)
    }

    fn render_tail_call_parameters<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        profile: &TailCallProfile,
        position: usize,
        destination: Destination<'_>,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        let Some(pattern) = profile.parameters.get(position).copied() else {
            return self.render_expression(tree, writer, profile.body, destination, context);
        };
        let argument = context
            .tail_calls
            .as_ref()
            .expect("invariant violated: tail-call profile has no context")
            .argument_names
            .get(position)
            .expect("invariant violated: tail-call profile exceeds dispatcher arity")
            .clone();
        // A closure created during an iteration must capture that iteration's value rather than
        // the mutable slot that advances the loop.
        let current_argument = context.allocate(format_smolstr!("$currentArgument{position}"));
        let value = tree.identifier(&argument);
        writer.constant(tree, &current_argument, value, false);
        let value = tree.identifier(&current_argument);
        let plan = self.pattern_plan(tree, pattern, value, Some(&current_argument), context)?;
        self.render_pattern_scope(tree, writer, plan, context, |tree, writer, context| {
            self.render_tail_call_parameters(
                tree,
                writer,
                profile,
                position + 1,
                destination,
                context,
            )
        })
    }

    fn render_global_tail_call_wrapper<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        wrapper: TailCallWrapper<'_>,
        exported: bool,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        let TailCallWrapper { name, source_name, group, profile, state } = wrapper;
        let arguments = profile
            .parameters
            .iter()
            .map(|pattern| self.allocate_pattern_argument(*pattern, context))
            .collect_vec();
        if profile.uncurried {
            return writer.function(name, source_name, arguments.clone(), exported, |writer| {
                self.render_tail_call_wrapper_result(
                    tree,
                    writer,
                    TailCallWrapperResult { group, state, arguments: &arguments },
                    context,
                )
            });
        }
        let Some((first, remaining)) = arguments.split_first() else {
            unreachable!("invariant violated: curried tail-call function has no parameters")
        };
        writer.function(name, source_name, vec![first.clone()], exported, |writer| {
            self.render_curried_tail_call_wrapper(
                tree,
                writer,
                CurriedTailCallWrapper { group, state, arguments: &arguments, remaining },
                context,
            )
        })
    }

    pub(super) fn render_local_tail_call_wrapper<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        wrapper: TailCallWrapper<'_>,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        let TailCallWrapper { name, source_name, group, profile, state } = wrapper;
        let _origin = tree.enter_origin(profile.origin);
        let arguments = profile
            .parameters
            .iter()
            .map(|pattern| self.allocate_pattern_argument(*pattern, context))
            .collect_vec();
        if profile.uncurried {
            return writer.constant_arrow(name, source_name, arguments.clone(), |writer| {
                self.render_tail_call_wrapper_result(
                    tree,
                    writer,
                    TailCallWrapperResult { group, state, arguments: &arguments },
                    context,
                )
            });
        }
        let Some((first, remaining)) = arguments.split_first() else {
            unreachable!("invariant violated: curried tail-call function has no parameters")
        };
        writer.constant_arrow(name, source_name, vec![first.clone()], |writer| {
            self.render_curried_tail_call_wrapper(
                tree,
                writer,
                CurriedTailCallWrapper { group, state, arguments: &arguments, remaining },
                context,
            )
        })
    }

    fn render_curried_tail_call_wrapper<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        wrapper: CurriedTailCallWrapper<'_>,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        let CurriedTailCallWrapper { group, state, arguments, remaining } = wrapper;
        let Some((argument, remaining)) = remaining.split_first() else {
            return self.render_tail_call_wrapper_result(
                tree,
                writer,
                TailCallWrapperResult { group, state, arguments },
                context,
            );
        };
        writer.return_arrow(vec![argument.clone()], |writer| {
            self.render_curried_tail_call_wrapper(
                tree,
                writer,
                CurriedTailCallWrapper { group, state, arguments, remaining },
                context,
            )
        })
    }

    fn render_tail_call_wrapper_result<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        wrapper: TailCallWrapperResult<'_>,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        let TailCallWrapperResult { group, state, arguments } = wrapper;
        if group.is_singleton() && !group.effect_step {
            return self.render_singleton_tail_call_loop(tree, writer, group, arguments, context);
        }
        if group.is_singleton() {
            self.render_singleton_effect_dispatcher(tree, writer, group, context)?;
        }

        let dispatcher = tree.identifier(&group.dispatcher_name);
        let state = tree.number(state.to_string());
        let mut values = vec![state];
        values.extend(arguments.iter().map(|argument| tree.identifier(argument)));
        for _ in arguments.len()..group.maximum_arity {
            values.push(tree.null());
        }
        let initial = tree.call(dispatcher, values);
        if !group.effect_step {
            writer.return_expression(tree, initial);
            return Ok(());
        }

        let initial_name = context.allocate("$initialStep");
        writer.constant(tree, &initial_name, initial, false);
        writer.return_arrow(Vec::new(), |writer| {
            self.render_tail_effect_loop(tree, writer, group, &initial_name, context)
        })
    }

    fn render_singleton_tail_call_loop<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        group: &TailCallGroup,
        arguments: &[SmolStr],
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        // Curried outer arguments can be captured by a reusable partial application. Copy every
        // argument before iterating so one invocation cannot mutate the next invocation's input.
        let mut argument_names = Vec::with_capacity(arguments.len());
        for (position, argument) in arguments.iter().enumerate() {
            let name = context.allocate(format_smolstr!("$argument{position}"));
            let value = tree.identifier(argument);
            writer.mutable_value(tree, &name, value);
            argument_names.push(name);
        }
        let tail_calls = TailCallContext::singleton(group, argument_names.into());
        let outer_tail_calls = context.tail_calls.replace(tail_calls);
        let result = self.render_tail_call_dispatcher(tree, writer, group, context);
        context.tail_calls = outer_tail_calls;
        result
    }

    fn render_singleton_effect_dispatcher<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        group: &TailCallGroup,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        let state_name = context.allocate("$state");
        let argument_names = (0..group.maximum_arity)
            .map(|position| context.allocate(format_smolstr!("$argument{position}")))
            .collect::<Rc<[_]>>();
        let tail_calls =
            TailCallContext::new(group, state_name.clone(), Rc::clone(&argument_names));
        let mut dispatcher_parameters = vec![state_name];
        dispatcher_parameters.extend(argument_names.iter().cloned());
        let outer_tail_calls = context.tail_calls.replace(tail_calls);
        writer.constant_arrow(&group.dispatcher_name, None, dispatcher_parameters, |writer| {
            self.render_tail_call_dispatcher(tree, writer, group, context)
        })?;
        context.tail_calls = outer_tail_calls;
        Ok(())
    }

    fn render_tail_effect_loop<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        group: &TailCallGroup,
        initial_name: &str,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        // A step returns `[false, value]` when complete or `[true, state, ...arguments]` for a
        // tail call. Constructing the next step only after the current action has run preserves
        // Effect construction timing, while resetting to the captured initial step makes the
        // generated thunk safely reusable.
        let step_name = context.allocate("$step");
        let result_name = context.allocate("$result");
        writer.mutable(&step_name);
        let initial = tree.identifier(initial_name);
        writer.assign(tree, &step_name, initial);
        let condition = tree.boolean(true);
        writer.while_loop(tree, condition, |tree, writer| {
            let step = tree.identifier(&step_name);
            let result = tree.call(step, Vec::new());
            writer.constant(tree, &result_name, result, false);
            let result = tree.identifier(&result_name);
            let marker_index = tree.number("0");
            let marker = tree.index(result, marker_index);
            let complete = tree.unary(UnaryOperator::LogicalNot, marker);
            writer.if_block(tree, complete, |tree, writer| {
                let result = tree.identifier(&result_name);
                let value_index = tree.number("1");
                let value = tree.index(result, value_index);
                writer.return_expression(tree, value);
            });

            let dispatcher = tree.identifier(&group.dispatcher_name);
            let values = (0..=group.maximum_arity).map(|position| {
                let result = tree.identifier(&result_name);
                let index = tree.number((position + 1).to_string());
                tree.index(result, index)
            });
            let values = values.collect_vec();
            let next_step = tree.call(dispatcher, values);
            writer.assign(tree, &step_name, next_step);
            Ok(())
        })
    }

    pub(super) fn render_tail_call<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        tail_call: TailCall,
        destination: Destination<'_>,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        let target = tail_call.target;
        let tail_calls = context
            .tail_calls
            .as_ref()
            .expect("invariant violated: rendered tail call has no context");
        let state_name = tail_calls.state_name.clone();
        let argument_names = Rc::clone(&tail_calls.argument_names);

        if matches!(destination, Destination::EffectTailEffectReturn) {
            let marker = tree.boolean(true);
            let state = tree.number(target.state.to_string());
            let mut values = vec![marker, state];
            for argument in tail_call.arguments {
                let value = self.expression_value(tree, writer, argument, context)?;
                let value = if tree.expression_is_atomic(&value) {
                    value
                } else {
                    self.materialize_value(tree, writer, value, "$tailArgument", context)
                };
                values.push(value);
            }
            for _ in target.arity..argument_names.len() {
                values.push(tree.null());
            }
            let result = tree.array(values);
            writer.return_expression(tree, result);
            return Ok(());
        }

        for (name, argument) in argument_names.iter().zip(tail_call.arguments) {
            let value = self.expression_value(tree, writer, argument, context)?;
            if tree.expression_identifier(&value) == Some(name) {
                continue;
            }
            writer.assign(tree, name, value);
        }
        for name in argument_names.iter().skip(target.arity) {
            let null = tree.null();
            writer.assign(tree, name, null);
        }
        if let Some(state_name) = state_name {
            let state = tree.number(target.state.to_string());
            writer.assign(tree, &state_name, state);
        }
        writer.continue_loop();
        Ok(())
    }
}
