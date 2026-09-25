//! Simplification of functional trees before backend-specific code generation.

use std::convert::Infallible;
use std::sync::Arc;

use rustc_hash::FxHashSet;
use smol_str::{SmolStr, format_smolstr};

use crate::tree::{
    Binding, EffectExpression, ExpressionId, ExpressionKind, GlobalId, Guard, Literal, LocalId,
    RecordUpdate, Storage, UnaryOperator,
};

pub fn inline_simple_bindings(
    storage: &mut Storage,
    expression: ExpressionId,
    recursive_globals: &FxHashSet<GlobalId>,
) {
    let mut visited = FxHashSet::default();
    optimize_expression(storage, expression, recursive_globals, &mut visited);
}

pub(crate) fn reachable_expressions(
    storage: &Storage,
    roots: impl IntoIterator<Item = ExpressionId>,
) -> FxHashSet<ExpressionId> {
    let mut reachable = FxHashSet::default();
    let mut pending = roots.into_iter().collect::<Vec<_>>();
    while let Some(expression) = pending.pop() {
        if !reachable.insert(expression) {
            continue;
        }
        for_each_expression_child(&storage[expression].kind, |child| pending.push(child));
    }
    reachable
}

pub(crate) fn expression_globals(
    storage: &Storage,
    expression: ExpressionId,
) -> FxHashSet<GlobalId> {
    let reachable = reachable_expressions(storage, [expression]);
    let globals = reachable.into_iter().filter_map(|expression| match &storage[expression].kind {
        ExpressionKind::Constructor { global } | ExpressionKind::Global { global } => {
            Some(global.id)
        }
        _ => None,
    });
    globals.collect()
}

fn optimize_expression(
    storage: &mut Storage,
    expression: ExpressionId,
    recursive_globals: &FxHashSet<GlobalId>,
    visited: &mut FxHashSet<ExpressionId>,
) {
    enum Pending {
        Enter(ExpressionId),
        Fold(ExpressionId),
        Inline(ExpressionId, Arc<[Binding]>, ExpressionId),
    }

    let mut pending = vec![Pending::Enter(expression)];
    while let Some(work) = pending.pop() {
        match work {
            Pending::Enter(expression) => {
                if !visited.insert(expression) {
                    continue;
                }
                let kind = &storage[expression].kind;
                if let ExpressionKind::Let { recursive: false, bindings, body } = kind {
                    // Inlining uses the entry-time bindings even if descendants mutate storage.
                    pending.push(Pending::Inline(expression, Arc::clone(bindings), *body));
                } else {
                    pending.push(Pending::Fold(expression));
                }
                let children_start = pending.len();
                for_each_expression_child(kind, |child| pending.push(Pending::Enter(child)));
                pending[children_start..].reverse();
            }
            Pending::Fold(expression) => {
                fold_literal_negation(storage, expression);
            }
            Pending::Inline(expression, bindings, body) => {
                if !fold_literal_negation(storage, expression) {
                    inline_bindings(storage, expression, &bindings, body, recursive_globals);
                }
            }
        }
    }
}

fn inline_bindings(
    storage: &mut Storage,
    expression: ExpressionId,
    bindings: &[Binding],
    body: ExpressionId,
    recursive_globals: &FxHashSet<GlobalId>,
) {
    // Substitution only replaces locals with trivial or simple expressions, so a binding that is
    // not simple on entry never becomes a candidate and its uses never need counting.
    let bindings = bindings.iter().map(|binding| {
        let candidate = is_simple_expression(storage, binding.expression, recursive_globals);
        (Binding::clone(binding), candidate)
    });
    let mut bindings = bindings.collect::<Vec<_>>();
    let mut inlined = false;
    while let Some(position) = bindings.iter().position(|(binding, candidate)| {
        *candidate && is_inlinable(storage, &bindings, body, binding, recursive_globals)
    }) {
        let (binding, _) = bindings.remove(position);
        for (remaining, _) in &bindings {
            substitute_local(
                storage,
                remaining.expression,
                binding.parameter.id,
                binding.expression,
            );
        }
        substitute_local(storage, body, binding.parameter.id, binding.expression);
        inlined = true;
    }
    if !inlined {
        return;
    }

    let replacement = if bindings.is_empty() {
        storage[body].kind.clone()
    } else {
        let bindings = bindings.into_iter().map(|(binding, _)| binding);
        ExpressionKind::Let { recursive: false, bindings: bindings.collect(), body }
    };
    storage.replace_expression_kind(expression, replacement);
    fold_literal_negation(storage, expression);
}

