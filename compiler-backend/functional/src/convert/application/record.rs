use smol_str::SmolStr;

use crate::convert::{Context, ConversionResult};
use crate::tree::{
    Binding, ExpressionId, ExpressionKind, Field, RecordUpdate, SynthesizedEvidence,
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
            let mut operands = [value, record];
            let bindings = self.ordered_operands(&mut operands, &["value", "record"], &[1, 0])?;
            let [value, record] = operands;
            let updates = [RecordUpdate::Leaf { field, expression: value }];
            let updated =
                self.expression(ExpressionKind::RecordUpdate { record, updates: updates.into() });
            return Ok(Some(self.bound_operands(bindings, updated)));
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
            return Ok(Some(self.expression(ExpressionKind::RecordDelete { record, field })));
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
        let mut constraints = constraints.iter();
        constraints.all(|constraint| {
            matches!(self.storage[*constraint].kind, ExpressionKind::TrivialEvidence)
        })
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
