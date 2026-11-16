use std::sync::Arc;

use building_types::QueryResult;
use lowering::{ExpressionArgument, ExpressionKind, ExpressionRecordItem, TermVariableResolution};
use rustc_hash::FxHashSet;
use smol_str::SmolStr;
use sugar::OperatorTree;

use crate::ExternalQueries;
use crate::context::{CheckContext, TermItem};
use crate::core::{kind, normalise, skolem, subsume, unify};
use crate::error::{ErrorCrumb, ErrorKind};
use crate::source::{binders, equations, types};
use crate::state::{CheckState, Hole, LocalVariable, TermVariableKey};
use crate::tree::{self, Literal, TermReference};
use crate::types::{RowField, Type, TypeId};

#[derive(Debug, Clone, Copy)]
pub struct Elaborated {
    pub expression: tree::ExpressionId,
    pub type_id: TypeId,
}

pub fn allocate(
    state: &mut CheckState,
    source: Option<lowering::ExpressionId>,
    type_id: TypeId,
    kind: tree::ExpressionKind,
) -> Elaborated {
    let expression = state.checked.tree.allocate_expression(source, type_id, kind);
    Elaborated { expression, type_id }
}

pub fn error_expression<Q>(state: &mut CheckState, context: &CheckContext<Q>) -> Elaborated
where
    Q: ExternalQueries,
{
    allocate(state, None, context.types.unknown, tree::ExpressionKind::Error)
}

pub fn lookup_term_type<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    (file_id, item_id): TermItem,
) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    if file_id == context.id {
        if let Some(&pending) = state.pending_terms.get(&item_id) {
            return Ok(pending);
        }
        let checked = state.checked.term_types.get(&item_id).copied();
        return Ok(checked.unwrap_or(context.types.unknown));
    }
    let checked = context.checked_dependency(file_id)?;
    let checked = checked.term_types.get(&item_id).copied();
    Ok(checked.unwrap_or(context.types.unknown))
}

pub fn operator_target<Q>(
    context: &CheckContext<Q>,
    (file_id, item_id): TermItem,
) -> QueryResult<Option<TermItem>>
where
    Q: ExternalQueries,
{
    let lowered = context.lowered_dependency(file_id)?;
    match lowered.tree.get_term_item_kind(item_id) {
        Some(lowering::TermItemKind::Operator { resolution, .. }) => Ok(*resolution),
        _ => Ok(Some((file_id, item_id))),
    }
}

fn is_constructor<Q>(context: &CheckContext<Q>, (file_id, item_id): TermItem) -> QueryResult<bool>
where
    Q: ExternalQueries,
{
    let lowered = context.lowered_dependency(file_id)?;
    let kind = lowered.tree.get_term_item_kind(item_id);
    Ok(matches!(kind, Some(lowering::TermItemKind::Constructor { .. })))
}

pub fn global_reference<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    source: Option<lowering::ExpressionId>,
    item: TermItem,
) -> QueryResult<Elaborated>
where
    Q: ExternalQueries,
{
    let Some(target) = operator_target(context, item)? else {
        return Ok(error_expression(state, context));
    };
    let type_id = lookup_term_type(state, context, target)?;
    let (file_id, item_id) = target;
    let reference = if is_constructor(context, target)? {
        TermReference::Constructor(file_id, item_id)
    } else {
        TermReference::Global(file_id, item_id)
    };
    Ok(allocate(state, source, type_id, tree::ExpressionKind::Reference(reference)))
}

pub fn variable_reference<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    source: Option<lowering::ExpressionId>,
    resolution: TermVariableResolution,
) -> QueryResult<Elaborated>
where
    Q: ExternalQueries,
{
    let key = match resolution {
        TermVariableResolution::Reference(file_id, item_id) => {
            return global_reference(state, context, source, (file_id, item_id));
        }
        TermVariableResolution::Binder(binder) => TermVariableKey::Binder(binder),
        TermVariableResolution::Let(group) => TermVariableKey::Let(group),
        TermVariableResolution::RecordPun(pun) => TermVariableKey::Pun(pun),
    };
    let Some(LocalVariable { type_id, reference }) = state.term_variables.get(&key).copied() else {
        return Ok(error_expression(state, context));
    };
    Ok(allocate(state, source, type_id, tree::ExpressionKind::Reference(reference)))
}

