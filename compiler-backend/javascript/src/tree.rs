use la_arena::{Arena, Idx, RawIdx};
use oxc_allocator::{Allocator, CloneIn, Vec as ArenaVec};
use oxc_ast::ast::{
    Argument, ArrayExpressionElement, ArrowFunctionBody, BindingPattern, Expression,
    FormalParameter, FormalParameterKind, FormalParameters, ObjectPropertyKind, PropertyKey,
};
use oxc_ast::builder::AstBuilder;
use oxc_span::SPAN;
use oxc_syntax::number::NumberBase;
use oxc_syntax::operator::{
    BinaryOperator as OxcBinaryOperator, LogicalOperator, UnaryOperator as OxcUnaryOperator,
};
use smol_str::SmolStr;

#[derive(Debug, PartialEq, Eq, Hash)]
pub(crate) struct ExpressionId(RawIdx);

pub(crate) struct Tree<'a> {
    allocator: &'a Allocator,
    builder: AstBuilder<'a>,
    expressions: Arena<Option<Expression<'a>>>,
}

pub(crate) enum ObjectProperty {
    Field { name: SmolStr, value: ExpressionId },
    Computed { key: ExpressionId, value: ExpressionId },
    Spread(ExpressionId),
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum UnaryOperator {
    LogicalNot,
    Negate,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum BinaryOperator {
    StrictEqual,
    BitwiseOr,
    LogicalAnd,
    Add,
    Subtract,
    Multiply,
}

impl<'a> Tree<'a> {
    pub(crate) fn new(allocator: &'a Allocator) -> Tree<'a> {
        Tree { allocator, builder: AstBuilder::new(allocator), expressions: Arena::new() }
    }

    fn allocate(&mut self, expression: Expression<'a>) -> ExpressionId {
        ExpressionId(self.expressions.alloc(Some(expression)).into_raw())
    }

    pub(crate) fn expression(&mut self, expression: ExpressionId) -> Expression<'a> {
        self.expressions[Idx::from_raw(expression.0)]
            .take()
            .expect("invariant violated: JavaScript expression was already consumed")
    }

    pub(crate) fn duplicate(&mut self, expression: &ExpressionId) -> ExpressionId {
        let expression = self.expressions[Idx::from_raw(expression.0)]
            .as_ref()
            .expect("invariant violated: JavaScript expression was already consumed")
            .clone_in(self.allocator);
        self.allocate(expression)
    }

    pub(crate) fn clear_call_purity(&mut self, expression: &ExpressionId) {
        let expression = self.expressions[Idx::from_raw(expression.0)]
            .as_mut()
            .expect("invariant violated: JavaScript expression was already consumed");
        if let Expression::CallExpression(call) = expression {
            call.pure = false;
        }
    }

    pub(crate) fn expression_is_atomic(&self, expression: &ExpressionId) -> bool {
        let expression = self.expressions[Idx::from_raw(expression.0)]
            .as_ref()
            .expect("invariant violated: JavaScript expression was already consumed");
        matches!(
            expression,
            Expression::Identifier(_)
                | Expression::StringLiteral(_)
                | Expression::NumericLiteral(_)
                | Expression::BooleanLiteral(_)
                | Expression::NullLiteral(_)
        )
    }

    pub(crate) fn expression_identifier(&self, expression: &ExpressionId) -> Option<&str> {
        let expression = self.expressions[Idx::from_raw(expression.0)]
            .as_ref()
            .expect("invariant violated: JavaScript expression was already consumed");
        let Expression::Identifier(identifier) = expression else { return None };
        Some(identifier.name.as_str())
    }

    fn text(&self, value: &str) -> &'a str {
        self.allocator.alloc_str(value)
    }

    pub(crate) fn identifier(&mut self, name: impl AsRef<str>) -> ExpressionId {
        let name = self.text(name.as_ref());
        let expression = Expression::new_identifier(SPAN, name, &self.builder);
        self.allocate(expression)
    }

    pub(crate) fn string(&mut self, value: impl AsRef<str>) -> ExpressionId {
        let value = self.text(value.as_ref());
        let expression = Expression::new_string_literal(SPAN, value, None, &self.builder);
        self.allocate(expression)
    }

    pub(crate) fn string_utf16(&mut self, value: &[u16]) -> ExpressionId {
        let (value, lone_surrogates) = oxc_string_value(value);
        let value = self.text(&value);
        let expression = Expression::new_string_literal_with_lone_surrogates(
            SPAN,
            value,
            None,
            lone_surrogates,
            &self.builder,
        );
        self.allocate(expression)
    }

    pub(crate) fn number(&mut self, value: impl AsRef<str>) -> ExpressionId {
        let raw = value.as_ref();
        let number = raw.parse().expect("invariant violated: JavaScript number is invalid");
        let raw = self.text(raw).into();
        let expression = Expression::new_numeric_literal(
            SPAN,
            number,
            Some(raw),
            NumberBase::Decimal,
            &self.builder,
        );
        self.allocate(expression)
    }

    pub(crate) fn boolean(&mut self, value: bool) -> ExpressionId {
        let expression = Expression::new_boolean_literal(SPAN, value, &self.builder);
        self.allocate(expression)
    }

    pub(crate) fn null(&mut self) -> ExpressionId {
        let expression = Expression::new_null_literal(SPAN, &self.builder);
        self.allocate(expression)
    }

    pub(crate) fn array(&mut self, elements: Vec<ExpressionId>) -> ExpressionId {
        let allocator = self.allocator;
        let elements = elements.into_iter().map(|element| {
            let expression = self.expression(element);
            ArrayExpressionElement::from(expression)
        });
        let elements = ArenaVec::from_iter_in(elements, &allocator);
        let expression = Expression::new_array_expression(SPAN, elements, &self.builder);
        self.allocate(expression)
    }

    pub(crate) fn object(&mut self, properties: Vec<ObjectProperty>) -> ExpressionId {
        let allocator = self.allocator;
        let properties = properties.into_iter().map(|property| match property {
            ObjectProperty::Field { name, value } => {
                let value = self.expression(value);
                let computed = name == "__proto__";
                let key = if property_is_identifier(&name) && !computed {
                    PropertyKey::new_static_identifier(SPAN, self.text(&name), &self.builder)
                } else {
                    PropertyKey::new_string_literal(SPAN, self.text(&name), None, &self.builder)
                };
                ObjectPropertyKind::new_object_property(
                    SPAN,
                    oxc_ast::ast::PropertyKind::Init,
                    key,
                    value,
                    false,
                    false,
                    computed,
                    &self.builder,
                )
            }
            ObjectProperty::Computed { key, value } => ObjectPropertyKind::new_object_property(
                SPAN,
                oxc_ast::ast::PropertyKind::Init,
                PropertyKey::from(self.expression(key)),
                self.expression(value),
                false,
                false,
                true,
                &self.builder,
            ),
            ObjectProperty::Spread(value) => {
                ObjectPropertyKind::new_spread_property(SPAN, self.expression(value), &self.builder)
            }
        });
        let properties = ArenaVec::from_iter_in(properties, &allocator);
        let expression = Expression::new_object_expression(SPAN, properties, &self.builder);
        self.allocate(expression)
    }

    pub(crate) fn call(
        &mut self,
        callee: ExpressionId,
        arguments: Vec<ExpressionId>,
    ) -> ExpressionId {
        self.call_with_purity(callee, arguments, false)
    }

    pub(crate) fn pure_call(
        &mut self,
        callee: ExpressionId,
        arguments: Vec<ExpressionId>,
    ) -> ExpressionId {
        self.call_with_purity(callee, arguments, true)
    }

    fn call_with_purity(
        &mut self,
        callee: ExpressionId,
        arguments: Vec<ExpressionId>,
        pure: bool,
    ) -> ExpressionId {
        let allocator = self.allocator;
        let callee = self.expression(callee);
        let arguments = arguments.into_iter().map(|argument| {
            let expression = self.expression(argument);
            Argument::from(expression)
        });
        let arguments = ArenaVec::from_iter_in(arguments, &allocator);
        let expression = Expression::new_call_expression_with_pure(
            SPAN,
            callee,
            None,
            arguments,
            false,
            pure,
            &self.builder,
        );
        self.allocate(expression)
    }

    pub(crate) fn member(
        &mut self,
        object: ExpressionId,
        property: impl AsRef<str>,
    ) -> ExpressionId {
        let object = self.expression(object);
        let property = property.as_ref();
        let expression = if property_is_identifier(property) {
            let property =
                oxc_ast::ast::IdentifierName::new(SPAN, self.text(property), &self.builder);
            Expression::new_static_member_expression(SPAN, object, property, false, &self.builder)
        } else {
            let property =
                Expression::new_string_literal(SPAN, self.text(property), None, &self.builder);
            Expression::new_computed_member_expression(SPAN, object, property, false, &self.builder)
        };
        self.allocate(expression)
    }

    pub(crate) fn index(&mut self, object: ExpressionId, index: ExpressionId) -> ExpressionId {
        let expression = Expression::new_computed_member_expression(
            SPAN,
            self.expression(object),
            self.expression(index),
            false,
            &self.builder,
        );
        self.allocate(expression)
    }

    pub(crate) fn unary(&mut self, operator: UnaryOperator, value: ExpressionId) -> ExpressionId {
        let operator = match operator {
            UnaryOperator::LogicalNot => OxcUnaryOperator::LogicalNot,
            UnaryOperator::Negate => OxcUnaryOperator::UnaryNegation,
        };
        let expression =
            Expression::new_unary_expression(SPAN, operator, self.expression(value), &self.builder);
        self.allocate(expression)
    }

    pub(crate) fn binary(
        &mut self,
        operator: BinaryOperator,
        left: ExpressionId,
        right: ExpressionId,
    ) -> ExpressionId {
        let left = self.expression(left);
        let right = self.expression(right);
        let expression = match operator {
            BinaryOperator::LogicalAnd => Expression::new_logical_expression(
                SPAN,
                left,
                LogicalOperator::And,
                right,
                &self.builder,
            ),
            operator => {
                let operator = match operator {
                    BinaryOperator::StrictEqual => OxcBinaryOperator::StrictEquality,
                    BinaryOperator::BitwiseOr => OxcBinaryOperator::BitwiseOR,
                    BinaryOperator::Add => OxcBinaryOperator::Addition,
                    BinaryOperator::Subtract => OxcBinaryOperator::Subtraction,
                    BinaryOperator::Multiply => OxcBinaryOperator::Multiplication,
                    BinaryOperator::LogicalAnd => unreachable!(),
                };
                Expression::new_binary_expression(SPAN, left, operator, right, &self.builder)
            }
        };
        self.allocate(expression)
    }

    pub(crate) fn arrow(&mut self, parameters: Vec<SmolStr>, body: ExpressionId) -> ExpressionId {
        let parameters = parameters.into_iter().map(|parameter| {
            let pattern =
                BindingPattern::new_binding_identifier(SPAN, self.text(&parameter), &self.builder);
            FormalParameter::new(
                SPAN,
                [],
                pattern,
                None,
                None,
                false,
                None,
                false,
                false,
                &self.builder,
            )
        });
        let parameters = ArenaVec::from_iter_in(parameters, &self.allocator);
        let parameters = FormalParameters::boxed(
            SPAN,
            FormalParameterKind::ArrowFormalParameters,
            parameters,
            None,
            &self.builder,
        );
        let body = ArrowFunctionBody::from(self.expression(body));
        let expression = Expression::new_arrow_function_expression(
            SPAN,
            false,
            None,
            parameters,
            None,
            body,
            &self.builder,
        );
        self.allocate(expression)
    }
}

fn oxc_string_value(value: &[u16]) -> (String, bool) {
    let lone_surrogates = char::decode_utf16(value.iter().copied()).any(|item| item.is_err());
    if !lone_surrogates {
        let value = String::from_utf16(value).unwrap_or_else(|_| {
            unreachable!("invariant violated: well-formed UTF-16 failed to decode")
        });
        return (value, false);
    }

    let mut encoded = String::new();
    for item in char::decode_utf16(value.iter().copied()) {
        match item {
            Ok('\u{fffd}') => encoded.push_str("\u{fffd}fffd"),
            Ok(character) => encoded.push(character),
            Err(error) => {
                encoded.push('\u{fffd}');
                encoded.push_str(&format!("{:04x}", error.unpaired_surrogate()));
            }
        }
    }
    (encoded, true)
}

fn property_is_identifier(name: &str) -> bool {
    let mut characters = name.chars();
    let Some(initial) = characters.next() else {
        return false;
    };
    let valid_initial = initial.is_ascii_alphabetic() || initial == '_' || initial == '$';
    let valid_subsequent = characters
        .all(|character| character.is_ascii_alphanumeric() || character == '_' || character == '$');
    valid_initial && valid_subsequent
}

#[cfg(test)]
mod tests {
    use oxc_allocator::Allocator;
    use oxc_ast::ast::Expression;

    use super::{Tree, oxc_string_value};

    #[test]
    fn duplicated_call_keeps_independent_purity() {
        let allocator = Allocator::default();
        let mut tree = Tree::new(&allocator);
        let function = tree.identifier("f");
        let original = tree.pure_call(function, vec![]);
        let copy = tree.duplicate(&original);

        tree.clear_call_purity(&copy);
        let outer = tree.call(copy, vec![]);

        let Expression::CallExpression(original) = tree.expression(original) else {
            panic!("expected original call")
        };
        let Expression::CallExpression(outer) = tree.expression(outer) else {
            panic!("expected outer call")
        };
        let Expression::CallExpression(inner) = &outer.callee else {
            panic!("expected nested call")
        };
        assert!(original.pure);
        assert!(!inner.pure);
    }

    #[test]
    fn oxc_string_value_preserves_utf16_code_units() {
        let (value, lone_surrogates) = oxc_string_value(&[0xd800, 0xfffd, 0xdfff, 0xd800, 0xdc00]);

        assert!(lone_surrogates);
        assert_eq!(value, "\u{fffd}d800\u{fffd}fffd\u{fffd}dfff\u{10000}");
    }
}
