use functional::tree::{
    ExpressionId as FunctionalExpressionId, ExpressionKind, Parameter, PatternId, PatternKind,
};
use smol_str::SmolStr;

use crate::error::ModuleResult;
use crate::tree::{BinaryOperator, ExpressionId, Tree};
use crate::writer::Writer;

use super::inline::pattern_parameter;
use super::syntax::{combine_conditions, literal_expression};
use super::{FunctionContext, Generator, PatternBinding, PatternPlan, RenderedExpression};

impl Generator<'_> {
    pub(super) fn render_pattern_scope<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        mut plan: PatternPlan,
        context: &mut FunctionContext,
        render: impl FnOnce(&mut Tree<'t>, &mut Writer<'t>, &mut FunctionContext) -> ModuleResult<()>,
    ) -> ModuleResult<()> {
        let condition = combine_conditions(tree, std::mem::take(&mut plan.conditions));
        if let Some(condition) = condition {
            writer.if_else(
                tree,
                condition,
                |tree, writer| {
                    self.render_pattern_bindings(tree, writer, plan.bindings);
                    render(tree, writer, context)
                },
                |_, writer| {
                    self.render_pattern_failure(writer);
                    Ok(())
                },
            )
        } else {
            self.render_pattern_bindings(tree, writer, plan.bindings);
            render(tree, writer, context)
        }
    }

    pub(super) fn render_pattern_bindings<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        bindings: Vec<PatternBinding>,
    ) {
        for binding in bindings {
            match binding {
                PatternBinding::Variable { name, value } => {
                    writer.constant(tree, &name, value, false);
                }
                PatternBinding::Constructor { names, value } => {
                    writer.constant_object_pattern(tree, &names, value);
                }
            }
        }
    }

    pub(super) fn render_pattern_failure(&self, writer: &mut Writer<'_>) {
        writer.throw_error("Pattern match failure");
    }

    pub(super) fn pattern_plan(
        &self,
        tree: &mut Tree,
        pattern: PatternId,
        value: ExpressionId,
        root_name: Option<&str>,
        context: &mut FunctionContext,
    ) -> ModuleResult<PatternPlan> {
        let mut plan = PatternPlan::default();
        self.extend_pattern_plan(tree, pattern, value, root_name, context, &mut plan)?;
        Ok(plan)
    }

    pub(super) fn extend_pattern_plan(
        &self,
        tree: &mut Tree,
        pattern: PatternId,
        value: ExpressionId,
        root_name: Option<&str>,
        context: &mut FunctionContext,
        plan: &mut PatternPlan,
    ) -> ModuleResult<()> {
        match &self.module.storage[pattern].kind {
            PatternKind::Variable(parameter) => {
                self.bind_pattern_parameter(parameter, value, root_name, context, plan);
            }
            PatternKind::Named { parameter, pattern } => {
                let binding = tree.duplicate(&value);
                self.bind_pattern_parameter(parameter, binding, root_name, context, plan);
                self.extend_pattern_plan(tree, *pattern, value, root_name, context, plan)?;
            }
            PatternKind::Wildcard => {}
            PatternKind::Literal(literal) => {
                let literal = literal_expression(tree, literal, self.module.file_id)?;
                plan.conditions.push(tree.binary(BinaryOperator::StrictEqual, value, literal));
            }
            PatternKind::Array(patterns) => {
                let array = tree.identifier("Array");
                let is_array = tree.member(array, "isArray");
                let argument = tree.duplicate(&value);
                let is_array = tree.call(is_array, vec![argument]);
                plan.conditions.push(is_array);
                let source = tree.duplicate(&value);
                let length = tree.member(source, "length");
                let expected = tree.number(patterns.len().to_string());
                plan.conditions.push(tree.binary(BinaryOperator::StrictEqual, length, expected));
                for (index, pattern) in patterns.iter().enumerate() {
                    let index = tree.number(index.to_string());
                    let source = tree.duplicate(&value);
                    let element = tree.index(source, index);
                    self.extend_pattern_plan(tree, *pattern, element, None, context, plan)?;
                }
            }
            PatternKind::Record(fields) => {
                for field in fields.iter() {
                    let source = tree.duplicate(&value);
                    let field_value = tree.member(source, field.field.name.as_str());
                    self.extend_pattern_plan(
                        tree,
                        field.pattern,
                        field_value,
                        None,
                        context,
                        plan,
                    )?;
                }
            }
            PatternKind::Constructor { global, arguments } => {
                let expected = tree.string(global.item_name.as_str());
                if arguments.is_empty() {
                    let source = tree.duplicate(&value);
                    plan.conditions.push(tree.binary(
                        BinaryOperator::StrictEqual,
                        source,
                        expected,
                    ));
                } else {
                    let source = tree.duplicate(&value);
                    let tag = tree.member(source, "tag");
                    plan.conditions.push(tree.binary(BinaryOperator::StrictEqual, tag, expected));
                }

                let mut argument_names = Vec::with_capacity(arguments.len());
                for pattern in arguments.iter() {
                    let name = pattern_parameter(&self.module.storage, *pattern)
                        .map(|parameter| context.allocate(&parameter.name));
                    argument_names.push(name);
                }
                let binding_position = plan.bindings.len();

                for (index, (pattern, name)) in
                    arguments.iter().zip(argument_names.iter()).enumerate()
                {
                    let field = format!("_{}", index + 1);
                    let source = tree.duplicate(&value);
                    let argument = tree.member(source, field);
                    self.extend_pattern_plan(
                        tree,
                        *pattern,
                        argument,
                        name.as_deref(),
                        context,
                        plan,
                    )?;
                }
                if argument_names.iter().any(Option::is_some) {
                    plan.bindings.insert(
                        binding_position,
                        PatternBinding::Constructor { names: argument_names, value },
                    );
                }
            }
        }
        Ok(())
    }

    fn bind_pattern_parameter(
        &self,
        parameter: &Parameter,
        value: ExpressionId,
        root_name: Option<&str>,
        context: &mut FunctionContext,
        plan: &mut PatternPlan,
    ) {
        if let Some(root_name) = root_name {
            context.bind_direct(parameter, SmolStr::new(root_name));
        } else {
            let name = context.allocate(&parameter.name);
            context.bind_direct(parameter, name.clone());
            plan.bindings.push(PatternBinding::Variable { name, value });
        }
    }

    pub(super) fn allocate_pattern_argument(
        &self,
        pattern: PatternId,
        context: &mut FunctionContext,
    ) -> SmolStr {
        let preferred = pattern_parameter(&self.module.storage, pattern)
            .map(|parameter| parameter.name.as_str())
            .unwrap_or("$argument");
        context.allocate(preferred)
    }

    pub(super) fn materialize_pattern_value<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        source: FunctionalExpressionId,
        value: RenderedExpression,
        context: &mut FunctionContext,
    ) -> ExpressionId {
        if !value.pending_evaluation
            || matches!(
                self.module.storage[source].kind,
                ExpressionKind::Literal { .. }
                    | ExpressionKind::Constructor { .. }
                    | ExpressionKind::Global { .. }
                    | ExpressionKind::Local { .. }
            )
        {
            return value.value;
        }
        let name = context.allocate("$scrutinee");
        writer.constant(tree, &name, value.value, false);
        tree.identifier(name)
    }
}