pub fn instantiate<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    elaborated: Elaborated,
) -> QueryResult<Elaborated>
where
    Q: ExternalQueries,
{
    let head = normalise::normalise(state, context, elaborated.type_id);
    if !matches!(context.lookup_type(head), Type::Forall(..) | Type::Constrained(..)) {
        return Ok(elaborated);
    }
    let instantiated = skolem::instantiate(state, context, head)?;
    let evidence = instantiated.constraints.iter().map(|&constraint| state.emit_wanted(constraint));
    let evidence = evidence.collect::<Arc<[_]>>();
    let kind = tree::ExpressionKind::Instantiate {
        expression: elaborated.expression,
        type_arguments: Arc::from(instantiated.arguments),
        evidence,
    };
    Ok(allocate(state, None, instantiated.inner, kind))
}

pub fn infer_expression<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: lowering::ExpressionId,
) -> QueryResult<Elaborated>
where
    Q: ExternalQueries,
{
    state.with_crumb(ErrorCrumb::Expression(id), |state| {
        let Some(sections) = context.sectioned.expressions.get(&id) else {
            return infer_expression_core(state, context, id);
        };
        let parameters = sections.iter().map(|&section| {
            let parameter = kind::fresh_type(state, context);
            let binder_kind = tree::BinderKind::Variable(SmolStr::new_static("_"));
            let binder = state.checked.tree.allocate_binder(None, parameter, binder_kind);
            let reference = TermReference::Binder(binder);
            let variable = LocalVariable { type_id: parameter, reference };
            state.term_variables.insert(TermVariableKey::Section(section), variable);
            (parameter, binder)
        });
        let (parameters, binders): (Vec<_>, Vec<_>) = parameters.unzip();

        let body = infer_expression_core(state, context, id)?;
        let body = instantiate(state, context, body)?;

        let lambda =
            parameters.iter().zip(&binders).rev().fold(body, |body, (&parameter, &binder)| {
                let type_id = context.intern_function(parameter, body.type_id);
                let kind = tree::ExpressionKind::Lambda { binder, body: body.expression };
                allocate(state, None, type_id, kind)
            });
        Ok(lambda)
    })
}

pub fn check_expression<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: lowering::ExpressionId,
    expected: TypeId,
) -> QueryResult<tree::ExpressionId>
where
    Q: ExternalQueries,
{
    let expected_head = normalise::normalise(state, context, expected);
    if let Type::Forall(..) | Type::Constrained(..) = context.lookup_type(expected_head) {
        return subsume::abstract_expression(state, context, expected_head, |state, inner| {
            check_expression(state, context, id, inner)
        });
    }

    state.with_crumb(ErrorCrumb::Expression(id), |state| {
        if context.sectioned.expressions.contains_key(&id) {
            let inferred = infer_expression(state, context, id)?;
            return subsume::subsume_expression(
                state,
                context,
                inferred.expression,
                inferred.type_id,
                expected_head,
            );
        }
        check_expression_core(state, context, id, expected_head)
    })
}

