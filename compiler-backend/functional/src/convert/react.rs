//! Recognition and lowering for the virtual React module.

use building_types::QueryResult;
use files::FileId;
use indexing::TermItemId;
use rustc_hash::FxHashSet;

use crate::error::UnsupportedState;
use crate::optimize::for_each_expression_child;
use crate::react::{ReactElement, ReactExpression};
use crate::tree::{Declaration, DeclarationKind, ExpressionId, ExpressionKind, GlobalId};

use super::{Context, ConversionResult};

#[derive(Clone, Copy)]
pub(super) struct ReactModule(Option<FileId>);

#[derive(Clone, Copy)]
enum ReactIntrinsic {
    Component,
    Element,
    ElementKeyed,
    Intrinsic,
    IntrinsicKeyed,
    Text,
    Array,
    Fragment,
    Empty,
}

impl<'c, Q: checking::ExternalQueries> Context<'c, Q> {
    pub(super) fn react_intrinsic(
        &mut self,
        function: ExpressionId,
        arguments: &[ExpressionId],
    ) -> ConversionResult<Option<ExpressionId>> {
        let ExpressionKind::Global { global } = self.storage[function].kind.clone() else {
            return Ok(None);
        };
        let GlobalId::Term(file_id, term_id) = global.id else { return Ok(None) };
        let Some(intrinsic) = self.react_intrinsic_identity(file_id, term_id)? else {
            return Ok(None);
        };
        let expression = match (intrinsic, arguments) {
            (ReactIntrinsic::Component, [render]) => ReactExpression::Component { render: *render },
            (ReactIntrinsic::Element, [_, component, props]) => {
                self.react_element(ReactElement::Component(*component), *props, None)
            }
            (ReactIntrinsic::ElementKeyed, [_, key, component, props]) => {
                self.react_element(ReactElement::Component(*component), *props, Some(*key))
            }
            (ReactIntrinsic::Intrinsic, [_, tag, props]) => {
                self.react_element(ReactElement::Intrinsic(*tag), *props, None)
            }
            (ReactIntrinsic::IntrinsicKeyed, [_, key, tag, props]) => {
                self.react_element(ReactElement::Intrinsic(*tag), *props, Some(*key))
            }
            (ReactIntrinsic::Text | ReactIntrinsic::Array, [value]) => return Ok(Some(*value)),
            (ReactIntrinsic::Fragment, [children]) => {
                ReactExpression::Fragment { children: *children }
            }
            (ReactIntrinsic::Empty, []) => ReactExpression::Empty,
            _ => return Ok(None),
        };
        Ok(Some(self.expression(ExpressionKind::React(expression))))
    }

    fn react_element(
        &self,
        component: ReactElement,
        props: ExpressionId,
        key: Option<ExpressionId>,
    ) -> ReactExpression {
        let static_multiple_children = match &self.storage[props].kind {
            ExpressionKind::Record { fields } => fields.iter().any(|field| {
                if field.field.name != "children" {
                    return false;
                }
                match &self.storage[field.expression].kind {
                    ExpressionKind::Array { elements } => elements.len() > 1,
                    _ => false,
                }
            }),
            _ => false,
        };
        ReactExpression::Element { component, props, key, static_multiple_children }
    }

    pub(super) fn react_value_intrinsic(
        &mut self,
        file_id: FileId,
        term_id: TermItemId,
    ) -> ConversionResult<Option<ExpressionId>> {
        let Some(intrinsic) = self.react_intrinsic_identity(file_id, term_id)? else {
            return Ok(None);
        };
        if matches!(intrinsic, ReactIntrinsic::Empty) {
            return Ok(Some(self.expression(ExpressionKind::React(ReactExpression::Empty))));
        }
        Ok(None)
    }

