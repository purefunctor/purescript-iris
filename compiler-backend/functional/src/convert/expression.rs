use std::sync::Arc;

use checking::tree as checking_tree;
use smol_str::format_smolstr;

use crate::error::UnsupportedState;
use crate::tree::{
    CaseAlternative, ExpressionId, ExpressionKind, Literal, PatternId, PatternKind, RecordField,
    RecordPatternField, RecordUpdate,
};

use super::declaration::{function_patterns, guarded_expression, let_bindings};
use super::evidence::evidence_variable;
use super::{BindingSource, Context, ConversionResult};

pub(super) fn convert_expression(
    context: &mut Context<'_, impl checking::ExternalQueries>,
    expression_id: checking_tree::ExpressionId,
) -> ConversionResult<ExpressionId> {
    let checked = Arc::clone(&context.checked);
    let expression = &checked.tree[expression_id];
    let kind = match &expression.kind {
        checking_tree::ExpressionKind::String { value, .. } => {
            ExpressionKind::Literal { literal: Literal::String(value.clone()) }
        }
        checking_tree::ExpressionKind::Char { value } => {
            ExpressionKind::Literal { literal: Literal::Char(*value) }
        }
        checking_tree::ExpressionKind::Boolean { value } => {
            ExpressionKind::Literal { literal: Literal::Boolean(*value) }
        }
        checking_tree::ExpressionKind::Integer { value } => {
            ExpressionKind::Literal { literal: Literal::Integer(*value) }
        }
        checking_tree::ExpressionKind::Number { value } => {
            ExpressionKind::Literal { literal: Literal::Number(value.clone()) }
        }
        checking_tree::ExpressionKind::Array { elements } => {
            let elements = expressions(context, elements)?;
            ExpressionKind::Array { elements: elements.into() }
        }
        checking_tree::ExpressionKind::Record { fields } => {
            let mut converted = Vec::new();
            for field in fields.iter() {
                let (label, expression) = match field {
                    checking_tree::RecordExpressionField::Field { label, expression }
                    | checking_tree::RecordExpressionField::Pun { label, expression, .. } => {
                        (label, expression)
                    }
                };
                let field = context.label_field(label.clone());
                let expression = convert_expression(context, *expression)?;
                converted.push(RecordField { field, expression });
            }
            ExpressionKind::Record { fields: converted.into() }
        }
        checking_tree::ExpressionKind::RecordAccess { record, labels } => {
            let mut record = convert_expression(context, *record)?;
            for label in labels.iter() {
                let field = context.label_field(label.clone());
                record = context.expression(ExpressionKind::Project { record, field });
            }
            return Ok(record);
        }
        checking_tree::ExpressionKind::RecordUpdate { record, updates } => {
            let record = convert_expression(context, *record)?;
            let updates = record_updates(context, updates)?;
            ExpressionKind::RecordUpdate { record, updates: updates.into() }
        }
        checking_tree::ExpressionKind::Constructor { resolution } => {
            let &(file_id, term_id) = resolution;
            if context.constructor_is_newtype(file_id, term_id)? {
                let parameter = context.fresh_parameter("value".into())?;
                let body =
                    context.expression(ExpressionKind::Local { parameter: parameter.clone() });
                return Ok(context.parameter_abstraction([parameter], body));
            }
            let global = context.term_global(file_id, term_id)?;
            if context.constructor_arity(file_id, term_id)? == 0 {
                ExpressionKind::Literal { literal: Literal::String(global.item_name.into()) }
            } else {
                ExpressionKind::Constructor { global }
            }
        }
        checking_tree::ExpressionKind::Variable { resolution }
        | checking_tree::ExpressionKind::RecordPun { resolution, .. } => {
            return variable(context, *resolution);
        }
        checking_tree::ExpressionKind::Section { binder } => {
            let parameter = context.checked_binder_parameter(*binder)?;
            ExpressionKind::Local { parameter }
        }
        checking_tree::ExpressionKind::TermApplication { function, argument } => {
            if let checking_tree::ExpressionKind::Constructor { resolution } =
                &checked.tree[*function].kind
            {
                let &(file_id, term_id) = resolution;
                if context.constructor_is_newtype(file_id, term_id)? {
                    return convert_expression(context, *argument);
                }
            }
            let function = convert_expression(context, *function)?;
            let argument = convert_expression(context, *argument)?;
            return context.typed_application(function, [argument], expression.type_id);
        }
        checking_tree::ExpressionKind::EvidenceApplication { function, evidence, constraint } => {
            let evidence_expression = evidence_variable(context, *evidence, Some(*constraint))?;
            let function = convert_expression(context, *function)?;
            let selection = context.synthetic_application(function, [evidence_expression])?;
            return context.record_closed_member_selection(
                function,
                *evidence,
                *constraint,
                selection,
            );
        }
        checking_tree::ExpressionKind::EvidenceAbstraction { binder, expression } => {
            let parameter = context.evidence_parameter(*binder)?;
            let body =
                context.evidence_scope(|context| convert_expression(context, *expression))?;
            return Ok(context.parameter_abstraction([parameter], body));
        }
        checking_tree::ExpressionKind::Lambda { binders, expression } => {
            let parameters = function_patterns(context, binders)?;
            let body =
                context.evidence_scope(|context| convert_expression(context, *expression))?;
            return Ok(context.abstraction(parameters, body));
        }
        checking_tree::ExpressionKind::IfThenElse { condition, then, else_ } => {
            ExpressionKind::IfThenElse {
                condition: convert_expression(context, *condition)?,
                then: convert_expression(context, *then)?,
                else_: convert_expression(context, *else_)?,
            }
        }
        checking_tree::ExpressionKind::Case { scrutinees, alternatives } => {
            let scrutinees = expressions(context, scrutinees)?;
            let alternatives =
                alternatives.iter().map(|alternative| case_alternative(context, alternative));
            let alternatives = alternatives.collect::<ConversionResult<Vec<_>>>()?;
            ExpressionKind::Case {
                scrutinees: scrutinees.into(),
                alternatives: alternatives.into(),
            }
        }
        checking_tree::ExpressionKind::Let { bindings, expression } => {
            let body = convert_expression(context, *expression)?;
            return let_bindings(context, bindings, body);
        }
        checking_tree::ExpressionKind::Error => ExpressionKind::Error,
    };
    Ok(context.expression(kind))
}

