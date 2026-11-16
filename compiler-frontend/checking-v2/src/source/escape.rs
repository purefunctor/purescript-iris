use itertools::Itertools;

use crate::ExternalQueries;
use crate::context::CheckContext;
use crate::core::fold;
use crate::error::{ErrorCrumb, ErrorKind};
use crate::state::CheckState;
use crate::tree::{
    ExpressionId, ExpressionKind, Guard, Guarded, LocalBinding, Parameter, RecordUpdate,
    ValueDeclaration,
};
use crate::types::{Name, Type, TypeId};

struct Escape {
    rigid: TypeId,
    solution: TypeId,
    source: Option<lowering::ExpressionId>,
}

struct EscapeWalk<'a, 'q, Q>
where
    Q: ExternalQueries,
{
    state: &'a CheckState,
    context: &'a CheckContext<'q, Q>,
    scope: Vec<Name>,
    escape: Option<Escape>,
}

pub fn check_value<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    scope: &[TypeId],
    value: &ValueDeclaration,
) -> bool
where
    Q: ExternalQueries,
{
    let mut walk = EscapeWalk { state, context, scope: Vec::new(), escape: None };
    walk.with_scope(scope, |walk| walk.value(value));

    let Some(Escape { rigid, solution, source }) = walk.escape else {
        return false;
    };
    let kind = ErrorKind::EscapedSkolem { rigid, solution };
    match source {
        Some(source) => {
            state.with_crumb(ErrorCrumb::Expression(source), |state| state.insert_error(kind));
        }
        None => state.insert_error(kind),
    }
    true
}

impl<Q> EscapeWalk<'_, '_, Q>
where
    Q: ExternalQueries,
{
    fn with_scope(&mut self, rigids: &[TypeId], action: impl FnOnce(&mut Self)) {
        let depth = self.scope.len();
        let names = rigids.iter().filter_map(|&rigid| match self.context.lookup_type(rigid) {
            Type::Rigid(rigid_id) => Some(self.context.lookup_rigid(rigid_id).name),
            _ => None,
        });
        let names = names.collect_vec();
        self.scope.extend(names);
        action(self);
        self.scope.truncate(depth);
    }

    fn check_type(&mut self, type_id: TypeId, source: Option<lowering::ExpressionId>) {
        if self.escape.is_some() {
            return;
        }
        let solution = fold::zonk(self.state, self.context, type_id);
        if let Some(rigid) = self.escaped_rigid(solution) {
            self.escape = Some(Escape { rigid, solution, source });
        }
    }

    fn escaped_rigid(&self, id: TypeId) -> Option<TypeId> {
        if !self.context.lookup_type_flags(id).has_rigid() {
            return None;
        }
        if let Type::Rigid(rigid_id) = self.context.lookup_type(id) {
            let name = self.context.lookup_rigid(rigid_id).name;
            return (!self.scope.contains(&name)).then_some(id);
        }
        let mut escaped = None;
        fold::map_children(self.context, id, |child| {
            if escaped.is_none() {
                escaped = self.escaped_rigid(child);
            }
            child
        });
        escaped
    }

    fn value(&mut self, value: &ValueDeclaration) {
        let parameter_binders = value.parameters.iter().filter_map(|parameter| match parameter {
            Parameter::Abstract(abstraction) => Some(abstraction.type_binders.iter().copied()),
            Parameter::Argument(_) => None,
        });
        let mut type_binders = value.type_binders.to_vec();
        type_binders.extend(parameter_binders.flatten());

        self.with_scope(&type_binders, |walk| {
            for equation in value.equations.iter() {
                walk.guarded(&equation.guarded, None);
            }
        });
    }

    fn guarded(&mut self, guarded: &Guarded, source: Option<lowering::ExpressionId>) {
        match guarded {
            Guarded::Unconditional(expression) => self.expression(*expression, source),
            Guarded::Conditionals(conditionals) => {
                for conditional in conditionals.iter() {
                    for guard in conditional.guards.iter() {
                        match guard {
                            Guard::Boolean(expression) | Guard::Pattern(_, expression) => {
                                self.expression(*expression, source);
                            }
                            Guard::Let(bindings) => self.bindings(bindings, source),
                        }
                    }
                    self.expression(conditional.expression, source);
                }
            }
        }
    }

    fn bindings(&mut self, bindings: &[LocalBinding], source: Option<lowering::ExpressionId>) {
        for binding in bindings {
            match binding {
                LocalBinding::Value(local) => self.value(&local.value),
                LocalBinding::Pattern { expression, .. } => self.expression(*expression, source),
            }
        }
    }

    fn expression(&mut self, id: ExpressionId, enclosing: Option<lowering::ExpressionId>) {
        let tree = &self.state.checked.tree;
        let expression = &tree.expressions[id];
        let source = expression.source.or(enclosing);

        match &expression.kind {
            ExpressionKind::Abstract { type_binders, expression, .. } => {
                self.with_scope(type_binders, |walk| walk.expression(*expression, source));
                return;
            }
            ExpressionKind::Instantiate { expression, .. } => {
                self.expression(*expression, source);
                return;
            }
            _ => self.check_type(expression.type_id, source),
        }

        match &expression.kind {
            ExpressionKind::Application { function, argument } => {
                self.expression(*function, source);
                self.expression(*argument, source);
            }
            ExpressionKind::Lambda { body, .. } => self.expression(*body, source),
            ExpressionKind::Case { scrutinees, branches } => {
                for &scrutinee in scrutinees.iter() {
                    self.expression(scrutinee, source);
                }
                for branch in branches.iter() {
                    self.guarded(&branch.guarded, source);
                }
            }
            ExpressionKind::Let { bindings, body } => {
                self.bindings(bindings, source);
                self.expression(*body, source);
            }
            ExpressionKind::IfThenElse { condition, then, else_ } => {
                self.expression(*condition, source);
                self.expression(*then, source);
                self.expression(*else_, source);
            }
            ExpressionKind::Array(elements) => {
                for &element in elements.iter() {
                    self.expression(element, source);
                }
            }
            ExpressionKind::Record(fields) => {
                for &(_, field) in fields.iter() {
                    self.expression(field, source);
                }
            }
            ExpressionKind::RecordAccess { record, .. } => self.expression(*record, source),
            ExpressionKind::RecordUpdate { record, updates } => {
                self.expression(*record, source);
                self.record_updates(updates, source);
            }
            ExpressionKind::Abstract { .. }
            | ExpressionKind::Instantiate { .. }
            | ExpressionKind::Reference(_)
            | ExpressionKind::Literal(_)
            | ExpressionKind::Hole(_)
            | ExpressionKind::Error => (),
        }
    }

    fn record_updates(&mut self, updates: &[RecordUpdate], source: Option<lowering::ExpressionId>) {
        for update in updates {
            match update {
                RecordUpdate::Leaf { expression, .. } => self.expression(*expression, source),
                RecordUpdate::Branch { updates, .. } => self.record_updates(updates, source),
            }
        }
    }
}
