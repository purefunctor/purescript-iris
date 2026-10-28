use smol_str::SmolStr;

use crate::convert::{Context, ConversionResult};
use crate::tree::{Binding, ExpressionId, ExpressionKind, Field, RecordField, RecordUpdate};

const RECORD_BUILDER: &str = "Record.Builder";

enum BuilderStep {
    Insert { field: Field, value: usize },
    Modify { field: Field, function: usize },
    Delete { field: Field },
    Rename { from: Field, to: Field },
    Union { record: usize },
}

struct BuilderOperand {
    expression: ExpressionId,
    name: &'static str,
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
        let record = match record {
            Some(record) => record,
            None => self.expression(ExpressionKind::Record { fields: [].into() }),
        };
        operands.push(BuilderOperand { expression: record, name: "record" });

        let bindings = self.bind_record_builder_operands(&mut operands)?;
        let mut record = operands[operands.len() - 1].expression;
        for step in steps {
            record = self.record_builder_step(record, step, &operands)?;
        }
        Ok(Some(self.bound_operands(bindings, record)))
    }

    fn record_builder_steps(
        &self,
        builder: ExpressionId,
        operands: &mut Vec<BuilderOperand>,
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
            Some((outer, inner, true))
        } else {
            None
        };
        if let Some((outer, inner, flipped)) = composition {
            let (first, second) = if flipped { (inner, outer) } else { (outer, inner) };
            let Some(first) = self.record_builder_steps(first, operands)? else {
                return Ok(None);
            };
            let Some(second) = self.record_builder_steps(second, operands)? else {
                return Ok(None);
            };
            let (inner, outer) = if flipped { (first, second) } else { (second, first) };
            return Ok(Some(inner.into_iter().chain(outer).collect()));
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
            operands.push(BuilderOperand { expression: value, name: "value" });
            BuilderStep::Insert { field, value: operands.len() - 1 }
        } else if self.known_term(function, RECORD_BUILDER, "modify")?
            && let [first, second, symbol, proxy, function] = *arguments
            && let Some(field) = self.known_record_label(symbol, &[first, second], proxy)
        {
            operands.push(BuilderOperand { expression: function, name: "modify" });
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
        } else if let Some(record) = self.known_record_builder_union(function, &arguments)? {
            operands.push(BuilderOperand { expression: record, name: "record" });
            BuilderStep::Union { record: operands.len() - 1 }
        } else {
            return Ok(None);
        };
        Ok(Some(vec![step]))
    }

    fn known_record_builder_union(
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

    fn bind_record_builder_operands(
        &mut self,
        operands: &mut [BuilderOperand],
    ) -> ConversionResult<Vec<Binding>> {
        let mut names = Vec::new();
        let mut expressions = Vec::new();
        let mut literals = Vec::with_capacity(operands.len());
        for operand in operands.iter() {
            let fields = self.record_literal_fields(operand.expression);
            match &fields {
                Some(fields) => {
                    names.extend(fields.iter().map(|field| SmolStr::clone(&field.field.name)));
                    expressions.extend(fields.iter().map(|field| field.expression));
                }
                None => {
                    names.push(SmolStr::new_static(operand.name));
                    expressions.push(operand.expression);
                }
            }
            literals.push(fields);
        }
        let names = names.iter().map(SmolStr::as_str).collect::<Vec<_>>();
        let bindings = self.ordered_operands(&mut expressions, &names, &[])?;

        let mut expressions = expressions.as_slice();
        for (operand, fields) in operands.iter_mut().zip(literals) {
            let count = fields.as_ref().map_or(1, Vec::len);
            let (current, remaining) = expressions.split_at(count);
            operand.expression = self.record_side(operand.expression, fields, current);
            expressions = remaining;
        }
        Ok(bindings)
    }

    fn record_builder_step(
        &mut self,
        record: ExpressionId,
        step: BuilderStep,
        operands: &[BuilderOperand],
    ) -> ConversionResult<ExpressionId> {
        match step {
            BuilderStep::Insert { field, value } => {
                Ok(self.record_insertion(record, field, operands[value].expression))
            }
            BuilderStep::Modify { field, function } => {
                let function = operands[function].expression;
                self.record_modification(record, field, function)
            }
            BuilderStep::Delete { field } => self.record_delete(field, record),
            BuilderStep::Rename { from, to } => self.record_rename(record, from, to),
            BuilderStep::Union { record: left } => {
                let left = operands[left].expression;
                if let Some(union) = self.record_union(left, record)? {
                    return Ok(union);
                }
                Ok(self.expression(ExpressionKind::RecordUnion { left, right: record }))
            }
        }
    }

    fn record_insertion(
        &mut self,
        record: ExpressionId,
        field: Field,
        expression: ExpressionId,
    ) -> ExpressionId {
        let kind = match &self.storage[record].kind {
            ExpressionKind::Record { fields } => {
                let mut fields = fields.to_vec();
                fields.push(RecordField { field, expression });
                ExpressionKind::Record { fields: fields.into() }
            }
            ExpressionKind::RecordUpdate { record, updates }
                if updates.iter().all(|update| matches!(update, RecordUpdate::Leaf { .. })) =>
            {
                let mut updates = updates.to_vec();
                updates.push(RecordUpdate::Leaf { field, expression });
                ExpressionKind::RecordUpdate { record: *record, updates: updates.into() }
            }
            _ => {
                let updates = [RecordUpdate::Leaf { field, expression }];
                ExpressionKind::RecordUpdate { record, updates: updates.into() }
            }
        };
        self.expression(kind)
    }

    fn record_modification(
        &mut self,
        record: ExpressionId,
        field: Field,
        function: ExpressionId,
    ) -> ConversionResult<ExpressionId> {
        if let Some(fields) = self.record_literal_fields(record)
            && fields.iter().all(|candidate| self.expression_is_stable(candidate.expression))
        {
            let mut modified = Vec::with_capacity(fields.len());
            for RecordField { field: current, expression } in fields {
                let expression = if current.name == field.name {
                    self.application(function, [expression])?
                } else {
                    expression
                };
                modified.push(RecordField { field: current, expression });
            }
            return Ok(self.expression(ExpressionKind::Record { fields: modified.into() }));
        }

        let mut operands = [record];
        let bindings = self.ordered_operands(&mut operands, &["record"], &[0, 0])?;
        let [record] = operands;
        let current = self.expression(ExpressionKind::Project { record, field: field.clone() });
        let modified = self.application(function, [current])?;
        let updates = [RecordUpdate::Leaf { field, expression: modified }];
        let updated =
            self.expression(ExpressionKind::RecordUpdate { record, updates: updates.into() });
        Ok(self.bound_operands(bindings, updated))
    }

    fn record_rename(
        &mut self,
        record: ExpressionId,
        from: Field,
        to: Field,
    ) -> ConversionResult<ExpressionId> {
        if let Some(fields) = self.record_literal_fields(record) {
            let (renamed, mut fields): (Vec<_>, Vec<_>) =
                fields.into_iter().partition(|field| field.field.name == from.name);
            let renamed = renamed.into_iter().map(|field| RecordField {
                field: Field::clone(&to),
                expression: field.expression,
            });
            fields.extend(renamed);
            return Ok(self.expression(ExpressionKind::Record { fields: fields.into() }));
        }

        let mut operands = [record];
        let bindings = self.ordered_operands(&mut operands, &["record"], &[0, 0])?;
        let [record] = operands;
        let value = self.expression(ExpressionKind::Project { record, field: from.clone() });
        let deleted = self.expression(ExpressionKind::RecordDelete { record, field: from });
        let updates = [RecordUpdate::Leaf { field: to, expression: value }];
        let renamed = self
            .expression(ExpressionKind::RecordUpdate { record: deleted, updates: updates.into() });
        Ok(self.bound_operands(bindings, renamed))
    }
}
