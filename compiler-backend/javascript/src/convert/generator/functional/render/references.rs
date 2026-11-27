use functional::optimize::for_each_expression_child;
use functional::tree::{
    EffectExpression, ExpressionId as FunctionalExpressionId, ExpressionKind, Global, GlobalId,
    Guard, GuardedAlternative, Module as FunctionalModule, PatternId, PatternKind, RecordUpdate,
};
use rustc_hash::FxHashSet;

use super::inline::is_abstraction;

pub(super) fn collect_expression_references(
    module: &FunctionalModule,
    expression: FunctionalExpressionId,
    seen: &mut FxHashSet<GlobalId>,
    globals: &mut Vec<Global>,
) {
    match &module.storage[expression].kind {
        ExpressionKind::Async(_) => {
            unreachable!("invariant violated: Async program was not lowered to steps")
        }
        ExpressionKind::Literal { .. }
        | ExpressionKind::Local { .. }
        | ExpressionKind::SynthesizedEvidence { .. }
        | ExpressionKind::TrivialEvidence => {}
        ExpressionKind::Constructor { global } | ExpressionKind::Global { global } => {
            if seen.insert(global.id) {
                globals.push(global.clone());
            }
        }
        ExpressionKind::Array { elements } => {
            for expression in elements.iter() {
                collect_expression_references(module, *expression, seen, globals);
            }
        }
        ExpressionKind::ArrayConcatenation { elements } => {
            for element in elements.iter() {
                collect_expression_references(module, element.expression(), seen, globals);
            }
        }
        ExpressionKind::Record { fields } => {
            for field in fields.iter() {
                collect_expression_references(module, field.expression, seen, globals);
            }
        }
        ExpressionKind::RecordUpdate { record, updates } => {
            collect_expression_references(module, *record, seen, globals);
            collect_update_references(module, updates, seen, globals);
        }
        ExpressionKind::Project { record, .. } | ExpressionKind::Unary { value: record, .. } => {
            collect_expression_references(module, *record, seen, globals);
        }
        ExpressionKind::Binary { left, right, .. }
        | ExpressionKind::Index { value: left, index: right } => {
            collect_expression_references(module, *left, seen, globals);
            collect_expression_references(module, *right, seen, globals);
        }
        ExpressionKind::Abstraction { body, .. }
        | ExpressionKind::UncurriedAbstraction { body, .. } => {
            collect_expression_references(module, *body, seen, globals);
        }
        ExpressionKind::Application { function, arguments, .. }
        | ExpressionKind::UncurriedApplication { function, arguments, .. } => {
            collect_expression_references(module, *function, seen, globals);
            for argument in arguments.iter() {
                collect_expression_references(module, *argument, seen, globals);
            }
        }
        kind @ (ExpressionKind::StyleX(_) | ExpressionKind::Effect { .. }) => {
            for_each_expression_child(kind, |child| {
                collect_expression_references(module, child, seen, globals);
            });
        }
        ExpressionKind::IfThenElse { condition, then, else_ } => {
            collect_expression_references(module, *condition, seen, globals);
            collect_expression_references(module, *then, seen, globals);
            collect_expression_references(module, *else_, seen, globals);
        }
        ExpressionKind::Case { scrutinees, alternatives } => {
            for scrutinee in scrutinees.iter() {
                collect_expression_references(module, *scrutinee, seen, globals);
            }
            for alternative in alternatives.iter() {
                for pattern in alternative.patterns.iter() {
                    collect_pattern_references(module, *pattern, seen, globals);
                }
                collect_expression_references(module, alternative.expression, seen, globals);
            }
        }
        ExpressionKind::Guarded { alternatives } => {
            collect_guarded_references(module, alternatives, seen, globals);
        }
        ExpressionKind::Let { bindings, body, .. } => {
            for binding in bindings.iter() {
                collect_expression_references(module, binding.expression, seen, globals);
            }
            collect_expression_references(module, *body, seen, globals);
        }
        ExpressionKind::LetPattern { pattern, value, body } => {
            collect_pattern_references(module, *pattern, seen, globals);
            collect_expression_references(module, *value, seen, globals);
            collect_expression_references(module, *body, seen, globals);
        }
    }
}