fn case_alternative(
    context: &mut Context<'_, impl checking::ExternalQueries>,
    alternative: &checking_tree::CaseAlternative,
) -> ConversionResult<CaseAlternative> {
    let patterns = patterns(context, &alternative.binders)?;
    let expression = guarded_expression(context, &alternative.guarded_expression)?;
    Ok(CaseAlternative { patterns: patterns.into(), expression })
}

fn expressions(
    context: &mut Context<'_, impl checking::ExternalQueries>,
    expressions: &[checking_tree::ExpressionId],
) -> ConversionResult<Vec<ExpressionId>> {
    let expressions = expressions.iter().map(|&expression| convert_expression(context, expression));
    expressions.collect::<ConversionResult<Vec<_>>>()
}

fn record_updates(
    context: &mut Context<'_, impl checking::ExternalQueries>,
    updates: &[checking_tree::RecordExpressionUpdate],
) -> ConversionResult<Vec<RecordUpdate>> {
    let mut converted = Vec::new();
    for update in updates {
        let update = match update {
            checking_tree::RecordExpressionUpdate::Leaf { label, expression } => {
                RecordUpdate::Leaf {
                    field: context.label_field(label.clone()),
                    expression: convert_expression(context, *expression)?,
                }
            }
            checking_tree::RecordExpressionUpdate::Branch { label, updates } => {
                let updates = record_updates(context, updates)?;
                RecordUpdate::Branch {
                    field: context.label_field(label.clone()),
                    updates: updates.into(),
                }
            }
            checking_tree::RecordExpressionUpdate::Error => {
                return Err(context.unsupported(UnsupportedState::RecordUpdateError));
            }
        };
        converted.push(update);
    }
    Ok(converted)
}

pub(super) fn patterns(
    context: &mut Context<'_, impl checking::ExternalQueries>,
    binders: &[checking_tree::BinderId],
) -> ConversionResult<Vec<PatternId>> {
    let patterns = binders.iter().map(|&binder| convert_pattern(context, binder));
    patterns.collect::<ConversionResult<Vec<_>>>()
}

