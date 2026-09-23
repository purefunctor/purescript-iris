//! JavaScript expressions for atomic functional syntax.

use files::FileId;
use functional::tree::{
    BinaryOperator as FunctionalBinaryOperator, Literal, ReflectableEvidence, ReflectableOrdering,
    SynthesizedEvidence, UnaryOperator as FunctionalUnaryOperator,
};
use itertools::Itertools;
use smol_str::{SmolStr, format_smolstr};

use crate::error::{ModuleError, ModuleResult, UnsupportedState};
use crate::tree::{BinaryOperator, ExpressionId, ObjectProperty, Tree, UnaryOperator};

pub(super) fn literal_expression(
    tree: &mut Tree,
    literal: &Literal,
    file_id: FileId,
) -> ModuleResult<ExpressionId> {
    match literal {
        Literal::String(value) => Ok(tree.string_utf16(value.as_utf16())),
        Literal::Char(value) => Ok(tree.string(value.to_string())),
        Literal::Boolean(value) => Ok(tree.boolean(*value)),
        Literal::Integer(value) => Ok(integer_expression(tree, *value)),
        Literal::Number(value) => {
            let number = value.parse::<f64>().map_err(|_| ModuleError::Unsupported {
                file_id,
                state: UnsupportedState::InvalidNumber { value: value.to_string() },
            })?;
            if !number.is_finite() {
                return Err(ModuleError::Unsupported {
                    file_id,
                    state: UnsupportedState::InvalidNumber { value: value.to_string() },
                });
            }
            Ok(tree.number(number.to_string()))
        }
    }
}

fn integer_expression(tree: &mut Tree, value: i32) -> ExpressionId {
    let value = tree.number(value.to_string());
    integer_coercion_expression(tree, value)
}

pub(super) fn integer_coercion_expression(tree: &mut Tree, value: ExpressionId) -> ExpressionId {
    let zero = tree.number("0");
    tree.binary(BinaryOperator::BitwiseOr, value, zero)
}

pub(super) fn unary_expression(
    tree: &mut Tree,
    operator: FunctionalUnaryOperator,
    value: ExpressionId,
) -> ExpressionId {
    match operator {
        FunctionalUnaryOperator::BooleanNot => tree.unary(UnaryOperator::LogicalNot, value),
        FunctionalUnaryOperator::IntegerNegate => {
            let value = tree.unary(UnaryOperator::Negate, value);
            integer_coercion_expression(tree, value)
        }
        FunctionalUnaryOperator::NumberNegate => {
            let zero = tree.number("0");
            tree.binary(BinaryOperator::Subtract, zero, value)
        }
    }
}

pub(super) fn binary_expression(
    tree: &mut Tree,
    operator: FunctionalBinaryOperator,
    left: ExpressionId,
    right: ExpressionId,
) -> ExpressionId {
    match operator {
        FunctionalBinaryOperator::IntegerAdd => {
            integer_binary_expression(tree, BinaryOperator::Add, left, right)
        }
        FunctionalBinaryOperator::IntegerSubtract => {
            integer_binary_expression(tree, BinaryOperator::Subtract, left, right)
        }
        FunctionalBinaryOperator::IntegerMultiply => {
            integer_binary_expression(tree, BinaryOperator::Multiply, left, right)
        }
    }
}

fn integer_binary_expression(
    tree: &mut Tree,
    operator: BinaryOperator,
    left: ExpressionId,
    right: ExpressionId,
) -> ExpressionId {
    let value = tree.binary(operator, left, right);
    integer_coercion_expression(tree, value)
}

pub(super) fn curried_call_expression(
    tree: &mut Tree,
    function: ExpressionId,
    arguments: Vec<ExpressionId>,
    synthetic: bool,
) -> ExpressionId {
    if arguments.is_empty() {
        return if synthetic {
            tree.pure_call(function, vec![])
        } else {
            tree.call(function, vec![])
        };
    }
    let arguments = arguments.into_iter();
    let mut arguments = arguments.peekable();
    let mut expression = function;
    while let Some(argument) = arguments.next() {
        expression = if synthetic && arguments.peek().is_none() {
            tree.pure_call(expression, vec![argument])
        } else {
            tree.call(expression, vec![argument])
        };
    }
    expression
}

pub(super) fn constructor_expression(tree: &mut Tree, name: &str, arity: usize) -> ExpressionId {
    if arity == 0 {
        return tree.string(name);
    }

    let arguments = (0..arity).map(|index| format_smolstr!("$value{index}")).collect_vec();
    let tag = tree.string(name);
    let mut properties = Vec::with_capacity(arguments.len() + 1);
    properties.push(ObjectProperty::Field { name: "tag".to_owned(), value: tag });
    for (index, argument) in arguments.iter().enumerate() {
        let value = tree.identifier(argument);
        properties.push(ObjectProperty::Field { name: format!("_{}", index + 1), value });
    }
    let mut expression = tree.object(properties);
    for argument in arguments.into_iter().rev() {
        expression = tree.arrow(vec![argument], expression);
    }
    expression
}

pub(super) fn synthesized_evidence_expression(
    tree: &mut Tree,
    evidence: &SynthesizedEvidence,
) -> ExpressionId {
    match evidence {
        SynthesizedEvidence::IsSymbol(symbol) => {
            let symbol = tree.string_utf16(symbol.as_utf16());
            let reflect = tree.arrow(vec![SmolStr::new_static("$proxy")], symbol);
            tree.object(vec![ObjectProperty::Field {
                name: "reflectSymbol".to_owned(),
                value: reflect,
            }])
        }
        SynthesizedEvidence::Reflectable(evidence) => {
            let value = match evidence {
                ReflectableEvidence::Integer(value) => integer_expression(tree, *value),
                ReflectableEvidence::String(value) => tree.string_utf16(value.as_utf16()),
                ReflectableEvidence::Boolean(value) => tree.boolean(*value),
                ReflectableEvidence::Ordering(ordering) => {
                    let tag = match ordering {
                        ReflectableOrdering::Less => "LT",
                        ReflectableOrdering::Equal => "EQ",
                        ReflectableOrdering::Greater => "GT",
                    };
                    tree.string(tag)
                }
            };
            let reflect = tree.arrow(vec![SmolStr::new_static("$proxy")], value);
            tree.object(vec![ObjectProperty::Field {
                name: "reflectType".to_owned(),
                value: reflect,
            }])
        }
    }
}

pub(super) fn combine_conditions(
    tree: &mut Tree,
    conditions: Vec<ExpressionId>,
) -> Option<ExpressionId> {
    let mut conditions = conditions.into_iter();
    let first = conditions.next()?;
    Some(
        conditions.fold(first, |condition, next| {
            tree.binary(BinaryOperator::LogicalAnd, condition, next)
        }),
    )
}