fn check_expression_core<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: lowering::ExpressionId,
    expected: TypeId,
) -> QueryResult<tree::ExpressionId>
where
    Q: ExternalQueries,
{
    let Some(kind) = context.lowered.tree.get_expression_kind(id) else {
        return Ok(error_expression(state, context).expression);
    };

    if let Type::Unification(_) = context.lookup_type(expected) {
        let inferred = infer_expression_core(state, context, id)?;
        let inferred = instantiate(state, context, inferred)?;
        unify::unify(state, context, expected, inferred.type_id)?;
        return Ok(inferred.expression);
    }

    match kind {
        ExpressionKind::Lambda { binders, expression } => {
            let lambda = check_lambda(state, context, binders, *expression, expected)?;
            state.checked.tree.expressions[lambda].source = Some(id);
            Ok(lambda)
        }

        ExpressionKind::IfThenElse { if_, then, else_ } => {
            let condition = check_optional(state, context, *if_, context.types.boolean)?;
            let then = check_optional(state, context, *then, expected)?;
            let else_ = check_optional(state, context, *else_, expected)?;
            let kind = tree::ExpressionKind::IfThenElse { condition, then, else_ };
            Ok(allocate(state, Some(id), expected, kind).expression)
        }

        ExpressionKind::CaseOf { trunk, branches } => {
            equations::check_case(state, context, id, trunk, branches, expected)
        }

        ExpressionKind::LetIn { bindings, expression } => {
            let bindings = equations::check_let_chunks(state, context, bindings)?;
            let body = check_optional(state, context, *expression, expected)?;
            let kind = tree::ExpressionKind::Let { bindings: Arc::from(bindings), body };
            Ok(allocate(state, Some(id), expected, kind).expression)
        }

        ExpressionKind::Parenthesized { parenthesized: Some(inner) } => {
            check_expression(state, context, *inner, expected)
        }

        ExpressionKind::Array { array } => {
            let array_element = array_element(state, context, expected);
            let Some(element) = array_element else {
                return check_by_inference(state, context, id, expected);
            };
            let elements = array
                .iter()
                .map(|&element_id| check_expression(state, context, element_id, element));
            let elements = elements.collect::<QueryResult<Vec<_>>>()?;
            let kind = tree::ExpressionKind::Array(Arc::from(elements));
            Ok(allocate(state, Some(id), expected, kind).expression)
        }

        ExpressionKind::Record { record } => {
            let Some(row) = record_row(state, context, expected) else {
                return check_by_inference(state, context, id, expected);
            };
            let row = normalise::row_parts(state, context, row);
            if row.tail.is_some() {
                return check_by_inference(state, context, id, expected);
            }
            check_record(state, context, id, record, &row.fields, expected)
        }

        ExpressionKind::Hole => {
            let name = hole_name(context, id);
            let crumbs = Vec::clone(&state.crumbs);
            state.holes.push(Hole { name: SmolStr::clone(&name), type_id: expected, crumbs });
            Ok(allocate(state, Some(id), expected, tree::ExpressionKind::Hole(name)).expression)
        }

        _ => check_by_inference(state, context, id, expected),
    }
}

fn check_by_inference<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: lowering::ExpressionId,
    expected: TypeId,
) -> QueryResult<tree::ExpressionId>
where
    Q: ExternalQueries,
{
    let inferred = infer_expression_core(state, context, id)?;
    subsume::subsume_expression(state, context, inferred.expression, inferred.type_id, expected)
}

fn check_record<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: lowering::ExpressionId,
    record: &[ExpressionRecordItem],
    fields: &[RowField],
    expected: TypeId,
) -> QueryResult<tree::ExpressionId>
where
    Q: ExternalQueries,
{
    let mut seen = FxHashSet::default();
    let mut checked = Vec::with_capacity(record.len());
    for item in record {
        let (label, value) = match item {
            ExpressionRecordItem::RecordField { name: Some(name), value, .. } => {
                (SmolStr::clone(name), RecordValue::Expression(*value))
            }
            ExpressionRecordItem::RecordPun { name: Some(name), resolution, .. } => {
                (SmolStr::clone(name), RecordValue::Pun(*resolution))
            }
            _ => continue,
        };
        if !seen.insert(SmolStr::clone(&label)) {
            state.insert_error(ErrorKind::DuplicateLabel { label });
            continue;
        }
        let Some(field) = fields.iter().find(|field| field.label == label) else {
            state.insert_error(ErrorKind::AdditionalProperty { label });
            continue;
        };
        let expression = match value {
            RecordValue::Expression(Some(value)) => {
                check_expression(state, context, value, field.id)?
            }
            RecordValue::Pun(Some(resolution)) => {
                let reference = variable_reference(state, context, None, resolution)?;
                subsume::subsume_expression(
                    state,
                    context,
                    reference.expression,
                    reference.type_id,
                    field.id,
                )?
            }
            _ => error_expression(state, context).expression,
        };
        checked.push((label, expression));
    }

    for field in fields {
        if !seen.contains(&field.label) {
            let label = SmolStr::clone(&field.label);
            state.insert_error(ErrorKind::PropertyIsMissing { label });
        }
    }

    let kind = tree::ExpressionKind::Record(Arc::from(checked));
    Ok(allocate(state, Some(id), expected, kind).expression)
}

enum RecordValue {
    Expression(Option<lowering::ExpressionId>),
    Pun(Option<TermVariableResolution>),
}

fn hole_name<Q>(context: &CheckContext<Q>, id: lowering::ExpressionId) -> SmolStr
where
    Q: ExternalQueries,
{
    let _ = (context, id);
    SmolStr::new_static("?hole")
}

fn record_row<Q>(state: &CheckState, context: &CheckContext<Q>, id: TypeId) -> Option<TypeId>
where
    Q: ExternalQueries,
{
    let id = normalise::normalise(state, context, id);
    let Type::Application(function, row) = context.lookup_type(id) else {
        return None;
    };
    let function = normalise::normalise(state, context, function);
    (function == context.types.record).then_some(row)
}

