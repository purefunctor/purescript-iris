use functional::tree::{ExpressionId as FunctionalExpressionId, ExpressionKind, PatternId};
use itertools::Itertools;
use smol_str::SmolStr;

use crate::error::ModuleResult;
use crate::program_text::NameIndex;
use crate::tree::{ExpressionId, Tree};
use crate::writer::Writer;

use super::{AbstractionBinding, Destination, FunctionContext, FunctionRenderer, Generator};

struct CurriedParameter<'a> {
    pattern: PatternId,
    argument: &'a str,
    remaining: &'a [PatternId],
    body: FunctionalExpressionId,
}

struct UncurriedParameters<'a> {
    patterns: &'a [PatternId],
    arguments: &'a [SmolStr],
    position: usize,
    body: FunctionalExpressionId,
}

#[derive(Clone, Copy)]
struct Abstraction<'a> {
    parameters: &'a [PatternId],
    body: FunctionalExpressionId,
    uncurried: bool,
}

pub(super) fn render_named_function(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    name: &str,
    source_name: Option<NameIndex>,
    expression: FunctionalExpressionId,
    exported: bool,
) -> ModuleResult<()> {
    let FunctionRenderer { generator, tree, writer, context } = renderer;
    match &generator.module.storage[expression].kind {
        ExpressionKind::Abstraction { parameters, body } => {
            let (argument, parameter) = generator.first_argument(parameters, context);
            writer.function(name, source_name, vec![argument.clone()], exported, |writer| {
                if let Some(parameter) = parameter {
                    generator.render_curried_parameter(
                        tree,
                        writer,
                        CurriedParameter {
                            pattern: parameter,
                            argument: &argument,
                            remaining: &parameters[1..],
                            body: *body,
                        },
                        context,
                    )
                } else {
                    generator.render_expression(tree, writer, *body, Destination::Return, context)
                }
            })
        }
        ExpressionKind::UncurriedAbstraction { parameters, body } => {
            let arguments = parameters
                .iter()
                .map(|pattern| generator.allocate_pattern_argument(*pattern, context))
                .collect_vec();
            writer.function(name, source_name, arguments.clone(), exported, |writer| {
                generator.render_uncurried_parameters(
                    tree,
                    writer,
                    UncurriedParameters {
                        patterns: parameters,
                        arguments: &arguments,
                        position: 0,
                        body: *body,
                    },
                    context,
                )
            })
        }
        _ => unreachable!("invariant violated: named JavaScript function is not an abstraction"),
    }
}

