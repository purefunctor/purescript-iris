//! Functional abstraction and parameter classification.

use functional::tree::{
    ArrayElement as FunctionalArrayElement, ExpressionId as FunctionalExpressionId, ExpressionKind,
    Parameter, PatternId, PatternKind, Storage,
};
use smol_str::SmolStr;

use crate::error::ModuleResult;
use crate::program_text::NameIndex;
use crate::tree::{ArrayElement, ExpressionId, ObjectProperty, Tree};

use super::expression::local_expression;
use super::syntax::{
    binary_expression, curried_call_expression, literal_expression,
    synthesized_evidence_expression, unary_expression,
};
use super::{FunctionContext, Generator};

pub(super) fn is_abstraction(kind: &ExpressionKind) -> bool {
    matches!(kind, ExpressionKind::Abstraction { .. } | ExpressionKind::UncurriedAbstraction { .. })
}

pub(super) fn pattern_parameter(storage: &Storage, pattern: PatternId) -> Option<&Parameter> {
    match &storage[pattern].kind {
        PatternKind::Variable(parameter) | PatternKind::Named { parameter, .. } => Some(parameter),
        PatternKind::Wildcard
        | PatternKind::Literal(_)
        | PatternKind::Array(_)
        | PatternKind::Record(_)
        | PatternKind::Constructor { .. } => None,
    }
}

