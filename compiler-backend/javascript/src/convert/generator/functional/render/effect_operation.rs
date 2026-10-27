use functional::optimize::{EffectPosition, local_is_captured, local_uses};
use functional::tree::{
    EffectPrimitive, ExpressionId as FunctionalExpressionId, ExpressionKind, Parameter,
    PatternKind, ReferenceModifyResult, ReferenceWriteResult,
};

use super::{Destination, FunctionRenderer, capture_effect_value};
use crate::error::ModuleResult;
use crate::tree::{ExpressionId, ObjectProperty};

pub(super) enum CapturedOperation {
    Primitive(CapturedPrimitive),
}

pub(super) enum CapturedPrimitive {
    NewReference {
        value: ExpressionId,
    },
    ReadReference {
        reference: ExpressionId,
    },
    WriteReference {
        value: ExpressionId,
        reference: ExpressionId,
        result: ReferenceWriteResult,
    },
    ModifyReference {
        function: CapturedFunction,
        reference: ExpressionId,
        result: ReferenceModifyResult,
    },
}

pub(super) enum CapturedFunction {
    Abstraction { parameter: Option<Parameter>, body: FunctionalExpressionId },
    Value(ExpressionId),
}

pub(super) fn capture_primitive(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    primitive: &EffectPrimitive,
) -> ModuleResult<CapturedOperation> {
    let primitive = match primitive {
        EffectPrimitive::NewReference { value } => {
            let value = capture_effect_value(renderer, *value, "$value")?;
            CapturedPrimitive::NewReference { value }
        }
        EffectPrimitive::ReadReference { reference } => {
            let reference = capture_effect_value(renderer, *reference, "$reference")?;
            CapturedPrimitive::ReadReference { reference }
        }
        EffectPrimitive::WriteReference { value, reference, result } => {
            let value = capture_effect_value(renderer, *value, "$value")?;
            let reference = capture_effect_value(renderer, *reference, "$reference")?;
            CapturedPrimitive::WriteReference { value, reference, result: *result }
        }
        EffectPrimitive::ModifyReference { function, reference, result } => {
            let function = capture_function(renderer, *function, "$function")?;
            let reference = capture_stable_value(renderer, *reference, "$reference")?;
            CapturedPrimitive::ModifyReference { function, reference, result: *result }
        }
    };
    Ok(CapturedOperation::Primitive(primitive))
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
    let value = capture_effect_value(renderer, expression, preferred_name)?;
    Ok(CapturedFunction::Value(value))
}

pub(super) fn execute_operation(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    operation: CapturedOperation,
) -> ModuleResult<Option<ExpressionId>> {
    match operation {
        CapturedOperation::Primitive(primitive) => execute_primitive(renderer, primitive),
    }
}

pub(super) fn render_operation_result(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    result: Option<ExpressionId>,
    destination: Destination<'_>,
) {
    let FunctionRenderer { generator, tree, writer, .. } = renderer;
    match (result, destination) {
        (None, Destination::Return) => writer.return_undefined(),
        (result, destination) => {
            let value = result.unwrap_or_else(|| tree.undefined());
            generator.render_destination(tree, writer, value, destination);
        }
    }
}

fn execute_primitive(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    primitive: CapturedPrimitive,
) -> ModuleResult<Option<ExpressionId>> {
    let tree = &mut *renderer.tree;
    let result = match primitive {
        CapturedPrimitive::NewReference { value } => {
            let field = ObjectProperty::Field { name: "value".into(), value };
            Some(tree.object(vec![field]))
        }
        CapturedPrimitive::ReadReference { reference } => Some(tree.member(reference, "value")),
        CapturedPrimitive::WriteReference { value, reference, result } => {
            let assignment = tree.member_assignment(reference, "value", value);
            match result {
                ReferenceWriteResult::Value => Some(assignment),
                ReferenceWriteResult::Unit => {
                    renderer.writer.evaluate(tree, assignment);
                    None
                }
            }
        }
        CapturedPrimitive::ModifyReference { function, reference, result } => {
            let current_reference = tree.duplicate(&reference);
            let current = tree.member(current_reference, "value");
            let modified = apply_function(renderer, function, current)?;
            let FunctionRenderer { tree, writer, context, .. } = renderer;
            match result {
                ReferenceModifyResult::State => {
                    Some(tree.member_assignment(reference, "value", modified))
                }
                ReferenceModifyResult::Unit => {
                    let assignment = tree.member_assignment(reference, "value", modified);
                    writer.evaluate(tree, assignment);
                    None
                }
                ReferenceModifyResult::Record => {
                    let name = context.allocate("$modified");
                    writer.constant(tree, &name, modified, false);
                    let record = tree.identifier(&name);
                    let state = tree.member(record, "state");
                    let assignment = tree.member_assignment(reference, "value", state);
                    writer.evaluate(tree, assignment);
                    let record = tree.identifier(&name);
                    Some(tree.member(record, "value"))
                }
            }
        }
    };
    Ok(result)
}

fn apply_function(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    function: CapturedFunction,
    argument: ExpressionId,
) -> ModuleResult<ExpressionId> {
    let FunctionRenderer { generator, tree, writer, context } = renderer;
    match function {
        CapturedFunction::Value(function) => Ok(tree.call(function, vec![argument])),
        CapturedFunction::Abstraction { parameter, body } => {
            if let Some(parameter) = parameter {
                let storage = &generator.module.storage;
                if local_uses(storage, body, parameter.id) <= 1
                    && !local_is_captured(storage, body, EffectPosition::Constructed, parameter.id)
                {
                    context.bind_inline(&parameter, argument);
                } else {
                    let name = context.allocate(&parameter.name);
                    writer.constant(tree, &name, argument, false);
                    context.bind_direct(&parameter, name);
                }
            }
            generator.expression_value(tree, writer, body, context)
        }
    }
}
