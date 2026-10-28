use smol_str::SmolStr;

use crate::convert::{Context, ConversionResult};
use crate::tree::{
    Binding, ExpressionId, ExpressionKind, Field, PatternKind, RecordField, RecordUpdate,
    SynthesizedEvidence,
};

const RECORD_BUILDER: &str = "Record.Builder";

enum BuilderStep {
    Insert { field: Field, value: usize },
    Modify { field: Field, function: usize },
    Delete { field: Field },
    Rename { from: Field, to: Field },
    Merge { fields: Vec<(Field, usize)> },
}

enum BuilderSource {
    Literal(Vec<(Field, usize)>),
    Record(usize),
}

enum BuiltRecord {
    Literal(Vec<RecordField>),
    Update { record: ExpressionId, updates: Vec<RecordField> },
}

impl<'c, Q> Context<'c, Q>
where
    Q: checking::ExternalQueries,
{
    pub(super) fn known_record_build(
        &mut self,
        function: ExpressionId,
        arguments: &[ExpressionId],
    ) -> ConversionResult<Option<ExpressionId>> {
        let (builder, record) = if self.known_term(function, RECORD_BUILDER, "build")?
            && let [builder, record] = *arguments
        {
            (builder, Some(record))
        } else if self.known_term(function, RECORD_BUILDER, "buildFromScratch")?
            && let [builder] = *arguments
        {
            (builder, None)
        } else {
            return Ok(None);
        };

        let mut operands = Vec::new();
        let Some(steps) = self.record_builder_steps(builder, &mut operands)? else {
            return Ok(None);
        };
        let source = match record {
            None => BuilderSource::Literal(Vec::new()),
            Some(record) => match self.literal_record_fields(record) {
                Some(fields) => {
                    let fields = fields.iter().map(|field| {
                        operands.push((field.expression, SmolStr::clone(&field.field.name)));
                        (field.field.clone(), operands.len() - 1)
                    });
                    BuilderSource::Literal(fields.collect())
                }
                None => {
                    operands.push((record, SmolStr::new_static("record")));
                    BuilderSource::Record(operands.len() - 1)
                }
            },
        };
        if let BuilderSource::Record(_) = source
            && steps
                .iter()
                .any(|step| matches!(step, BuilderStep::Delete { .. } | BuilderStep::Rename { .. }))
        {
            return Ok(None);
        }

        let bindings = self.bind_record_builder_operands(&mut operands, &source, &steps)?;
        let mut built = match source {
            BuilderSource::Literal(fields) => {
                let fields = fields
                    .into_iter()
                    .map(|(field, operand)| RecordField { field, expression: operands[operand].0 });
                BuiltRecord::Literal(fields.collect())
            }
            BuilderSource::Record(operand) => {
                BuiltRecord::Update { record: operands[operand].0, updates: Vec::new() }
            }
        };
        for step in steps {
            let Some(next) = self.record_builder_step(built, step, &operands)? else {
                return Ok(None);
            };
            built = next;
        }

        let record = match built {
            BuiltRecord::Literal(fields) => {
                self.expression(ExpressionKind::Record { fields: fields.into() })
            }
            BuiltRecord::Update { record, updates } if updates.is_empty() => record,
            BuiltRecord::Update { record, updates } => {
                let updates = updates.into_iter().map(|RecordField { field, expression }| {
                    RecordUpdate::Leaf { field, expression }
                });
                let updates = updates.collect();
                self.expression(ExpressionKind::RecordUpdate { record, updates })
            }
        };
        if bindings.is_empty() {
            return Ok(Some(record));
        }
        Ok(Some(self.expression(ExpressionKind::Let {
            recursive: false,
            bindings: bindings.into(),
            body: record,
        })))
    }

    fn record_builder_steps(
        &self,
        builder: ExpressionId,
        operands: &mut Vec<(ExpressionId, SmolStr)>,
    ) -> ConversionResult<Option<Vec<BuilderStep>>> {
        let ExpressionKind::Application { function, arguments, .. } = &self.storage[builder].kind
        else {
            return Ok(None);
        };
        let (function, arguments) = self.application_spine(*function, arguments);

        let composition = if let Some(&[outer, inner]) = self.known_instance_member_arguments(
            function,
            &arguments,
            "Control.Semigroupoid",
            "compose",
            RECORD_BUILDER,
            "semigroupoidBuilder",
        )? {
            Some((outer, inner, false))
        } else if self.known_term(function, "Control.Semigroupoid", "composeFlipped")?
            && let [dictionary, inner, outer] = *arguments
            && self.known_named_instance(dictionary, RECORD_BUILDER, "semigroupoidBuilder")?
        {
            Some((inner, outer, true))
        } else {
            None
        };
        if let Some((written_first, written_second, applied_in_written_order)) = composition {
            let Some(first) = self.record_builder_steps(written_first, operands)? else {
                return Ok(None);
            };
            let Some(second) = self.record_builder_steps(written_second, operands)? else {
                return Ok(None);
            };
            let (applied_first, applied_second) =
                if applied_in_written_order { (first, second) } else { (second, first) };
            return Ok(Some(applied_first.into_iter().chain(applied_second).collect()));
        }

        if let Some([]) = self.known_instance_member_arguments(
            function,
            &arguments,
            "Control.Category",
            "identity",
            RECORD_BUILDER,
            "categoryBuilder",
        )? {
            return Ok(Some(Vec::new()));
        }
        if self.known_term(function, RECORD_BUILDER, "nub")?
            && let [constraint] = *arguments
            && self.known_record_constraints(&[constraint])
        {
            return Ok(Some(Vec::new()));
        }

        let step = if self.known_term(function, RECORD_BUILDER, "insert")?
            && let [cons, lacks, symbol, proxy, value] = *arguments
            && let Some(field) = self.known_record_label(symbol, &[cons, lacks], proxy)
        {
            operands.push((value, SmolStr::new_static("insertValue")));
            BuilderStep::Insert { field, value: operands.len() - 1 }
        } else if self.known_term(function, RECORD_BUILDER, "modify")?
            && let [first, second, symbol, proxy, function] = *arguments
            && let Some(field) = self.known_record_label(symbol, &[first, second], proxy)
        {
            operands.push((function, SmolStr::new_static("modifyFunction")));
            BuilderStep::Modify { field, function: operands.len() - 1 }
        } else if self.known_term(function, RECORD_BUILDER, "delete")?
            && let [symbol, lacks, cons, proxy] = *arguments
            && let Some(field) = self.known_record_label(symbol, &[lacks, cons], proxy)
        {
            BuilderStep::Delete { field }
        } else if self.known_term(function, RECORD_BUILDER, "rename")?
            && let [from_symbol, to_symbol, from_cons, from_lacks, to_cons, to_lacks, from, to] =
                *arguments
            && let Some(from) = self.known_record_label(
                from_symbol,
                &[from_cons, from_lacks, to_cons, to_lacks],
                from,
            )
            && let Some(to) = self.known_record_label(to_symbol, &[], to)
        {
            BuilderStep::Rename { from, to }
        } else if let Some(record) = self.known_record_builder_merge(function, &arguments)?
            && let Some(fields) = self.literal_record_fields(record)
        {
            let fields = fields.iter().map(|field| {
                operands.push((field.expression, SmolStr::clone(&field.field.name)));
                (field.field.clone(), operands.len() - 1)
            });
            BuilderStep::Merge { fields: fields.collect() }
        } else {
            return Ok(None);
        };
        Ok(Some(vec![step]))
    }

    fn known_record_builder_merge(
        &self,
        function: ExpressionId,
        arguments: &[ExpressionId],
    ) -> ConversionResult<Option<ExpressionId>> {
        if self.known_term(function, RECORD_BUILDER, "union")?
            && let [union, record] = *arguments
            && self.known_record_constraints(&[union])
        {
            return Ok(Some(record));
        }
        for item_name in ["merge", "disjointUnion"] {
            if self.known_term(function, RECORD_BUILDER, item_name)?
                && let [union, nub, record] = *arguments
                && self.known_record_constraints(&[union, nub])
            {
                return Ok(Some(record));
            }
        }
        Ok(None)
    }

    fn known_record_label(
        &self,
        symbol: ExpressionId,
        constraints: &[ExpressionId],
        proxy: ExpressionId,
    ) -> Option<Field> {
        let ExpressionKind::SynthesizedEvidence { evidence: SynthesizedEvidence::IsSymbol(label) } =
            &self.storage[symbol].kind
        else {
            return None;
        };
        if !self.known_record_constraints(constraints) || !self.expression_is_stable(proxy) {
            return None;
        }
        let label = label.to_utf8().ok()?;
        Some(self.label_field(label.into()))
    }

    fn known_record_constraints(&self, constraints: &[ExpressionId]) -> bool {
        constraints.iter().all(|constraint| {
            matches!(self.storage[*constraint].kind, ExpressionKind::TrivialEvidence)
        })
    }

    fn bind_record_builder_operands(
        &mut self,
        operands: &mut [(ExpressionId, SmolStr)],
        source: &BuilderSource,
        steps: &[BuilderStep],
    ) -> ConversionResult<Vec<Binding>> {
        let evaluates = |context: &Self, expression: ExpressionId| {
            !context.expression_is_stable(expression)
                && !matches!(context.storage[expression].kind, ExpressionKind::Abstraction { .. })
        };
        let record_is_read = match source {
            BuilderSource::Record(record) => {
                let modifies = steps.iter().any(|step| matches!(step, BuilderStep::Modify { .. }));
                modifies && evaluates(self, operands[*record].0)
            }
            BuilderSource::Literal(_) => false,
        };
        let evaluated = operands.iter().filter(|(expression, _)| evaluates(self, *expression));
        if !record_is_read && evaluated.count() <= 1 {
            return Ok(Vec::new());
        }
        let mut bindings = Vec::new();
        for (expression, name) in operands.iter_mut() {
            if evaluates(self, *expression) {
                *expression = self.let_bound_local(*expression, name, &mut bindings)?;
            }
        }
        Ok(bindings)
    }

    fn record_builder_step(
        &mut self,
        built: BuiltRecord,
        step: BuilderStep,
        operands: &[(ExpressionId, SmolStr)],
    ) -> ConversionResult<Option<BuiltRecord>> {
        let built = match (built, step) {
            (BuiltRecord::Literal(mut fields), BuilderStep::Insert { field, value }) => {
                fields.push(RecordField { field, expression: operands[value].0 });
                BuiltRecord::Literal(fields)
            }
            (BuiltRecord::Update { record, mut updates }, BuilderStep::Insert { field, value }) => {
                updates.push(RecordField { field, expression: operands[value].0 });
                BuiltRecord::Update { record, updates }
            }
            (BuiltRecord::Literal(mut fields), BuilderStep::Modify { field, function }) => {
                let Some(modified) = fields.iter_mut().find(|other| other.field.name == field.name)
                else {
                    return Ok(None);
                };
                let value = modified.expression;
                modified.expression = self.function_application(operands[function].0, value)?;
                BuiltRecord::Literal(fields)
            }
            (
                BuiltRecord::Update { record, mut updates },
                BuilderStep::Modify { field, function },
            ) => {
                let function = operands[function].0;
                match updates.iter_mut().find(|other| other.field.name == field.name) {
                    Some(modified) => {
                        let value = modified.expression;
                        modified.expression = self.function_application(function, value)?;
                    }
                    None => {
                        let source =
                            self.expression(ExpressionKind::clone(&self.storage[record].kind));
                        let value = self.expression(ExpressionKind::Project {
                            record: source,
                            field: field.clone(),
                        });
                        let expression = self.function_application(function, value)?;
                        updates.push(RecordField { field, expression });
                    }
                }
                BuiltRecord::Update { record, updates }
            }
            (BuiltRecord::Literal(mut fields), BuilderStep::Delete { field }) => {
                fields.retain(|other| other.field.name != field.name);
                BuiltRecord::Literal(fields)
            }
            (BuiltRecord::Literal(mut fields), BuilderStep::Rename { from, to }) => {
                let Some(position) = fields.iter().position(|other| other.field.name == from.name)
                else {
                    return Ok(None);
                };
                let RecordField { expression, .. } = fields.remove(position);
                fields.push(RecordField { field: to, expression });
                BuiltRecord::Literal(fields)
            }
            (BuiltRecord::Literal(fields), BuilderStep::Merge { fields: merged }) => {
                BuiltRecord::Literal(merged_record_fields(fields, merged, operands))
            }
            (BuiltRecord::Update { record, updates }, BuilderStep::Merge { fields: merged }) => {
                let updates = merged_record_fields(updates, merged, operands);
                BuiltRecord::Update { record, updates }
            }
            (
                BuiltRecord::Update { .. },
                BuilderStep::Delete { .. } | BuilderStep::Rename { .. },
            ) => return Ok(None),
        };
        Ok(Some(built))
    }

    fn function_application(
        &mut self,
        function: ExpressionId,
        argument: ExpressionId,
    ) -> ConversionResult<ExpressionId> {
        if let ExpressionKind::Abstraction { parameters, body } = &self.storage[function].kind
            && let [pattern] = parameters.as_ref()
            && let PatternKind::Variable(parameter) = &self.storage[*pattern].kind
        {
            let binding = Binding {
                parameter: parameter.clone(),
                expression: argument,
                source_order: 0,
                origin: None,
            };
            let body = *body;
            return Ok(self.expression(ExpressionKind::Let {
                recursive: false,
                bindings: [binding].into(),
                body,
            }));
        }
        self.application(function, [argument])
    }
}

fn merged_record_fields(
    mut fields: Vec<RecordField>,
    merged: Vec<(Field, usize)>,
    operands: &[(ExpressionId, SmolStr)],
) -> Vec<RecordField> {
    for (field, operand) in merged {
        let expression = operands[operand].0;
        match fields.iter_mut().find(|other| other.field.name == field.name) {
            Some(existing) => existing.expression = expression,
            None => fields.push(RecordField { field, expression }),
        }
    }
    fields
}
