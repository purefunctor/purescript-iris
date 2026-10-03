//! JavaScript rendering for native React expressions.

use functional::optimize::for_each_expression_child;
use functional::react::{ReactElement, ReactExpression};
use functional::tree::{DeclarationKind, ExpressionId, ExpressionKind, Module};
use rustc_hash::FxHashSet;

use crate::error::ModuleResult;
use crate::tree::{ObjectProperty, Tree};
use crate::writer::Writer;

use super::{FunctionContext, Generator, RenderedExpression};

pub(super) fn required_imports(module: &Module) -> (bool, bool, bool) {
    let mut jsx = false;
    let mut jsxs = false;
    let mut fragment = false;
    let roots = module.declarations.iter().filter_map(|declaration| match declaration.kind {
        DeclarationKind::Value(root) => Some(root),
        _ => None,
    });
    let mut pending = roots.collect::<Vec<_>>();
    let mut visited = FxHashSet::default();
    while let Some(expression) = pending.pop() {
        if !visited.insert(expression) {
            continue;
        }
        let kind = &module.storage[expression].kind;
        match kind {
            ExpressionKind::React(ReactExpression::Element {
                static_multiple_children, ..
            }) => {
                if *static_multiple_children {
                    jsxs = true
                } else {
                    jsx = true
                }
            }
            ExpressionKind::React(ReactExpression::Fragment { children }) => {
                if fragment_has_static_children(module, *children) {
                    jsxs = true;
                } else {
                    jsx = true;
                }
                fragment = true;
            }
            _ => {}
        }
        for_each_expression_child(kind, |child| pending.push(child));
    }
    (jsx, jsxs, fragment)
}

fn fragment_has_static_children(module: &Module, children: ExpressionId) -> bool {
    matches!(module.storage[children].kind, ExpressionKind::Array { .. })
}

impl Generator<'_> {
    pub(super) fn render_react_expression<'a, 't>(
        &self,
        tree: &'a mut Tree<'t>,
        writer: &'a mut Writer<'t>,
        react: &ReactExpression,
        context: &'a mut FunctionContext,
    ) -> ModuleResult<RenderedExpression> {
        let value = match react {
            ReactExpression::Component { render } => {
                return self.rendered_expression(tree, writer, *render, context);
            }
            ReactExpression::Element { component, props, key, static_multiple_children } => {
                // A keyed source call evaluates its key before its element target and props.
                let key = if let Some(key) = key {
                    let mut key = self.rendered_expression(tree, writer, *key, context)?;
                    self.materialize_rendered_expression(
                        tree,
                        writer,
                        &mut key,
                        "$reactKey",
                        context,
                    );
                    Some(key.value)
                } else {
                    None
                };
                let mut target = match component {
                    ReactElement::Component(target) | ReactElement::Intrinsic(target) => {
                        self.rendered_expression(tree, writer, *target, context)?
                    }
                };
                self.materialize_rendered_expression(
                    tree,
                    writer,
                    &mut target,
                    "$reactType",
                    context,
                );
                let props = self.rendered_expression(tree, writer, *props, context)?.value;
                let function = if *static_multiple_children {
                    self.react_jsxs.as_ref().expect("React expression has no jsxs import")
                } else {
                    self.react_jsx.as_ref().expect("React expression has no jsx import")
                };
                let mut arguments = vec![target.value, props];
                if let Some(key) = key {
                    arguments.push(key);
                }
                let function = tree.identifier(function);
                tree.call(function, arguments)
            }
            ReactExpression::Fragment { children } => {
                let function = if fragment_has_static_children(self.module, *children) {
                    self.react_jsxs.as_ref().expect("React fragment has no jsxs import")
                } else {
                    self.react_jsx.as_ref().expect("React fragment has no jsx import")
                };
                let children = self.rendered_expression(tree, writer, *children, context)?.value;
                let props = tree.object(vec![ObjectProperty::Field {
                    name: "children".into(),
                    value: children,
                }]);
                let fragment =
                    self.react_fragment.as_ref().expect("React expression has no Fragment import");
                let function = tree.identifier(function);
                let fragment = tree.identifier(fragment);
                tree.call(function, vec![fragment, props])
            }
            ReactExpression::Empty => tree.null(),
        };
        Ok(RenderedExpression { value, pending_evaluation: true })
    }
}
