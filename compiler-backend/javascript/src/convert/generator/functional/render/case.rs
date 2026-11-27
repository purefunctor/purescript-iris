use functional::tree::{
    CaseAlternative, ExpressionId as FunctionalExpressionId, ExpressionKind, Guard,
    GuardedAlternative,
};

use crate::error::ModuleResult;
use crate::tree::{ExpressionId, Tree};
use crate::writer::Writer;

use super::syntax::combine_conditions;
use super::{Destination, FunctionContext, FunctionRenderer, Generator, PatternPlan};

enum CaseScrutinee {
    Reusable(FunctionalExpressionId),
    Rendered(ExpressionId),
}

struct GuardSequence<'a, 'd> {
    guards: &'a [Guard],
    position: usize,
    expression: FunctionalExpressionId,
    destination: Destination<'d>,
}

pub(super) fn render_case(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    scrutinees: &[FunctionalExpressionId],
    alternatives: &[CaseAlternative],
    destination: Destination<'_>,
) -> ModuleResult<()> {
    let FunctionRenderer { generator, tree, writer, context } = renderer;
    let mut values = Vec::with_capacity(scrutinees.len());
    for &scrutinee in scrutinees {
        if generator.functional_expression_is_reusable(scrutinee, context) {
            values.push(CaseScrutinee::Reusable(scrutinee));
            continue;
        }
        let value = generator.rendered_expression(tree, writer, scrutinee, context)?;
        let value = generator.materialize_pattern_value(tree, writer, scrutinee, value, context);
        values.push(CaseScrutinee::Rendered(value));
    }
    let scrutinees = values;
    match destination {
        Destination::Return
        | Destination::TailEffectThunkReturn
        | Destination::TailEffectReturn
        | Destination::EffectReturn
        | Destination::EffectTailEffectReturn
        | Destination::AssignAndBreak { .. }
        | Destination::EffectAssignAndBreak { .. }
        | Destination::DiscardAndBreak { .. }
        | Destination::EffectDiscardAndBreak { .. }
        | Destination::Continue
        | Destination::EffectContinue => generator.render_case_alternatives(
            tree,
            writer,
            &scrutinees,
            alternatives,
            destination,
            context,
        ),
        Destination::Assign(_)
        | Destination::EffectAssign(_)
        | Destination::Discard
        | Destination::EffectDiscard => {
            let label = context.allocate("$case");
            writer.labeled_block(&label, |writer| {
                generator.render_case_alternatives(
                    tree,
                    writer,
                    &scrutinees,
                    alternatives,
                    destination.and_break(&label),
                    context,
                )
            })
        }
    }
}

impl Generator<'_> {
    fn render_case_alternatives<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        scrutinees: &[CaseScrutinee],
        alternatives: &[CaseAlternative],
        destination: Destination<'_>,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        for alternative in alternatives {
            let _origin = tree.enter_origin(alternative.origin);
            let mut plan = PatternPlan::default();
            for (pattern, scrutinee) in alternative.patterns.iter().zip(scrutinees) {
                let value = match scrutinee {
                    CaseScrutinee::Reusable(expression) => self
                        .inline_expression(tree, *expression, context)?
                        .expect("invariant violated: a reusable scrutinee cannot be inlined"),
                    CaseScrutinee::Rendered(value) => tree.duplicate(value),
                };
                self.extend_pattern_plan(tree, *pattern, value, None, context, &mut plan)?;
            }
            let condition = combine_conditions(tree, std::mem::take(&mut plan.conditions));
            if let Some(condition) = condition {
                writer.if_block(tree, condition, |tree, writer| {
                    self.render_pattern_bindings(tree, writer, plan.bindings);
                    self.render_case_alternative_expression(
                        tree,
                        writer,
                        alternative.expression,
                        destination,
                        context,
                    )
                })?;
            } else if matches!(
                self.module.storage[alternative.expression].kind,
                ExpressionKind::Guarded { .. }
            ) {
                writer.block(|writer| {
                    self.render_pattern_bindings(tree, writer, plan.bindings);
                    self.render_case_alternative_expression(
                        tree,
                        writer,
                        alternative.expression,
                        destination,
                        context,
                    )
                })?;
            } else {
                self.render_pattern_bindings(tree, writer, plan.bindings);
                self.render_expression(tree, writer, alternative.expression, destination, context)?;
                return Ok(());
            }
        }
        self.render_pattern_failure(writer);
        Ok(())
    }

    fn render_case_alternative_expression<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        expression: FunctionalExpressionId,
        destination: Destination<'_>,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        if let ExpressionKind::Guarded { alternatives } = &self.module.storage[expression].kind {
            self.render_guard_alternatives(tree, writer, alternatives, destination, context)
        } else {
            self.render_expression(tree, writer, expression, destination, context)
        }
    }
}

