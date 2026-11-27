use functional::optimize::{
    EffectPosition, for_each_expression_child, local_is_captured, local_uses,
};
use functional::tree::{
    EffectExpression, EffectStatement, ExpressionId as FunctionalExpressionId, ExpressionKind,
    LocalId, Origin, Parameter, Storage,
};
use smol_str::SmolStr;

use crate::error::ModuleResult;
use crate::tree::{ExpressionId, ObjectProperty, Tree};
use crate::writer::Writer;

use super::effect_loop::{CapturedLoop, capture_loop, execute_loop};
use super::expression::rendered_expression_is_reusable;
use super::{Destination, FunctionContext, FunctionRenderer, Generator, LocalBinding};

pub(super) enum CapturedEffectAction {
    Expression { value: ExpressionId, origin: Option<Origin> },
    Effect(Box<CapturedEffect>),
}

pub(super) enum CapturedEffect {
    Pure { value: ExpressionId },
    Bind { action: CapturedEffectAction, parameter: Parameter, body: FunctionalExpressionId },
    Map { function: ExpressionId, action: CapturedEffectAction },
    Apply { function_action: CapturedEffectAction, argument_action: CapturedEffectAction },
    Operation(CapturedOperation),
    Statement { statement: CapturedStatement, unit: FunctionalExpressionId },
}

pub(super) enum CapturedStatement {
    ReferenceWrite { target: ExpressionId, value: ExpressionId },
    Loop(CapturedLoop),
    When { condition: ExpressionId, action: CapturedEffectAction, origin: Option<Origin> },
}

pub(super) enum CapturedOperation {
    ReferenceNew { value: ExpressionId },
    ReferenceRead { target: ExpressionId },
    ReferenceWrite { target: ExpressionId, value: ExpressionId },
    UncurriedApplication { function: ExpressionId, arguments: Vec<ExpressionId> },
}

impl CapturedOperation {
    fn execute(self, tree: &mut Tree) -> ExpressionId {
        match self {
            CapturedOperation::ReferenceNew { value } => {
                tree.object(vec![ObjectProperty::Field { name: "value".into(), value }])
            }
            CapturedOperation::ReferenceRead { target } => target,
            CapturedOperation::ReferenceWrite { target, value } => tree.assignment(target, value),
            CapturedOperation::UncurriedApplication { function, arguments } => {
                tree.call(function, arguments)
            }
        }
    }
}

