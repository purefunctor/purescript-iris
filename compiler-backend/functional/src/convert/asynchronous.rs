use std::sync::Arc;

use itertools::Itertools;
use rustc_hash::FxHashMap;

use crate::optimize::{local_uses, reachable_expressions};
use crate::tree::{
    AsyncExpression, Binding, Declaration, DeclarationKind, EffectExpression, ExpressionId,
    ExpressionKind, Origin, Parameter,
};

use super::{Context, ConversionResult};

const LOWERING_CONSTRUCTORS: [(&str, &str); 7] = [
    ("Iris.Effect.Fiber", "Pure"),
    ("Iris.Effect.Fiber", "Throw"),
    ("Iris.Effect.Fiber", "Run"),
    ("Iris.Effect.Fiber", "Bind"),
    ("Iris.Effect.Fiber", "Catch"),
    ("Data.Either", "Left"),
    ("Data.Either", "Right"),
];

#[derive(Clone, Copy)]
enum AsyncOperation {
    Pure,
    LiftEffect,
    Service,
    Sync,
    Provide,
    Map,
    Apply,
    KeepFirst,
    KeepSecond,
    Bind,
    Fail,
    Catch,
    MapError,
    Attempt,
}

#[derive(Clone)]
enum Continuation {
    Inline { parameter: Parameter, body: ExpressionId, origin: Option<Origin> },
    Join { function: Parameter },
}

#[derive(Clone, Copy, Default)]
struct StatementEffects {
    performs: bool,
    controls: bool,
}