fn array_element<Q>(state: &CheckState, context: &CheckContext<Q>, id: TypeId) -> Option<TypeId>
where
    Q: ExternalQueries,
{
    let id = normalise::normalise(state, context, id);
    let Type::Application(function, element) = context.lookup_type(id) else {
        return None;
    };
    let function = normalise::normalise(state, context, function);
    (function == context.types.array).then_some(element)
}

fn check_lambda<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    binders: &[lowering::BinderId],
    body: Option<lowering::ExpressionId>,
    expected: TypeId,
) -> QueryResult<tree::ExpressionId>
where
    Q: ExternalQueries,
{
    let Some((&binder, rest)) = binders.split_first() else {
        return check_optional(state, context, body, expected);
    };

    let expected_head = normalise::normalise(state, context, expected);
    if let Type::Forall(..) | Type::Constrained(..) = context.lookup_type(expected_head) {
        return subsume::abstract_expression(state, context, expected_head, |state, inner| {
            check_lambda(state, context, binders, body, inner)
        });
    }

    let (argument, result) = expect_function(state, context, expected_head)?;
    let binder = binders::check_binder(state, context, binder, argument)?;
    let body = check_lambda(state, context, rest, body, result)?;
    let kind = tree::ExpressionKind::Lambda { binder, body };
    Ok(state.checked.tree.allocate_expression(None, expected_head, kind))
}

pub fn expect_function<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: TypeId,
) -> QueryResult<(TypeId, TypeId)>
where
    Q: ExternalQueries,
{
    if let Some(parts) = normalise::function_parts(state, context, id) {
        return Ok(parts);
    }
    let argument = kind::fresh_type(state, context);
    let result = kind::fresh_type(state, context);
    let function = context.intern_function(argument, result);
    unify::unify(state, context, id, function)?;
    Ok((argument, result))
}

fn check_optional<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: Option<lowering::ExpressionId>,
    expected: TypeId,
) -> QueryResult<tree::ExpressionId>
where
    Q: ExternalQueries,
{
    match id {
        Some(id) => check_expression(state, context, id, expected),
        None => Ok(error_expression(state, context).expression),
    }
}

fn infer_optional<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: Option<lowering::ExpressionId>,
) -> QueryResult<Elaborated>
where
    Q: ExternalQueries,
{
    match id {
        Some(id) => infer_expression(state, context, id),
        None => Ok(error_expression(state, context)),
    }
}

fn literal<Q>(
    state: &mut CheckState,
    id: lowering::ExpressionId,
    type_id: TypeId,
    literal: Option<Literal>,
    context: &CheckContext<Q>,
) -> Elaborated
where
    Q: ExternalQueries,
{
    match literal {
        Some(literal) => allocate(state, Some(id), type_id, tree::ExpressionKind::Literal(literal)),
        None => allocate(state, Some(id), context.types.unknown, tree::ExpressionKind::Error),
    }
}