impl Generator<'_> {
    pub(super) fn render_effect_destination<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        expression: FunctionalExpressionId,
        destination: Destination<'_>,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        if let ExpressionKind::Effect { effect: EffectExpression::Pure(value) } =
            &self.module.storage[expression].kind
        {
            if destination.is_discard() && self.discarded_value_is_stable(*value, context) {
                match destination {
                    Destination::DiscardAndBreak { label } => writer.break_label(label),
                    Destination::Continue => writer.continue_loop(),
                    _ => {}
                }
                return Ok(());
            }
            return self.render_expression(tree, writer, *value, destination, context);
        }
        if let ExpressionKind::Effect { effect } = &self.module.storage[expression].kind {
            if let EffectExpression::Bind { action, parameter, body } = effect
                && matches!(
                    self.module.storage[*action].kind,
                    ExpressionKind::IfThenElse { .. }
                        | ExpressionKind::Case { .. }
                        | ExpressionKind::Guarded { .. }
                )
            {
                if local_uses(&self.module.storage, *body, parameter.id) == 0 {
                    self.render_expression(
                        tree,
                        writer,
                        *action,
                        Destination::EffectDiscard,
                        context,
                    )?;
                } else {
                    let name = context.allocate(&parameter.name);
                    writer.mutable(&name);
                    self.render_expression(
                        tree,
                        writer,
                        *action,
                        Destination::EffectAssign(&name),
                        context,
                    )?;
                    context.bind_direct(parameter, name);
                }
                return self.render_expression(tree, writer, *body, destination.effect(), context);
            }
            let mut renderer = self.renderer(tree, writer, context);
            let effect = capture_effect(&mut renderer, effect)?;
            return execute_effect(&mut renderer, effect, destination);
        }

        let effect = self.expression_value(tree, writer, expression, context)?;
        let value = tree.call(effect, Vec::new());
        self.render_destination(tree, writer, value, destination);
        Ok(())
    }

    fn discarded_value_is_stable(
        &self,
        expression: FunctionalExpressionId,
        context: &FunctionContext,
    ) -> bool {
        match &self.module.storage[expression].kind {
            ExpressionKind::Literal { .. } | ExpressionKind::Constructor { .. } => true,
            ExpressionKind::Global { global } => !self.lazy_global_names.contains_key(&global.id),
            ExpressionKind::Local { parameter } => {
                matches!(context.locals.get(&parameter.id), Some(LocalBinding::Direct(_)))
            }
            _ => false,
        }
    }

    pub(super) fn render_effect_expression_destination<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        effect: &EffectExpression,
        destination: Destination<'_>,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        let mut renderer = self.renderer(tree, writer, context);
        let effect = capture_effect(&mut renderer, effect)?;
        let break_label = match destination {
            Destination::Return => {
                writer.return_arrow(Vec::new(), |writer| {
                    let mut renderer = self.renderer(tree, writer, context);
                    execute_effect(&mut renderer, effect, Destination::Return)
                })?;
                None
            }
            Destination::TailEffectThunkReturn => {
                writer.return_arrow(Vec::new(), |writer| {
                    let mut renderer = self.renderer(tree, writer, context);
                    execute_effect(&mut renderer, effect, Destination::TailEffectReturn)
                })?;
                None
            }
            Destination::Assign(name) => {
                writer.assign_arrow(name, Vec::new(), |writer| {
                    let mut renderer = self.renderer(tree, writer, context);
                    execute_effect(&mut renderer, effect, Destination::Return)
                })?;
                None
            }
            Destination::AssignAndBreak { name, label } => {
                writer.assign_arrow(name, Vec::new(), |writer| {
                    let mut renderer = self.renderer(tree, writer, context);
                    execute_effect(&mut renderer, effect, Destination::Return)
                })?;
                Some(label)
            }
            Destination::Discard => None,
            Destination::DiscardAndBreak { label } => Some(label),
            Destination::Continue => {
                writer.continue_loop();
                None
            }
            Destination::TailEffectReturn
            | Destination::EffectReturn
            | Destination::EffectTailEffectReturn
            | Destination::EffectAssign(_)
            | Destination::EffectAssignAndBreak { .. }
            | Destination::EffectDiscard
            | Destination::EffectDiscardAndBreak { .. }
            | Destination::EffectContinue => {
                unreachable!("invariant violated: effect destination returned an effect thunk")
            }
        };
        if let Some(label) = break_label {
            writer.break_label(label);
        }
        Ok(())
    }

    pub(super) fn render_tail_effect_thunk_destination<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        expression: FunctionalExpressionId,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        let mut renderer = self.renderer(tree, writer, context);
        let effect = capture_effect_value(&mut renderer, expression, "$effect")?;
        writer.return_arrow(Vec::new(), |writer| {
            let value = tree.call(effect, Vec::new());
            self.render_destination(tree, writer, value, Destination::TailEffectReturn);
            Ok(())
        })
    }
}

pub(super) fn effect_expression(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    effect: &EffectExpression,
) -> ModuleResult<ExpressionId> {
    let effect = capture_effect(renderer, effect)?;
    let FunctionRenderer { generator, tree, writer, context } = renderer;
    let (closure, result) = writer.arrow(tree, None, Vec::new(), |tree, writer| {
        let mut renderer = generator.renderer(tree, writer, context);
        execute_effect(&mut renderer, effect, Destination::Return)
    });
    result?;
    Ok(closure)
}

