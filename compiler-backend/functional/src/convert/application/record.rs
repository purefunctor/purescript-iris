use smol_str::SmolStr;

use crate::convert::{Context, ConversionResult};
use crate::tree::{
    Binding, ExpressionId, ExpressionKind, Field, Literal, RecordField, RecordUpdate,
    SynthesizedEvidence,
};

impl<'c, Q> Context<'c, Q>
where
    Q: checking::ExternalQueries,
{
    pub(super) fn known_record_application(
        &mut self,
        function: ExpressionId,
        arguments: &[ExpressionId],
    ) -> ConversionResult<Option<ExpressionId>> {
        if self.known_term(function, "Record", "get")?
            && let [symbol, constraint, proxy, record] = *arguments
            && let Some(field) = self.known_record_label(symbol, &[constraint], proxy)
        {
            return Ok(Some(self.expression(ExpressionKind::Project { record, field })));
        }
        if (self.known_term(function, "Record", "set")?
            || self.known_term(function, "Record", "insert")?)
            && let [symbol, first, second, proxy, value, record] = *arguments
            && let Some(field) = self.known_record_label(symbol, &[first, second], proxy)
        {
            return self.record_set(field, value, record).map(Some);
        }
        if self.known_term(function, "Record", "modify")?
            && let [symbol, first, second, proxy, modification, record] = *arguments
            && let Some(field) = self.known_record_label(symbol, &[first, second], proxy)
        {
            let mut operands = [modification, record];
            let bindings =
                self.ordered_operands(&mut operands, &["modify", "record"], &[1, 0, 1])?;
            let [modification, record] = operands;
            let current = self.expression(ExpressionKind::Project { record, field: field.clone() });
            let modified = self.application(modification, [current])?;
            let updates = [RecordUpdate::Leaf { field, expression: modified }];
            let updated =
                self.expression(ExpressionKind::RecordUpdate { record, updates: updates.into() });
            return Ok(Some(self.bound_operands(bindings, updated)));
        }
        if self.known_term(function, "Record", "delete")?
            && let [symbol, first, second, proxy, record] = *arguments
            && let Some(field) = self.known_record_label(symbol, &[first, second], proxy)
        {
            return self.record_delete(field, record).map(Some);
        }
        if self.known_term(function, "Record.Unsafe", "unsafeGet")?
            && let [label, record] = *arguments
            && let Some(field) = self.known_literal_label(label)
        {
            return Ok(Some(self.expression(ExpressionKind::Project { record, field })));
        }
        if self.known_term(function, "Record.Unsafe", "unsafeSet")?
            && let [label, value, record] = *arguments
            && let Some(field) = self.known_literal_label(label)
        {
            return self.record_set(field, value, record).map(Some);
        }
        if self.known_term(function, "Record.Unsafe", "unsafeDelete")?
            && let [label, record] = *arguments
            && let Some(field) = self.known_literal_label(label)
        {
            return self.record_delete(field, record).map(Some);
        }
        if self.known_term(function, "Record.Unsafe", "unsafeHas")?
            && let [label, record] = *arguments
            && let Some(field) = self.known_literal_label(label)
            && let Some(fields) = self.record_literal_fields(record)
        {
            let present = fields.iter().any(|candidate| candidate.field.name == field.name);
            let literal = Literal::Boolean(present);
            let present = self.expression(ExpressionKind::Literal { literal });
            let (bindings, _) = self.ordered_record_fields(&fields, &[])?;
            return Ok(Some(self.bound_operands(bindings, present)));
        }
        let operands = if self.known_term(function, "Record", "union")?
            && let [constraint, left, right] = *arguments
            && self.known_record_constraints(&[constraint])
        {
            Some((left, right))
        } else if (self.known_term(function, "Record", "merge")?
            || self.known_term(function, "Record", "disjointUnion")?)
            && let [first, second, left, right] = *arguments
            && self.known_record_constraints(&[first, second])
        {
            Some((left, right))
        } else if self.known_term(function, "Record.Unsafe.Union", "unsafeUnion")?
            && let [left, right] = *arguments
        {
            Some((left, right))
        } else {
            None
        };
        let Some((left, right)) = operands else { return Ok(None) };
        self.record_union(left, right)
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

    pub(super) fn known_uncurried_record_application(
        &mut self,
        function: ExpressionId,
        arguments: &[ExpressionId],
    ) -> ConversionResult<Option<ExpressionId>> {
        if self.known_term(function, "Record.Unsafe.Union", "unsafeUnionFn")?
            && let [left, right] = *arguments
        {
            return self.record_union(left, right);
        }
        Ok(None)
    }

    fn known_literal_label(&self, label: ExpressionId) -> Option<Field> {
        let ExpressionKind::Literal { literal: Literal::String(label) } = &self.storage[label].kind
        else {
            return None;
        };
        let label = label.to_utf8().ok()?;
        Some(self.label_field(label.into()))
    }

    fn known_record_constraints(&self, constraints: &[ExpressionId]) -> bool {
        let mut constraints = constraints.iter();
        constraints.all(|constraint| {
            matches!(self.storage[*constraint].kind, ExpressionKind::TrivialEvidence)
        })
    }

    fn record_set(
        &mut self,
        field: Field,
        value: ExpressionId,
        record: ExpressionId,
    ) -> ConversionResult<ExpressionId> {
        let mut operands = [value, record];
        let bindings = self.ordered_operands(&mut operands, &["value", "record"], &[1, 0])?;
        let [value, record] = operands;
        let updates = [RecordUpdate::Leaf { field, expression: value }];
        let updated =
            self.expression(ExpressionKind::RecordUpdate { record, updates: updates.into() });
        Ok(self.bound_operands(bindings, updated))
    }

    fn record_delete(
        &mut self,
        field: Field,
        record: ExpressionId,
    ) -> ConversionResult<ExpressionId> {
        let Some(fields) = self.record_literal_fields(record) else {
            return Ok(self.expression(ExpressionKind::RecordDelete { record, field }));
        };
        let uses = (0..fields.len()).filter(|&index| fields[index].field.name != field.name);
        let uses = uses.collect::<Vec<_>>();
        let (bindings, operands) = self.ordered_record_fields(&fields, &uses)?;
        let fields = uses.iter().map(|&index| RecordField {
            field: Field::clone(&fields[index].field),
            expression: operands[index],
        });
        let record = self.expression(ExpressionKind::Record { fields: fields.collect() });
        Ok(self.bound_operands(bindings, record))
    }

    fn ordered_record_fields(
        &mut self,
        fields: &[RecordField],
        uses: &[usize],
    ) -> ConversionResult<(Vec<Binding>, Vec<ExpressionId>)> {
        let names = fields.iter().map(|field| field.field.name.as_str()).collect::<Vec<_>>();
        let mut operands = fields.iter().map(|field| field.expression).collect::<Vec<_>>();
        let bindings = self.ordered_operands(&mut operands, &names, uses)?;
        Ok((bindings, operands))
    }

    fn record_union(
        &mut self,
        left: ExpressionId,
        right: ExpressionId,
    ) -> ConversionResult<Option<ExpressionId>> {
        let left_fields = self.record_literal_fields(left);
        let right_fields = self.record_literal_fields(right);
        if left_fields.is_none() && right_fields.is_none() {
            return Ok(None);
        }

        let mut names = Vec::new();
        let mut operands = Vec::new();
        for (side, fields) in [(left, &left_fields), (right, &right_fields)] {
            match fields {
                Some(fields) => {
                    names.extend(fields.iter().map(|field| SmolStr::clone(&field.field.name)));
                    operands.extend(fields.iter().map(|field| field.expression));
                }
                None => {
                    names.push(SmolStr::new_static("record"));
                    operands.push(side);
                }
            }
        }
        let names = names.iter().map(SmolStr::as_str).collect::<Vec<_>>();
        let left_count = left_fields.as_ref().map_or(1, Vec::len);

        if let (Some(left_fields), Some(right_fields)) = (&left_fields, &right_fields) {
            let mut uses = Vec::with_capacity(operands.len());
            for (index, right_field) in right_fields.iter().enumerate() {
                let overriding = left_fields
                    .iter()
                    .position(|left_field| left_field.field.name == right_field.field.name);
                uses.push(overriding.unwrap_or(left_count + index));
            }
            for (index, left_field) in left_fields.iter().enumerate() {
                let overridden = right_fields
                    .iter()
                    .any(|right_field| right_field.field.name == left_field.field.name);
                if !overridden {
                    uses.push(index);
                }
            }
            let bindings = self.ordered_operands(&mut operands, &names, &uses)?;
            let fields = uses.iter().map(|&index| {
                let field = if index < left_count {
                    &left_fields[index].field
                } else {
                    &right_fields[index - left_count].field
                };
                RecordField { field: Field::clone(field), expression: operands[index] }
            });
            let record = self.expression(ExpressionKind::Record { fields: fields.collect() });
            return Ok(Some(self.bound_operands(bindings, record)));
        }

        let uses = (left_count..operands.len()).chain(0..left_count).collect::<Vec<_>>();
        let bindings = self.ordered_operands(&mut operands, &names, &uses)?;
        let (left_operands, right_operands) = operands.split_at(left_count);
        let left = self.record_side(left, left_fields, left_operands);
        let right = self.record_side(right, right_fields, right_operands);
        let union = self.expression(ExpressionKind::RecordUnion { left, right });
        Ok(Some(self.bound_operands(bindings, union)))
    }

    fn record_literal_fields(&self, expression: ExpressionId) -> Option<Vec<RecordField>> {
        let ExpressionKind::Record { fields } = &self.storage[expression].kind else {
            return None;
        };
        Some(fields.to_vec())
    }

    fn record_side(
        &mut self,
        side: ExpressionId,
        fields: Option<Vec<RecordField>>,
        operands: &[ExpressionId],
    ) -> ExpressionId {
        let Some(fields) = fields else { return operands[0] };
        let fields = fields
            .into_iter()
            .zip(operands)
            .map(|(field, &expression)| RecordField { field: field.field, expression });
        let fields = ExpressionKind::Record { fields: fields.collect() };
        self.storage.replace_expression_kind(side, fields);
        side
    }

    fn ordered_operands(
        &mut self,
        operands: &mut [ExpressionId],
        names: &[&str],
        uses: &[usize],
    ) -> ConversionResult<Vec<Binding>> {
        let unstable = |index: &usize| !self.expression_is_stable(operands[*index]);
        let sources = (0..operands.len()).filter(unstable);
        let evaluations = uses.iter().copied().filter(unstable);
        if sources.eq(evaluations) {
            return Ok(Vec::new());
        }

        let last = (0..operands.len()).rev().find(unstable);
        let unbound = last.filter(|last| uses.iter().filter(|index| *index == last).count() == 1);
        let mut bindings = Vec::new();
        for (index, (operand, name)) in operands.iter_mut().zip(names).enumerate() {
            if unbound == Some(index) || self.expression_is_stable(*operand) {
                continue;
            }
            let parameter = self.fresh_parameter(SmolStr::new(name))?;
            let local = self.expression(ExpressionKind::Local { parameter: parameter.clone() });
            let expression = std::mem::replace(operand, local);
            let source_order = bindings.len();
            bindings.push(Binding { parameter, expression, source_order, origin: None });
        }
        Ok(bindings)
    }

    fn bound_operands(&mut self, bindings: Vec<Binding>, body: ExpressionId) -> ExpressionId {
        if bindings.is_empty() {
            return body;
        }
        self.expression(ExpressionKind::Let { recursive: false, bindings: bindings.into(), body })
    }
}
