use building_types::QueryResult;
use lowering::{
    ExpressionId, ExpressionRecordItem, JsxElementKind, StringLiteral, TermVariableResolution,
};

use super::{
    ElaboratedExpression, allocate_error_expression, allocate_expression, application, collections,
};
use crate::context::CheckContext;
use crate::core::{toolkit, unification};
use crate::state::CheckState;
use crate::{ExternalQueries, tree};

enum Argument<'a> {
    String(StringLiteral),
    Record(&'a [ExpressionRecordItem], &'a [ExpressionId]),
    Array(&'a [ExpressionId]),
    Source(ExpressionId),
    Value(ElaboratedExpression),
}

pub(super) fn infer_element<Q: ExternalQueries>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    kind: &JsxElementKind,
    resolution: Option<TermVariableResolution>,
    attributes: &[ExpressionRecordItem],
    children: &[ExpressionId],
) -> QueryResult<ElaboratedExpression> {
    if matches!(kind, JsxElementKind::Fragment) {
        return infer_application(state, context, "fragment", [Argument::Array(children)]);
    }
    let mut props = Vec::new();
    let mut key = None;
    for attribute in attributes {
        if let ExpressionRecordItem::RecordField { name: Some(name), value } = attribute
            && name == "key"
        {
            key = *value;
        } else {
            props.push(attribute.clone());
        }
    }
    let mut arguments = Vec::new();
    if let Some(key) = key {
        arguments.push(Argument::Source(key));
    }
    let function = match kind {
        JsxElementKind::Intrinsic(tag) => {
            arguments.push(Argument::String(tag.clone().into()));
            if key.is_some() { "intrinsicKeyed" } else { "intrinsic" }
        }
        JsxElementKind::Component => {
            let Some(resolution) = resolution else {
                return Ok(allocate_error_expression(
                    state,
                    context.unknown("missing JSX component"),
                ));
            };
            let type_id = toolkit::lookup_term_variable(state, context, resolution)?;
            let resolution = tree::VariableResolution::Source(resolution);
            let component =
                allocate_expression(state, type_id, tree::ExpressionKind::Variable { resolution });
            arguments.push(Argument::Value(component));
            if key.is_some() { "elementKeyed" } else { "element" }
        }
        JsxElementKind::Fragment => unreachable!(),
    };
    arguments.push(Argument::Record(&props, children));
    infer_application(state, context, function, arguments)
}

pub(super) fn infer_text<Q: ExternalQueries>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    value: &StringLiteral,
) -> QueryResult<ElaboratedExpression> {
    infer_application(state, context, "text", [Argument::String(value.clone())])
}

fn react_function<Q: ExternalQueries>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    name: &str,
) -> QueryResult<ElaboratedExpression> {
    let file_id = context.queries.module_file("Iris.React").expect("missing built-in Iris.React");
    let indexed = context.queries.indexed(file_id)?;
    let (term_id, _) = indexed
        .items
        .iter_terms()
        .find(|(_, item)| item.name.as_deref() == Some(name))
        .expect("missing built-in React function");
    let type_id = toolkit::lookup_file_term(state, context, file_id, term_id)?;
    let resolution = TermVariableResolution::Reference(file_id, term_id);
    let resolution = tree::VariableResolution::Source(resolution);
    Ok(allocate_expression(state, type_id, tree::ExpressionKind::Variable { resolution }))
}

fn infer_application<'a, Q: ExternalQueries>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    name: &str,
    arguments: impl IntoIterator<Item = Argument<'a>>,
) -> QueryResult<ElaboratedExpression> {
    let mut function = react_function(state, context, name)?;

    // Check generated records against each parameter rather than inferring them
    // first: a component may require a rank-polymorphic property. Materializing
    // the normal application also retains evidence for constrained components.
    for value in arguments {
        let Some(application::UnanchoredApplication { implicit, argument, result }) =
            application::check_unanchored_application(state, context, function.type_id)?
        else {
            let argument = state.fresh_unification(context.queries, context.prim.t);
            let result = state.fresh_unification(context.queries, context.prim.t);
            let expected = context.intern_function(argument, result);
            unification::unify(state, context, function.type_id, expected)?;
            return Ok(allocate_error_expression(state, result));
        };
        let value = super::check_expected_expression(
            state,
            context,
            argument,
            |state, argument| match value {
                Argument::String(value) => {
                    let kind =
                        tree::ExpressionKind::String { kind: lowering::StringKind::String, value };
                    let value = allocate_expression(state, context.prim.string, kind);
                    application::subtype_expression(state, context, value, argument)
                }
                Argument::Record(record, children) => {
                    let children = match children {
                        [] => None,
                        [child] => {
                            let jsx = react_function(state, context, "empty")?.type_id;
                            Some(super::check_expression(state, context, *child, jsx)?)
                        }
                        _ => Some(infer_application(
                            state,
                            context,
                            "array",
                            [Argument::Array(children)],
                        )?),
                    };
                    collections::check_record_with_field(
                        state,
                        context,
                        record,
                        argument,
                        children.map(|children| ("children", children)),
                    )
                }
                Argument::Array(array) => collections::check_array(state, context, array, argument),
                Argument::Source(source) => {
                    super::check_expression(state, context, source, argument)
                }
                Argument::Value(value) => {
                    super::check_elaborated_expression(state, context, value, argument)
                }
            },
        )?;
        function = application::materialize_application(state, function, implicit, result, value);
    }
    Ok(function)
}
