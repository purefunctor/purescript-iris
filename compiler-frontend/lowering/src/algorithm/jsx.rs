use rustc_hash::FxHashSet;
use smol_str::SmolStr;
use stabilizing::ExpectId;
use syntax::cst;

use super::{Context, State, recursive};
use crate::{
    ExpressionId, ExpressionKind, ExpressionRecordItem, JsxElementKind, LoweringError, NotInScope,
    StringLiteral, TermVariableResolution,
};

pub(super) fn lower_element(
    state: &mut State,
    context: &Context,
    element: &cst::ExpressionJsxElement,
) -> ExpressionKind {
    let expression = cst::Expression::ExpressionJsxElement(element.clone());
    let id = context.stabilized.lookup_cst(&expression).expect_id();
    let opening = element.opening();
    let name = opening.as_ref().and_then(|opening| opening.name_token());
    let name = name.as_ref().map(|name| name.text(context.source));

    if let Some(closing) = element.closing() {
        let closing_name = closing.name_token();
        let closing_name = closing_name.as_ref().map(|name| name.text(context.source));
        if name != closing_name {
            state.errors.push(LoweringError::JsxTagMismatch {
                id: context.stabilized.lookup_cst(&closing).expect_id(),
                expected: name.map(SmolStr::from),
            });
        }
    }

    let (kind, resolution) = match name {
        None => (JsxElementKind::Fragment, None),
        Some(name) if name.contains('.') || name.starts_with(char::is_uppercase) => {
            let (qualifier, name) = name
                .rsplit_once('.')
                .map_or((None, name), |(qualifier, name)| (Some(qualifier), name));
            let mut characters = name.chars();
            let name = characters
                .next()
                .into_iter()
                .flat_map(char::to_lowercase)
                .chain(characters)
                .collect::<String>();
            let resolution = resolve_function(state, context, id, qualifier, &name);
            (JsxElementKind::Component, resolution)
        }
        Some(name) => (JsxElementKind::Intrinsic(name.into()), None),
    };

    let mut names = FxHashSet::default();
    let has_children = element.children().is_some_and(|children| {
        children.children().any(|child| match child {
            cst::Expression::ExpressionJsxText(text) => {
                !text_value(context, &text).as_utf16().is_empty()
            }
            _ => true,
        })
    });
    if has_children {
        names.insert(SmolStr::from("children"));
    }
    let attributes = opening.iter().flat_map(cst::JsxOpening::attributes).map(|attribute| {
        let name = attribute.name_token().map(|name| SmolStr::from(name.text(context.source)));
        if let Some(name) = &name
            && !names.insert(name.clone())
        {
            state.errors.push(LoweringError::DuplicateJsxAttribute {
                id: context.stabilized.lookup_cst(&attribute).expect_id(),
                name: name.clone(),
            });
        }
        let value =
            attribute.expression().map(|value| recursive::lower_expression(state, context, &value));
        ExpressionRecordItem::RecordField { name, value }
    });
    let attributes = attributes.collect();

    let children = element.children();
    let children = children.iter().flat_map(cst::JsxChildren::children).filter_map(|child| {
        if let cst::Expression::ExpressionJsxText(text) = &child
            && text_value(context, text).as_utf16().is_empty()
        {
            return None;
        }
        Some(recursive::lower_expression(state, context, &child))
    });
    let children = children.collect();

    ExpressionKind::JsxElement { kind, resolution, attributes, children }
}

pub(super) fn lower_text(context: &Context, text: &cst::ExpressionJsxText) -> ExpressionKind {
    let value = text_value(context, text);
    ExpressionKind::JsxText { value }
}

fn resolve_function(
    state: &mut State,
    context: &Context,
    id: ExpressionId,
    qualifier: Option<&str>,
    name: &str,
) -> Option<TermVariableResolution> {
    let resolution = state.resolve_term_full(context, qualifier, name);
    if resolution.is_none() {
        let name =
            qualifier.map_or_else(|| name.into(), |qualifier| format!("{qualifier}.{name}").into());
        state.errors.push(LoweringError::NotInScope(NotInScope::JsxFunction { id, name }));
    }
    resolution
}

fn text_value(context: &Context, text: &cst::ExpressionJsxText) -> StringLiteral {
    let Some(token) = text.text_token() else { return StringLiteral::from("") };
    let text = token.text(context.source);
    let mut result = String::new();
    let mut lines = text.split('\n').peekable();
    let mut first = true;
    while let Some(line) = lines.next() {
        let line = line.trim_end_matches('\r');
        let line = if first { line } else { line.trim_start_matches([' ', '\t']) };
        let line = if lines.peek().is_none() { line } else { line.trim_end_matches([' ', '\t']) };
        if !line.is_empty() {
            if !result.is_empty() {
                result.push(' ');
            }
            result.push_str(line);
        }
        first = false;
    }
    StringLiteral::from(result)
}