fn is_inlinable(
    storage: &Storage,
    bindings: &[(Binding, bool)],
    body: ExpressionId,
    binding: &Binding,
    recursive_globals: &FxHashSet<GlobalId>,
) -> bool {
    let roots = bindings.iter().map(|(binding, _)| binding.expression).chain([body]);
    if is_trivial_expression(storage, binding.expression, recursive_globals) {
        local_uses_up_to(storage, roots, binding.parameter.id, 1) > 0
    } else {
        is_simple_expression(storage, binding.expression, recursive_globals)
            && local_uses_up_to(storage, roots, binding.parameter.id, 2) == 1
    }
}

fn fold_literal_negation(storage: &mut Storage, expression: ExpressionId) -> bool {
    let (operator, value) = match &storage[expression].kind {
        ExpressionKind::Unary { operator, value } => (*operator, *value),
        _ => return false,
    };
    let Some(literal) = folded_negation(storage, operator, value) else {
        return false;
    };
    storage.replace_expression_kind(expression, ExpressionKind::Literal { literal });
    true
}

fn folded_negation(
    storage: &Storage,
    operator: UnaryOperator,
    value: ExpressionId,
) -> Option<Literal> {
    match (operator, &storage[value].kind) {
        (
            UnaryOperator::IntegerNegate,
            ExpressionKind::Literal { literal: Literal::Integer(value) },
        ) => Some(Literal::Integer(value.wrapping_neg())),
        (
            UnaryOperator::NumberNegate,
            ExpressionKind::Literal { literal: Literal::Number(value) },
        ) => Some(Literal::Number(negated_number(value))),
        (
            UnaryOperator::BooleanNot | UnaryOperator::IntegerNegate | UnaryOperator::NumberNegate,
            _,
        ) => None,
    }
}

fn negated_number(value: &str) -> SmolStr {
    if value.parse::<f64>().ok() == Some(0.0) {
        return SmolStr::new("0.0");
    }
    match value.strip_prefix('-') {
        Some(value) => SmolStr::new(value),
        None => format_smolstr!("-{value}"),
    }
}

pub fn local_uses(storage: &Storage, expression: ExpressionId, parameter: LocalId) -> usize {
    local_uses_up_to(storage, [expression], parameter, usize::MAX)
}

/// Counts uses of `parameter` under `roots`, stopping once `limit` uses are found.
pub fn local_uses_up_to(
    storage: &Storage,
    roots: impl IntoIterator<Item = ExpressionId>,
    parameter: LocalId,
    limit: usize,
) -> usize {
    let mut uses = 0;
    let mut pending = roots.into_iter().collect::<Vec<_>>();
    while let Some(expression) = pending.pop() {
        if uses >= limit {
            break;
        }
        let kind = &storage[expression].kind;
        if matches!(kind, ExpressionKind::Local { parameter: local } if local.id == parameter) {
            uses += 1;
            continue;
        }
        // Visiting children left to right keeps the pending stack shallow on right-nested
        // chains such as do blocks, whose continuation is the last child.
        let children_start = pending.len();
        for_each_expression_child(kind, |child| pending.push(child));
        pending[children_start..].reverse();
    }
    uses
}

