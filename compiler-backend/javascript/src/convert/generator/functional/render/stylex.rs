//! JavaScript rendering for semantic StyleX expressions.

use functional::optimize::for_each_expression_child;
use functional::stylex::{StyleXCallTarget, StyleXConditionalCase, StyleXExpression};
use functional::tree::{
    DeclarationKind, ExpressionKind, Global, GlobalId, InstanceIdentity, Module,
};
use rustc_hash::FxHashSet;

use crate::error::ModuleResult;
use crate::tree::{BinaryOperator, ExpressionId, ObjectProperty, Tree};
use crate::writer::Writer;

use super::{Destination, FunctionContext, Generator, RenderedExpression};

pub(super) fn collect_stylex_references(module: &Module) -> Vec<Global> {
    let mut expressions = Vec::new();
    for declaration in module.declarations.iter() {
        if let DeclarationKind::Value(expression) = declaration.kind {
            expressions.push((expression, false));
        }
    }
    let mut visited = FxHashSet::default();
    let mut globals = FxHashSet::default();
    let mut references = Vec::new();
    while let Some((expression, static_context)) = expressions.pop() {
        if !visited.insert((expression, static_context)) {
            continue;
        }
        let kind = &module.storage[expression].kind;
        let static_context = static_context || matches!(kind, ExpressionKind::StyleX(_));
        if static_context
            && let ExpressionKind::Constructor { global } | ExpressionKind::Global { global } = kind
            && global_file(global.id) != module.file_id
            && globals.insert(global.id)
        {
            references.push(Global::clone(global));
        }
        for_each_expression_child(kind, |expression| {
            expressions.push((expression, static_context));
        });
    }
    references
}

fn global_file(id: GlobalId) -> files::FileId {
    match id {
        GlobalId::Term(file_id, _) | GlobalId::Generated(file_id, _) => file_id,
        GlobalId::Instance(
            InstanceIdentity::Declared(file_id, _) | InstanceIdentity::Derived(file_id, _),
        ) => file_id,
    }
}

