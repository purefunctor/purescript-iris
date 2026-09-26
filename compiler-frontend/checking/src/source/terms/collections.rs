use std::sync::Arc;

use building_types::QueryResult;
use smol_str::SmolStr;

use crate::context::CheckContext;
use crate::core::{RowField, Type, TypeId, normalise, toolkit, unification};
use crate::state::CheckState;
use crate::{ExternalQueries, tree};

use super::ElaboratedExpression;

struct InferredRecordFieldExpression {
    elaborated: ElaboratedExpression,
    field_type: TypeId,
}

fn infer_record_field_expression<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    expression: lowering::ExpressionId,
) -> QueryResult<InferredRecordFieldExpression>
where
    Q: ExternalQueries,
{
    let inferred = super::infer_expression(state, context, expression)?;

    let (elaborated, expand) = if should_instantiate_record_field(context, expression) {
        let elaborated = super::application::instantiate_expression(state, context, inferred)?;
        (elaborated, true)
    } else {
        (inferred, false)
    };

    let field_type = if expand {
        normalise::expand(state, context, elaborated.type_id)?
    } else {
        elaborated.type_id
    };
    Ok(InferredRecordFieldExpression { elaborated, field_type })
}

fn should_instantiate_record_field<Q>(
    context: &CheckContext<Q>,
    expression: lowering::ExpressionId,
) -> bool
where
    Q: ExternalQueries,
{
    let Some(kind) = context.lowered.tree.get_expression_kind(expression) else {
        return false;
    };

    if matches!(
        kind,
        lowering::ExpressionKind::Constructor { .. }
            | lowering::ExpressionKind::Variable { .. }
            | lowering::ExpressionKind::OperatorName { .. }
    ) {
        return true;
    }

    if let lowering::ExpressionKind::Typed { expression: Some(expression), .. }
    | lowering::ExpressionKind::Parenthesized { parenthesized: Some(expression) } = kind
    {
        return should_instantiate_record_field(context, *expression);
    }

    if let lowering::ExpressionKind::Application { function: Some(function), arguments } = kind
        && let Some(lowering::ExpressionArgument::Type(_)) = arguments.iter().next()
    {
        return should_instantiate_record_field(context, *function);
    }

    false
}

fn record_pun_expression<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    source: lowering::RecordPunId,
    resolution: lowering::TermVariableResolution,
) -> QueryResult<ElaboratedExpression>
where
    Q: ExternalQueries,
{
    let type_id = toolkit::lookup_term_variable(state, context, resolution)?;
    let resolution = tree::VariableResolution::Source(resolution);
    let kind = tree::ExpressionKind::RecordPun { source, resolution };
    Ok(super::allocate_expression(state, type_id, kind))
}

#[derive(Copy, Clone, Debug)]
enum ArrayMode {
    Infer,
    Check,
}

#[derive(Copy, Clone)]
enum RecordMode<'a> {
    Infer,
    Check { expected_fields: &'a [RowField] },
}

pub fn infer_array<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    array: &[lowering::ExpressionId],
) -> QueryResult<ElaboratedExpression>
where
    Q: ExternalQueries,
{
    let element = state.fresh_unification(context.queries, context.prim.t);
    let elements = array_core(state, context, array, ArrayMode::Infer, element)?;
    let type_id = context.intern_application(context.prim.array, element);
    let kind = tree::ExpressionKind::Array { elements };
    Ok(super::allocate_expression(state, type_id, kind))
}

pub fn check_array<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    array: &[lowering::ExpressionId],
    expected: TypeId,
) -> QueryResult<ElaboratedExpression>
where
    Q: ExternalQueries,
{
    if let Some(element) = expected_array_element(state, context, expected)? {
        let elements = array_core(state, context, array, ArrayMode::Check, element)?;
        let kind = tree::ExpressionKind::Array { elements };
        return Ok(super::allocate_expression(state, expected, kind));
    }

    let inferred = infer_array(state, context, array)?;
    unification::subtype(state, context, inferred.type_id, expected)?;
    Ok(inferred)
}

fn expected_array_element<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    expected: TypeId,
) -> QueryResult<Option<TypeId>>
where
    Q: ExternalQueries,
{
    let expected = normalise::expand(state, context, expected)?;
    let Type::Application(constructor, element) = *context.lookup_type(expected) else {
        return Ok(None);
    };

    let constructor = normalise::expand(state, context, constructor)?;
    if constructor == context.prim.array { Ok(Some(element)) } else { Ok(None) }
}

fn array_core<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    array: &[lowering::ExpressionId],
    mode: ArrayMode,
    element: TypeId,
) -> QueryResult<Arc<[tree::ExpressionId]>>
where
    Q: ExternalQueries,
{
    let mut elements = Vec::new();

    for expression in array {
        let checked = match mode {
            ArrayMode::Infer => {
                let inferred = super::infer_expression(state, context, *expression)?;
                unification::subtype(state, context, inferred.type_id, element)?;
                inferred
            }
            ArrayMode::Check => super::check_expression(state, context, *expression, element)?,
        };
        elements.push(checked.expression);
    }

    Ok(elements.into())
}