fn substitute_local(
    storage: &mut Storage,
    expression: ExpressionId,
    parameter: LocalId,
    replacement: ExpressionId,
) {
    let mut pending = vec![expression];
    while let Some(expression) = pending.pop() {
        let kind = &storage[expression].kind;
        if matches!(kind, ExpressionKind::Local { parameter: local } if local.id == parameter) {
            let replacement = ExpressionKind::clone(&storage[replacement].kind);
            storage.replace_expression_kind(expression, replacement);
            continue;
        }
        let children_start = pending.len();
        for_each_expression_child(kind, |child| pending.push(child));
        pending[children_start..].reverse();
    }
}

fn is_trivial_expression(
    storage: &Storage,
    expression: ExpressionId,
    recursive_globals: &FxHashSet<GlobalId>,
) -> bool {
    match &storage[expression].kind {
        ExpressionKind::Literal { .. }
        | ExpressionKind::Constructor { .. }
        | ExpressionKind::Local { .. }
        | ExpressionKind::SynthesizedEvidence { .. }
        | ExpressionKind::TrivialEvidence => true,
        ExpressionKind::Global { global } => !recursive_globals.contains(&global.id),
        _ => false,
    }
}

fn is_simple_expression(
    storage: &Storage,
    expression: ExpressionId,
    recursive_globals: &FxHashSet<GlobalId>,
) -> bool {
    let mut pending = vec![expression];
    while let Some(expression) = pending.pop() {
        match &storage[expression].kind {
            ExpressionKind::Literal { .. }
            | ExpressionKind::Constructor { .. }
            | ExpressionKind::Local { .. }
            | ExpressionKind::Abstraction { .. }
            | ExpressionKind::UncurriedAbstraction { .. }
            | ExpressionKind::SynthesizedEvidence { .. }
            | ExpressionKind::TrivialEvidence => {}
            ExpressionKind::Global { global } => {
                if recursive_globals.contains(&global.id) {
                    return false;
                }
            }
            ExpressionKind::Array { elements } => pending.extend(elements.iter().rev().copied()),
            ExpressionKind::Record { fields } => {
                pending.extend(fields.iter().rev().map(|field| field.expression));
            }
            ExpressionKind::Project { record, .. }
            | ExpressionKind::Unary { value: record, .. } => {
                pending.push(*record);
            }
            ExpressionKind::Binary { left, right, .. } => {
                pending.push(*right);
                pending.push(*left);
            }
            ExpressionKind::RecordUpdate { .. }
            | ExpressionKind::Error
            | ExpressionKind::Application { .. }
            | ExpressionKind::UncurriedApplication { .. }
            | ExpressionKind::StyleX(_)
            | ExpressionKind::IfThenElse { .. }
            | ExpressionKind::Case { .. }
            | ExpressionKind::Guarded { .. }
            | ExpressionKind::Let { .. }
            | ExpressionKind::LetPattern { .. }
            | ExpressionKind::Effect { .. } => return false,
        }
    }
    true
}

pub fn for_each_expression_child(kind: &ExpressionKind, mut visit: impl FnMut(ExpressionId)) {
    let result: Result<(), Infallible> = try_for_each_expression_child(kind, |child| {
        visit(child);
        Ok(())
    });
    let Ok(()) = result;
}

