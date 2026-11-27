use functional::tree::{
    ArrayElement as FunctionalArrayElement, BinaryOperator as FunctionalBinaryOperator, Binding,
    ExpressionId as FunctionalExpressionId, ExpressionKind, Global, Parameter, RecordField,
    RecordUpdate,
};
use itertools::Itertools;
use rustc_hash::FxHashSet;
use smol_str::format_smolstr;
use std::rc::Rc;

use crate::error::{ModuleResult, UnsupportedState};
use crate::tree::{ArrayElement, ExpressionId, ObjectProperty, Tree};
use crate::writer::{BindingCallTarget, Writer};

use super::case::{render_case, render_guarded};
use super::effect::effect_expression;
use super::inline::is_abstraction;
use super::syntax::{binary_expression, unary_expression};
use super::tail_call::{
    TailCallContext, TailCallIdentity, TailCallProfile, local_profiles, tail_call_groups,
};
use super::{
    AbstractionBinding, Destination, FunctionContext, FunctionRenderer, Generator, LocalBinding,
    RenderedExpression, TailCallWrapper, binding_source_name, global_file,
};

impl Generator<'_> {
    pub(super) fn render_expression<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        expression: FunctionalExpressionId,
        destination: Destination<'_>,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        let _origin = tree.enter_origin(self.module.storage[expression].origin);
        if matches!(
            destination,
            Destination::Return
                | Destination::TailEffectThunkReturn
                | Destination::EffectTailEffectReturn
        ) && let Some(tail_call) = context
            .tail_calls
            .as_ref()
            .and_then(|tail_calls| tail_calls.call(self.module, expression))
        {
            return self.render_tail_call(tree, writer, tail_call, destination, context);
        }

        match &self.module.storage[expression].kind {
            ExpressionKind::IfThenElse { condition, then, else_ } => {
                let condition = self.expression_value(tree, writer, *condition, context)?;
                writer.if_else_with_state(
                    tree,
                    condition,
                    context,
                    |tree, writer, context| {
                        self.render_expression(tree, writer, *then, destination, context)
                    },
                    |tree, writer, context| {
                        self.render_expression(tree, writer, *else_, destination, context)
                    },
                )
            }
            ExpressionKind::Case { scrutinees, alternatives } => {
                let mut renderer = self.renderer(tree, writer, context);
                render_case(&mut renderer, scrutinees, alternatives, destination)
            }
            ExpressionKind::Guarded { alternatives } => {
                let mut renderer = self.renderer(tree, writer, context);
                render_guarded(&mut renderer, alternatives, destination)
            }
            ExpressionKind::Let { recursive, bindings, body } => {
                let mut renderer = self.renderer(tree, writer, context);
                render_let(&mut renderer, *recursive, bindings)?;
                self.render_expression(tree, writer, *body, destination, context)
            }
            ExpressionKind::LetPattern { pattern, value, body } => {
                let source = *value;
                let value = self.rendered_expression(tree, writer, source, context)?;
                let value = self.materialize_pattern_value(tree, writer, source, value, context);
                let plan = self.pattern_plan(tree, *pattern, value, None, context)?;
                self.render_pattern_scope(tree, writer, plan, context, |tree, writer, context| {
                    self.render_expression(tree, writer, *body, destination, context)
                })
            }
            ExpressionKind::Effect { effect } if !destination.is_effect() => self
                .render_effect_expression_destination(tree, writer, effect, destination, context),
            ExpressionKind::UncurriedApplication { function, arguments, .. }
                if arguments.is_empty()
                    && matches!(
                        self.module.storage[*function].kind,
                        ExpressionKind::Effect { .. }
                    )
                    && matches!(
                        destination,
                        Destination::Return
                            | Destination::Assign(_)
                            | Destination::AssignAndBreak { .. }
                            | Destination::Discard
                            | Destination::DiscardAndBreak { .. }
                            | Destination::Continue
                    ) =>
            {
                self.render_expression(tree, writer, *function, destination.effect(), context)
            }
            ExpressionKind::Abstraction { .. } | ExpressionKind::UncurriedAbstraction { .. }
                if matches!(destination, Destination::Return) =>
            {
                self.render_returned_function(tree, writer, expression, context)
            }
            _ => {
                if matches!(destination, Destination::TailEffectThunkReturn) {
                    return self
                        .render_tail_effect_thunk_destination(tree, writer, expression, context);
                }
                if destination.is_effect() {
                    return self.render_effect_destination(
                        tree,
                        writer,
                        expression,
                        destination.value(),
                        context,
                    );
                }
                let value = self.expression_value(tree, writer, expression, context)?;
                self.render_destination(tree, writer, value, destination);
                Ok(())
            }
        }
    }

    pub(super) fn render_destination<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        value: ExpressionId,
        destination: Destination<'_>,
    ) {
        match destination {
            Destination::Return => writer.return_expression(tree, value),
            Destination::TailEffectReturn => {
                let complete = tree.boolean(false);
                let result = tree.array(vec![complete, value]);
                writer.return_expression(tree, result);
            }
            Destination::Assign(name) => {
                writer.assign(tree, name, value);
            }
            Destination::AssignAndBreak { name, label } => {
                writer.assign(tree, name, value);
                writer.break_label(label);
            }
            Destination::Discard => {
                if !tree.expression_is_atomic(&value) {
                    writer.expression_statement(tree, value);
                }
            }
            Destination::DiscardAndBreak { label } => {
                if !tree.expression_is_atomic(&value) {
                    writer.expression_statement(tree, value);
                }
                writer.break_label(label);
            }
            Destination::Continue => {
                if !tree.expression_is_atomic(&value) {
                    writer.expression_statement(tree, value);
                }
                writer.continue_loop();
            }
            Destination::TailEffectThunkReturn
            | Destination::EffectReturn
            | Destination::EffectTailEffectReturn
            | Destination::EffectAssign(_)
            | Destination::EffectAssignAndBreak { .. }
            | Destination::EffectDiscard
            | Destination::EffectDiscardAndBreak { .. }
            | Destination::EffectContinue => {
                unreachable!("invariant violated: effect destination was not rendered directly")
            }
        }
    }

    pub(super) fn expression_value<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        expression: FunctionalExpressionId,
        context: &mut FunctionContext,
    ) -> ModuleResult<ExpressionId> {
        let expression = self.rendered_expression(tree, writer, expression, context)?;
        Ok(expression.value)
    }

    pub(super) fn rendered_expression<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        expression: FunctionalExpressionId,
        context: &mut FunctionContext,
    ) -> ModuleResult<RenderedExpression> {
        if let Some(expression) = self.try_inline_expression(tree, expression, context)? {
            return Ok(RenderedExpression { value: expression, pending_evaluation: true });
        }
        self.render_non_inline_expression(tree, writer, expression, context)
    }

    fn render_non_inline_expression<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        expression: FunctionalExpressionId,
        context: &mut FunctionContext,
    ) -> ModuleResult<RenderedExpression> {
        let _origin = tree.enter_origin(self.module.storage[expression].origin);
        match &self.module.storage[expression].kind {
            ExpressionKind::Async(_) => {
                unreachable!("invariant violated: Async program was not lowered to steps")
            }
            ExpressionKind::Array { elements } => {
                let mut values = Vec::with_capacity(elements.len());
                for element in elements.iter() {
                    let value =
                        if let Some(value) = self.try_inline_expression(tree, *element, context)? {
                            RenderedExpression { value, pending_evaluation: true }
                        } else {
                            if self.expression_rendering_is_eager(*element, context) {
                                self.materialize_rendered_expressions(
                                    tree,
                                    writer,
                                    &mut values,
                                    "$element",
                                    context,
                                );
                            }
                            self.render_non_inline_expression(tree, writer, *element, context)?
                        };
                    values.push(value);
                }
                let values = values.into_iter().map(|value| value.value).collect_vec();
                let value = tree.array(values);
                Ok(RenderedExpression { value, pending_evaluation: true })
            }
            ExpressionKind::ArrayConcatenation { elements } => {
                let mut values = Vec::with_capacity(elements.len());
                for element in elements.iter() {
                    let expression = element.expression();
                    let value = if let Some(value) =
                        self.try_inline_expression(tree, expression, context)?
                    {
                        RenderedExpression { value, pending_evaluation: true }
                    } else {
                        if self.expression_rendering_is_eager(expression, context) {
                            self.materialize_rendered_expressions(
                                tree,
                                writer,
                                &mut values,
                                "$element",
                                context,
                            );
                        }
                        self.render_non_inline_expression(tree, writer, expression, context)?
                    };
                    values.push(value);
                }
                let values = elements.iter().zip(values).map(|(element, value)| match element {
                    FunctionalArrayElement::Value(_) => ArrayElement::Value(value.value),
                    FunctionalArrayElement::Spread(_) => ArrayElement::Spread(value.value),
                });
                let value = tree.spread_array(values.collect_vec());
                Ok(RenderedExpression { value, pending_evaluation: true })
            }
            ExpressionKind::Record { fields } => {
                let mut rendered_fields = Vec::with_capacity(fields.len());
                for field in fields.iter() {
                    let value = if let Some(value) =
                        self.try_inline_expression(tree, field.expression, context)?
                    {
                        RenderedExpression { value, pending_evaluation: true }
                    } else {
                        if self.expression_rendering_is_eager(field.expression, context) {
                            let values = rendered_fields.iter_mut().map(|(_, value)| value);
                            for value in values {
                                self.materialize_rendered_expression(
                                    tree, writer, value, "$field", context,
                                );
                            }
                        }
                        self.render_field_value(tree, writer, field, context)?
                    };
                    rendered_fields.push((field.field.name.clone(), value));
                }
                let properties = rendered_fields
                    .into_iter()
                    .map(|(name, value)| ObjectProperty::Field { name, value: value.value });
                let value = tree.object(properties.collect_vec());
                Ok(RenderedExpression { value, pending_evaluation: true })
            }
            ExpressionKind::RecordUpdate { record, updates } => {
                let value =
                    self.record_update_expression(tree, writer, *record, updates, context)?;
                Ok(RenderedExpression { value, pending_evaluation: true })
            }
            ExpressionKind::Project { record, field } => {
                let record = self.rendered_expression(tree, writer, *record, context)?;
                let value = tree.member(record.value, field.name.as_str());
                Ok(RenderedExpression { value, pending_evaluation: true })
            }
            ExpressionKind::Unary { operator, value } => {
                let value = self.rendered_expression(tree, writer, *value, context)?;
                let value = unary_expression(tree, *operator, value.value);
                Ok(RenderedExpression { value, pending_evaluation: true })
            }
            ExpressionKind::Binary { operator, left, right } => {
                if matches!(
                    operator,
                    FunctionalBinaryOperator::BooleanAnd | FunctionalBinaryOperator::BooleanOr
                ) && !self.expression_can_inline(*right)
                {
                    return self.render_short_circuit_expression(
                        tree, writer, *operator, *left, *right, context,
                    );
                }
                let mut left = self.rendered_expression(tree, writer, *left, context)?;
                let right =
                    if let Some(value) = self.try_inline_expression(tree, *right, context)? {
                        RenderedExpression { value, pending_evaluation: true }
                    } else {
                        if self.expression_rendering_is_eager(*right, context) {
                            self.materialize_rendered_expression(
                                tree, writer, &mut left, "$left", context,
                            );
                        }
                        self.render_non_inline_expression(tree, writer, *right, context)?
                    };
                let runtime_namespace = self.runtime_namespace.as_ref();
                let value =
                    binary_expression(tree, runtime_namespace, *operator, left.value, right.value);
                Ok(RenderedExpression { value, pending_evaluation: true })
            }
            ExpressionKind::Index { value, index } => {
                let mut value = self.rendered_expression(tree, writer, *value, context)?;
                let index =
                    if let Some(index) = self.try_inline_expression(tree, *index, context)? {
                        RenderedExpression { value: index, pending_evaluation: true }
                    } else {
                        if self.expression_rendering_is_eager(*index, context) {
                            self.materialize_rendered_expression(
                                tree, writer, &mut value, "$value", context,
                            );
                        }
                        self.render_non_inline_expression(tree, writer, *index, context)?
                    };
                let value = tree.index(value.value, index.value);
                Ok(RenderedExpression { value, pending_evaluation: true })
            }
            ExpressionKind::Abstraction { .. } | ExpressionKind::UncurriedAbstraction { .. } => {
                let value = self.render_closure(tree, writer, expression, None, context)?;
                Ok(RenderedExpression { value, pending_evaluation: false })
            }
            ExpressionKind::Application { function, arguments, synthetic } => {
                let mut function = self.rendered_expression(tree, writer, *function, context)?;
                if *synthetic {
                    tree.clear_call_purity(&function.value);
                }
                if arguments.is_empty() {
                    let value = if *synthetic {
                        tree.pure_call(function.value, Vec::new())
                    } else {
                        tree.call(function.value, Vec::new())
                    };
                    return Ok(RenderedExpression { value, pending_evaluation: true });
                }
                for (index, argument) in arguments.iter().enumerate() {
                    let argument = if let Some(value) =
                        self.try_inline_expression(tree, *argument, context)?
                    {
                        RenderedExpression { value, pending_evaluation: true }
                    } else {
                        if self.expression_rendering_is_eager(*argument, context) {
                            self.materialize_rendered_expression(
                                tree,
                                writer,
                                &mut function,
                                "$function",
                                context,
                            );
                        }
                        self.render_non_inline_expression(tree, writer, *argument, context)?
                    };
                    let outermost = index + 1 == arguments.len();
                    let value = if *synthetic && outermost {
                        tree.pure_call(function.value, vec![argument.value])
                    } else {
                        tree.call(function.value, vec![argument.value])
                    };
                    function = RenderedExpression { value, pending_evaluation: true };
                }
                Ok(function)
            }
            ExpressionKind::UncurriedApplication { function, arguments, synthetic } => {
                let mut function = self.rendered_expression(tree, writer, *function, context)?;
                if *synthetic {
                    tree.clear_call_purity(&function.value);
                }
                let mut values = Vec::with_capacity(arguments.len());
                for argument in arguments.iter() {
                    let value = if let Some(value) =
                        self.try_inline_expression(tree, *argument, context)?
                    {
                        RenderedExpression { value, pending_evaluation: true }
                    } else {
                        if self.expression_rendering_is_eager(*argument, context) {
                            self.materialize_rendered_expression(
                                tree,
                                writer,
                                &mut function,
                                "$function",
                                context,
                            );
                            self.materialize_rendered_expressions(
                                tree,
                                writer,
                                &mut values,
                                "$argument",
                                context,
                            );
                        }
                        self.render_non_inline_expression(tree, writer, *argument, context)?
                    };
                    values.push(value);
                }
                let values = values.into_iter().map(|value| value.value).collect_vec();
                let value = if *synthetic {
                    tree.pure_call(function.value, values)
                } else {
                    tree.call(function.value, values)
                };
                Ok(RenderedExpression { value, pending_evaluation: true })
            }
            ExpressionKind::StyleX(stylex) => {
                self.render_stylex_expression(tree, writer, stylex, context)
            }
            ExpressionKind::IfThenElse { condition, then, else_ }
                if self.expression_can_inline(*then) && self.expression_can_inline(*else_) =>
            {
                let condition = self.expression_value(tree, writer, *condition, context)?;
                let then = self.inline_expression(tree, *then, context)?;
                let else_ = self.inline_expression(tree, *else_, context)?;
                let (Some(then), Some(else_)) = (then, else_) else {
                    unreachable!(
                        "invariant violated: inline eligibility did not match expression rendering"
                    )
                };
                let value = tree.conditional(condition, then, else_);
                Ok(RenderedExpression { value, pending_evaluation: true })
            }
            ExpressionKind::Effect { effect } => {
                let mut renderer = self.renderer(tree, writer, context);
                let value = effect_expression(&mut renderer, effect)?;
                Ok(RenderedExpression { value, pending_evaluation: true })
            }
            ExpressionKind::Let { recursive: false, bindings, body } => {
                let name = context.allocate("$result");
                let mut renderer = self.renderer(tree, writer, context);
                render_let(&mut renderer, false, bindings)?;
                let value = self.expression_value(tree, writer, *body, context)?;
                writer.constant(tree, &name, value, false);
                let value = tree.identifier(name);
                Ok(RenderedExpression { value, pending_evaluation: false })
            }
            ExpressionKind::IfThenElse { .. }
            | ExpressionKind::Case { .. }
            | ExpressionKind::Guarded { .. }
            | ExpressionKind::Let { .. }
            | ExpressionKind::LetPattern { .. } => {
                let name = context.allocate("$result");
                writer.mutable(&name);
                self.render_expression(
                    tree,
                    writer,
                    expression,
                    Destination::Assign(&name),
                    context,
                )?;
                let value = tree.identifier(name);
                Ok(RenderedExpression { value, pending_evaluation: false })
            }
            ExpressionKind::Literal { .. }
            | ExpressionKind::Constructor { .. }
            | ExpressionKind::Global { .. }
            | ExpressionKind::Local { .. }
            | ExpressionKind::SynthesizedEvidence { .. }
            | ExpressionKind::TrivialEvidence => {
                unreachable!("invariant violated: atomic functional expression was not inlineable")
            }
        }
    }

    fn render_field_value<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        field: &RecordField,
        context: &mut FunctionContext,
    ) -> ModuleResult<RenderedExpression> {
        if !is_abstraction(&self.module.storage[field.expression].kind) {
            return self.render_non_inline_expression(tree, writer, field.expression, context);
        }
        let _origin = tree.enter_origin(self.module.storage[field.expression].origin);
        let source_name = Some(writer.register_name(&field.field.name));
        let value = self.render_closure(tree, writer, field.expression, source_name, context)?;
        Ok(RenderedExpression { value, pending_evaluation: false })
    }

    fn render_short_circuit_expression<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        operator: FunctionalBinaryOperator,
        left: FunctionalExpressionId,
        right: FunctionalExpressionId,
        context: &mut FunctionContext,
    ) -> ModuleResult<RenderedExpression> {
        let condition = self.expression_value(tree, writer, left, context)?;
        let name = context.allocate("$result");
        writer.mutable(&name);
        let evaluate_right =
            |tree: &mut Tree<'t>, writer: &mut Writer<'t>, context: &mut FunctionContext| {
                self.render_expression(tree, writer, right, Destination::Assign(&name), context)
            };
        let short_circuit =
            |tree: &mut Tree<'t>, writer: &mut Writer<'t>, _: &mut FunctionContext| {
                let value = tree.boolean(operator == FunctionalBinaryOperator::BooleanOr);
                writer.assign(tree, &name, value);
                Ok(())
            };
        if operator == FunctionalBinaryOperator::BooleanAnd {
            writer.if_else_with_state(tree, condition, context, evaluate_right, short_circuit)?;
        } else {
            writer.if_else_with_state(tree, condition, context, short_circuit, evaluate_right)?;
        }
        let value = tree.identifier(name);
        Ok(RenderedExpression { value, pending_evaluation: false })
    }

    fn expression_rendering_is_eager(
        &self,
        expression: FunctionalExpressionId,
        context: &FunctionContext,
    ) -> bool {
        match &self.module.storage[expression].kind {
            ExpressionKind::Literal { .. }
            | ExpressionKind::Constructor { .. }
            | ExpressionKind::Global { .. }
            | ExpressionKind::Local { .. }
            | ExpressionKind::Abstraction { .. }
            | ExpressionKind::UncurriedAbstraction { .. }
            | ExpressionKind::SynthesizedEvidence { .. }
            | ExpressionKind::TrivialEvidence => false,
            ExpressionKind::Array { elements } => {
                elements.iter().any(|element| self.expression_rendering_is_eager(*element, context))
            }
            ExpressionKind::ArrayConcatenation { elements } => elements
                .iter()
                .any(|element| self.expression_rendering_is_eager(element.expression(), context)),
            ExpressionKind::Record { fields } => fields
                .iter()
                .any(|field| self.expression_rendering_is_eager(field.expression, context)),
            ExpressionKind::RecordUpdate { record, updates } => {
                self.expression_rendering_is_eager(*record, context)
                    || record_updates_reuse_source(updates)
                        && !self.functional_expression_is_reusable(*record, context)
                    || self.record_updates_rendering_is_eager(updates, context)
            }
            ExpressionKind::Project { record, .. }
            | ExpressionKind::Unary { value: record, .. } => {
                self.expression_rendering_is_eager(*record, context)
            }
            ExpressionKind::Binary { operator, left, right } => {
                self.expression_rendering_is_eager(*left, context)
                    || if matches!(
                        operator,
                        FunctionalBinaryOperator::BooleanAnd | FunctionalBinaryOperator::BooleanOr
                    ) {
                        !self.expression_can_inline(*right)
                    } else {
                        self.expression_rendering_is_eager(*right, context)
                    }
            }
            ExpressionKind::Index { value, index } => {
                self.expression_rendering_is_eager(*value, context)
                    || self.expression_rendering_is_eager(*index, context)
            }
            ExpressionKind::Application { function, arguments, .. }
            | ExpressionKind::UncurriedApplication { function, arguments, .. } => {
                self.expression_rendering_is_eager(*function, context)
                    || arguments
                        .iter()
                        .any(|argument| self.expression_rendering_is_eager(*argument, context))
            }
            ExpressionKind::StyleX(stylex) => stylex
                .try_for_each_child(|child| {
                    if self.expression_rendering_is_eager(child, context) {
                        Err(())
                    } else {
                        Ok(())
                    }
                })
                .is_err(),
            ExpressionKind::IfThenElse { condition, then, else_ } => {
                self.expression_rendering_is_eager(*condition, context)
                    || !self.expression_can_inline(*then)
                    || !self.expression_can_inline(*else_)
            }
            ExpressionKind::Case { .. }
            | ExpressionKind::Guarded { .. }
            | ExpressionKind::Let { .. }
            | ExpressionKind::LetPattern { .. }
            | ExpressionKind::Effect { .. }
            | ExpressionKind::Async(_) => true,
        }
    }

    pub(super) fn functional_expression_is_reusable(
        &self,
        expression: FunctionalExpressionId,
        context: &FunctionContext,
    ) -> bool {
        match &self.module.storage[expression].kind {
            ExpressionKind::Literal { .. } => true,
            ExpressionKind::Constructor { global } | ExpressionKind::Global { global } => {
                global_file(global.id) == self.module.file_id
                    && !self.lazy_global_names.contains_key(&global.id)
            }
            ExpressionKind::Local { parameter } => {
                matches!(
                    context.locals.get(&parameter.id),
                    Some(LocalBinding::Direct(_) | LocalBinding::Inline(_))
                )
            }
            ExpressionKind::Array { .. }
            | ExpressionKind::ArrayConcatenation { .. }
            | ExpressionKind::Record { .. }
            | ExpressionKind::RecordUpdate { .. }
            | ExpressionKind::Project { .. }
            | ExpressionKind::Unary { .. }
            | ExpressionKind::Binary { .. }
            | ExpressionKind::Index { .. }
            | ExpressionKind::Abstraction { .. }
            | ExpressionKind::UncurriedAbstraction { .. }
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
            | ExpressionKind::TrivialEvidence => false,
        }
    }

    fn record_updates_rendering_is_eager(
        &self,
        updates: &[RecordUpdate],
        context: &FunctionContext,
    ) -> bool {
        updates.iter().any(|update| self.record_update_rendering_is_eager(update, context))
    }

    fn record_update_rendering_is_eager(
        &self,
        update: &RecordUpdate,
        context: &FunctionContext,
    ) -> bool {
        match update {
            RecordUpdate::Leaf { expression, .. } => {
                self.expression_rendering_is_eager(*expression, context)
            }
            RecordUpdate::Branch { updates, .. } => {
                self.record_updates_rendering_is_eager(updates, context)
            }
        }
    }

    fn materialize_rendered_expression<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        expression: &mut RenderedExpression,
        preferred: &str,
        context: &mut FunctionContext,
    ) {
        if rendered_expression_is_reusable(tree, expression) {
            return;
        }
        let name = context.allocate(preferred);
        let identifier = tree.identifier(&name);
        let value = std::mem::replace(&mut expression.value, identifier);
        writer.constant(tree, &name, value, false);
        expression.pending_evaluation = false;
    }

    fn materialize_rendered_expressions<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        expressions: &mut [RenderedExpression],
        preferred: &str,
        context: &mut FunctionContext,
    ) {
        for expression in expressions {
            self.materialize_rendered_expression(tree, writer, expression, preferred, context);
        }
    }

    pub(super) fn materialize_value<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        value: ExpressionId,
        preferred: &str,
        context: &mut FunctionContext,
    ) -> ExpressionId {
        let name = context.allocate(preferred);
        writer.constant(tree, &name, value, false);
        tree.identifier(name)
    }

    fn record_update_expression<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        record: FunctionalExpressionId,
        updates: &[RecordUpdate],
        context: &mut FunctionContext,
    ) -> ModuleResult<ExpressionId> {
        let record = self.rendered_expression(tree, writer, record, context)?;
        let record_is_reusable = rendered_expression_is_reusable(tree, &record);
        self.record_updates(tree, writer, record.value, record_is_reusable, updates, context)
    }

    fn record_updates<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        mut record: ExpressionId,
        record_is_reusable: bool,
        updates: &[RecordUpdate],
        context: &mut FunctionContext,
    ) -> ModuleResult<ExpressionId> {
        let mut properties = Vec::with_capacity(updates.len() + 1);
        let record = if record_updates_reuse_source(updates) {
            if !record_is_reusable {
                record = self.materialize_value(tree, writer, record, "$record", context);
            }
            properties.push(ObjectProperty::Spread(tree.duplicate(&record)));
            Some(record)
        } else {
            properties.push(ObjectProperty::Spread(record));
            None
        };
        for update in updates {
            if self.record_update_rendering_is_eager(update, context) {
                let value = tree.object(std::mem::take(&mut properties));
                let value = self.materialize_value(tree, writer, value, "$record", context);
                properties.push(ObjectProperty::Spread(value));
            }
            match update {
                RecordUpdate::Leaf { field, expression } => {
                    let value = self.rendered_expression(tree, writer, *expression, context)?;
                    properties.push(ObjectProperty::Field {
                        name: field.name.clone(),
                        value: value.value,
                    });
                }
                RecordUpdate::Branch { field, updates } => {
                    // Nested updates revisit their original source paths. Reusing the path here
                    // preserves each observable property read while the root record remains stable.
                    let record =
                        record.as_ref().expect("invariant violated: nested update has no source");
                    let source = tree.duplicate(record);
                    let nested = tree.member(source, field.name.as_str());
                    let value =
                        self.record_updates(tree, writer, nested, true, updates, context)?;
                    properties.push(ObjectProperty::Field { name: field.name.clone(), value });
                }
            }
        }
        Ok(tree.object(properties))
    }

    pub(super) fn global_expression(
        &self,
        tree: &mut Tree,
        global: &Global,
    ) -> ModuleResult<ExpressionId> {
        let file_id = global_file(global.id);
        if file_id == self.module.file_id {
            if let Some(lazy_name) = self.lazy_global_names.get(&global.id) {
                let lazy = tree.identifier(lazy_name);
                return Ok(tree.call(lazy, Vec::new()));
            }
            let name = self.global_names.get(&global.id).ok_or_else(|| {
                self.unsupported(UnsupportedState::MissingGlobal {
                    name: global.item_name.to_string(),
                })
            })?;
            Ok(tree.identifier(name))
        } else {
            if let Some(binding) = self.external_named_imports.get(&global.id) {
                return Ok(tree.identifier(&binding.local_binding_name));
            }
            let namespace = self
                .external_module_namespaces
                .get(&file_id)
                .expect("invariant violated: external JavaScript global has no module namespace");
            self.rendered_module_namespaces.borrow_mut().insert(file_id);
            let namespace = tree.identifier(namespace);
            Ok(tree.member(namespace, global.item_name.as_str()))
        }
    }
}