fn infer_expression_core<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: lowering::ExpressionId,
) -> QueryResult<Elaborated>
where
    Q: ExternalQueries,
{
    let Some(kind) = context.lowered.tree.get_expression_kind(id) else {
        return Ok(error_expression(state, context));
    };

    match kind {
        ExpressionKind::Typed { expression, type_ } => {
            let Some(type_) = *type_ else {
                return infer_optional(state, context, *expression);
            };
            let annotation = types::check_type(state, context, type_, context.types.type_)?;
            let expression = check_optional(state, context, *expression, annotation)?;
            Ok(Elaborated { expression, type_id: annotation })
        }

        ExpressionKind::OperatorChain { .. } => {
            let Some(Ok(tree)) = context.bracketed.expressions.get(&id) else {
                return Ok(error_expression(state, context));
            };
            infer_operator_tree(state, context, tree)
        }

        ExpressionKind::InfixChain { head, tail } => {
            let mut left = infer_optional(state, context, *head)?;
            for pair in tail.iter() {
                let function = infer_optional(state, context, pair.tick)?;
                let applied = apply_elaborated(state, context, function, left)?;
                let right = match pair.element {
                    Some(element) => Argument::Expression(element),
                    None => Argument::Missing,
                };
                left = apply(state, context, applied, right)?;
            }
            Ok(left)
        }

        ExpressionKind::Negate { negate, expression } => {
            let Some(negate) = *negate else {
                return Ok(error_expression(state, context));
            };
            let function = variable_reference(state, context, None, negate)?;
            let argument = match expression {
                Some(expression) => Argument::Expression(*expression),
                None => Argument::Missing,
            };
            apply(state, context, function, argument)
        }

        ExpressionKind::Application { function, arguments } => {
            let mut function = infer_optional(state, context, *function)?;
            for argument in arguments.iter() {
                function = match argument {
                    ExpressionArgument::Term(Some(term)) => {
                        apply(state, context, function, Argument::Expression(*term))?
                    }
                    ExpressionArgument::Term(None) => {
                        apply(state, context, function, Argument::Missing)?
                    }
                    ExpressionArgument::Type(type_) => {
                        apply_type(state, context, function, *type_)?
                    }
                };
            }
            Ok(function)
        }

        ExpressionKind::IfThenElse { if_, then, else_ } => {
            let condition = check_optional(state, context, *if_, context.types.boolean)?;
            let then = infer_optional(state, context, *then)?;
            let then = instantiate(state, context, then)?;
            let else_ = infer_optional(state, context, *else_)?;
            let else_ = instantiate(state, context, else_)?;
            unify::unify(state, context, then.type_id, else_.type_id)?;
            let kind = tree::ExpressionKind::IfThenElse {
                condition,
                then: then.expression,
                else_: else_.expression,
            };
            Ok(allocate(state, Some(id), then.type_id, kind))
        }

        ExpressionKind::LetIn { bindings, expression } => {
            let bindings = equations::check_let_chunks(state, context, bindings)?;
            let body = infer_optional(state, context, *expression)?;
            let kind =
                tree::ExpressionKind::Let { bindings: Arc::from(bindings), body: body.expression };
            Ok(allocate(state, Some(id), body.type_id, kind))
        }

        ExpressionKind::Lambda { binders, expression } => {
            let mut parameters = Vec::with_capacity(binders.len());
            for &binder in binders.iter() {
                let argument = kind::fresh_type(state, context);
                let binder = binders::check_binder(state, context, binder, argument)?;
                parameters.push((binder, argument));
            }
            let body = infer_optional(state, context, *expression)?;
            let lambda = parameters.into_iter().rev().fold(body, |body, (binder, argument)| {
                let type_id = context.intern_function(argument, body.type_id);
                let kind = tree::ExpressionKind::Lambda { binder, body: body.expression };
                allocate(state, None, type_id, kind)
            });
            state.checked.tree.expressions[lambda.expression].source = Some(id);
            Ok(lambda)
        }

        ExpressionKind::CaseOf { trunk, branches } => {
            let result = kind::fresh_type(state, context);
            let expression = equations::check_case(state, context, id, trunk, branches, result)?;
            Ok(Elaborated { expression, type_id: result })
        }

        ExpressionKind::Do { bind, discard, statements } => {
            equations::infer_do(state, context, id, *bind, *discard, statements)
        }

        ExpressionKind::Ado { map, apply, pure, statements, expression } => {
            equations::infer_ado(state, context, id, (*map, *apply, *pure), statements, *expression)
        }

        ExpressionKind::Constructor { resolution }
        | ExpressionKind::OperatorName { resolution } => {
            let Some(resolution) = *resolution else {
                return Ok(error_expression(state, context));
            };
            global_reference(state, context, Some(id), resolution)
        }

        ExpressionKind::Variable { resolution } => {
            let Some(resolution) = *resolution else {
                return Ok(error_expression(state, context));
            };
            variable_reference(state, context, Some(id), resolution)
        }

        ExpressionKind::Section => {
            let key = TermVariableKey::Section(id);
            let Some(LocalVariable { type_id, reference }) =
                state.term_variables.get(&key).copied()
            else {
                return Ok(error_expression(state, context));
            };
            Ok(allocate(state, Some(id), type_id, tree::ExpressionKind::Reference(reference)))
        }

        ExpressionKind::Hole => {
            let type_id = kind::fresh_type(state, context);
            let name = hole_name(context, id);
            let crumbs = Vec::clone(&state.crumbs);
            state.holes.push(Hole { name: SmolStr::clone(&name), type_id, crumbs });
            Ok(allocate(state, Some(id), type_id, tree::ExpressionKind::Hole(name)))
        }

        ExpressionKind::String { value, .. } => {
            let value = value.as_ref().map(|value| {
                Literal::String(SmolStr::from(String::from_utf16_lossy(value.as_utf16())))
            });
            Ok(literal(state, id, context.types.string, value, context))
        }

        ExpressionKind::Char { value } => {
            Ok(literal(state, id, context.types.char, value.map(Literal::Char), context))
        }

        ExpressionKind::Boolean { boolean } => {
            Ok(literal(state, id, context.types.boolean, Some(Literal::Boolean(*boolean)), context))
        }

        ExpressionKind::Integer { value } => {
            Ok(literal(state, id, context.types.int, value.map(Literal::Integer), context))
        }

        ExpressionKind::Number { value } => {
            let value = value.as_ref().map(|value| Literal::Number(SmolStr::clone(value)));
            Ok(literal(state, id, context.types.number, value, context))
        }

        ExpressionKind::Array { array } => {
            let element = kind::fresh_type(state, context);
            let mut elements = Vec::with_capacity(array.len());
            for &element_id in array.iter() {
                let inferred = infer_expression(state, context, element_id)?;
                let inferred = instantiate(state, context, inferred)?;
                unify::unify(state, context, element, inferred.type_id)?;
                elements.push(inferred.expression);
            }
            let type_id = context.intern_application(context.types.array, element);
            let kind = tree::ExpressionKind::Array(Arc::from(elements));
            Ok(allocate(state, Some(id), type_id, kind))
        }

        ExpressionKind::Record { record } => infer_record(state, context, id, record),

        ExpressionKind::Parenthesized { parenthesized } => {
            infer_optional(state, context, *parenthesized)
        }

        ExpressionKind::RecordAccess { record, labels } => {
            let mut current = infer_optional(state, context, *record)?;
            let labels = labels.as_deref().unwrap_or_default();
            for label in labels {
                current = instantiate(state, context, current)?;
                let field = kind::fresh_type(state, context);
                let rest = kind::fresh_unification(state, context, context.types.row_type);
                let field_row = RowField { label: SmolStr::clone(&label.name), id: field };
                let row = context.intern_row(vec![field_row], Some(rest));
                let record_type = context.intern_record(row);
                unify::unify(state, context, record_type, current.type_id)?;
                let kind = tree::ExpressionKind::RecordAccess {
                    record: current.expression,
                    label: SmolStr::clone(&label.name),
                };
                current = allocate(state, None, field, kind);
            }
            state.checked.tree.expressions[current.expression].source = Some(id);
            Ok(current)
        }

        ExpressionKind::RecordUpdate { record, updates } => {
            let record = infer_optional(state, context, *record)?;
            let record = instantiate(state, context, record)?;
            let (input, output, updates) = check_record_updates(state, context, updates)?;
            let input_type = context.intern_record(input);
            unify::unify(state, context, input_type, record.type_id)?;
            let output_type = context.intern_record(output);
            let kind = tree::ExpressionKind::RecordUpdate {
                record: record.expression,
                updates: Arc::from(updates),
            };
            Ok(allocate(state, Some(id), output_type, kind))
        }
    }
}

