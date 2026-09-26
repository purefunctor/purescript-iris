//! Identifies operator and expression sections.
//!
//! A section is an underscore `_` shorthand used to create a lambda.
//!
//! ```purescript
//! operators = (_ + 1)
//!
//! ifThenElse = if _ then _ else _
//!
//! recordAccess = _.foo.bar.baz
//!
//! recordUpdate = _ { life = 42 }
//! ```
//!
//! This module traverses every expression in [`LoweredModule`] and
//! associates immediate [`ExpressionKind::Section`] to it. This is
//! done only in certain syntactic contexts.
//!
//! The analyzer's ID-centric architecture allows for a relatively
//! simple type inference rule for sections: type checker creates
//! unification variables for each section, and then binds them
//! into  th environment using the section's [`ExpressionId`] as
//! the key.
//!
//! This enables fully syntax-driven checking for sections without
//! first desugaring into a separate representation.
//!
//! [`ExpressionId`]: lowering::ExpressionId

use lowering::{ExpressionArgument, ExpressionId, ExpressionKind, LoweredModule, RecordUpdate};
use rustc_hash::FxHashMap;

/// The result of identifying sections in a module.
#[derive(Debug, PartialEq, Eq, Default)]
pub struct Sectioned {
    pub expressions: FxHashMap<ExpressionId, SectionResult>,
}

pub type SectionResult = Vec<ExpressionId>;

/// Identifies sections in all expressions in a module.
pub fn sectioned(lowered: &LoweredModule) -> Sectioned {
    let mut expressions = FxHashMap::default();

    for (id, kind) in lowered.tree.iter_expression() {
        let mut sections = Vec::new();

        visit_sections(kind, |child_id| {
            if let Some(ExpressionKind::Section) = lowered.tree.get_expression_kind(child_id) {
                sections.push(child_id);
            }
        });

        if !sections.is_empty() {
            expressions.insert(id, sections);
        }
    }

    Sectioned { expressions }
}

fn visit_sections<F>(kind: &ExpressionKind, mut f: F)
where
    F: FnMut(ExpressionId),
{
    match kind {
        ExpressionKind::Typed { expression: Some(id), .. } => {
            f(*id);
        }
        ExpressionKind::OperatorChain { head, tail } => {
            if let Some(id) = *head {
                f(id);
            }
            for pair in tail.iter() {
                if let Some(id) = pair.element {
                    f(id);
                }
            }
        }
        ExpressionKind::InfixChain { head, tail } => {
            if let Some(id) = *head {
                f(id);
            }
            for pair in tail.iter() {
                if let Some(id) = pair.tick {
                    f(id);
                }
                if let Some(id) = pair.element {
                    f(id);
                }
            }
        }
        ExpressionKind::Negate { expression: Some(id), .. } => {
            f(*id);
        }
        ExpressionKind::Application { function, arguments } => {
            if let Some(id) = *function {
                f(id);
            }
            for argument in arguments.iter() {
                if let ExpressionArgument::Term(Some(id)) = argument {
                    f(*id);
                }
            }
        }
        ExpressionKind::IfThenElse { if_, then, else_ } => {
            if let Some(id) = *if_ {
                f(id);
            }
            if let Some(id) = *then {
                f(id);
            }
            if let Some(id) = *else_ {
                f(id);
            }
        }
        ExpressionKind::CaseOf { trunk, .. } => {
            for id in trunk.iter() {
                f(*id);
            }
        }
        ExpressionKind::Parenthesized { parenthesized: Some(id) } => {
            f(*id);
        }
        ExpressionKind::RecordAccess { record: Some(id), .. } => {
            f(*id);
        }
        ExpressionKind::RecordUpdate { record, updates } => {
            if let Some(id) = *record {
                f(id);
            }
            visit_record_updates(updates, &mut f);
        }
        ExpressionKind::Array { array } => {
            for id in array.iter() {
                f(*id);
            }
        }
        ExpressionKind::Record { record } => {
            for field in record.iter() {
                if let lowering::ExpressionRecordItem::RecordField { value: Some(id), .. } = field {
                    f(*id);
                }
            }
        }
        _ => {}
    }
}

fn visit_record_updates<F>(updates: &[RecordUpdate], f: &mut F)
where
    F: FnMut(ExpressionId),
{
    for update in updates {
        match update {
            RecordUpdate::Leaf { expression, .. } => {
                if let Some(id) = *expression {
                    f(id);
                }
            }
            RecordUpdate::Branch { updates, .. } => {
                visit_record_updates(updates, f);
            }
        }
    }
}