fn render_let(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    recursive: bool,
    bindings: &[Binding],
) -> ModuleResult<()> {
    let FunctionRenderer { generator, tree, writer, context } = renderer;
    if recursive
        && bindings
            .iter()
            .all(|binding| is_abstraction(&generator.module.storage[binding.expression].kind))
    {
        let names =
            bindings.iter().map(|binding| context.allocate(&binding.parameter.name)).collect_vec();
        for (binding, name) in bindings.iter().zip(&names) {
            context.bind_direct(&binding.parameter, name.clone());
        }

        let profiles = local_profiles(generator.module, bindings);
        let dispatcher_name = |profiles: &[TailCallProfile]| {
            let mut dispatcher_names = profiles.iter().map(|profile| {
                let TailCallIdentity::Local(local) = profile.identity else {
                    unreachable!("invariant violated: local tail-call group contains a global")
                };
                let position = bindings
                    .iter()
                    .position(|binding| binding.parameter.id == local)
                    .expect("invariant violated: local tail-call profile has no binding");
                names[position].as_str()
            });
            let dispatcher_suffix = dispatcher_names.join("_");
            context.allocate(format_smolstr!("$tail_{dispatcher_suffix}"))
        };
        let groups = tail_call_groups(generator.module, &profiles, dispatcher_name);
        let mut optimized = FxHashSet::default();
        for group in &groups {
            if !group.is_singleton() {
                let state_name = context.allocate("$state");
                let argument_names = (0..group.maximum_arity)
                    .map(|position| context.allocate(format_smolstr!("$argument{position}")))
                    .collect::<Rc<[_]>>();
                let tail_calls =
                    TailCallContext::new(group, state_name.clone(), Rc::clone(&argument_names));
                let mut dispatcher_parameters = vec![state_name];
                dispatcher_parameters.extend(argument_names.iter().cloned());
                let outer_tail_calls = context.tail_calls.replace(tail_calls);
                writer.constant_arrow(
                    &group.dispatcher_name,
                    None,
                    dispatcher_parameters,
                    |writer| generator.render_tail_call_dispatcher(tree, writer, group, context),
                )?;
                context.tail_calls = outer_tail_calls;
            }

            for (state, profile) in group.profiles.iter().enumerate() {
                let TailCallIdentity::Local(local) = profile.identity else {
                    unreachable!("invariant violated: local tail-call group contains a global")
                };
                let position = bindings
                    .iter()
                    .position(|binding| binding.parameter.id == local)
                    .expect("invariant violated: local tail-call profile has no binding");
                let source_name =
                    binding_source_name(&bindings[position]).map(|name| writer.register_name(name));
                generator.render_local_tail_call_wrapper(
                    tree,
                    writer,
                    TailCallWrapper { name: &names[position], source_name, group, profile, state },
                    context,
                )?;
                optimized.insert(profile.identity);
            }
        }

        for (binding, name) in bindings.iter().zip(&names) {
            let identity = TailCallIdentity::Local(binding.parameter.id);
            if optimized.contains(&identity) {
                continue;
            }
            let _origin = tree.enter_origin(generator.binding_origin(binding));
            let source_name = binding_source_name(binding);
            match &generator.module.storage[binding.expression].kind {
                ExpressionKind::Abstraction { parameters, body } => {
                    generator.render_abstraction_binding(
                        tree,
                        writer,
                        AbstractionBinding {
                            name,
                            source_name,
                            parameters,
                            body: *body,
                            uncurried: false,
                        },
                        context,
                    )?;
                }
                ExpressionKind::UncurriedAbstraction { parameters, body } => {
                    generator.render_abstraction_binding(
                        tree,
                        writer,
                        AbstractionBinding {
                            name,
                            source_name,
                            parameters,
                            body: *body,
                            uncurried: true,
                        },
                        context,
                    )?;
                }
                _ => {
                    unreachable!("invariant violated: recursive closure group contains a value")
                }
            }
        }
        return Ok(());
    }
    if recursive {
        let mut renderer = generator.renderer(tree, writer, context);
        return render_lazy_let(&mut renderer, bindings);
    }
    for binding in bindings {
        let _origin = tree.enter_origin(generator.binding_origin(binding));
        let name = context.allocate(&binding.parameter.name);
        let source_name = binding_source_name(binding);
        let mut value = binding.expression;
        while let ExpressionKind::Let { recursive: false, bindings, body } =
            &generator.module.storage[value].kind
        {
            render_let(&mut generator.renderer(tree, writer, context), false, bindings)?;
            value = *body;
        }
        match &generator.module.storage[value].kind {
            ExpressionKind::Abstraction { parameters, body } => {
                context.bind_direct(&binding.parameter, name.clone());
                generator.render_abstraction_binding(
                    tree,
                    writer,
                    AbstractionBinding {
                        name: &name,
                        source_name,
                        parameters,
                        body: *body,
                        uncurried: false,
                    },
                    context,
                )?;
            }
            ExpressionKind::UncurriedAbstraction { parameters, body } => {
                context.bind_direct(&binding.parameter, name.clone());
                generator.render_abstraction_binding(
                    tree,
                    writer,
                    AbstractionBinding {
                        name: &name,
                        source_name,
                        parameters,
                        body: *body,
                        uncurried: true,
                    },
                    context,
                )?;
            }
            _ => {
                let value = generator.expression_value(tree, writer, value, context)?;
                writer.constant(tree, &name, value, false);
                context.bind_direct(&binding.parameter, name);
            }
        }
    }
    Ok(())
}