fn capture_effect(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    effect: &EffectExpression,
) -> ModuleResult<CapturedEffect> {
    match effect {
        EffectExpression::Pure(value) => {
            let value = capture_effect_value(renderer, *value, "$value")?;
            Ok(CapturedEffect::Pure { value })
        }
        EffectExpression::Bind { action, parameter, body } => {
            let action = capture_effect_action(renderer, *action, "$action")?;
            Ok(CapturedEffect::Bind { action, parameter: parameter.clone(), body: *body })
        }
        EffectExpression::Map { function, action } => {
            let function = capture_effect_value(renderer, *function, "$function")?;
            let action = capture_effect_action(renderer, *action, "$action")?;
            Ok(CapturedEffect::Map { function, action })
        }
        EffectExpression::Apply { function_action, argument_action } => {
            let function_action =
                capture_effect_action(renderer, *function_action, "$functionAction")?;
            let argument_action =
                capture_effect_action(renderer, *argument_action, "$argumentAction")?;
            Ok(CapturedEffect::Apply { function_action, argument_action })
        }
        EffectExpression::ReferenceNew(value) => {
            let value = capture_effect_value(renderer, *value, "$value")?;
            Ok(CapturedEffect::Operation(CapturedOperation::ReferenceNew { value }))
        }
        EffectExpression::ReferenceRead(reference) => {
            let target = capture_reference_target(renderer, *reference)?;
            Ok(CapturedEffect::Operation(CapturedOperation::ReferenceRead { target }))
        }
        EffectExpression::ReferenceWrite { reference, value } => {
            let target = capture_reference_target(renderer, *reference)?;
            let value = capture_effect_value(renderer, *value, "$value")?;
            Ok(CapturedEffect::Operation(CapturedOperation::ReferenceWrite { target, value }))
        }
        EffectExpression::UncurriedApplication { function, arguments } => {
            let function = capture_effect_value(renderer, *function, "$function")?;
            let arguments = arguments
                .iter()
                .map(|argument| capture_effect_value(renderer, *argument, "$argument"));
            let arguments = arguments.collect::<ModuleResult<Vec<_>>>()?;
            Ok(CapturedEffect::Operation(CapturedOperation::UncurriedApplication {
                function,
                arguments,
            }))
        }
        EffectExpression::Statement { statement, unit } => {
            let statement = capture_statement(renderer, statement)?;
            Ok(CapturedEffect::Statement { statement, unit: *unit })
        }
    }
}

fn capture_statement(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    statement: &EffectStatement,
) -> ModuleResult<CapturedStatement> {
    match statement {
        EffectStatement::ReferenceWrite { reference, value } => {
            let target = capture_reference_target(renderer, *reference)?;
            let value = capture_effect_value(renderer, *value, "$value")?;
            Ok(CapturedStatement::ReferenceWrite { target, value })
        }
        EffectStatement::Loop(effect_loop) => {
            Ok(CapturedStatement::Loop(capture_loop(renderer, effect_loop)?))
        }
        EffectStatement::When { condition, action } => {
            let condition = capture_effect_value(renderer, *condition, "$condition")?;
            let origin = renderer.generator.module.storage[*action].origin;
            let action = capture_effect_action(renderer, *action, "$action")?;
            Ok(CapturedStatement::When { condition, action, origin })
        }
    }
}

fn capture_reference_target(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    reference: FunctionalExpressionId,
) -> ModuleResult<ExpressionId> {
    if let ExpressionKind::Local { parameter } = &renderer.generator.module.storage[reference].kind
        && let Some(LocalBinding::Reference(name)) = renderer.context.locals.get(&parameter.id)
    {
        let _origin =
            renderer.tree.enter_origin(renderer.generator.module.storage[reference].origin);
        return Ok(renderer.tree.identifier(name));
    }
    let reference = capture_effect_value(renderer, reference, "$reference")?;
    Ok(renderer.tree.member(reference, "value"))
}

fn reference_stays_local(
    storage: &Storage,
    body: FunctionalExpressionId,
    reference: LocalId,
) -> bool {
    let is_reference = |expression: FunctionalExpressionId| {
        matches!(
            &storage[expression].kind,
            ExpressionKind::Local { parameter } if parameter.id == reference
        )
    };
    let mut pending = vec![body];
    while let Some(expression) = pending.pop() {
        if is_reference(expression) {
            return false;
        }
        match &storage[expression].kind {
            ExpressionKind::Effect { effect: EffectExpression::ReferenceRead(operand) }
                if is_reference(*operand) => {}
            ExpressionKind::Effect {
                effect: EffectExpression::ReferenceWrite { reference: operand, value },
            } if is_reference(*operand) => pending.push(*value),
            ExpressionKind::Effect {
                effect:
                    EffectExpression::Statement {
                        statement: EffectStatement::ReferenceWrite { reference: operand, value },
                        ..
                    },
            } if is_reference(*operand) => pending.push(*value),
            kind => for_each_expression_child(kind, |child| pending.push(child)),
        }
    }
    true
}

