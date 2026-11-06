use functional::tree::{
    EffectLoop, ExpressionId as FunctionalExpressionId, ExpressionKind, Parameter, PatternKind,
};

use super::{
    CapturedEffectAction, Destination, FunctionRenderer, capture_effect_action,
    capture_effect_value, execute_effect_action_destination, execute_effect_action_value,
};
use crate::error::ModuleResult;
use crate::tree::{ExpressionId, UnaryOperator};

pub(super) enum CapturedLoop {
    Range { start: ExpressionId, end: ExpressionId, body: CapturedFunction },
    Each { array: ExpressionId, body: CapturedFunction },
    While { condition: CapturedEffectAction, body: CapturedEffectAction },
    Until { condition: CapturedEffectAction },
}

pub(super) enum CapturedFunction {
    Abstraction { parameter: Option<Parameter>, body: FunctionalExpressionId },
    Value(ExpressionId),
}

pub(super) fn capture_loop(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    effect_loop: &EffectLoop,
) -> ModuleResult<CapturedLoop> {
    let effect_loop = match effect_loop {
        EffectLoop::Range { start, end, body } => {
            let start = capture_effect_value(renderer, *start, "$start")?;
            let end = capture_stable_value(renderer, *end, "$end")?;
            let body = capture_function(renderer, *body, "$body")?;
            CapturedLoop::Range { start, end, body }
        }
        EffectLoop::Each { array, body } => {
            let array = capture_stable_value(renderer, *array, "$array")?;
            let body = capture_function(renderer, *body, "$body")?;
            CapturedLoop::Each { array, body }
        }
        EffectLoop::While { condition, body } => {
            let condition = capture_effect_action(renderer, *condition, "$condition")?;
            let body = capture_effect_action(renderer, *body, "$body")?;
            CapturedLoop::While { condition, body }
        }
        EffectLoop::Until { condition } => {
            let condition = capture_effect_action(renderer, *condition, "$condition")?;
            CapturedLoop::Until { condition }
        }
    };
    Ok(effect_loop)
}

fn capture_stable_value(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    expression: FunctionalExpressionId,
    preferred_name: &str,
) -> ModuleResult<ExpressionId> {
    let is_literal = matches!(
        renderer.generator.module.storage[expression].kind,
        ExpressionKind::Literal { .. }
    );
    let value = capture_effect_value(renderer, expression, preferred_name)?;
    if is_literal || renderer.tree.expression_is_atomic(&value) {
        return Ok(value);
    }
    let name = renderer.context.allocate(preferred_name);
    renderer.writer.constant(renderer.tree, &name, value, false);
    Ok(renderer.tree.identifier(name))
}

fn capture_function(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    expression: FunctionalExpressionId,
    preferred_name: &str,
) -> ModuleResult<CapturedFunction> {
    let storage = &renderer.generator.module.storage;
    if let ExpressionKind::Abstraction { parameters, body } = &storage[expression].kind
        && let [pattern] = parameters.as_ref()
    {
        let parameter = match &storage[*pattern].kind {
            PatternKind::Variable(parameter) => Some(Some(parameter.clone())),
            PatternKind::Named { parameter, pattern }
                if matches!(storage[*pattern].kind, PatternKind::Wildcard) =>
            {
                Some(Some(parameter.clone()))
            }
            PatternKind::Wildcard => Some(None),
            PatternKind::Named { .. }
            | PatternKind::Literal(_)
            | PatternKind::Array(_)
            | PatternKind::Record(_)
            | PatternKind::Constructor { .. } => None,
        };
        if let Some(parameter) = parameter {
            return Ok(CapturedFunction::Abstraction { parameter, body: *body });
        }
    }
    let value = capture_stable_value(renderer, expression, preferred_name)?;
    Ok(CapturedFunction::Value(value))
}