fn render_lazy_let(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    bindings: &[Binding],
) -> ModuleResult<()> {
    let FunctionRenderer { generator, tree, writer, context } = renderer;
    let runtime = generator
        .runtime_namespace
        .as_deref()
        .expect("invariant violated: recursive local values require the JavaScript runtime");
    let mut bindings = bindings.iter().collect_vec();
    bindings.sort_by_key(|binding| binding.source_order);
    let names =
        bindings.iter().map(|binding| context.allocate(&binding.parameter.name)).collect_vec();
    let accessors =
        names.iter().map(|name| context.allocate(format_smolstr!("$lazy_{name}"))).collect_vec();
    for ((binding, name), accessor) in bindings.iter().zip(&names).zip(&accessors) {
        let _ = name;
        context.bind_lazy(&binding.parameter, accessor.clone());
        writer.mutable(accessor);
    }
    for (binding, accessor) in bindings.iter().zip(&accessors) {
        let _origin = tree.enter_origin(generator.binding_origin(binding));
        let runtime = tree.identifier(runtime);
        let binding_function = tree.member(runtime, "binding");
        let source_name = tree.string(binding.parameter.name.as_str());
        let binding_function = tree.expression(binding_function);
        let source_name = tree.expression(source_name);
        writer.binding_call(
            BindingCallTarget::Assignment(accessor),
            binding_function,
            source_name,
            |writer| {
                generator.render_expression(
                    tree,
                    writer,
                    binding.expression,
                    Destination::Return,
                    context,
                )
            },
        )?;
    }
    for ((binding, name), accessor) in bindings.iter().zip(&names).zip(&accessors) {
        let _origin = tree.enter_origin(generator.binding_origin(binding));
        let accessor_expression = tree.identifier(accessor);
        let value = tree.call(accessor_expression, Vec::new());
        writer.constant(tree, name, value, false);
        context.bind_direct(&binding.parameter, name.clone());
    }
    Ok(())
}

pub(super) fn rendered_expression_is_reusable(
    tree: &Tree,
    expression: &RenderedExpression,
) -> bool {
    !expression.pending_evaluation || tree.expression_is_atomic(&expression.value)
}

fn record_updates_reuse_source(updates: &[RecordUpdate]) -> bool {
    updates.iter().any(|update| matches!(update, RecordUpdate::Branch { .. }))
}

pub(super) fn local_expression(
    generator: &Generator<'_>,
    tree: &mut Tree,
    parameter: &Parameter,
    context: &FunctionContext,
) -> ModuleResult<ExpressionId> {
    match context.locals.get(&parameter.id) {
        Some(LocalBinding::Direct(name)) => Ok(tree.identifier(name)),
        Some(LocalBinding::Inline(expression)) => Ok(tree.duplicate(expression)),
        Some(LocalBinding::Lazy(name)) => {
            let accessor = tree.identifier(name);
            Ok(tree.call(accessor, Vec::new()))
        }
        Some(LocalBinding::Reference(_)) => {
            unreachable!("invariant violated: local reference is used outside its operations")
        }
        None => Err(generator
            .unsupported(UnsupportedState::MissingLocal { name: parameter.name.to_string() })),
    }
}