fn infer_record<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: lowering::ExpressionId,
    record: &[ExpressionRecordItem],
) -> QueryResult<Elaborated>
where
    Q: ExternalQueries,
{
    let mut seen = FxHashSet::default();
    let mut fields = Vec::with_capacity(record.len());
    let mut checked = Vec::with_capacity(record.len());
    for item in record {
        let (label, inferred) = match item {
            ExpressionRecordItem::RecordField { name: Some(name), value, .. } => {
                let inferred = infer_optional(state, context, *value)?;
                let should_instantiate =
                    value.is_some_and(|value| property_should_instantiate(context, value));
                let inferred = if should_instantiate {
                    instantiate(state, context, inferred)?
                } else {
                    inferred
                };
                (SmolStr::clone(name), inferred)
            }
            ExpressionRecordItem::RecordPun { name: Some(name), resolution, .. } => {
                let inferred = match resolution {
                    Some(resolution) => variable_reference(state, context, None, *resolution)?,
                    None => error_expression(state, context),
                };
                let inferred = instantiate(state, context, inferred)?;
                (SmolStr::clone(name), inferred)
            }
            _ => continue,
        };
        if !seen.insert(SmolStr::clone(&label)) {
            state.insert_error(ErrorKind::DuplicateLabel { label });
            continue;
        }
        fields.push(RowField { label: SmolStr::clone(&label), id: inferred.type_id });
        checked.push((label, inferred.expression));
    }

    let row = context.intern_row(fields, None);
    let type_id = context.intern_record(row);
    let kind = tree::ExpressionKind::Record(Arc::from(checked));
    Ok(allocate(state, Some(id), type_id, kind))
}