impl Generator<'_> {
    pub(super) fn render_stylex_expression<'a, 't>(
        &self,
        tree: &'a mut Tree<'t>,
        writer: &'a mut Writer<'t>,
        stylex: &StyleXExpression,
        context: &'a mut FunctionContext,
    ) -> ModuleResult<RenderedExpression> {
        let value = match stylex {
            StyleXExpression::Call { target, arguments } => {
                let function = self.stylex_function(tree, *target);
                let mut values = Vec::with_capacity(arguments.len());
                for argument in arguments.iter() {
                    let argument = self.rendered_expression(tree, writer, *argument, context)?;
                    values.push(argument.value);
                }
                tree.call(function, values)
            }
            StyleXExpression::Conditional { condition, style } => {
                let condition = self.rendered_expression(tree, writer, *condition, context)?;
                let style = if let Some(style) =
                    self.try_inline_expression(tree, *style, context)?
                {
                    style
                } else {
                    let name = context.allocate("$stylexConditional");
                    let outer_tail_calls = context.tail_calls.take();
                    let result = writer.constant_arrow(&name, vec![], |writer| {
                        self.render_expression(tree, writer, *style, Destination::Return, context)
                    });
                    context.tail_calls = outer_tail_calls;
                    result?;
                    let function = tree.identifier(name);
                    tree.call(function, vec![])
                };
                tree.binary(BinaryOperator::LogicalAnd, condition.value, style)
            }
            StyleXExpression::ConditionalCase(_) => {
                unreachable!("invariant violated: escaped StyleX conditional case")
            }
            StyleXExpression::ConditionalValue { default, cases } => {
                self.render_stylex_conditional_value(tree, writer, *default, cases, context)?
            }
        };
        Ok(RenderedExpression { value, pending_evaluation: true })
    }

    pub(super) fn inline_stylex_expression(
        &self,
        tree: &mut Tree<'_>,
        stylex: &StyleXExpression,
        context: &mut FunctionContext,
    ) -> ModuleResult<Option<ExpressionId>> {
        let value = match stylex {
            StyleXExpression::Call { target, arguments } => {
                let function = self.stylex_function(tree, *target);
                let Some(arguments) = arguments
                    .iter()
                    .map(|argument| self.inline_expression(tree, *argument, context))
                    .collect::<ModuleResult<Option<Vec<_>>>>()?
                else {
                    return Ok(None);
                };
                tree.call(function, arguments)
            }
            StyleXExpression::Conditional { condition, style } => {
                let Some(condition) = self.inline_expression(tree, *condition, context)? else {
                    return Ok(None);
                };
                let Some(style) = self.inline_expression(tree, *style, context)? else {
                    return Ok(None);
                };
                tree.binary(BinaryOperator::LogicalAnd, condition, style)
            }
            StyleXExpression::ConditionalCase(_) => {
                unreachable!("invariant violated: escaped StyleX conditional case")
            }
            StyleXExpression::ConditionalValue { default, cases } => {
                let Some(default) = self.inline_expression(tree, *default, context)? else {
                    return Ok(None);
                };
                let mut properties =
                    vec![ObjectProperty::Field { name: "default".to_owned(), value: default }];
                for case in cases.iter() {
                    let Some((key, value)) = self.inline_stylex_case(tree, case, context)? else {
                        return Ok(None);
                    };
                    properties.push(ObjectProperty::Computed { key, value });
                }
                tree.object(properties)
            }
        };
        Ok(Some(value))
    }

    fn stylex_function(&self, tree: &mut Tree<'_>, target: StyleXCallTarget) -> ExpressionId {
        let namespace = self
            .stylex_namespace
            .as_ref()
            .expect("invariant violated: StyleX expression has no module namespace");
        let mut namespace = tree.identifier(namespace);
        if matches!(target, StyleXCallTarget::Types(_)) {
            namespace = tree.member(namespace, "types");
        }
        tree.member(namespace, target.name())
    }

    fn stylex_when_function(
        &self,
        tree: &mut Tree<'_>,
        case: &StyleXConditionalCase,
    ) -> ExpressionId {
        let namespace = self
            .stylex_namespace
            .as_ref()
            .expect("invariant violated: StyleX expression has no module namespace");
        let namespace = tree.identifier(namespace);
        let namespace = tree.member(namespace, "when");
        tree.member(namespace, case.relation.name())
    }

    fn render_stylex_conditional_value<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        default: functional::tree::ExpressionId,
        cases: &[StyleXConditionalCase],
        context: &mut FunctionContext,
    ) -> ModuleResult<ExpressionId> {
        let default = self.rendered_expression(tree, writer, default, context)?;
        let mut properties =
            vec![ObjectProperty::Field { name: "default".to_owned(), value: default.value }];
        for case in cases {
            let function = self.stylex_when_function(tree, case);
            let selector = self.rendered_expression(tree, writer, case.selector, context)?;
            let mut arguments = vec![selector.value];
            if let Some(marker) = case.marker {
                let marker = self.rendered_expression(tree, writer, marker, context)?;
                arguments.push(marker.value);
            }
            let key = tree.call(function, arguments);
            let value = self.rendered_expression(tree, writer, case.value, context)?;
            properties.push(ObjectProperty::Computed { key, value: value.value });
        }
        Ok(tree.object(properties))
    }

    fn inline_stylex_case(
        &self,
        tree: &mut Tree<'_>,
        case: &StyleXConditionalCase,
        context: &mut FunctionContext,
    ) -> ModuleResult<Option<(ExpressionId, ExpressionId)>> {
        let function = self.stylex_when_function(tree, case);
        let Some(selector) = self.inline_expression(tree, case.selector, context)? else {
            return Ok(None);
        };
        let mut arguments = vec![selector];
        if let Some(marker) = case.marker {
            let Some(marker) = self.inline_expression(tree, marker, context)? else {
                return Ok(None);
            };
            arguments.push(marker);
        }
        let key = tree.call(function, arguments);
        let Some(value) = self.inline_expression(tree, case.value, context)? else {
            return Ok(None);
        };
        Ok(Some((key, value)))
    }
}