fn collect_update_references(
    module: &FunctionalModule,
    updates: &[RecordUpdate],
    seen: &mut FxHashSet<GlobalId>,
    globals: &mut Vec<Global>,
) {
    for update in updates {
        match update {
            RecordUpdate::Leaf { expression, .. } => {
                collect_expression_references(module, *expression, seen, globals);
            }
            RecordUpdate::Branch { updates, .. } => {
                collect_update_references(module, updates, seen, globals);
            }
        }
    }
}

fn collect_pattern_references(
    module: &FunctionalModule,
    pattern: PatternId,
    seen: &mut FxHashSet<GlobalId>,
    globals: &mut Vec<Global>,
) {
    match &module.storage[pattern].kind {
        PatternKind::Variable(_) | PatternKind::Wildcard | PatternKind::Literal(_) => {}
        PatternKind::Named { pattern, .. } => {
            collect_pattern_references(module, *pattern, seen, globals);
        }
        PatternKind::Array(patterns) => {
            for pattern in patterns.iter() {
                collect_pattern_references(module, *pattern, seen, globals);
            }
        }
        PatternKind::Record(fields) => {
            for field in fields.iter() {
                collect_pattern_references(module, field.pattern, seen, globals);
            }
        }
        PatternKind::Constructor { global, arguments } => {
            if seen.insert(global.id) {
                globals.push(global.clone());
            }
            for pattern in arguments.iter() {
                collect_pattern_references(module, *pattern, seen, globals);
            }
        }
    }
}

fn collect_guarded_references(
    module: &FunctionalModule,
    alternatives: &[GuardedAlternative],
    seen: &mut FxHashSet<GlobalId>,
    globals: &mut Vec<Global>,
) {
    for alternative in alternatives {
        for guard in alternative.guards.iter() {
            match guard {
                Guard::Boolean(expression) => {
                    collect_expression_references(module, *expression, seen, globals);
                }
                Guard::Pattern { expression, pattern } => {
                    collect_expression_references(module, *expression, seen, globals);
                    collect_pattern_references(module, *pattern, seen, globals);
                }
            }
        }
        collect_expression_references(module, alternative.expression, seen, globals);
    }
}

pub(super) fn collect_expression_globals(
    module: &FunctionalModule,
    expression: FunctionalExpressionId,
    descend_abstractions: bool,
    globals: &mut FxHashSet<GlobalId>,
) {
    match &module.storage[expression].kind {
        ExpressionKind::Global { global } | ExpressionKind::Constructor { global } => {
            globals.insert(global.id);
        }
        ExpressionKind::Abstraction { body, .. }
        | ExpressionKind::UncurriedAbstraction { body, .. } => {
            if descend_abstractions {
                collect_expression_globals(module, *body, descend_abstractions, globals);
            }
        }
        ExpressionKind::Let { recursive, bindings, body } => {
            let lazy_values = *recursive
                && !bindings
                    .iter()
                    .all(|binding| is_abstraction(&module.storage[binding.expression].kind));
            if descend_abstractions || lazy_values {
                for binding in bindings.iter() {
                    collect_expression_globals(
                        module,
                        binding.expression,
                        descend_abstractions,
                        globals,
                    );
                }
            } else if !*recursive {
                for binding in bindings.iter() {
                    collect_expression_globals(module, binding.expression, false, globals);
                }
            }
            collect_expression_globals(module, *body, descend_abstractions, globals);
        }
        _ => collect_expression_children(module, expression, descend_abstractions, globals),
    }
}