fn property_should_instantiate<Q>(context: &CheckContext<Q>, id: lowering::ExpressionId) -> bool
where
    Q: ExternalQueries,
{
    match context.lowered.tree.get_expression_kind(id) {
        Some(ExpressionKind::Variable { .. } | ExpressionKind::Constructor { .. }) => true,
        Some(ExpressionKind::Parenthesized { parenthesized: Some(inner) }) => {
            property_should_instantiate(context, *inner)
        }
        _ => false,
    }
}

type UpdatedRows = (TypeId, TypeId, Vec<tree::RecordUpdate>);

fn check_record_updates<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    updates: &[lowering::RecordUpdate],
) -> QueryResult<UpdatedRows>
where
    Q: ExternalQueries,
{
    let rest = kind::fresh_unification(state, context, context.types.row_type);
    let mut input_fields = Vec::with_capacity(updates.len());
    let mut output_fields = Vec::with_capacity(updates.len());
    let mut checked = Vec::with_capacity(updates.len());
    let mut seen = FxHashSet::default();

    for update in updates {
        match update {
            lowering::RecordUpdate::Leaf { name: Some(name), expression, .. } => {
                if !seen.insert(SmolStr::clone(name)) {
                    state.insert_error(ErrorKind::DuplicateLabel { label: SmolStr::clone(name) });
                    continue;
                }
                let input = kind::fresh_type(state, context);
                let value = infer_optional(state, context, *expression)?;
                let value = instantiate(state, context, value)?;
                input_fields.push(RowField { label: SmolStr::clone(name), id: input });
                output_fields.push(RowField { label: SmolStr::clone(name), id: value.type_id });
                let label = SmolStr::clone(name);
                checked.push(tree::RecordUpdate::Leaf { label, expression: value.expression });
            }
            lowering::RecordUpdate::Branch { name: Some(name), updates, .. } => {
                if !seen.insert(SmolStr::clone(name)) {
                    state.insert_error(ErrorKind::DuplicateLabel { label: SmolStr::clone(name) });
                    continue;
                }
                let (input, output, nested) = check_record_updates(state, context, updates)?;
                let input = context.intern_record(input);
                let output = context.intern_record(output);
                input_fields.push(RowField { label: SmolStr::clone(name), id: input });
                output_fields.push(RowField { label: SmolStr::clone(name), id: output });
                let label = SmolStr::clone(name);
                checked.push(tree::RecordUpdate::Branch { label, updates: Arc::from(nested) });
            }
            _ => continue,
        }
    }

    let input = context.intern_row(input_fields, Some(rest));
    let output = context.intern_row(output_fields, Some(rest));
    Ok((input, output, checked))
}

fn infer_operator_tree<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    tree: &OperatorTree<lowering::ExpressionId>,
) -> QueryResult<Elaborated>
where
    Q: ExternalQueries,
{
    match tree {
        OperatorTree::Leaf(None) => Ok(error_expression(state, context)),
        OperatorTree::Leaf(Some(id)) => infer_expression(state, context, *id),
        OperatorTree::Branch(operator_id, children) => {
            let Some(operator) = context.lowered.tree.get_term_operator(*operator_id) else {
                return Ok(error_expression(state, context));
            };
            let function = global_reference(state, context, None, operator)?;
            let [left, right] = &**children;
            let applied = apply(state, context, function, Argument::Tree(left))?;
            apply(state, context, applied, Argument::Tree(right))
        }
    }
}

#[derive(Clone, Copy)]
pub enum Argument<'a> {
    Expression(lowering::ExpressionId),
    Tree(&'a OperatorTree<lowering::ExpressionId>),
    Missing,
}

fn check_argument<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    argument: Argument<'_>,
    expected: TypeId,
) -> QueryResult<tree::ExpressionId>
where
    Q: ExternalQueries,
{
    match argument {
        Argument::Expression(id) => check_expression(state, context, id, expected),
        Argument::Tree(OperatorTree::Leaf(Some(id))) => {
            check_expression(state, context, *id, expected)
        }
        Argument::Tree(tree) => {
            let inferred = infer_operator_tree(state, context, tree)?;
            subsume::subsume_expression(
                state,
                context,
                inferred.expression,
                inferred.type_id,
                expected,
            )
        }
        Argument::Missing => Ok(error_expression(state, context).expression),
    }
}