pub(super) fn render_guarded(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    alternatives: &[GuardedAlternative],
    destination: Destination<'_>,
) -> ModuleResult<()> {
    let FunctionRenderer { generator, tree, writer, context } = renderer;
    match destination {
        Destination::Return
        | Destination::TailEffectThunkReturn
        | Destination::TailEffectReturn
        | Destination::EffectReturn
        | Destination::EffectTailEffectReturn
        | Destination::AssignAndBreak { .. }
        | Destination::EffectAssignAndBreak { .. }
        | Destination::DiscardAndBreak { .. }
        | Destination::EffectDiscardAndBreak { .. }
        | Destination::Continue
        | Destination::EffectContinue => {
            generator.render_guard_alternatives(
                tree,
                writer,
                alternatives,
                destination,
                context,
            )?;
            generator.render_pattern_failure(writer);
            Ok(())
        }
        Destination::Assign(_)
        | Destination::EffectAssign(_)
        | Destination::Discard
        | Destination::EffectDiscard => {
            let label = context.allocate("$guard");
            writer.labeled_block(&label, |writer| {
                generator.render_guard_alternatives(
                    tree,
                    writer,
                    alternatives,
                    destination.and_break(&label),
                    context,
                )?;
                generator.render_pattern_failure(writer);
                Ok(())
            })
        }
    }
}

impl Generator<'_> {
    fn render_guard_alternatives<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        alternatives: &[GuardedAlternative],
        destination: Destination<'_>,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        for alternative in alternatives {
            self.render_guards(
                tree,
                writer,
                GuardSequence {
                    guards: &alternative.guards,
                    position: 0,
                    expression: alternative.expression,
                    destination,
                },
                context,
            )?;
        }
        Ok(())
    }

    fn render_guards<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        sequence: GuardSequence<'_, '_>,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        let GuardSequence { guards, position, expression, destination } = sequence;
        let Some(guard) = guards.get(position) else {
            return self.render_expression(tree, writer, expression, destination, context);
        };
        match guard {
            Guard::Boolean(condition) => {
                let _origin = tree.enter_origin(self.module.storage[*condition].origin);
                let condition = self.expression_value(tree, writer, *condition, context)?;
                writer.if_block(tree, condition, |tree, writer| {
                    self.render_guards(
                        tree,
                        writer,
                        GuardSequence { guards, position: position + 1, expression, destination },
                        context,
                    )
                })
            }
            Guard::Pattern { expression: value, pattern } => {
                let source = *value;
                let value = {
                    let _origin = tree.enter_origin(self.module.storage[source].origin);
                    let value = self.rendered_expression(tree, writer, source, context)?;
                    self.materialize_pattern_value(tree, writer, source, value, context)
                };
                let _origin = tree.enter_origin(self.module.storage[*pattern].origin);
                let mut plan = self.pattern_plan(tree, *pattern, value, None, context)?;
                let condition = combine_conditions(tree, std::mem::take(&mut plan.conditions));
                if let Some(condition) = condition {
                    writer.if_block(tree, condition, |tree, writer| {
                        self.render_pattern_bindings(tree, writer, plan.bindings);
                        self.render_guards(
                            tree,
                            writer,
                            GuardSequence {
                                guards,
                                position: position + 1,
                                expression,
                                destination,
                            },
                            context,
                        )
                    })
                } else {
                    self.render_pattern_bindings(tree, writer, plan.bindings);
                    self.render_guards(
                        tree,
                        writer,
                        GuardSequence { guards, position: position + 1, expression, destination },
                        context,
                    )
                }
            }
        }
    }
}