fn collect_expression_children(
    module: &FunctionalModule,
    expression: FunctionalExpressionId,
    descend_abstractions: bool,
    globals: &mut FxHashSet<GlobalId>,
) {
    if descend_abstractions {
        let mut seen = FxHashSet::default();
        let mut references = Vec::new();
        collect_expression_references(module, expression, &mut seen, &mut references);
        globals.extend(references.into_iter().map(|global| global.id));
        return;
    }
    match &module.storage[expression].kind {
        ExpressionKind::Async(_) => {
            unreachable!("invariant violated: Async program was not lowered to steps")
        }
        ExpressionKind::Literal { .. }
        | ExpressionKind::Constructor { .. }
        | ExpressionKind::Global { .. }
        | ExpressionKind::Local { .. }
        | ExpressionKind::SynthesizedEvidence { .. }
        | ExpressionKind::TrivialEvidence => {}
        ExpressionKind::Array { elements } => {
            for expression in elements.iter() {
                collect_expression_globals(module, *expression, false, globals);
            }
        }
        ExpressionKind::ArrayConcatenation { elements } => {
            for element in elements.iter() {
                collect_expression_globals(module, element.expression(), false, globals);
            }
        }
        ExpressionKind::Record { fields } => {
            for field in fields.iter() {
                collect_expression_globals(module, field.expression, false, globals);
            }
        }
        ExpressionKind::RecordUpdate { record, updates } => {
            collect_expression_globals(module, *record, false, globals);
            collect_update_globals(module, updates, false, globals);
        }
        ExpressionKind::Project { record, .. } | ExpressionKind::Unary { value: record, .. } => {
            collect_expression_globals(module, *record, false, globals);
        }
        ExpressionKind::Binary { left, right, .. }
        | ExpressionKind::Index { value: left, index: right } => {
            collect_expression_globals(module, *left, false, globals);
            collect_expression_globals(module, *right, false, globals);
        }
        ExpressionKind::Abstraction { .. } | ExpressionKind::UncurriedAbstraction { .. } => {}
        ExpressionKind::Application { function, arguments, .. }
        | ExpressionKind::UncurriedApplication { function, arguments, .. } => {
            collect_expression_globals(module, *function, false, globals);
            for argument in arguments.iter() {
                collect_expression_globals(module, *argument, false, globals);
            }
        }
        kind @ ExpressionKind::StyleX(_) => {
            for_each_expression_child(kind, |child| {
                collect_expression_globals(module, child, false, globals);
            });
        }
        ExpressionKind::IfThenElse { condition, then, else_ } => {
            collect_expression_globals(module, *condition, false, globals);
            collect_expression_globals(module, *then, false, globals);
            collect_expression_globals(module, *else_, false, globals);
        }
        ExpressionKind::Case { scrutinees, alternatives } => {
            for expression in scrutinees.iter() {
                collect_expression_globals(module, *expression, false, globals);
            }
            for alternative in alternatives.iter() {
                collect_expression_globals(module, alternative.expression, false, globals);
            }
        }
        ExpressionKind::Guarded { alternatives } => {
            for alternative in alternatives.iter() {
                for guard in alternative.guards.iter() {
                    let expression = match guard {
                        Guard::Boolean(expression) | Guard::Pattern { expression, .. } => {
                            *expression
                        }
                    };
                    collect_expression_globals(module, expression, false, globals);
                }
                collect_expression_globals(module, alternative.expression, false, globals);
            }
        }
        ExpressionKind::Let { .. } => unreachable!("let expressions are handled by the caller"),
        ExpressionKind::LetPattern { value, body, .. } => {
            collect_expression_globals(module, *value, false, globals);
            collect_expression_globals(module, *body, false, globals);
        }
        kind @ ExpressionKind::Effect { effect } => match effect {
            EffectExpression::Bind { action, .. } => {
                collect_expression_globals(module, *action, false, globals);
            }
            _ => for_each_expression_child(kind, |child| {
                collect_expression_globals(module, child, false, globals);
            }),
        },
    }
}

fn collect_update_globals(
    module: &FunctionalModule,
    updates: &[RecordUpdate],
    descend_abstractions: bool,
    globals: &mut FxHashSet<GlobalId>,
) {
    for update in updates {
        match update {
            RecordUpdate::Leaf { expression, .. } => {
                collect_expression_globals(module, *expression, descend_abstractions, globals);
            }
            RecordUpdate::Branch { updates, .. } => {
                collect_update_globals(module, updates, descend_abstractions, globals);
            }
        }
    }
}