pub(super) fn execute_loop(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    effect_loop: CapturedLoop,
) -> ModuleResult<()> {
    let FunctionRenderer { generator, tree, writer, context } = renderer;
    match effect_loop {
        CapturedLoop::Range { start, end, body } => {
            let index = match &body {
                CapturedFunction::Abstraction { parameter: Some(parameter), .. } => {
                    context.allocate(&parameter.name)
                }
                CapturedFunction::Abstraction { parameter: None, .. }
                | CapturedFunction::Value(_) => context.allocate("$index"),
            };
            let start = tree.duplicate(&start);
            let end = tree.duplicate(&end);
            writer.for_loop(tree, &index, start, end, |tree, writer| {
                let mut renderer = generator.renderer(tree, writer, context);
                let argument = renderer.tree.identifier(&index);
                execute_loop_body(&mut renderer, body, argument)
            })
        }
        CapturedLoop::Each { array, body } => {
            let length = context.allocate("$length");
            let array_length = tree.duplicate(&array);
            let array_length = tree.member(array_length, "length");
            writer.constant(tree, &length, array_length, false);
            let index = context.allocate("$index");
            let start = tree.number("0");
            let end = tree.identifier(&length);
            writer.for_loop(tree, &index, start, end, |tree, writer| {
                let mut renderer = generator.renderer(tree, writer, context);
                let array = renderer.tree.duplicate(&array);
                let position = renderer.tree.identifier(&index);
                let element = renderer.tree.index(array, position);
                execute_loop_body(&mut renderer, body, element)
            })
        }
        CapturedLoop::While { condition, body } => {
            let mut renderer = generator.renderer(tree, writer, context);
            execute_conditional_loop(&mut renderer, condition, false, Some(body))
        }
        CapturedLoop::Until { condition } => {
            let mut renderer = generator.renderer(tree, writer, context);
            execute_conditional_loop(&mut renderer, condition, true, None)
        }
    }
}

fn execute_loop_body(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    body: CapturedFunction,
    argument: ExpressionId,
) -> ModuleResult<()> {
    let FunctionRenderer { generator, tree, writer, context } = renderer;
    match body {
        CapturedFunction::Abstraction { parameter, body } => {
            if let Some(parameter) = parameter {
                let name = match tree.expression_identifier(&argument) {
                    Some(name) => name.into(),
                    None => {
                        let name = context.allocate(&parameter.name);
                        writer.constant(tree, &name, argument, false);
                        name
                    }
                };
                context.bind_direct(&parameter, name);
            }
            generator.render_expression(tree, writer, body, Destination::EffectContinue, context)
        }
        CapturedFunction::Value(function) => {
            let function = tree.duplicate(&function);
            let action = tree.call(function, vec![argument]);
            let value = tree.call(action, Vec::new());
            generator.render_destination(tree, writer, value, Destination::Continue);
            Ok(())
        }
    }
}

fn execute_conditional_loop(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    condition: CapturedEffectAction,
    until: bool,
    body: Option<CapturedEffectAction>,
) -> ModuleResult<()> {
    let FunctionRenderer { generator, tree, writer, context } = renderer;
    match condition {
        CapturedEffectAction::Expression { value, origin } => {
            let condition = {
                let _origin = tree.enter_origin(origin);
                tree.call(value, Vec::new())
            };
            let condition =
                if until { tree.unary(UnaryOperator::LogicalNot, condition) } else { condition };
            writer.while_loop(tree, condition, |tree, writer| {
                let mut renderer = generator.renderer(tree, writer, context);
                execute_loop_action(&mut renderer, body)
            })
        }
        CapturedEffectAction::Effect(effect) => {
            let always = tree.boolean(true);
            writer.while_loop(tree, always, |tree, writer| {
                let mut renderer = generator.renderer(tree, writer, context);
                let action = CapturedEffectAction::Effect(effect);
                let condition = execute_effect_action_value(&mut renderer, action, "$condition")?;
                let FunctionRenderer { tree, writer, .. } = &mut renderer;
                let exit = if until {
                    condition
                } else {
                    tree.unary(UnaryOperator::LogicalNot, condition)
                };
                writer.if_block(tree, exit, |_, writer| writer.break_loop());
                execute_loop_action(&mut renderer, body)
            })
        }
    }
}

fn execute_loop_action(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    action: Option<CapturedEffectAction>,
) -> ModuleResult<()> {
    if let Some(action) = action {
        execute_effect_action_destination(renderer, action, Destination::Continue)?;
    }
    renderer.writer.remove_trailing_continue();
    Ok(())
}