pub(super) fn capture_effect_action(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    expression: FunctionalExpressionId,
    preferred_name: &str,
) -> ModuleResult<CapturedEffectAction> {
    let generator = renderer.generator;
    if let ExpressionKind::Effect { effect } = &generator.module.storage[expression].kind {
        let effect = capture_effect(renderer, effect)?;
        return Ok(CapturedEffectAction::Effect(Box::new(effect)));
    }

    let origin = generator.module.storage[expression].origin;
    let value = capture_effect_value(renderer, expression, preferred_name)?;
    Ok(CapturedEffectAction::Expression { value, origin })
}

pub(super) fn capture_effect_value(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    expression: FunctionalExpressionId,
    preferred_name: &str,
) -> ModuleResult<ExpressionId> {
    let FunctionRenderer { generator, tree, writer, context } = renderer;
    let _origin = tree.enter_origin(generator.module.storage[expression].origin);
    let expression_is_reusable = generator.functional_expression_is_reusable(expression, context);
    let value = generator.rendered_expression(tree, writer, expression, context)?;
    if expression_is_reusable || rendered_expression_is_reusable(tree, &value) {
        return Ok(value.value);
    }
    let name = context.allocate(preferred_name);
    writer.constant(tree, &name, value.value, false);
    Ok(tree.identifier(name))
}

fn execute_effect(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    effect: CapturedEffect,
    destination: Destination<'_>,
) -> ModuleResult<()> {
    match effect {
        CapturedEffect::Pure { value } => {
            renderer.generator.render_destination(
                renderer.tree,
                renderer.writer,
                value,
                destination,
            );
            Ok(())
        }
        CapturedEffect::Bind { action, parameter, body } => {
            if let CapturedEffectAction::Effect(effect) = &action
                && let CapturedEffect::Pure { value } = effect.as_ref()
                && local_uses(&renderer.generator.module.storage, body, parameter.id) <= 1
                && (renderer.tree.expression_is_atomic(value)
                    || !local_is_captured(
                        &renderer.generator.module.storage,
                        body,
                        EffectPosition::Executed,
                        parameter.id,
                    ))
            {
                let value = renderer.tree.duplicate(value);
                renderer.context.bind_inline(&parameter, value);
            } else if let CapturedEffectAction::Effect(effect) = &action
                && let CapturedEffect::Operation(CapturedOperation::ReferenceNew { value }) =
                    effect.as_ref()
                && reference_stays_local(&renderer.generator.module.storage, body, parameter.id)
            {
                let name = renderer.context.allocate(&parameter.name);
                let value = renderer.tree.duplicate(value);
                renderer.writer.mutable_value(renderer.tree, &name, value);
                renderer.context.bind_reference(&parameter, name);
            } else if let CapturedEffectAction::Effect(effect) = &action
                && let CapturedEffect::Statement { .. } = effect.as_ref()
            {
                if local_uses(&renderer.generator.module.storage, body, parameter.id) == 0 {
                    execute_effect_action_destination(renderer, action, Destination::Discard)?;
                } else {
                    let result = execute_effect_action_value(renderer, action, &parameter.name)?;
                    renderer.context.bind_inline(&parameter, result);
                }
            } else {
                let (_, parameter_name) = execute_effect_action(renderer, action, &parameter.name)?;
                renderer.context.bind_direct(&parameter, parameter_name);
            }
            renderer.generator.render_expression(
                renderer.tree,
                renderer.writer,
                body,
                destination.effect(),
                renderer.context,
            )
        }
        CapturedEffect::Map { function, action } => {
            let value = execute_effect_action_value(renderer, action, "$value")?;
            let result = renderer.tree.call(function, vec![value]);
            renderer.generator.render_destination(
                renderer.tree,
                renderer.writer,
                result,
                destination,
            );
            Ok(())
        }
        CapturedEffect::Operation(operation) => {
            let result = operation.execute(renderer.tree);
            renderer.generator.render_destination(
                renderer.tree,
                renderer.writer,
                result,
                destination,
            );
            Ok(())
        }
        CapturedEffect::Statement { statement, unit } => {
            execute_statement(renderer, statement)?;
            render_unit_destination(renderer, unit, destination)
        }
        CapturedEffect::Apply { function_action, argument_action } => {
            let function = if matches!(&argument_action, CapturedEffectAction::Effect(_)) {
                execute_effect_action(renderer, function_action, "$function")?.0
            } else {
                execute_effect_action_value(renderer, function_action, "$function")?
            };
            let argument = execute_effect_action_value(renderer, argument_action, "$argument")?;
            let result = renderer.tree.call(function, vec![argument]);
            renderer.generator.render_destination(
                renderer.tree,
                renderer.writer,
                result,
                destination,
            );
            Ok(())
        }
    }
}