fn infer_argument<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    argument: Argument<'_>,
) -> QueryResult<Elaborated>
where
    Q: ExternalQueries,
{
    match argument {
        Argument::Expression(id) => infer_expression(state, context, id),
        Argument::Tree(tree) => infer_operator_tree(state, context, tree),
        Argument::Missing => Ok(error_expression(state, context)),
    }
}

pub fn apply<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    function: Elaborated,
    argument: Argument<'_>,
) -> QueryResult<Elaborated>
where
    Q: ExternalQueries,
{
    let function = instantiate(state, context, function)?;
    let function_type = normalise::normalise(state, context, function.type_id);

    if let Some((argument_type, result_type)) =
        normalise::function_parts(state, context, function_type)
    {
        let argument = check_argument(state, context, argument, argument_type)?;
        let kind = tree::ExpressionKind::Application { function: function.expression, argument };
        return Ok(allocate(state, None, result_type, kind));
    }

    if let Type::Unification(_) = context.lookup_type(function_type) {
        let argument_type = kind::fresh_type(state, context);
        let result_type = kind::fresh_type(state, context);
        let expected = context.intern_function(argument_type, result_type);
        unify::unify(state, context, function_type, expected)?;
        let argument = check_argument(state, context, argument, argument_type)?;
        let kind = tree::ExpressionKind::Application { function: function.expression, argument };
        return Ok(allocate(state, None, result_type, kind));
    }

    if let Type::Unknown = context.lookup_type(function_type) {
        infer_argument(state, context, argument)?;
        return Ok(error_expression(state, context));
    }

    let inferred = infer_argument(state, context, argument)?;
    let inferred = instantiate(state, context, inferred)?;
    let result_type = kind::fresh_type(state, context);
    let expected = context.intern_function(inferred.type_id, result_type);
    unify::unify(state, context, expected, function_type)?;
    let kind = tree::ExpressionKind::Application {
        function: function.expression,
        argument: inferred.expression,
    };
    Ok(allocate(state, None, result_type, kind))
}

pub fn apply_elaborated<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    function: Elaborated,
    argument: Elaborated,
) -> QueryResult<Elaborated>
where
    Q: ExternalQueries,
{
    let function = instantiate(state, context, function)?;
    let (argument_type, result_type) = expect_function(state, context, function.type_id)?;
    let argument = subsume::subsume_expression(
        state,
        context,
        argument.expression,
        argument.type_id,
        argument_type,
    )?;
    let kind = tree::ExpressionKind::Application { function: function.expression, argument };
    Ok(allocate(state, None, result_type, kind))
}

fn apply_type<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    function: Elaborated,
    type_: Option<lowering::TypeId>,
) -> QueryResult<Elaborated>
where
    Q: ExternalQueries,
{
    let mut prefix_arguments = Vec::new();
    let mut current = normalise::normalise(state, context, function.type_id);
    let mut substitution = rustc_hash::FxHashMap::default();
    loop {
        let Type::Forall(binder_id, inner) = context.lookup_type(current) else {
            let function_type = current;
            state.insert_error(ErrorKind::CannotApplyExpressionOfTypeOnType { function_type });
            if let Some(type_) = type_ {
                types::infer_type(state, context, type_)?;
            }
            return Ok(error_expression(state, context));
        };
        let binder = context.lookup_forall_binder(binder_id);
        let kind = crate::core::fold::substitute_variables(context, &substitution, binder.kind);
        if binder.visible {
            let argument = match type_ {
                Some(type_) => types::check_type(state, context, type_, kind)?,
                None => context.types.unknown,
            };
            prefix_arguments.push(argument);
            substitution.insert(binder.name, argument);
            let inner = crate::core::fold::substitute_variables(context, &substitution, inner);
            let kind = tree::ExpressionKind::Instantiate {
                expression: function.expression,
                type_arguments: Arc::from(prefix_arguments),
                evidence: Arc::from([]),
            };
            return Ok(allocate(state, None, inner, kind));
        }
        let argument = kind::fresh_unification(state, context, kind);
        prefix_arguments.push(argument);
        substitution.insert(binder.name, argument);
        current = normalise::normalise(state, context, inner);
    }
}