impl<'c, Q> Context<'c, Q>
where
    Q: checking::ExternalQueries,
{
    pub(super) fn known_async_application(
        &mut self,
        function: ExpressionId,
        arguments: &[ExpressionId],
    ) -> ConversionResult<Option<ExpressionId>> {
        let mut selected = None;
        for (name, arity, operation) in [
            ("liftEffect", 1, AsyncOperation::LiftEffect),
            ("service", 1, AsyncOperation::Service),
            ("sync", 1, AsyncOperation::Sync),
            ("provide", 2, AsyncOperation::Provide),
            ("fail", 1, AsyncOperation::Fail),
            ("catch", 2, AsyncOperation::Catch),
            ("mapError", 2, AsyncOperation::MapError),
            ("attempt", 1, AsyncOperation::Attempt),
        ] {
            if arguments.len() == arity && self.known_term(function, "Iris.Effect.Async", name)? {
                selected = Some((operation, arguments.to_vec()));
                break;
            }
        }
        for (module, member, instance, arity, operation) in [
            ("Control.Applicative", "pure", "applicativeAsync", 1, AsyncOperation::Pure),
            ("Data.Functor", "map", "functorAsync", 2, AsyncOperation::Map),
            ("Control.Apply", "apply", "applyAsync", 2, AsyncOperation::Apply),
            ("Control.Bind", "bind", "bindAsync", 2, AsyncOperation::Bind),
        ] {
            if selected.is_none()
                && let Some(operands) = self.known_instance_member_arguments(
                    function,
                    arguments,
                    module,
                    member,
                    "Iris.Effect.Async",
                    instance,
                )?
                && operands.len() == arity
            {
                selected = Some((operation, operands.to_vec()));
            }
        }
        for (member, operation) in
            [("applyFirst", AsyncOperation::KeepFirst), ("applySecond", AsyncOperation::KeepSecond)]
        {
            if selected.is_none()
                && self.known_term(function, "Control.Apply", member)?
                && let [dictionary, operands @ ..] = arguments
                && operands.len() == 2
                && self.known_named_instance(*dictionary, "Iris.Effect.Async", "applyAsync")?
            {
                selected = Some((operation, operands.to_vec()));
            }
        }
        if selected.is_none()
            && let Some([dictionary, action, continuation]) = self.known_instance_member_arguments(
                function,
                arguments,
                "Control.Bind",
                "discard",
                "Control.Bind",
                "discardUnit",
            )?
            && self.known_named_instance(*dictionary, "Iris.Effect.Async", "bindAsync")?
        {
            selected = Some((AsyncOperation::Bind, vec![*action, *continuation]));
        }
        let Some((operation, operands)) = selected else { return Ok(None) };
        if !self.lowering_constructors_available()? {
            return Ok(None);
        }
        self.lower_async(operation, &operands).map(Some)
    }

    fn lowering_constructors_available(&mut self) -> ConversionResult<bool> {
        if let Some(available) = self.lowering_constructors_available {
            return Ok(available);
        }
        let mut available = true;
        for (module_name, constructor_name) in LOWERING_CONSTRUCTORS {
            available &= self.constructor_exists(module_name, constructor_name)?;
        }
        self.lowering_constructors_available = Some(available);
        Ok(available)
    }

    fn constructor_exists(
        &self,
        module_name: &str,
        constructor_name: &str,
    ) -> ConversionResult<bool> {
        let Some(file_id) = self.queries.module_file(module_name) else { return Ok(false) };
        let indexed = self.indexed_module(file_id)?;
        let mut terms = indexed.items.iter_terms();
        Ok(terms.any(|(term_id, item)| {
            item.name.as_deref() == Some(constructor_name)
                && indexed.constructor_type(term_id).is_some()
        }))
    }

    fn lower_async(
        &mut self,
        operation: AsyncOperation,
        operands: &[ExpressionId],
    ) -> ConversionResult<ExpressionId> {
        let (operands, bindings) =
            self.construction_operands(operands, "asyncArgument", |kind| match operation {
                AsyncOperation::Service => {
                    matches!(kind, ExpressionKind::SynthesizedEvidence { .. })
                }
                _ => matches!(kind, ExpressionKind::Async(AsyncExpression::Program { .. })),
            })?;

        let parameter = self.fresh_parameter("environment".into())?;
        let environment = self.expression(ExpressionKind::Local { parameter: parameter.clone() });
        let body = match (operation, operands.as_slice()) {
            (AsyncOperation::Pure, [value]) => *value,
            (AsyncOperation::Service, [dictionary]) => {
                let getter = self.service_getter(*dictionary)?;
                self.sync_application(getter, environment)?
            }
            (AsyncOperation::LiftEffect, [action]) => {
                self.expression(ExpressionKind::Async(AsyncExpression::Effect(*action)))
            }
            (AsyncOperation::Sync, [program]) => {
                let action = self.sync_application(*program, environment)?;
                self.expression(ExpressionKind::Async(AsyncExpression::Effect(action)))
            }
            (AsyncOperation::Provide, [provided, program]) => {
                self.execute_async(*program, *provided)?
            }
            (AsyncOperation::Bind, [program, continuation]) => {
                let value = self.execute_async(*program, environment)?;
                let origin = self.storage[*continuation].origin.or(self.origin);
                let (parameter, continuation) = self.bound_continuation(*continuation)?;
                let body = self.execute_async(continuation, environment)?;
                let binding = Binding { parameter, expression: value, source_order: 0, origin };
                self.sync_bindings(vec![binding], body)
            }
            (AsyncOperation::Map, [function, program]) => {
                let value = self.execute_async(*program, environment)?;
                let parameter = self.fresh_parameter("mappedValue".into())?;
                let local = self.expression(ExpressionKind::Local { parameter: parameter.clone() });
                let body = self.sync_application(*function, local)?;
                let binding =
                    Binding { parameter, expression: value, source_order: 0, origin: self.origin };
                self.sync_bindings(vec![binding], body)
            }
            (AsyncOperation::Apply, [function, argument]) => {
                let (function, function_local) =
                    self.sequenced_value(*function, environment, "appliedFunction", 0)?;
                let (argument, argument_local) =
                    self.sequenced_value(*argument, environment, "appliedArgument", 1)?;
                let body = self.application(function_local, [argument_local])?;
                self.sync_bindings(vec![function, argument], body)
            }
            (AsyncOperation::KeepFirst | AsyncOperation::KeepSecond, [first, second]) => {
                let (first, first_local) =
                    self.sequenced_value(*first, environment, "firstValue", 0)?;
                let (second, second_local) =
                    self.sequenced_value(*second, environment, "secondValue", 1)?;
                let body = match operation {
                    AsyncOperation::KeepFirst => first_local,
                    _ => second_local,
                };
                self.sync_bindings(vec![first, second], body)
            }
            (AsyncOperation::Fail, [error]) => {
                self.expression(ExpressionKind::Async(AsyncExpression::Fail(*error)))
            }
            (AsyncOperation::Catch, [program, handler]) => {
                let body = self.execute_async(*program, environment)?;
                let parameter = self.fresh_parameter("caughtError".into())?;
                let error = self.expression(ExpressionKind::Local { parameter: parameter.clone() });
                let handler = self.sync_application(*handler, error)?;
                let handler = self.execute_async(handler, environment)?;
                let catch = AsyncExpression::Catch { body, parameter, handler };
                self.expression(ExpressionKind::Async(catch))
            }
            (AsyncOperation::MapError, [function, program]) => {
                let body = self.execute_async(*program, environment)?;
                let parameter = self.fresh_parameter("caughtError".into())?;
                let error = self.expression(ExpressionKind::Local { parameter: parameter.clone() });
                let error = self.sync_application(*function, error)?;
                let handler = self.expression(ExpressionKind::Async(AsyncExpression::Fail(error)));
                let catch = AsyncExpression::Catch { body, parameter, handler };
                self.expression(ExpressionKind::Async(catch))
            }
            (AsyncOperation::Attempt, [program]) => {
                let value = self.execute_async(*program, environment)?;
                let parameter = self.fresh_parameter("attemptValue".into())?;
                let local = self.expression(ExpressionKind::Local { parameter: parameter.clone() });
                let succeeded = self.constructor_value("Right", [local]);
                let binding =
                    Binding { parameter, expression: value, source_order: 0, origin: self.origin };
                let body = self.sync_bindings(vec![binding], succeeded);
                let parameter = self.fresh_parameter("caughtError".into())?;
                let error = self.expression(ExpressionKind::Local { parameter: parameter.clone() });
                let handler = self.constructor_value("Left", [error]);
                let catch = AsyncExpression::Catch { body, parameter, handler };
                self.expression(ExpressionKind::Async(catch))
            }
            _ => unreachable!("invariant violated: invalid asynchronous effect application"),
        };
        let program = AsyncExpression::Program { environment: parameter, body };
        let program = self.expression(ExpressionKind::Async(program));
        Ok(self.sync_bindings(bindings, program))
    }

    fn sequenced_value(
        &mut self,
        program: ExpressionId,
        environment: ExpressionId,
        name: &str,
        source_order: usize,
    ) -> ConversionResult<(Binding, ExpressionId)> {
        let expression = self.execute_async(program, environment)?;
        let parameter = self.fresh_parameter(name.into())?;
        let local = self.expression(ExpressionKind::Local { parameter: parameter.clone() });
        Ok((Binding { parameter, expression, source_order, origin: self.origin }, local))
    }

    fn execute_async(
        &mut self,
        program: ExpressionId,
        environment: ExpressionId,
    ) -> ConversionResult<ExpressionId> {
        let origin = self.storage[program].origin;
        self.with_origin(origin, |context| match context.storage[program].kind.clone() {
            ExpressionKind::Async(AsyncExpression::Program { environment: parameter, body }) => {
                if local_uses(&context.storage, body, parameter.id) == 0 {
                    return Ok(body);
                }
                let binding =
                    Binding { parameter, expression: environment, source_order: 0, origin: None };
                Ok(context.sync_bindings(vec![binding], body))
            }
            ExpressionKind::Let { recursive, bindings, body } => {
                let body = context.execute_async(body, environment)?;
                Ok(context.expression(ExpressionKind::Let { recursive, bindings, body }))
            }
            ExpressionKind::LetPattern { pattern, value, body } => {
                let body = context.execute_async(body, environment)?;
                Ok(context.expression(ExpressionKind::LetPattern { pattern, value, body }))
            }
            ExpressionKind::IfThenElse { condition, then, else_ } => {
                let then = context.execute_async(then, environment)?;
                let else_ = context.execute_async(else_, environment)?;
                Ok(context.expression(ExpressionKind::IfThenElse { condition, then, else_ }))
            }
            ExpressionKind::Case { scrutinees, alternatives } => {
                let mut alternatives = alternatives.to_vec();
                for alternative in &mut alternatives {
                    alternative.expression =
                        context.execute_async(alternative.expression, environment)?;
                }
                let alternatives = alternatives.into();
                Ok(context.expression(ExpressionKind::Case { scrutinees, alternatives }))
            }
            ExpressionKind::Guarded { alternatives } => {
                let mut alternatives = alternatives.to_vec();
                for alternative in &mut alternatives {
                    alternative.expression =
                        context.execute_async(alternative.expression, environment)?;
                }
                let alternatives = alternatives.into();
                Ok(context.expression(ExpressionKind::Guarded { alternatives }))
            }
            _ => {
                let step = context.application(program, [environment])?;
                Ok(context.expression(ExpressionKind::Async(AsyncExpression::Step(step))))
            }
        })
    }

    pub(super) fn lower_async_programs(
        &mut self,
        declarations: &[Declaration],
    ) -> ConversionResult<()> {
        let roots = declarations.iter().filter_map(|declaration| match declaration.kind {
            DeclarationKind::Value(expression) => Some(expression),
            DeclarationKind::Constructor { .. } | DeclarationKind::Foreign => None,
        });
        let reachable = reachable_expressions(&self.storage, roots);
        let programs = reachable.into_iter().filter(|expression| {
            matches!(
                self.storage[*expression].kind,
                ExpressionKind::Async(AsyncExpression::Program { .. })
            )
        });
        let programs = programs.sorted_unstable().collect_vec();
        let mut effects = FxHashMap::default();
        for program in programs {
            let ExpressionKind::Async(AsyncExpression::Program { environment, body }) =
                self.storage[program].kind.clone()
            else {
                unreachable!("invariant violated: Async program changed before lowering")
            };
            let origin = self.storage[program].origin;
            let lowered = self.with_origin(origin, |context| {
                let effect = context.sequence_statement(body, None, &mut effects)?;
                let step = context.deferred_step(effect);
                Ok::<_, super::ConversionError>(context.parameter_abstraction([environment], step))
            })?;
            let kind = ExpressionKind::clone(&self.storage[lowered].kind);
            self.storage.replace_expression_kind(program, kind);
        }
        Ok(())
    }

    fn statement_effects(
        &self,
        expression: ExpressionId,
        effects: &mut FxHashMap<ExpressionId, StatementEffects>,
    ) -> StatementEffects {
        if let Some(&cached) = effects.get(&expression) {
            return cached;
        }
        let mut children = Vec::new();
        let mut result = StatementEffects::default();
        match &self.storage[expression].kind {
            ExpressionKind::Async(AsyncExpression::Effect(_)) => result.performs = true,
            ExpressionKind::Async(
                AsyncExpression::Step(_) | AsyncExpression::Fail(_) | AsyncExpression::Catch { .. },
            ) => result.controls = true,
            ExpressionKind::Let { bindings, body, .. } => {
                children.extend(bindings.iter().map(|binding| binding.expression));
                children.push(*body);
            }
            ExpressionKind::LetPattern { body, .. } => children.push(*body),
            ExpressionKind::IfThenElse { then, else_, .. } => children.extend([*then, *else_]),
            ExpressionKind::Case { alternatives, .. } => {
                children.extend(alternatives.iter().map(|alternative| alternative.expression));
            }
            ExpressionKind::Guarded { alternatives } => {
                children.extend(alternatives.iter().map(|alternative| alternative.expression));
            }
            _ => {}
        }
        for child in children {
            let child = self.statement_effects(child, effects);
            result.performs |= child.performs;
            result.controls |= child.controls;
        }
        effects.insert(expression, result);
        result
    }

    fn sequence_statement(
        &mut self,
        expression: ExpressionId,
        continuation: Option<Continuation>,
        effects: &mut FxHashMap<ExpressionId, StatementEffects>,
    ) -> ConversionResult<ExpressionId> {
        let origin = self.storage[expression].origin;
        self.with_origin(origin, |context| {
            let statement = context.statement_effects(expression, effects);
            let sequential = matches!(
                context.storage[expression].kind,
                ExpressionKind::Let { recursive: false, .. }
            );
            if !statement.controls && !(statement.performs && sequential) {
                if !statement.performs {
                    return Ok(context.finish_statement(expression, continuation));
                }
                let action = context.statement_action(expression, effects)?;
                let (parameter, body, origin) = match continuation {
                    Some(Continuation::Inline { parameter, body, origin }) => {
                        (parameter, body, origin)
                    }
                    continuation => {
                        let parameter = context.fresh_parameter("value".into())?;
                        let value = context
                            .expression(ExpressionKind::Local { parameter: parameter.clone() });
                        (parameter, context.finish_statement(value, continuation), None)
                    }
                };
                let effect = EffectExpression::Bind { action, parameter, body };
                return Ok(context.with_origin(origin, |context| {
                    context.expression(ExpressionKind::Effect { effect })
                }));
            }
            match context.storage[expression].kind.clone() {
                ExpressionKind::Async(AsyncExpression::Step(step)) => {
                    context.suspend(step, continuation)
                }
                ExpressionKind::Async(AsyncExpression::Fail(error)) => {
                    let step = context.constructor_value("Throw", [error]);
                    Ok(context.expression(ExpressionKind::Effect {
                        effect: EffectExpression::Pure(step),
                    }))
                }
                ExpressionKind::Async(AsyncExpression::Catch { body, parameter, handler }) => {
                    let body = context.sequence_statement(body, None, effects)?;
                    let body = context.resumed_step(body);
                    let handler = context.sequence_statement(handler, None, effects)?;
                    let handler = context.resumed_step(handler);
                    let handler = context.parameter_abstraction([parameter], handler);
                    let step = context.constructor_value("Catch", [body, handler]);
                    context.suspend(step, continuation)
                }
                ExpressionKind::Let { recursive: false, bindings, body } => {
                    let mut sequenced = context.sequence_statement(body, continuation, effects)?;
                    for binding in context.flattened_bindings(&bindings).iter().rev() {
                        let continuation = Continuation::Inline {
                            parameter: binding.parameter.clone(),
                            body: sequenced,
                            origin: binding.origin,
                        };
                        sequenced = context.sequence_statement(
                            binding.expression,
                            Some(continuation),
                            effects,
                        )?;
                    }
                    Ok(sequenced)
                }
                ExpressionKind::Let { recursive: true, bindings, body } => {
                    let body = context.sequence_statement(body, continuation, effects)?;
                    Ok(context.expression(ExpressionKind::Let { recursive: true, bindings, body }))
                }
                ExpressionKind::LetPattern { pattern, value, body } => {
                    let body = context.sequence_statement(body, continuation, effects)?;
                    Ok(context.expression(ExpressionKind::LetPattern { pattern, value, body }))
                }
                ExpressionKind::IfThenElse { condition, then, else_ } => {
                    context.join_branches(continuation, |context, continuation| {
                        let then =
                            context.sequence_statement(then, continuation.clone(), effects)?;
                        let else_ = context.sequence_statement(else_, continuation, effects)?;
                        Ok(ExpressionKind::IfThenElse { condition, then, else_ })
                    })
                }
                ExpressionKind::Case { scrutinees, alternatives } => {
                    context.join_branches(continuation, |context, continuation| {
                        let mut alternatives = alternatives.to_vec();
                        for alternative in &mut alternatives {
                            alternative.expression = context.sequence_statement(
                                alternative.expression,
                                continuation.clone(),
                                effects,
                            )?;
                        }
                        let alternatives = alternatives.into();
                        Ok(ExpressionKind::Case { scrutinees, alternatives })
                    })
                }
                ExpressionKind::Guarded { alternatives } => {
                    context.join_branches(continuation, |context, continuation| {
                        let mut alternatives = alternatives.to_vec();
                        for alternative in &mut alternatives {
                            alternative.expression = context.sequence_statement(
                                alternative.expression,
                                continuation.clone(),
                                effects,
                            )?;
                        }
                        Ok(ExpressionKind::Guarded { alternatives: alternatives.into() })
                    })
                }
                _ => unreachable!("invariant violated: Async control flow outside a statement"),
            }
        })
    }

    fn flattened_bindings(&self, bindings: &[Binding]) -> Vec<Binding> {
        let mut flattened = Vec::with_capacity(bindings.len());
        let mut pending = bindings.iter().rev().cloned().collect_vec();
        while let Some(mut binding) = pending.pop() {
            if let ExpressionKind::Let { recursive: false, bindings: nested, body } =
                &self.storage[binding.expression].kind
            {
                binding.expression = *body;
                pending.push(binding);
                pending.extend(nested.iter().rev().cloned());
                continue;
            }
            flattened.push(binding);
        }
        flattened
    }

    fn join_branches(
        &mut self,
        continuation: Option<Continuation>,
        branches: impl FnOnce(&mut Self, Option<Continuation>) -> ConversionResult<ExpressionKind>,
    ) -> ConversionResult<ExpressionId> {
        let Some(Continuation::Inline { parameter, body, origin }) = continuation else {
            let kind = branches(self, continuation)?;
            return Ok(self.expression(kind));
        };
        let function = self.fresh_parameter("join".into())?;
        let join =
            self.with_origin(origin, |context| context.parameter_abstraction([parameter], body));
        let kind = branches(self, Some(Continuation::Join { function: function.clone() }))?;
        let body = self.expression(kind);
        let binding =
            Binding { parameter: function, expression: join, source_order: 0, origin: None };
        Ok(self.sync_bindings(vec![binding], body))
    }

    fn statement_action(
        &mut self,
        expression: ExpressionId,
        effects: &mut FxHashMap<ExpressionId, StatementEffects>,
    ) -> ConversionResult<ExpressionId> {
        if !self.statement_effects(expression, effects).performs {
            let effect = EffectExpression::Pure(expression);
            return Ok(self.expression(ExpressionKind::Effect { effect }));
        }
        let origin = self.storage[expression].origin;
        self.with_origin(origin, |context| {
            let kind = match context.storage[expression].kind.clone() {
                ExpressionKind::Async(AsyncExpression::Effect(action)) => return Ok(action),
                ExpressionKind::Let { recursive: false, bindings, body } => {
                    let mut action = context.statement_action(body, effects)?;
                    for binding in context.flattened_bindings(&bindings).iter().rev() {
                        action = if context.statement_effects(binding.expression, effects).performs
                        {
                            let bound = context.statement_action(binding.expression, effects)?;
                            let parameter = binding.parameter.clone();
                            let effect =
                                EffectExpression::Bind { action: bound, parameter, body: action };
                            context.with_origin(binding.origin, |context| {
                                context.expression(ExpressionKind::Effect { effect })
                            })
                        } else {
                            context.sync_bindings(vec![binding.clone()], action)
                        };
                    }
                    return Ok(action);
                }
                ExpressionKind::Let { recursive: true, bindings, body } => {
                    let body = context.statement_action(body, effects)?;
                    ExpressionKind::Let { recursive: true, bindings, body }
                }
                ExpressionKind::LetPattern { pattern, value, body } => {
                    let body = context.statement_action(body, effects)?;
                    ExpressionKind::LetPattern { pattern, value, body }
                }
                ExpressionKind::IfThenElse { condition, then, else_ } => {
                    let then = context.statement_action(then, effects)?;
                    let else_ = context.statement_action(else_, effects)?;
                    ExpressionKind::IfThenElse { condition, then, else_ }
                }
                ExpressionKind::Case { scrutinees, alternatives } => {
                    let mut alternatives = alternatives.to_vec();
                    for alternative in &mut alternatives {
                        alternative.expression =
                            context.statement_action(alternative.expression, effects)?;
                    }
                    ExpressionKind::Case { scrutinees, alternatives: alternatives.into() }
                }
                ExpressionKind::Guarded { alternatives } => {
                    let mut alternatives = alternatives.to_vec();
                    for alternative in &mut alternatives {
                        alternative.expression =
                            context.statement_action(alternative.expression, effects)?;
                    }
                    ExpressionKind::Guarded { alternatives: alternatives.into() }
                }
                _ => unreachable!("invariant violated: Async effect outside a statement"),
            };
            Ok(context.expression(kind))
        })
    }

    fn finish_statement(
        &mut self,
        value: ExpressionId,
        continuation: Option<Continuation>,
    ) -> ExpressionId {
        match continuation {
            None => {
                let step = self.constructor_value("Pure", [value]);
                self.expression(ExpressionKind::Effect { effect: EffectExpression::Pure(step) })
            }
            Some(Continuation::Inline { parameter, body, origin }) => {
                let binding = Binding { parameter, expression: value, source_order: 0, origin };
                self.sync_bindings(vec![binding], body)
            }
            Some(Continuation::Join { function }) => {
                let function = self.expression(ExpressionKind::Local { parameter: function });
                self.expression(ExpressionKind::Application {
                    function,
                    arguments: Arc::from([value]),
                    synthetic: true,
                })
            }
        }
    }

    fn suspend(
        &mut self,
        step: ExpressionId,
        continuation: Option<Continuation>,
    ) -> ConversionResult<ExpressionId> {
        let step = match continuation {
            None => step,
            Some(Continuation::Inline { parameter, body, origin }) => {
                let body = self.resumed_step(body);
                let continuation = self.with_origin(origin, |context| {
                    context.parameter_abstraction([parameter], body)
                });
                self.constructor_value("Bind", [step, continuation])
            }
            Some(continuation @ Continuation::Join { .. }) => {
                let parameter = self.fresh_parameter("value".into())?;
                let value = self.expression(ExpressionKind::Local { parameter: parameter.clone() });
                let body = self.finish_statement(value, Some(continuation));
                let body = self.resumed_step(body);
                let continuation = self.parameter_abstraction([parameter], body);
                self.constructor_value("Bind", [step, continuation])
            }
        };
        Ok(self.expression(ExpressionKind::Effect { effect: EffectExpression::Pure(step) }))
    }

    fn deferred_step(&mut self, effect: ExpressionId) -> ExpressionId {
        if let ExpressionKind::Effect { effect: EffectExpression::Pure(step) } =
            self.storage[effect].kind
            && self.constructs_step(step)
        {
            return step;
        }
        self.run_step(effect)
    }

    fn resumed_step(&mut self, effect: ExpressionId) -> ExpressionId {
        if let ExpressionKind::Effect { effect: EffectExpression::Pure(step) } =
            self.storage[effect].kind
        {
            return step;
        }
        self.run_step(effect)
    }

    fn run_step(&mut self, effect: ExpressionId) -> ExpressionId {
        let body = self.executed_effect(effect);
        let thunk = ExpressionKind::UncurriedAbstraction { parameters: Arc::from([]), body };
        let thunk = self.expression(thunk);
        self.constructor_value("Run", [thunk])
    }

    fn executed_effect(&mut self, effect: ExpressionId) -> ExpressionId {
        let origin = self.storage[effect].origin;
        self.with_origin(origin, |context| {
            let kind = match context.storage[effect].kind.clone() {
                ExpressionKind::Let { recursive, bindings, body } => {
                    let body = context.executed_effect(body);
                    ExpressionKind::Let { recursive, bindings, body }
                }
                ExpressionKind::LetPattern { pattern, value, body } => {
                    let body = context.executed_effect(body);
                    ExpressionKind::LetPattern { pattern, value, body }
                }
                ExpressionKind::IfThenElse { condition, then, else_ } => {
                    let then = context.executed_effect(then);
                    let else_ = context.executed_effect(else_);
                    ExpressionKind::IfThenElse { condition, then, else_ }
                }
                ExpressionKind::Case { scrutinees, mut alternatives } => {
                    let alternatives_mut = Arc::make_mut(&mut alternatives);
                    for alternative in alternatives_mut {
                        alternative.expression = context.executed_effect(alternative.expression);
                    }
                    ExpressionKind::Case { scrutinees, alternatives }
                }
                ExpressionKind::Guarded { mut alternatives } => {
                    let alternatives_mut = Arc::make_mut(&mut alternatives);
                    for alternative in alternatives_mut {
                        alternative.expression = context.executed_effect(alternative.expression);
                    }
                    ExpressionKind::Guarded { alternatives }
                }
                _ => ExpressionKind::UncurriedApplication {
                    function: effect,
                    arguments: Arc::from([]),
                    synthetic: false,
                },
            };
            context.expression(kind)
        })
    }

    fn constructs_step(&self, expression: ExpressionId) -> bool {
        let mut pending = vec![expression];
        while let Some(expression) = pending.pop() {
            match &self.storage[expression].kind {
                ExpressionKind::Literal { .. }
                | ExpressionKind::Constructor { .. }
                | ExpressionKind::Global { .. }
                | ExpressionKind::Local { .. }
                | ExpressionKind::Abstraction { .. }
                | ExpressionKind::UncurriedAbstraction { .. }
                | ExpressionKind::Effect { .. } => {}
                ExpressionKind::Record { fields } => {
                    pending.extend(fields.iter().map(|field| field.expression));
                }
                ExpressionKind::Project { record: value, .. }
                | ExpressionKind::Unary { value, .. } => pending.push(*value),
                ExpressionKind::Binary { left, right, .. } => pending.extend([*left, *right]),
                _ => return false,
            }
        }
        true
    }
}