pub fn try_for_each_expression_child<Error>(
    kind: &ExpressionKind,
    mut visit: impl FnMut(ExpressionId) -> Result<(), Error>,
) -> Result<(), Error> {
    match kind {
        ExpressionKind::Error
        | ExpressionKind::Literal { .. }
        | ExpressionKind::Constructor { .. }
        | ExpressionKind::Global { .. }
        | ExpressionKind::Local { .. }
        | ExpressionKind::SynthesizedEvidence { .. }
        | ExpressionKind::TrivialEvidence => {}
        ExpressionKind::Array { elements } => {
            for &element in elements.iter() {
                visit(element)?;
            }
        }
        ExpressionKind::Record { fields } => {
            for field in fields.iter() {
                visit(field.expression)?;
            }
        }
        ExpressionKind::RecordUpdate { record, updates } => {
            visit(*record)?;
            try_for_each_update_child(updates, &mut visit)?;
        }
        ExpressionKind::Project { record, .. } | ExpressionKind::Unary { value: record, .. } => {
            visit(*record)?;
        }
        ExpressionKind::Binary { left, right, .. } => {
            visit(*left)?;
            visit(*right)?;
        }
        ExpressionKind::Abstraction { body, .. }
        | ExpressionKind::UncurriedAbstraction { body, .. } => visit(*body)?,
        ExpressionKind::Application { function, arguments, .. }
        | ExpressionKind::UncurriedApplication { function, arguments, .. } => {
            visit(*function)?;
            for &argument in arguments.iter() {
                visit(argument)?;
            }
        }
        ExpressionKind::StyleX(stylex) => stylex.try_for_each_child(&mut visit)?,
        ExpressionKind::IfThenElse { condition, then, else_ } => {
            visit(*condition)?;
            visit(*then)?;
            visit(*else_)?;
        }
        ExpressionKind::Case { scrutinees, alternatives } => {
            for &scrutinee in scrutinees.iter() {
                visit(scrutinee)?;
            }
            for alternative in alternatives.iter() {
                visit(alternative.expression)?;
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
                    visit(expression)?;
                }
                visit(alternative.expression)?;
            }
        }
        ExpressionKind::Let { bindings, body, .. } => {
            for binding in bindings.iter() {
                visit(binding.expression)?;
            }
            visit(*body)?;
        }
        ExpressionKind::LetPattern { value, body, .. } => {
            visit(*value)?;
            visit(*body)?;
        }
        ExpressionKind::Effect { effect } => match effect {
            EffectExpression::Pure(value) => visit(*value)?,
            EffectExpression::Bind { action, body, .. } => {
                visit(*action)?;
                visit(*body)?;
            }
            EffectExpression::Map { function, action } => {
                visit(*function)?;
                visit(*action)?;
            }
            EffectExpression::Apply { function_action, argument_action } => {
                visit(*function_action)?;
                visit(*argument_action)?;
            }
        },
    }
    Ok(())
}