pub fn infer_record<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    record: &[lowering::ExpressionRecordItem],
) -> QueryResult<ElaboratedExpression>
where
    Q: ExternalQueries,
{
    let (type_id, fields, complete) = record_core(state, context, record, RecordMode::Infer)?;
    if complete {
        let kind = tree::ExpressionKind::Record { fields };
        Ok(super::allocate_expression(state, type_id, kind))
    } else {
        Ok(super::allocate_error_expression(state, type_id))
    }
}

pub fn check_record<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    record: &[lowering::ExpressionRecordItem],
    expected: TypeId,
) -> QueryResult<ElaboratedExpression>
where
    Q: ExternalQueries,
{
    let normalised = normalise::expand(state, context, expected)?;
    if let Type::Application(constructor, row_type) = *context.lookup_type(normalised) {
        let constructor = normalise::expand(state, context, constructor)?;
        if constructor == context.prim.record {
            let row_type = normalise::expand(state, context, row_type)?;
            if let Type::Row(row_id) = *context.lookup_type(row_type) {
                let expected_fields = context.lookup_row_type(row_id);
                let (record_type, fields, complete) = record_core(
                    state,
                    context,
                    record,
                    RecordMode::Check { expected_fields: &expected_fields.fields },
                )?;
                unification::subtype(state, context, record_type, expected)?;
                if complete {
                    let kind = tree::ExpressionKind::Record { fields };
                    return Ok(super::allocate_expression(state, record_type, kind));
                } else {
                    return Ok(super::allocate_error_expression(state, record_type));
                }
            }
        }
    }

    let inferred = infer_record(state, context, record)?;
    unification::subtype(state, context, inferred.type_id, expected)?;
    Ok(inferred)
}

fn find_expected_field(expected_fields: &[RowField], label: &SmolStr) -> Option<TypeId> {
    // Row fields are stable-sorted by label, so the partition point is the
    // first field with this label, which is the one that shadows duplicates.
    let index = expected_fields.partition_point(|field| field.label < *label);
    expected_fields.get(index).filter(|field| field.label == *label).map(|field| field.id)
}

fn expected_record_field(mode: RecordMode<'_>, label: &SmolStr) -> Option<TypeId> {
    match mode {
        RecordMode::Infer => None,
        RecordMode::Check { expected_fields } => find_expected_field(expected_fields, label),
    }
}

struct ElaboratedRecordField {
    row: RowField,
    checked: tree::RecordExpressionField,
}

fn elaborate_record_field<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    name: &SmolStr,
    value: lowering::ExpressionId,
    mode: RecordMode<'_>,
) -> QueryResult<ElaboratedRecordField>
where
    Q: ExternalQueries,
{
    let label = SmolStr::clone(name);

    let (id, expression) = if let Some(expected_type) = expected_record_field(mode, &label) {
        let checked = super::check_expression(state, context, value, expected_type)?;
        (expected_type, checked.expression)
    } else {
        let inferred = infer_record_field_expression(state, context, value)?;
        (inferred.field_type, inferred.elaborated.expression)
    };

    let checked = tree::RecordExpressionField::Field { label: SmolStr::clone(&label), expression };
    Ok(ElaboratedRecordField { row: RowField { label, id }, checked })
}

fn elaborate_record_pun<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    pun: lowering::RecordPunId,
    name: &SmolStr,
    resolution: lowering::TermVariableResolution,
    mode: RecordMode<'_>,
) -> QueryResult<ElaboratedRecordField>
where
    Q: ExternalQueries,
{
    let label = SmolStr::clone(name);
    let variable = record_pun_expression(state, context, pun, resolution)?;

    let (id, expression) = if let Some(expected_type) = expected_record_field(mode, &label) {
        let checked = super::check_elaborated_expression(state, context, variable, expected_type)?;
        (expected_type, checked.expression)
    } else {
        let inferred = super::application::instantiate_expression(state, context, variable)?;
        let field_type = normalise::expand(state, context, inferred.type_id)?;
        (field_type, inferred.expression)
    };

    state.checked.node_types.puns.insert(pun, id);

    let checked =
        tree::RecordExpressionField::Pun { source: pun, label: SmolStr::clone(&label), expression };
    Ok(ElaboratedRecordField { row: RowField { label, id }, checked })
}

fn record_core<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    record: &[lowering::ExpressionRecordItem],
    mode: RecordMode<'_>,
) -> QueryResult<(TypeId, Arc<[tree::RecordExpressionField]>, bool)>
where
    Q: ExternalQueries,
{
    let mut fields = Vec::new();
    let mut checked_fields = Vec::new();
    let mut complete = true;

    for field in record.iter() {
        let field = match field {
            lowering::ExpressionRecordItem::RecordField { name, value } => {
                let (Some(name), Some(value)) = (name, value) else {
                    complete = false;
                    continue;
                };
                elaborate_record_field(state, context, name, *value, mode)?
            }
            lowering::ExpressionRecordItem::RecordPun { id, name, resolution } => {
                let (Some(name), Some(resolution)) = (name, resolution) else {
                    complete = false;
                    continue;
                };
                elaborate_record_pun(state, context, *id, name, *resolution, mode)?
            }
        };

        fields.push(field.row);
        checked_fields.push(field.checked);
    }

    let row_type = context.intern_row(fields, None);
    let type_id = context.intern_application(context.prim.record, row_type);
    Ok((type_id, checked_fields.into(), complete))
}