impl Generator<'_> {
    fn first_argument(
        &self,
        parameters: &[PatternId],
        context: &mut FunctionContext,
    ) -> (SmolStr, Option<PatternId>) {
        match parameters.first().copied() {
            Some(pattern) => (self.allocate_pattern_argument(pattern, context), Some(pattern)),
            None => (SmolStr::new_static(""), None),
        }
    }

    fn render_curried_parameter<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        parameter: CurriedParameter<'_>,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        let CurriedParameter { pattern, argument, remaining, body } = parameter;
        let _origin = tree.enter_origin(self.module.storage[pattern].origin);
        let value = tree.identifier(argument);
        let plan = self.pattern_plan(tree, pattern, value, Some(argument), context)?;
        self.render_pattern_scope(tree, writer, plan, context, |tree, writer, context| {
            let Some((pattern, remaining)) = remaining.split_first() else {
                return self.render_expression(tree, writer, body, Destination::Return, context);
            };
            let _origin = tree.enter_origin(self.module.storage[*pattern].origin);
            let argument = self.allocate_pattern_argument(*pattern, context);
            writer.return_arrow(vec![argument.clone()], |writer| {
                self.render_curried_parameter(
                    tree,
                    writer,
                    CurriedParameter { pattern: *pattern, argument: &argument, remaining, body },
                    context,
                )
            })
        })
    }

    fn render_uncurried_parameters<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        parameters: UncurriedParameters<'_>,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        let UncurriedParameters { patterns, arguments, position, body } = parameters;
        let Some(pattern) = patterns.get(position).copied() else {
            return self.render_expression(tree, writer, body, Destination::Return, context);
        };
        let _origin = tree.enter_origin(self.module.storage[pattern].origin);
        let argument = &arguments[position];
        let value = tree.identifier(argument);
        let plan = self.pattern_plan(tree, pattern, value, Some(argument), context)?;
        self.render_pattern_scope(tree, writer, plan, context, |tree, writer, context| {
            self.render_uncurried_parameters(
                tree,
                writer,
                UncurriedParameters { patterns, arguments, position: position + 1, body },
                context,
            )
        })
    }

    pub(super) fn render_returned_function<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        expression: FunctionalExpressionId,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        let source_name = writer.function_name();
        if self.expression_can_inline(expression) {
            let function = self.inline_function(tree, expression, source_name, context)?;
            writer.return_function(tree, function);
            return Ok(());
        }
        let closure = self.render_closure(tree, writer, expression, source_name, context)?;
        writer.return_expression(tree, closure);
        Ok(())
    }

    fn inline_function(
        &self,
        tree: &mut Tree,
        expression: FunctionalExpressionId,
        source_name: Option<NameIndex>,
        context: &mut FunctionContext,
    ) -> ModuleResult<ExpressionId> {
        let _origin = tree.enter_origin(self.module.storage[expression].origin);
        let function = match &self.module.storage[expression].kind {
            ExpressionKind::Abstraction { parameters, body } => {
                self.inline_abstraction(tree, parameters, *body, false, source_name, context)?
            }
            ExpressionKind::UncurriedAbstraction { parameters, body } => {
                self.inline_abstraction(tree, parameters, *body, true, source_name, context)?
            }
            _ => unreachable!("invariant violated: inline function is not an abstraction"),
        };
        let function = function
            .expect("invariant violated: inline eligibility did not match expression rendering");
        Ok(function)
    }

    pub(super) fn render_closure<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        expression: FunctionalExpressionId,
        source_name: Option<NameIndex>,
        context: &mut FunctionContext,
    ) -> ModuleResult<ExpressionId> {
        let abstraction = match &self.module.storage[expression].kind {
            ExpressionKind::Abstraction { parameters, body } => {
                Abstraction { parameters, body: *body, uncurried: false }
            }
            ExpressionKind::UncurriedAbstraction { parameters, body } => {
                Abstraction { parameters, body: *body, uncurried: true }
            }
            _ => unreachable!("invariant violated: closure is not an abstraction"),
        };
        let outer_tail_calls = context.tail_calls.take();
        let arguments = self.abstraction_arguments(&abstraction, context);
        let (closure, result) =
            writer.arrow(tree, source_name, arguments.clone(), |tree, writer| {
                self.render_abstraction_body(tree, writer, &abstraction, &arguments, context)
            });
        context.tail_calls = outer_tail_calls;
        result?;
        Ok(closure)
    }

    pub(super) fn render_abstraction_binding<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        binding: AbstractionBinding<'_>,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        let AbstractionBinding { name, source_name, parameters, body, uncurried } = binding;
        let abstraction = Abstraction { parameters, body, uncurried };
        // A recursive call in a nested closure starts a distinct invocation; it cannot continue
        // the loop whose current iteration created that closure.
        let outer_tail_calls = context.tail_calls.take();
        let arguments = self.abstraction_arguments(&abstraction, context);
        let source_name = source_name.map(|name| writer.register_name(name));
        let result = writer.constant_arrow(name, source_name, arguments.clone(), |writer| {
            self.render_abstraction_body(tree, writer, &abstraction, &arguments, context)
        });
        context.tail_calls = outer_tail_calls;
        result
    }

    fn abstraction_arguments(
        &self,
        abstraction: &Abstraction<'_>,
        context: &mut FunctionContext,
    ) -> Vec<SmolStr> {
        let parameters = if abstraction.uncurried {
            abstraction.parameters
        } else {
            &abstraction.parameters[..abstraction.parameters.len().min(1)]
        };
        parameters.iter().map(|pattern| self.allocate_pattern_argument(*pattern, context)).collect()
    }

    fn render_abstraction_body<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        abstraction: &Abstraction<'_>,
        arguments: &[SmolStr],
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        let Abstraction { parameters, body, uncurried } = *abstraction;
        if uncurried {
            let parameters =
                UncurriedParameters { patterns: parameters, arguments, position: 0, body };
            return self.render_uncurried_parameters(tree, writer, parameters, context);
        }
        let (Some(pattern), Some(argument)) = (parameters.first(), arguments.first()) else {
            return self.render_expression(tree, writer, body, Destination::Return, context);
        };
        let parameter =
            CurriedParameter { pattern: *pattern, argument, remaining: &parameters[1..], body };
        self.render_curried_parameter(tree, writer, parameter, context)
    }
}