fn try_for_each_update_child<Error>(
    updates: &[RecordUpdate],
    visit: &mut impl FnMut(ExpressionId) -> Result<(), Error>,
) -> Result<(), Error> {
    let mut pending = vec![updates.iter()];
    while let Some(updates) = pending.last_mut() {
        let Some(update) = updates.next() else {
            pending.pop();
            continue;
        };
        match update {
            RecordUpdate::Leaf { expression, .. } => visit(*expression)?,
            RecordUpdate::Branch { updates, .. } => pending.push(updates.iter()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stylex::{StyleXConditionalCase, StyleXExpression, StyleXWhenRelation};
    use crate::tree::{Expression, Field, FieldIdentity, Parameter};

    fn expression(index: u32) -> ExpressionId {
        ExpressionId::from_raw(index.into())
    }

    fn visited_children(kind: &ExpressionKind) -> Vec<ExpressionId> {
        let mut children = Vec::new();
        for_each_expression_child(kind, |child| children.push(child));
        children
    }

    fn nested_arrays(depth: usize) -> (Storage, ExpressionId, ExpressionId) {
        let mut storage = Storage::default();
        let replacement = storage.allocate_expression(Expression {
            kind: ExpressionKind::Literal { literal: Literal::Integer(42) },
        });
        let local =
            ExpressionKind::Local { parameter: Parameter { id: LocalId(0), name: "value".into() } };
        let mut root = storage.allocate_expression(Expression { kind: local });
        for _ in 0..depth {
            root = storage.allocate_expression(Expression {
                kind: ExpressionKind::Array { elements: [root].into() },
            });
        }
        (storage, root, replacement)
    }

    #[test]
    fn deep_traversals_do_not_use_the_call_stack() {
        let (mut storage, root, replacement) = nested_arrays(100_000);
        inline_simple_bindings(&mut storage, root, &FxHashSet::default());
        assert_eq!(local_uses(&storage, root, LocalId(0)), 1);
        substitute_local(&mut storage, root, LocalId(0), replacement);
        assert_eq!(local_uses(&storage, root, LocalId(0)), 0);

        let parameter = Parameter { id: LocalId(1), name: "bound".into() };
        let body = storage.allocate_expression(Expression {
            kind: ExpressionKind::Local { parameter: Parameter::clone(&parameter) },
        });
        let binding = storage.allocate_expression(Expression {
            kind: ExpressionKind::Let {
                recursive: false,
                bindings: [Binding { parameter, expression: root, source_order: 0 }].into(),
                body,
            },
        });
        inline_simple_bindings(&mut storage, binding, &FxHashSet::default());
        assert_eq!(storage[binding].kind, storage[root].kind);
        assert_eq!(local_uses(&storage, binding, LocalId(1)), 0);
    }

    #[test]
    fn simplicity_preserves_abstraction_boundaries_and_recursive_globals() {
        let mut storage = Storage::default();
        let global = GlobalId::Generated(files::FileId::new(0), crate::tree::GeneratedGlobalId(0));
        let body = storage.allocate_expression(Expression {
            kind: ExpressionKind::Global {
                global: crate::tree::Global { id: global, item_name: "recursive".into() },
            },
        });
        let recursive_globals = FxHashSet::from_iter([global]);
        assert!(is_simple_expression(&storage, body, &FxHashSet::default()));
        assert!(!is_simple_expression(&storage, body, &recursive_globals));
        for kind in [
            ExpressionKind::Abstraction { parameters: [].into(), body },
            ExpressionKind::UncurriedAbstraction { parameters: [].into(), body },
        ] {
            let abstraction = storage.allocate_expression(Expression { kind });
            assert!(is_simple_expression(&storage, abstraction, &recursive_globals));
        }
    }

    #[test]
    fn local_uses_counts_shared_subtree_occurrences() {
        let (mut storage, shared, _) = nested_arrays(3);
        let root = storage.allocate_expression(Expression {
            kind: ExpressionKind::Array { elements: [shared, shared, shared].into() },
        });
        assert_eq!(local_uses(&storage, root, LocalId(0)), 3);
        assert_eq!(local_uses(&storage, root, LocalId(1)), 0);
    }

    #[test]
    fn substitution_does_not_descend_into_a_new_replacement() {
        let (mut storage, replacement, _) = nested_arrays(1);
        let target = storage.allocate_expression(Expression {
            kind: ExpressionKind::Local {
                parameter: Parameter { id: LocalId(0), name: "target".into() },
            },
        });
        substitute_local(&mut storage, target, LocalId(0), replacement);
        assert_eq!(storage[target].kind, storage[replacement].kind);
        assert_eq!(local_uses(&storage, target, LocalId(0)), 1);
    }

    #[test]
    fn optimization_preserves_dfs_order_and_does_not_revisit_mutated_shared_nodes() {
        let (mut storage, local, value) = nested_arrays(0);
        let negation = storage.allocate_expression(Expression {
            kind: ExpressionKind::Unary { operator: UnaryOperator::IntegerNegate, value: local },
        });
        let binding = storage.allocate_expression(Expression {
            kind: ExpressionKind::Let {
                recursive: false,
                bindings: [Binding {
                    parameter: Parameter { id: LocalId(0), name: "value".into() },
                    expression: value,
                    source_order: 0,
                }]
                .into(),
                body: negation,
            },
        });
        let later_negation = storage.allocate_expression(Expression {
            kind: ExpressionKind::Unary { operator: UnaryOperator::IntegerNegate, value: local },
        });
        let root = storage.allocate_expression(Expression {
            kind: ExpressionKind::Array { elements: [binding, negation, later_negation].into() },
        });
        let mut visited = FxHashSet::default();
        optimize_expression(&mut storage, root, &FxHashSet::default(), &mut visited);
        assert_eq!(visited.len(), 6);
        assert_eq!(storage[local].kind, storage[value].kind);
        assert_eq!(
            storage[binding].kind,
            ExpressionKind::Literal { literal: Literal::Integer(-42) }
        );
        assert_eq!(storage[later_negation].kind, storage[binding].kind);
        assert_eq!(
            storage[negation].kind,
            ExpressionKind::Unary { operator: UnaryOperator::IntegerNegate, value: local }
        );
    }

    #[test]
    fn deep_record_updates_do_not_use_the_call_stack() {
        let (mut storage, local, replacement) = nested_arrays(0);
        let field = Field { identity: FieldIdentity::Label("field".into()), name: "field".into() };
        let mut updates: Arc<[RecordUpdate]> =
            Arc::from([RecordUpdate::Leaf { field: Field::clone(&field), expression: local }]);
        let mut retained = Vec::new();
        for _ in 0..100_000 {
            retained.push(Arc::clone(&updates));
            updates = Arc::from([RecordUpdate::Branch { field: Field::clone(&field), updates }]);
        }
        let root = storage.allocate_expression(Expression {
            kind: ExpressionKind::RecordUpdate { record: replacement, updates },
        });

        assert_eq!(visited_children(&storage[root].kind), vec![replacement, local]);
        inline_simple_bindings(&mut storage, root, &FxHashSet::default());
        assert_eq!(local_uses(&storage, root, LocalId(0)), 1);
        substitute_local(&mut storage, root, LocalId(0), replacement);
        assert_eq!(local_uses(&storage, root, LocalId(0)), 0);

        // Retained levels keep recursive Arc destruction out of this traversal test.
        drop(storage);
        while let Some(updates) = retained.pop() {
            drop(updates);
        }
    }

    #[test]
    fn nested_record_updates_visit_in_order_and_stop_at_first_error() {
        let field = Field { identity: FieldIdentity::Label("field".into()), name: "field".into() };
        let kind = ExpressionKind::RecordUpdate {
            record: expression(0),
            updates: [
                RecordUpdate::Leaf { field: field.clone(), expression: expression(1) },
                RecordUpdate::Branch {
                    field: field.clone(),
                    updates: [
                        RecordUpdate::Leaf { field: field.clone(), expression: expression(2) },
                        RecordUpdate::Leaf { field: field.clone(), expression: expression(3) },
                    ]
                    .into(),
                },
                RecordUpdate::Leaf { field, expression: expression(4) },
            ]
            .into(),
        };
        assert_eq!(visited_children(&kind), (0..5).map(expression).collect::<Vec<_>>());

        let mut visited = Vec::new();
        let result = try_for_each_expression_child(&kind, |child| {
            visited.push(child);
            if child == expression(2) { Err(child) } else { Ok(()) }
        });
        assert_eq!(result, Err(expression(2)));
        assert_eq!(visited, (0..3).map(expression).collect::<Vec<_>>());
    }

    #[test]
    fn stylex_conditional_cases_visit_optional_markers_in_order() {
        let kind = ExpressionKind::StyleX(StyleXExpression::ConditionalValue {
            default: expression(0),
            cases: [
                StyleXConditionalCase {
                    relation: StyleXWhenRelation::Ancestor,
                    selector: expression(1),
                    marker: None,
                    value: expression(2),
                },
                StyleXConditionalCase {
                    relation: StyleXWhenRelation::Descendant,
                    selector: expression(3),
                    marker: Some(expression(4)),
                    value: expression(5),
                },
            ]
            .into(),
        });
        assert_eq!(visited_children(&kind), (0..6).map(expression).collect::<Vec<_>>());

        let mut visited = Vec::new();
        let result = try_for_each_expression_child(&kind, |child| {
            visited.push(child);
            if child == expression(4) { Err(child) } else { Ok(()) }
        });
        assert_eq!(result, Err(expression(4)));
        assert_eq!(visited, (0..5).map(expression).collect::<Vec<_>>());
    }
}