pub(super) fn convert_pattern(
    context: &mut Context<'_, impl checking::ExternalQueries>,
    binder_id: checking_tree::BinderId,
) -> ConversionResult<PatternId> {
    let checked = Arc::clone(&context.checked);
    let binder = &checked.tree[binder_id];
    let kind = match &binder.kind {
        checking_tree::BinderKind::Typed { binder, .. } => {
            return convert_pattern(context, *binder);
        }
        checking_tree::BinderKind::Integer { value } => {
            PatternKind::Literal(Literal::Integer(i64::from(*value)))
        }
        checking_tree::BinderKind::Number { negative, value } => {
            let value = if *negative { format_smolstr!("-{value}") } else { value.clone() };
            PatternKind::Literal(Literal::Number(value))
        }
        checking_tree::BinderKind::Variable => {
            PatternKind::Variable(context.checked_binder_parameter(binder_id)?)
        }
        checking_tree::BinderKind::Named { name, binder } => PatternKind::Named {
            parameter: context.parameter(context.binding_source(binder_id), name.clone())?,
            pattern: convert_pattern(context, *binder)?,
        },
        checking_tree::BinderKind::Wildcard => PatternKind::Wildcard,
        checking_tree::BinderKind::String { value } => {
            PatternKind::Literal(Literal::String(value.clone()))
        }
        checking_tree::BinderKind::Char { value } => PatternKind::Literal(Literal::Char(*value)),
        checking_tree::BinderKind::Boolean { value } => {
            PatternKind::Literal(Literal::Boolean(*value))
        }
        checking_tree::BinderKind::Array { elements } => {
            PatternKind::Array(patterns(context, elements)?.into())
        }
        checking_tree::BinderKind::Record { fields } => {
            let converted =
                fields.iter().map(|field| record_pattern_field(context, binder_id, field));
            let converted = converted.collect::<ConversionResult<Vec<_>>>()?;
            PatternKind::Record(converted.into())
        }
        checking_tree::BinderKind::Constructor { resolution, arguments } => {
            let &(file_id, term_id) = resolution;
            if context.constructor_is_newtype(file_id, term_id)? {
                let [argument] = arguments.as_ref() else {
                    return Err(context.unsupported(UnsupportedState::BinderError(binder_id)));
                };
                return convert_pattern(context, *argument);
            }
            PatternKind::Constructor {
                global: context.term_global(file_id, term_id)?,
                arguments: patterns(context, arguments)?.into(),
            }
        }
        checking_tree::BinderKind::Error => {
            return Err(context.unsupported(UnsupportedState::BinderError(binder_id)));
        }
    };
    Ok(context.pattern(kind))
}

fn record_pattern_field(
    context: &mut Context<'_, impl checking::ExternalQueries>,
    binder_id: checking_tree::BinderId,
    field: &checking_tree::RecordBinderField,
) -> ConversionResult<RecordPatternField> {
    let (label, pattern) = match field {
        checking_tree::RecordBinderField::Field { label, binder } => {
            (label.clone(), convert_pattern(context, *binder)?)
        }
        checking_tree::RecordBinderField::Pun { label } => {
            let Some(source) = context.record_pun_source(binder_id, label) else {
                return Err(context.unsupported(UnsupportedState::BinderError(binder_id)));
            };
            let parameter = context.record_pun_parameter(source, label.clone())?;
            (label.clone(), context.pattern(PatternKind::Variable(parameter)))
        }
    };
    Ok(RecordPatternField { field: context.label_field(label), pattern })
}

fn variable(
    context: &mut Context<'_, impl checking::ExternalQueries>,
    resolution: checking_tree::VariableResolution,
) -> ConversionResult<ExpressionId> {
    match resolution {
        checking_tree::VariableResolution::Generated(binder) => {
            let parameter = context.checked_binder_parameter(binder)?;
            Ok(context.expression(ExpressionKind::Local { parameter }))
        }
        checking_tree::VariableResolution::Source(resolution) => match resolution {
            lowering::TermVariableResolution::Binder(binder) => {
                let name = context.source_binder_name(binder);
                let parameter = context.parameter(BindingSource::SourceBinder(binder), name)?;
                Ok(context.expression(ExpressionKind::Local { parameter }))
            }
            lowering::TermVariableResolution::Let(source) => {
                let parameter = context.local_parameter(source)?;
                Ok(context.expression(ExpressionKind::Local { parameter }))
            }
            lowering::TermVariableResolution::RecordPun(source) => {
                let name = context.record_pun_name(source);
                let parameter = context.record_pun_parameter(source, name)?;
                Ok(context.expression(ExpressionKind::Local { parameter }))
            }
            lowering::TermVariableResolution::Reference(file_id, term_id) => {
                if let Some(expression) = context.stylex_value_intrinsic(file_id, term_id)? {
                    return Ok(expression);
                }
                let global = context.term_global(file_id, term_id)?;
                Ok(context.expression(ExpressionKind::Global { global }))
            }
        },
    }
}