impl Generator<'_> {
    pub(super) fn inline_expression(
        &self,
        tree: &mut Tree,
        expression: FunctionalExpressionId,
        context: &mut FunctionContext,
    ) -> ModuleResult<Option<ExpressionId>> {
        let _origin = tree.enter_origin(self.module.storage[expression].origin);
        let expression = match &self.module.storage[expression].kind {
            ExpressionKind::Literal { literal } => {
                literal_expression(tree, literal, self.module.file_id)?
            }
            ExpressionKind::Array { elements } => {
                let Some(elements) = elements
                    .iter()
                    .map(|element| self.inline_expression(tree, *element, context))
                    .collect::<ModuleResult<Option<Vec<_>>>>()?
                else {
                    return Ok(None);
                };
                tree.array(elements)
            }
            ExpressionKind::ArrayConcatenation { elements } => {
                let mut values = Vec::with_capacity(elements.len());
                for element in elements.iter() {
                    let Some(value) =
                        self.inline_expression(tree, element.expression(), context)?
                    else {
                        return Ok(None);
                    };
                    values.push(match element {
                        FunctionalArrayElement::Value(_) => ArrayElement::Value(value),
                        FunctionalArrayElement::Spread(_) => ArrayElement::Spread(value),
                    });
                }
                tree.spread_array(values)
            }
            ExpressionKind::Record { fields } => {
                let mut properties = Vec::with_capacity(fields.len());
                for field in fields.iter() {
                    let Some(value) = self.inline_expression(tree, field.expression, context)?
                    else {
                        return Ok(None);
                    };
                    properties
                        .push(ObjectProperty::Field { name: field.field.name.clone(), value });
                }
                tree.object(properties)
            }
            ExpressionKind::Project { record, field } => {
                let Some(record) = self.inline_expression(tree, *record, context)? else {
                    return Ok(None);
                };
                tree.member(record, field.name.as_str())
            }
            ExpressionKind::Unary { operator, value } => {
                let Some(value) = self.inline_expression(tree, *value, context)? else {
                    return Ok(None);
                };
                unary_expression(tree, *operator, value)
            }
            ExpressionKind::Binary { operator, left, right } => {
                let Some(left) = self.inline_expression(tree, *left, context)? else {
                    return Ok(None);
                };
                let Some(right) = self.inline_expression(tree, *right, context)? else {
                    return Ok(None);
                };
                binary_expression(tree, self.runtime_namespace.as_ref(), *operator, left, right)
            }
            ExpressionKind::Index { value, index } => {
                let Some(value) = self.inline_expression(tree, *value, context)? else {
                    return Ok(None);
                };
                let Some(index) = self.inline_expression(tree, *index, context)? else {
                    return Ok(None);
                };
                tree.index(value, index)
            }
            ExpressionKind::Constructor { global } => self.global_expression(tree, global)?,
            ExpressionKind::Global { global } => self.global_expression(tree, global)?,
            ExpressionKind::Local { parameter } => {
                local_expression(self, tree, parameter, context)?
            }
            ExpressionKind::Abstraction { parameters, body } => {
                let Some(expression) =
                    self.inline_abstraction(tree, parameters, *body, false, None, context)?
                else {
                    return Ok(None);
                };
                expression
            }
            ExpressionKind::UncurriedAbstraction { parameters, body } => {
                let Some(expression) =
                    self.inline_abstraction(tree, parameters, *body, true, None, context)?
                else {
                    return Ok(None);
                };
                expression
            }
            ExpressionKind::Application { function, arguments, synthetic } => {
                let Some(function) = self.inline_expression(tree, *function, context)? else {
                    return Ok(None);
                };
                if *synthetic {
                    tree.clear_call_purity(&function);
                }
                let Some(arguments) = arguments
                    .iter()
                    .map(|argument| self.inline_expression(tree, *argument, context))
                    .collect::<ModuleResult<Option<Vec<_>>>>()?
                else {
                    return Ok(None);
                };
                curried_call_expression(tree, function, arguments, *synthetic)
            }
            ExpressionKind::UncurriedApplication { function, arguments, synthetic } => {
                let Some(function) = self.inline_expression(tree, *function, context)? else {
                    return Ok(None);
                };
                if *synthetic {
                    tree.clear_call_purity(&function);
                }
                let Some(arguments) = arguments
                    .iter()
                    .map(|argument| self.inline_expression(tree, *argument, context))
                    .collect::<ModuleResult<Option<Vec<_>>>>()?
                else {
                    return Ok(None);
                };
                if *synthetic {
                    tree.pure_call(function, arguments)
                } else {
                    tree.call(function, arguments)
                }
            }
            ExpressionKind::StyleX(stylex) => {
                let Some(expression) = self.inline_stylex_expression(tree, stylex, context)? else {
                    return Ok(None);
                };
                expression
            }
            ExpressionKind::SynthesizedEvidence { evidence } => {
                synthesized_evidence_expression(tree, evidence)
            }
            ExpressionKind::TrivialEvidence => tree.object(Vec::new()),
            ExpressionKind::IfThenElse { condition, then, else_ } => {
                let Some(condition) = self.inline_expression(tree, *condition, context)? else {
                    return Ok(None);
                };
                let Some(then) = self.inline_expression(tree, *then, context)? else {
                    return Ok(None);
                };
                let Some(else_) = self.inline_expression(tree, *else_, context)? else {
                    return Ok(None);
                };
                tree.conditional(condition, then, else_)
            }
            ExpressionKind::RecordUpdate { .. }
            | ExpressionKind::Case { .. }
            | ExpressionKind::Guarded { .. }
            | ExpressionKind::Let { .. }
            | ExpressionKind::LetPattern { .. }
            | ExpressionKind::Effect { .. }
            | ExpressionKind::Async(_) => return Ok(None),
        };
        Ok(Some(expression))
    }

    pub(super) fn try_inline_expression(
        &self,
        tree: &mut Tree,
        expression: FunctionalExpressionId,
        context: &mut FunctionContext,
    ) -> ModuleResult<Option<ExpressionId>> {
        if !self.expression_can_inline(expression) {
            return Ok(None);
        }
        let expression = self
            .inline_expression(tree, expression, context)?
            .expect("invariant violated: inline eligibility did not match expression rendering");
        Ok(Some(expression))
    }

    pub(super) fn expression_can_inline(&self, expression: FunctionalExpressionId) -> bool {
        match &self.module.storage[expression].kind {
            ExpressionKind::Literal { .. }
            | ExpressionKind::Constructor { .. }
            | ExpressionKind::Global { .. }
            | ExpressionKind::Local { .. }
            | ExpressionKind::SynthesizedEvidence { .. }
            | ExpressionKind::TrivialEvidence => true,
            ExpressionKind::Array { elements } => {
                elements.iter().all(|element| self.expression_can_inline(*element))
            }
            ExpressionKind::ArrayConcatenation { elements } => {
                elements.iter().all(|element| self.expression_can_inline(element.expression()))
            }
            ExpressionKind::Record { fields } => {
                fields.iter().all(|field| self.expression_can_inline(field.expression))
            }
            ExpressionKind::Project { record, .. }
            | ExpressionKind::Unary { value: record, .. } => self.expression_can_inline(*record),
            ExpressionKind::Binary { left, right, .. }
            | ExpressionKind::Index { value: left, index: right } => {
                self.expression_can_inline(*left) && self.expression_can_inline(*right)
            }
            ExpressionKind::Abstraction { parameters, body }
            | ExpressionKind::UncurriedAbstraction { parameters, body } => {
                parameters.iter().all(|pattern| self.pattern_can_inline(*pattern))
                    && self.expression_can_inline(*body)
            }
            ExpressionKind::Application { function, arguments, .. }
            | ExpressionKind::UncurriedApplication { function, arguments, .. } => {
                self.expression_can_inline(*function)
                    && arguments.iter().all(|argument| self.expression_can_inline(*argument))
            }
            ExpressionKind::StyleX(stylex) => stylex
                .try_for_each_child(|child| {
                    if self.expression_can_inline(child) { Ok(()) } else { Err(()) }
                })
                .is_ok(),
            ExpressionKind::IfThenElse { condition, then, else_ } => {
                self.expression_can_inline(*condition)
                    && self.expression_can_inline(*then)
                    && self.expression_can_inline(*else_)
            }
            ExpressionKind::RecordUpdate { .. }
            | ExpressionKind::Case { .. }
            | ExpressionKind::Guarded { .. }
            | ExpressionKind::Let { .. }
            | ExpressionKind::LetPattern { .. }
            | ExpressionKind::Effect { .. }
            | ExpressionKind::Async(_) => false,
        }
    }

    fn pattern_can_inline(&self, pattern: PatternId) -> bool {
        match &self.module.storage[pattern].kind {
            PatternKind::Variable(_) | PatternKind::Wildcard => true,
            PatternKind::Named { pattern, .. } => self.pattern_can_inline(*pattern),
            PatternKind::Literal(_)
            | PatternKind::Array(_)
            | PatternKind::Record(_)
            | PatternKind::Constructor { .. } => false,
        }
    }

    pub(super) fn inline_abstraction(
        &self,
        tree: &mut Tree,
        parameters: &[PatternId],
        body: FunctionalExpressionId,
        uncurried: bool,
        source_name: Option<NameIndex>,
        context: &mut FunctionContext,
    ) -> ModuleResult<Option<ExpressionId>> {
        let mut arguments = Vec::with_capacity(parameters.len());
        for pattern in parameters {
            let argument = self.allocate_pattern_argument(*pattern, context);
            if !self.bind_inline_pattern(*pattern, &argument, context) {
                return Ok(None);
            }
            arguments.push(argument);
        }
        let Some(mut body) = self.inline_expression(tree, body, context)? else {
            return Ok(None);
        };
        if uncurried {
            body = tree.arrow(source_name, arguments, body);
        } else if arguments.is_empty() {
            body = tree.arrow(source_name, Vec::new(), body);
        } else {
            let outermost = arguments.remove(0);
            for (pattern, argument) in parameters[1..].iter().zip(arguments).rev() {
                let _origin = tree.enter_origin(self.module.storage[*pattern].origin);
                body = tree.arrow(source_name, vec![argument], body);
            }
            body = tree.arrow(source_name, vec![outermost], body);
        }
        Ok(Some(body))
    }

    fn bind_inline_pattern(
        &self,
        pattern: PatternId,
        argument: &str,
        context: &mut FunctionContext,
    ) -> bool {
        match &self.module.storage[pattern].kind {
            PatternKind::Variable(parameter) => {
                context.bind_direct(parameter, SmolStr::new(argument));
                true
            }
            PatternKind::Named { parameter, pattern } => {
                context.bind_direct(parameter, SmolStr::new(argument));
                self.bind_inline_pattern(*pattern, argument, context)
            }
            PatternKind::Wildcard => true,
            PatternKind::Literal(_)
            | PatternKind::Array(_)
            | PatternKind::Record(_)
            | PatternKind::Constructor { .. } => false,
        }
    }
}