    fn react_intrinsic_identity(
        &self,
        file_id: FileId,
        term_id: TermItemId,
    ) -> QueryResult<Option<ReactIntrinsic>> {
        if self.react_module().0 != Some(file_id) {
            return Ok(None);
        }
        let indexed = self.indexed_module(file_id)?;
        let intrinsic = match indexed.items[term_id].name.as_deref() {
            Some("component") => ReactIntrinsic::Component,
            Some("element") => ReactIntrinsic::Element,
            Some("elementKeyed") => ReactIntrinsic::ElementKeyed,
            Some("intrinsic") => ReactIntrinsic::Intrinsic,
            Some("intrinsicKeyed") => ReactIntrinsic::IntrinsicKeyed,
            Some("text") => ReactIntrinsic::Text,
            Some("array") => ReactIntrinsic::Array,
            Some("fragment") => ReactIntrinsic::Fragment,
            Some("empty") => ReactIntrinsic::Empty,
            _ => return Ok(None),
        };
        Ok(Some(intrinsic))
    }

    pub(super) fn validate_react_uses(&self, declarations: &[Declaration]) -> ConversionResult<()> {
        for declaration in declarations {
            let DeclarationKind::Value(root) = declaration.kind else { continue };
            self.validate_react_expression(root, root, declaration)?;
        }
        Ok(())
    }

    pub(super) fn materialize_react_functions(
        &mut self,
        declarations: &[Declaration],
    ) -> ConversionResult<()> {
        let roots = declarations.iter().filter_map(|declaration| match declaration.kind {
            DeclarationKind::Value(root) => Some((root, declaration.global.id)),
            _ => None,
        });
        let mut pending = roots.collect::<Vec<_>>();
        let mut visited = FxHashSet::default();
        while let Some((expression, declaration)) = pending.pop() {
            if !visited.insert(expression) {
                continue;
            }
            if let ExpressionKind::Global { global } = &self.storage[expression].kind
                && let GlobalId::Term(file_id, term_id) = global.id
                && let Some(intrinsic) = self.react_intrinsic_identity(file_id, term_id)?
            {
                let names: &[&str] = match intrinsic {
                    ReactIntrinsic::Component => {
                        return Err(self
                            .unsupported(UnsupportedState::InvalidReactComponent { declaration }));
                    }
                    ReactIntrinsic::Element | ReactIntrinsic::Intrinsic => {
                        &["dictionary", "target", "props"]
                    }
                    ReactIntrinsic::ElementKeyed | ReactIntrinsic::IntrinsicKeyed => {
                        &["dictionary", "key", "target", "props"]
                    }
                    ReactIntrinsic::Text | ReactIntrinsic::Array => &["value"],
                    ReactIntrinsic::Fragment => &["children"],
                    ReactIntrinsic::Empty => &[],
                };
                let mut parameters = Vec::new();
                let mut arguments = Vec::new();
                for name in names {
                    let parameter = self.fresh_parameter((*name).into())?;
                    arguments.push(
                        self.expression(ExpressionKind::Local { parameter: parameter.clone() }),
                    );
                    parameters.push(parameter);
                }
                let body = self
                    .react_intrinsic(expression, &arguments)?
                    .expect("React intrinsic arity mismatch");
                let function = self.parameter_abstraction(parameters, body);
                let kind = self.storage[function].kind.clone();
                self.storage.replace_expression_kind(expression, kind);
            } else {
                for_each_expression_child(&self.storage[expression].kind, |child| {
                    pending.push((child, declaration))
                });
            }
        }
        Ok(())
    }

    fn validate_react_expression(
        &self,
        expression: ExpressionId,
        root: ExpressionId,
        declaration: &Declaration,
    ) -> ConversionResult<()> {
        if let ExpressionKind::React(ReactExpression::Component { .. }) =
            &self.storage[expression].kind
            && (expression != root || declaration.recursive_group.is_some())
        {
            return Err(self.unsupported(UnsupportedState::InvalidReactComponent {
                declaration: declaration.global.id,
            }));
        }
        let mut result = Ok(());
        for_each_expression_child(&self.storage[expression].kind, |child| {
            if result.is_ok() {
                result = self.validate_react_expression(child, root, declaration);
            }
        });
        result
    }

    fn react_module(&self) -> ReactModule {
        *self.react_module.get_or_init(|| ReactModule(self.queries.module_file("Iris.React")))
    }

    pub(super) fn react_module_is_virtual(&self, file_id: FileId) -> bool {
        self.react_module().0 == Some(file_id)
    }
}