fn render_unit_destination(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    unit: FunctionalExpressionId,
    destination: Destination<'_>,
) -> ModuleResult<()> {
    let FunctionRenderer { generator, tree, writer, context } = renderer;
    match destination {
        Destination::Discard => {}
        Destination::DiscardAndBreak { label } => writer.break_label(label),
        Destination::Continue => writer.continue_loop(),
        destination => {
            let unit = generator.expression_value(tree, writer, unit, context)?;
            generator.render_destination(tree, writer, unit, destination);
        }
    }
    Ok(())
}

fn execute_statement(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    statement: CapturedStatement,
) -> ModuleResult<()> {
    let (condition, action, origin) = match statement {
        CapturedStatement::ReferenceWrite { target, value } => {
            let assignment = renderer.tree.assignment(target, value);
            renderer.writer.expression_statement(renderer.tree, assignment);
            return Ok(());
        }
        CapturedStatement::Loop(effect_loop) => return execute_loop(renderer, effect_loop),
        CapturedStatement::When { condition, action, origin } => (condition, action, origin),
    };
    let FunctionRenderer { generator, tree, writer, context } = renderer;
    let condition = tree.duplicate(&condition);
    writer.if_block(tree, condition, |tree, writer| {
        let _origin = tree.enter_origin(origin);
        let mut renderer = generator.renderer(tree, writer, context);
        execute_effect_action_destination(&mut renderer, action, Destination::Discard)
    })
}

pub(super) fn execute_effect_action_destination(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    action: CapturedEffectAction,
    destination: Destination<'_>,
) -> ModuleResult<()> {
    match action {
        CapturedEffectAction::Expression { value, origin } => {
            let FunctionRenderer { generator, tree, writer, .. } = renderer;
            let _origin = tree.enter_origin(origin);
            let value = tree.call(value, Vec::new());
            generator.render_destination(tree, writer, value, destination);
            Ok(())
        }
        CapturedEffectAction::Effect(effect) => execute_effect(renderer, *effect, destination),
    }
}

pub(super) fn execute_effect_action_value(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    action: CapturedEffectAction,
    preferred_name: &str,
) -> ModuleResult<ExpressionId> {
    match action {
        CapturedEffectAction::Expression { value, origin } => {
            let _origin = renderer.tree.enter_origin(origin);
            Ok(renderer.tree.call(value, Vec::new()))
        }
        CapturedEffectAction::Effect(effect) => match *effect {
            CapturedEffect::Operation(operation) => Ok(operation.execute(renderer.tree)),
            CapturedEffect::Statement { statement, unit } => {
                execute_statement(renderer, statement)?;
                let FunctionRenderer { generator, tree, writer, context } = renderer;
                generator.expression_value(tree, writer, unit, context)
            }
            effect => {
                let name = renderer.context.allocate(preferred_name);
                renderer.writer.mutable(&name);
                execute_effect(renderer, effect, Destination::Assign(&name))?;
                Ok(renderer.tree.identifier(name))
            }
        },
    }
}

fn execute_effect_action(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    action: CapturedEffectAction,
    preferred_name: &str,
) -> ModuleResult<(ExpressionId, SmolStr)> {
    let name = renderer.context.allocate(preferred_name);
    match action {
        CapturedEffectAction::Expression { value, origin } => {
            let _origin = renderer.tree.enter_origin(origin);
            let value = renderer.tree.call(value, Vec::new());
            renderer.writer.constant(renderer.tree, &name, value, false);
        }
        CapturedEffectAction::Effect(effect) => match *effect {
            CapturedEffect::Pure { value } => {
                renderer.writer.constant(renderer.tree, &name, value, false);
            }
            CapturedEffect::Operation(operation) => {
                let value = operation.execute(renderer.tree);
                renderer.writer.constant(renderer.tree, &name, value, false);
            }
            effect => {
                renderer.writer.mutable(&name);
                execute_effect(renderer, effect, Destination::Assign(&name))?;
            }
        },
    }
    Ok((renderer.tree.identifier(&name), name))
}