pub fn infer_record_access<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    record: lowering::ExpressionId,
    labels: &[lowering::RecordAccessLabel],
) -> QueryResult<ElaboratedExpression>
where
    Q: ExternalQueries,
{
    let record = super::infer_expression(state, context, record)?;
    let mut current_type = record.type_id;
    let mut checked_labels = Vec::with_capacity(labels.len());

    for label in labels.iter() {
        let name = SmolStr::clone(&label.name);

        let field_type = state.fresh_unification(context.queries, context.prim.t);
        let tail_type = state.fresh_unification(context.queries, context.prim.row_type);

        let field = RowField { label: SmolStr::clone(&name), id: field_type };
        let row_type = context.intern_row([field], Some(tail_type));
        let record_type = context.intern_application(context.prim.record, row_type);

        unification::subtype(state, context, current_type, record_type)?;
        state.checked.node_types.record_access_labels.insert(label.id, field_type);
        current_type = field_type;
        checked_labels.push(name);
    }

    let labels = Arc::from(checked_labels);
    let kind = tree::ExpressionKind::RecordAccess { record: record.expression, labels };
    Ok(super::allocate_expression(state, current_type, kind))
}

pub fn infer_record_update<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    record: lowering::ExpressionId,
    updates: &[lowering::RecordUpdate],
) -> QueryResult<ElaboratedExpression>
where
    Q: ExternalQueries,
{
    let updates = infer_record_updates(state, context, updates)?;
    let ElaboratedRecordUpdates { input_fields, output_fields, tail, updates } = updates;

    let input_row = context.intern_row(input_fields, Some(tail));
    let input_record = context.intern_application(context.prim.record, input_row);

    let output_row = context.intern_row(output_fields, Some(tail));
    let output_record = context.intern_application(context.prim.record, output_row);

    let record = super::check_expression(state, context, record, input_record)?;

    let kind = tree::ExpressionKind::RecordUpdate { record: record.expression, updates };
    Ok(super::allocate_expression(state, output_record, kind))
}

struct ElaboratedRecordUpdates {
    input_fields: Vec<RowField>,
    output_fields: Vec<RowField>,
    tail: TypeId,
    updates: Arc<[tree::RecordExpressionUpdate]>,
}

fn infer_record_updates<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    updates: &[lowering::RecordUpdate],
) -> QueryResult<ElaboratedRecordUpdates>
where
    Q: ExternalQueries,
{
    let mut input_fields = Vec::new();
    let mut output_fields = Vec::new();
    let mut checked_updates = Vec::new();

    for update in updates {
        match update {
            lowering::RecordUpdate::Leaf { name, expression } => {
                let Some(name) = name else {
                    checked_updates.push(tree::RecordExpressionUpdate::Error);
                    continue;
                };
                let label = SmolStr::clone(name);

                let input_id = state.fresh_unification(context.queries, context.prim.t);
                let (output_id, expression) = if let Some(expression) = expression {
                    let inferred = infer_record_field_expression(state, context, *expression)?;
                    (inferred.field_type, inferred.elaborated.expression)
                } else {
                    let output_id = context.unknown("missing record update expression");
                    let expression = super::allocate_error_expression(state, output_id);
                    (output_id, expression.expression)
                };

                let update = tree::RecordExpressionUpdate::Leaf {
                    label: SmolStr::clone(&label),
                    expression,
                };
                checked_updates.push(update);

                input_fields.push(RowField { label: label.clone(), id: input_id });
                output_fields.push(RowField { label, id: output_id });
            }
            lowering::RecordUpdate::Branch { name, updates } => {
                let Some(name) = name else {
                    checked_updates.push(tree::RecordExpressionUpdate::Error);
                    continue;
                };
                let label = SmolStr::clone(name);

                let updates = infer_record_updates(state, context, updates)?;
                let ElaboratedRecordUpdates {
                    input_fields: branch_input_fields,
                    output_fields: branch_output_fields,
                    tail,
                    updates,
                } = updates;

                let in_row = context.intern_row(branch_input_fields, Some(tail));
                let in_id = context.intern_application(context.prim.record, in_row);

                let out_row = context.intern_row(branch_output_fields, Some(tail));
                let out_id = context.intern_application(context.prim.record, out_row);

                input_fields.push(RowField { label: label.clone(), id: in_id });
                output_fields.push(RowField { label: label.clone(), id: out_id });

                let update = tree::RecordExpressionUpdate::Branch { label, updates };
                checked_updates.push(update);
            }
        }
    }

    let tail = state.fresh_unification(context.queries, context.prim.row_type);

    let updates = checked_updates.into();
    Ok(ElaboratedRecordUpdates { input_fields, output_fields, tail, updates })
}
