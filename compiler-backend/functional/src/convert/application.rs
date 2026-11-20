use std::sync::Arc;

use building_types::QueryResult;
use files::FileId;
use indexing::{IndexedTermItemKind, TypeItemId};
use itertools::Itertools;
use smol_str::{SmolStr, format_smolstr};

use crate::optimize::local_uses;
use crate::tree::{
    ArrayElement, BinaryOperator, Binding, CaseAlternative, Comparison, EffectExpression,
    EffectLoop, EffectStatement, ExpressionId, ExpressionKind, FieldIdentity, Global, GlobalId,
    InstanceIdentity, Literal, Parameter, PatternId, PatternKind, PrimitiveType, RecordField,
    RecordUpdate, UnaryOperator,
};

use super::known::{KnownInstance, KnownTerm};
use super::{Context, ConversionResult};

mod builder;

const BINARY_INSTANCE_MEMBERS: &[(KnownTerm, KnownInstance, BinaryOperator)] = &[
    (
        KnownTerm::DataHeytingAlgebraConj,
        KnownInstance::HeytingAlgebraBoolean,
        BinaryOperator::BooleanAnd,
    ),
    (
        KnownTerm::DataHeytingAlgebraDisj,
        KnownInstance::HeytingAlgebraBoolean,
        BinaryOperator::BooleanOr,
    ),
    (KnownTerm::DataSemiringAdd, KnownInstance::SemiringInt, BinaryOperator::IntegerAdd),
    (KnownTerm::DataRingSub, KnownInstance::RingInt, BinaryOperator::IntegerSubtract),
    (KnownTerm::DataSemiringMul, KnownInstance::SemiringInt, BinaryOperator::IntegerMultiply),
    (KnownTerm::DataSemiringAdd, KnownInstance::SemiringNumber, BinaryOperator::NumberAdd),
    (KnownTerm::DataRingSub, KnownInstance::RingNumber, BinaryOperator::NumberSubtract),
    (KnownTerm::DataSemiringMul, KnownInstance::SemiringNumber, BinaryOperator::NumberMultiply),
    (
        KnownTerm::DataEuclideanRingDiv,
        KnownInstance::EuclideanRingInt,
        BinaryOperator::IntegerDivide,
    ),
    (
        KnownTerm::DataEuclideanRingMod,
        KnownInstance::EuclideanRingInt,
        BinaryOperator::IntegerModulo,
    ),
    (
        KnownTerm::DataEuclideanRingDiv,
        KnownInstance::EuclideanRingNumber,
        BinaryOperator::NumberDivide,
    ),
    (KnownTerm::DataSemigroupAppend, KnownInstance::SemigroupString, BinaryOperator::StringAppend),
];

const BINARY_FUNCTIONS: &[(KnownTerm, BinaryOperator)] = &[
    (KnownTerm::DataIntBitsAnd, BinaryOperator::IntegerAnd),
    (KnownTerm::DataIntBitsOr, BinaryOperator::IntegerOr),
    (KnownTerm::DataIntBitsXor, BinaryOperator::IntegerXor),
    (KnownTerm::DataIntBitsShl, BinaryOperator::IntegerShiftLeft),
    (KnownTerm::DataIntBitsShr, BinaryOperator::IntegerShiftRight),
    (KnownTerm::DataIntBitsZshr, BinaryOperator::IntegerZeroFillShiftRight),
];

const UNARY_FUNCTIONS: &[(KnownTerm, UnaryOperator)] = &[
    (KnownTerm::DataIntBitsComplement, UnaryOperator::IntegerComplement),
    (KnownTerm::DataArrayLength, UnaryOperator::ArrayLength),
];

const EQ_INSTANCES: &[(KnownInstance, PrimitiveType)] = &[
    (KnownInstance::EqBoolean, PrimitiveType::Boolean),
    (KnownInstance::EqInt, PrimitiveType::Integer),
    (KnownInstance::EqNumber, PrimitiveType::Number),
    (KnownInstance::EqChar, PrimitiveType::Char),
    (KnownInstance::EqString, PrimitiveType::String),
];

const ORD_INSTANCES: &[(KnownInstance, PrimitiveType)] = &[
    (KnownInstance::OrdBoolean, PrimitiveType::Boolean),
    (KnownInstance::OrdInt, PrimitiveType::Integer),
    (KnownInstance::OrdNumber, PrimitiveType::Number),
    (KnownInstance::OrdChar, PrimitiveType::Char),
    (KnownInstance::OrdString, PrimitiveType::String),
];

const ORD_FUNCTIONS: &[(KnownTerm, Comparison)] = &[
    (KnownTerm::DataOrdLessThan, Comparison::LessThan),
    (KnownTerm::DataOrdLessThanOrEq, Comparison::LessThanOrEqual),
    (KnownTerm::DataOrdGreaterThan, Comparison::GreaterThan),
    (KnownTerm::DataOrdGreaterThanOrEq, Comparison::GreaterThanOrEqual),
];

#[derive(Clone, Copy)]
enum ModifyResult {
    State,
    Record,
    Unit,
}

impl<'c, Q> Context<'c, Q>
where
    Q: checking::ExternalQueries,
{
    pub(super) fn application(
        &mut self,
        function: ExpressionId,
        arguments: impl IntoIterator<Item = ExpressionId>,
    ) -> ConversionResult<ExpressionId> {
        self.application_with_synthetic(function, arguments, false, None)
    }

    pub(super) fn typed_application(
        &mut self,
        function: ExpressionId,
        arguments: impl IntoIterator<Item = ExpressionId>,
        result_type: checking::TypeId,
    ) -> ConversionResult<ExpressionId> {
        self.application_with_synthetic(function, arguments, false, Some(result_type))
    }

    pub(super) fn synthetic_application(
        &mut self,
        function: ExpressionId,
        arguments: impl IntoIterator<Item = ExpressionId>,
    ) -> ConversionResult<ExpressionId> {
        self.application_with_synthetic(function, arguments, true, None)
    }

    fn application_with_synthetic(
        &mut self,
        function: ExpressionId,
        arguments: impl IntoIterator<Item = ExpressionId>,
        synthetic: bool,
        result_type: Option<checking::TypeId>,
    ) -> ConversionResult<ExpressionId> {
        let arguments = arguments.into_iter();
        let arguments = arguments.collect_vec();
        if arguments.is_empty() {
            return Ok(function);
        }
        let mut synthetic = synthetic || self.application_is_synthetic(function);
        let (known_function, known_arguments) = self.application_spine(function, &arguments);
        if let ExpressionKind::Constructor { global } = &self.storage[known_function].kind {
            let global = global.clone();
            let GlobalId::Term(file_id, term_id) = global.id else {
                unreachable!("invariant violated: constructor has a non-term global identity")
            };
            let arity = self.constructor_arity(file_id, term_id)?;
            if known_arguments.len() == arity {
                return Ok(self.constructor_value(&global.item_name, known_arguments));
            }
            synthetic = true;
        }
        if let Some(known) = self.known_term(known_function)
            && let Some(expression) = self.known_application(
                known_function,
                known,
                &known_arguments,
                synthetic,
                result_type,
            )?
        {
            return Ok(expression);
        }
        if let Some(expression) =
            self.stylex_intrinsic(known_function, &known_arguments, result_type)?
        {
            return Ok(expression);
        }
        Ok(self.expression(ExpressionKind::Application {
            function,
            arguments: arguments.into(),
            synthetic,
        }))
    }

    fn known_application(
        &mut self,
        known_function: ExpressionId,
        known: KnownTerm,
        known_arguments: &[ExpressionId],
        synthetic: bool,
        result_type: Option<checking::TypeId>,
    ) -> ConversionResult<Option<ExpressionId>> {
        if let KnownTerm::DataFunctionUncurriedMkFn(arity) = known
            && arity >= 1
            && let [function] = known_arguments
            && let Some((parameters, body)) = self.uncurried_parameters(*function, arity)
        {
            let parameters = parameters.into();
            let abstraction = ExpressionKind::UncurriedAbstraction { parameters, body };
            return Ok(Some(self.expression(abstraction)));
        }
        if let KnownTerm::DataFunctionUncurriedRunFn(arity) = known
            && let Some((function, arguments)) = known_arguments.split_first()
            && arguments.len() == arity
        {
            if self.known_term(*function) == Some(KnownTerm::RecordUnsafeUnionUnsafeUnionFn)
                && let [left, right] = arguments
                && let Some(fields) = self.literal_record_union(*left, *right)
            {
                return Ok(Some(self.expression(ExpressionKind::Record { fields })));
            }
            return Ok(Some(self.expression(ExpressionKind::UncurriedApplication {
                function: *function,
                arguments: arguments.into(),
                synthetic,
            })));
        }
        if let KnownTerm::EffectUncurriedMkEffectFn(arity)
        | KnownTerm::ControlMonadSTUncurriedMkSTFn(arity) = known
            && arity >= 1
            && let [function] = known_arguments
            && let Some((parameters, action)) = self.uncurried_parameters(*function, arity)
        {
            let body = self.expression(ExpressionKind::UncurriedApplication {
                function: action,
                arguments: [].into(),
                synthetic: false,
            });
            let parameters = parameters.into();
            let abstraction = ExpressionKind::UncurriedAbstraction { parameters, body };
            return Ok(Some(self.expression(abstraction)));
        }
        if let KnownTerm::EffectUncurriedRunEffectFn(arity)
        | KnownTerm::ControlMonadSTUncurriedRunSTFn(arity) = known
            && arity >= 1
            && let Some((function, arguments)) = known_arguments.split_first()
            && arguments.len() == arity
        {
            let effect = EffectExpression::UncurriedApplication {
                function: *function,
                arguments: arguments.into(),
            };
            return Ok(Some(self.expression(ExpressionKind::Effect { effect })));
        }
        if known == KnownTerm::DataFunctionApply
            && let [function, argument] = known_arguments
        {
            let application =
                self.application_with_synthetic(*function, [*argument], synthetic, result_type)?;
            return Ok(Some(application));
        }
        if known == KnownTerm::DataFunctionApplyFlipped
            && let [argument, function] = known_arguments
        {
            let application =
                self.flipped_application(*argument, *function, synthetic, result_type)?;
            return Ok(Some(application));
        }
        if let Some(composition) =
            self.known_composition_application(known, known_arguments, synthetic)?
        {
            return Ok(Some(composition));
        }
        if let Some(record) = self.known_record_build(known, known_arguments)? {
            return Ok(Some(record));
        }
        if let Some(reference) = self.known_reference_application(known, known_arguments)? {
            return Ok(Some(reference));
        }
        if let Some(expression) = self.known_service_application(known, known_arguments)? {
            return Ok(Some(expression));
        }
        if let Some(expression) = self.known_sync_application(known, known_arguments)? {
            return Ok(Some(expression));
        }
        if let Some(expression) = self.known_async_application(known, known_arguments)? {
            return Ok(Some(expression));
        }
        if let Some(effect) =
            self.known_effect_application(known_function, known, known_arguments)?
        {
            return Ok(Some(self.expression(ExpressionKind::Effect { effect })));
        }
        if let Some([argument]) = self.known_instance_member_arguments(
            known,
            known_arguments,
            KnownTerm::ControlCategoryIdentity,
            KnownInstance::CategoryFunction,
        ) {
            return Ok(Some(*argument));
        }
        if known == KnownTerm::UnsafeCoerceUnsafeCoerce
            && let [argument] = known_arguments
        {
            return Ok(Some(*argument));
        }
        if matches!(
            known,
            KnownTerm::EffectUnsafePerformEffect | KnownTerm::ControlMonadSTInternalRun
        ) && let [action] = known_arguments
            && let ExpressionKind::Effect { effect: EffectExpression::Pure(value) } =
                &self.storage[*action].kind
        {
            return Ok(Some(*value));
        }
        if known == KnownTerm::PartialUnsafeUnsafePartial
            && let [function, arguments @ ..] = known_arguments
            && let Some(body) = self.discharged_partial_abstraction(*function)
        {
            let application = self.application_with_synthetic(
                body,
                arguments.iter().copied(),
                synthetic,
                result_type,
            )?;
            return Ok(Some(application));
        }
        if known == KnownTerm::SafeCoerceCoerce
            && let [_, argument] = known_arguments
        {
            return Ok(Some(*argument));
        }
        if let Some(kind) = self.known_operator_application(known, known_arguments)? {
            return Ok(Some(self.expression(kind)));
        }
        Ok(None)
    }

    pub(super) fn constructor_value(
        &mut self,
        name: &str,
        arguments: impl IntoIterator<Item = ExpressionId>,
    ) -> ExpressionId {
        let tag =
            self.expression(ExpressionKind::Literal { literal: Literal::String(name.into()) });
        let mut fields =
            vec![RecordField { field: self.label_field("tag".into()), expression: tag }];
        for (index, expression) in arguments.into_iter().enumerate() {
            let field = self.label_field(format_smolstr!("_{}", index + 1));
            fields.push(RecordField { field, expression });
        }
        self.expression(ExpressionKind::Record { fields: fields.into() })
    }

    fn known_composition_application(
        &mut self,
        known: KnownTerm,
        arguments: &[ExpressionId],
        synthetic: bool,
    ) -> ConversionResult<Option<ExpressionId>> {
        if let Some(arguments) = self.known_instance_member_arguments(
            known,
            arguments,
            KnownTerm::ControlSemigroupoidCompose,
            KnownInstance::SemigroupoidFunction,
        ) && let [outer, inner, remaining @ ..] = arguments
            && let [] | [_] = remaining
        {
            let composition = self.function_composition(
                *outer,
                *inner,
                remaining.first().copied(),
                false,
                synthetic,
            )?;
            return Ok(Some(composition));
        }
        if known == KnownTerm::ControlSemigroupoidComposeFlipped
            && let [dictionary, inner, outer, remaining @ ..] = arguments
            && let [] | [_] = remaining
            && self.known_instance(*dictionary) == Some(KnownInstance::SemigroupoidFunction)
        {
            let composition = self.function_composition(
                *outer,
                *inner,
                remaining.first().copied(),
                true,
                synthetic,
            )?;
            return Ok(Some(composition));
        }
        Ok(None)
    }

    fn known_operator_application(
        &mut self,
        known: KnownTerm,
        arguments: &[ExpressionId],
    ) -> QueryResult<Option<ExpressionKind>> {
        if let Some([value]) = self.known_instance_member_arguments(
            known,
            arguments,
            KnownTerm::DataHeytingAlgebraNot,
            KnownInstance::HeytingAlgebraBoolean,
        ) {
            return Ok(Some(ExpressionKind::Unary {
                operator: UnaryOperator::BooleanNot,
                value: *value,
            }));
        }
        if known == KnownTerm::DataRingNegate
            && let [dictionary, value] = arguments
        {
            if self.known_instance(*dictionary) == Some(KnownInstance::RingInt) {
                return Ok(Some(ExpressionKind::Unary {
                    operator: UnaryOperator::IntegerNegate,
                    value: *value,
                }));
            }
            if self.known_instance(*dictionary) == Some(KnownInstance::RingNumber) {
                return Ok(Some(ExpressionKind::Unary {
                    operator: UnaryOperator::NumberNegate,
                    value: *value,
                }));
            }
        }
        for &(member, instance, operator) in BINARY_INSTANCE_MEMBERS {
            if let Some([left, right]) =
                self.known_instance_member_arguments(known, arguments, member, instance)
            {
                return Ok(Some(ExpressionKind::Binary { operator, left: *left, right: *right }));
            }
        }
        for &(function, operator) in BINARY_FUNCTIONS {
            if known == function
                && let [left, right] = arguments
            {
                return Ok(Some(ExpressionKind::Binary { operator, left: *left, right: *right }));
            }
        }
        for &(function, operator) in UNARY_FUNCTIONS {
            if known == function
                && let [value] = arguments
            {
                return Ok(Some(ExpressionKind::Unary { operator, value: *value }));
            }
        }
        if let Some([left, right]) = self.known_instance_member_arguments(
            known,
            arguments,
            KnownTerm::DataSemigroupAppend,
            KnownInstance::SemigroupArray,
        ) {
            return Ok(Some(self.array_concatenation(*left, *right)));
        }
        if known == KnownTerm::DataSemigroupConcatArray
            && let [left, right] = arguments
        {
            return Ok(Some(self.array_concatenation(*left, *right)));
        }
        if known == KnownTerm::DataStringCodePointsToCodePointArray
            && let [string] = arguments
            && let ExpressionKind::Literal { literal: Literal::String(string) } =
                &self.storage[*string].kind
        {
            let code_points = char::decode_utf16(string.as_utf16().iter().copied());
            let code_points = code_points.map(|code_point| match code_point {
                Ok(character) => u32::from(character),
                Err(error) => u32::from(error.unpaired_surrogate()),
            });
            let code_points = code_points.collect_vec();
            let elements = code_points.into_iter().map(|code_point| {
                let literal = Literal::Integer(i64::from(code_point));
                self.expression(ExpressionKind::Literal { literal })
            });
            return Ok(Some(ExpressionKind::Array { elements: elements.collect() }));
        }
        if known == KnownTerm::DataArrayUnsafeIndex
            && let [_, value, index] = arguments
        {
            return Ok(Some(ExpressionKind::Index { value: *value, index: *index }));
        }
        if known == KnownTerm::RecordUnsafeUnsafeGet
            && let [label, record] = arguments
            && let Some(label) = self.literal_label(*label)
        {
            let field = self.label_field(label);
            return Ok(Some(ExpressionKind::Project { record: *record, field }));
        }
        if known == KnownTerm::RecordUnsafeUnsafeSet
            && let [label, value, record] = arguments
            && let Some(label) = self.literal_label(*label)
        {
            let field = self.label_field(label);
            let updates = [RecordUpdate::Leaf { field, expression: *value }].into();
            return Ok(Some(ExpressionKind::RecordUpdate { record: *record, updates }));
        }
        if known == KnownTerm::RecordUnsafeUnsafeHas
            && let [label, record] = arguments
            && let Some(label) = self.literal_label(*label)
            && let Some(fields) = self.literal_record_fields(*record)
        {
            let has = fields.iter().any(|field| field.field.name == label);
            return Ok(Some(ExpressionKind::Literal { literal: Literal::Boolean(has) }));
        }
        if known == KnownTerm::RecordUnsafeUnsafeDelete
            && let [label, record] = arguments
            && let Some(label) = self.literal_label(*label)
            && let Some(fields) = self.literal_record_fields(*record)
        {
            let fields = fields.iter().filter(|field| field.field.name != label).cloned();
            return Ok(Some(ExpressionKind::Record { fields: fields.collect() }));
        }
        if known == KnownTerm::RecordUnsafeUnionUnsafeUnion
            && let [left, right] = arguments
            && let Some(fields) = self.literal_record_union(*left, *right)
        {
            return Ok(Some(ExpressionKind::Record { fields }));
        }
        if known == KnownTerm::DataEqEq
            && let [record, left, right] = arguments
            && let Some(operand) = self.known_primitive_instance(*record, EQ_INSTANCES)
        {
            let operator = BinaryOperator::Comparison { operand, comparison: Comparison::Equal };
            return Ok(Some(ExpressionKind::Binary { operator, left: *left, right: *right }));
        }
        if known == KnownTerm::DataEqNotEq
            && let [dictionary, left, right] = arguments
            && let Some(operand) = self.known_primitive_instance(*dictionary, EQ_INSTANCES)
        {
            let operator = BinaryOperator::Comparison { operand, comparison: Comparison::NotEqual };
            return Ok(Some(ExpressionKind::Binary { operator, left: *left, right: *right }));
        }
        for &(function, comparison) in ORD_FUNCTIONS {
            if known == function
                && let [dictionary, left, right] = arguments
                && let Some(operand) = self.known_primitive_instance(*dictionary, ORD_INSTANCES)
            {
                let kind = self.primitive_comparison(operand, comparison, *left, *right);
                return Ok(Some(kind));
            }
        }
        Ok(None)
    }

    fn primitive_comparison(
        &mut self,
        operand: PrimitiveType,
        comparison: Comparison,
        left: ExpressionId,
        right: ExpressionId,
    ) -> ExpressionKind {
        let negated = match (operand, comparison) {
            (PrimitiveType::Number, Comparison::GreaterThan) => Some(Comparison::LessThanOrEqual),
            (PrimitiveType::Number, Comparison::GreaterThanOrEqual) => Some(Comparison::LessThan),
            _ => None,
        };
        let Some(negated) = negated else {
            let operator = BinaryOperator::Comparison { operand, comparison };
            return ExpressionKind::Binary { operator, left, right };
        };
        let operator = BinaryOperator::Comparison { operand, comparison: negated };
        let value = self.expression(ExpressionKind::Binary { operator, left, right });
        ExpressionKind::Unary { operator: UnaryOperator::BooleanNot, value }
    }

    pub(super) fn known_comparison_case(
        &mut self,
        scrutinees: &[ExpressionId],
        alternatives: &[CaseAlternative],
    ) -> ConversionResult<Option<ExpressionId>> {
        let [scrutinee] = scrutinees else { return Ok(None) };
        let (function, arguments) = self.application_spine(*scrutinee, &[]);
        if self.known_term(function) != Some(KnownTerm::DataOrdCompare) {
            return Ok(None);
        }
        let [record, left, right] = *arguments.as_slice() else { return Ok(None) };
        let Some(operand) = self.known_primitive_instance(record, ORD_INSTANCES) else {
            return Ok(None);
        };

        let mut branches = Vec::new();
        let mut covered = Vec::new();
        let mut fallback = None;
        for alternative in alternatives {
            let [pattern] = alternative.patterns.as_ref() else { return Ok(None) };
            if matches!(self.storage[alternative.expression].kind, ExpressionKind::Guarded { .. }) {
                return Ok(None);
            }
            match &self.storage[*pattern].kind {
                PatternKind::Wildcard => {
                    fallback = Some(alternative.expression);
                    break;
                }
                PatternKind::Variable(parameter)
                    if local_uses(&self.storage, alternative.expression, parameter.id) == 0 =>
                {
                    fallback = Some(alternative.expression);
                    break;
                }
                PatternKind::Constructor { global, arguments } if arguments.is_empty() => {
                    let GlobalId::Term(file_id, term_id) = global.id else { return Ok(None) };
                    let constructor = Some((file_id, term_id));
                    let known_terms = &self.checked_core.known_terms;
                    let comparison = if constructor == known_terms.ordering_lt {
                        Comparison::LessThan
                    } else if constructor == known_terms.ordering_eq {
                        Comparison::Equal
                    } else if constructor == known_terms.ordering_gt {
                        Comparison::GreaterThan
                    } else {
                        return Ok(None);
                    };
                    if !covered.contains(&comparison) {
                        covered.push(comparison);
                    }
                    branches.push((comparison, alternative.expression));
                }
                _ => return Ok(None),
            }
        }
        let fallback = match fallback {
            Some(fallback) => fallback,
            None if covered.len() == 3 => {
                let Some((_, fallback)) = branches.pop() else { return Ok(None) };
                fallback
            }
            None => return Ok(None),
        };

        let mut bindings = Vec::new();
        let mut operands = [left, right];
        for (operand, name) in operands.iter_mut().zip(["compareLeft", "compareRight"]) {
            if !self.expression_is_stable(*operand) {
                *operand = self.let_bound_local(*operand, name, &mut bindings)?;
            }
        }

        let mut expression = fallback;
        for (comparison, branch) in branches.into_iter().rev() {
            let [left, right] = operands
                .map(|operand| self.expression(ExpressionKind::clone(&self.storage[operand].kind)));
            let condition = self.primitive_comparison(operand, comparison, left, right);
            let condition = self.expression(condition);
            expression = self.expression(ExpressionKind::IfThenElse {
                condition,
                then: branch,
                else_: expression,
            });
        }

        if bindings.is_empty() {
            return Ok(Some(expression));
        }
        Ok(Some(self.expression(ExpressionKind::Let {
            recursive: false,
            bindings: bindings.into(),
            body: expression,
        })))
    }

    fn literal_label(&self, expression: ExpressionId) -> Option<SmolStr> {
        let ExpressionKind::Literal { literal: Literal::String(label) } =
            &self.storage[expression].kind
        else {
            return None;
        };
        label.to_utf8().ok().map(SmolStr::from)
    }

    fn literal_record_fields(&self, expression: ExpressionId) -> Option<&Arc<[RecordField]>> {
        let ExpressionKind::Record { fields } = &self.storage[expression].kind else {
            return None;
        };
        let labelled =
            fields.iter().all(|field| matches!(field.field.identity, FieldIdentity::Label(_)));
        labelled.then_some(fields)
    }

    fn literal_record_union(
        &self,
        left: ExpressionId,
        right: ExpressionId,
    ) -> Option<Arc<[RecordField]>> {
        let left = self.literal_record_fields(left)?;
        let right = self.literal_record_fields(right)?;
        let right = right.iter().filter(|field| {
            left.iter().all(|left_field| left_field.field.name != field.field.name)
        });
        Some(left.iter().chain(right).cloned().collect())
    }

    fn array_concatenation(&self, left: ExpressionId, right: ExpressionId) -> ExpressionKind {
        let mut elements = Vec::new();
        for operand in [left, right] {
            match &self.storage[operand].kind {
                ExpressionKind::Array { elements: values } => {
                    elements.extend(values.iter().copied().map(ArrayElement::Value));
                }
                ExpressionKind::ArrayConcatenation { elements: operand_elements } => {
                    elements.extend(operand_elements.iter().copied());
                }
                _ => elements.push(ArrayElement::Spread(operand)),
            }
        }
        let values = elements.iter().map(|element| match element {
            ArrayElement::Value(value) => Some(*value),
            ArrayElement::Spread(_) => None,
        });
        match values.collect::<Option<Vec<_>>>() {
            Some(values) => ExpressionKind::Array { elements: values.into() },
            None => ExpressionKind::ArrayConcatenation { elements: elements.into() },
        }
    }

    fn known_effect_application(
        &mut self,
        function: ExpressionId,
        known: KnownTerm,
        arguments: &[ExpressionId],
    ) -> ConversionResult<Option<EffectExpression>> {
        let thunk_member = matches!(
            known,
            KnownTerm::ControlApplicativePure
                | KnownTerm::ControlBindBind
                | KnownTerm::DataFunctorMap
                | KnownTerm::ControlApplyApply
        );
        if thunk_member
            && let Some((dictionary, operands)) = arguments.split_first()
            && self.known_thunk_instance(*dictionary, Some(self.class_member_parent(function)?))
        {
            match (known, operands) {
                (KnownTerm::ControlApplicativePure, [value]) => {
                    return Ok(Some(EffectExpression::Pure(*value)));
                }
                (KnownTerm::ControlBindBind, [action, continuation]) => {
                    if let Some((parameter, body)) = self.effect_continuation(*continuation)? {
                        return Ok(Some(EffectExpression::Bind {
                            action: *action,
                            parameter,
                            body,
                        }));
                    }
                }
                (KnownTerm::DataFunctorMap, [function, action]) => {
                    return Ok(Some(EffectExpression::Map {
                        function: *function,
                        action: *action,
                    }));
                }
                (KnownTerm::ControlApplyApply, [function_action, argument_action]) => {
                    return Ok(Some(EffectExpression::Apply {
                        function_action: *function_action,
                        argument_action: *argument_action,
                    }));
                }
                _ => (),
            }
        }
        if let Some((action, continuation)) =
            self.known_discard_arguments(known, arguments, |context, dictionary| {
                context.known_thunk_instance(dictionary, None)
            })
            && let Some((parameter, body)) = self.effect_continuation(continuation)?
        {
            return Ok(Some(EffectExpression::Bind { action, parameter, body }));
        }
        if let Some((condition, action)) =
            self.known_conditional_arguments(known, arguments, |context, dictionary| {
                context.known_thunk_instance(dictionary, None)
            })
        {
            return self.unit_statement(EffectStatement::When { condition, action });
        }
        let Some(effect_loop) = known_effect_loop(known, arguments) else {
            return Ok(None);
        };
        self.unit_statement(EffectStatement::Loop(effect_loop))
    }

    pub(super) fn unit_statement(
        &mut self,
        statement: EffectStatement,
    ) -> ConversionResult<Option<EffectExpression>> {
        let Some(unit) = self.unit_value()? else { return Ok(None) };
        Ok(Some(EffectExpression::Statement { statement, unit }))
    }

    pub(super) fn known_discard_arguments(
        &self,
        known: KnownTerm,
        arguments: &[ExpressionId],
        known_bind: impl Fn(&Self, ExpressionId) -> bool,
    ) -> Option<(ExpressionId, ExpressionId)> {
        if let Some([dictionary, action, continuation]) = self.known_instance_member_arguments(
            known,
            arguments,
            KnownTerm::ControlBindDiscard,
            KnownInstance::DiscardUnit,
        ) && known_bind(self, *dictionary)
        {
            return Some((*action, *continuation));
        }
        None
    }

    pub(super) fn known_conditional_arguments(
        &mut self,
        known: KnownTerm,
        arguments: &[ExpressionId],
        known_applicative: impl Fn(&Self, ExpressionId) -> bool,
    ) -> Option<(ExpressionId, ExpressionId)> {
        for (function, negated) in [
            (KnownTerm::ControlApplicativeWhen, false),
            (KnownTerm::ControlApplicativeUnless, true),
        ] {
            if known == function
                && let [dictionary, condition, action] = *arguments
                && known_applicative(self, dictionary)
            {
                let condition = if negated {
                    let operator = UnaryOperator::BooleanNot;
                    self.expression(ExpressionKind::Unary { operator, value: condition })
                } else {
                    condition
                };
                return Some((condition, action));
            }
        }
        None
    }

    fn known_reference_application(
        &mut self,
        known: KnownTerm,
        arguments: &[ExpressionId],
    ) -> ConversionResult<Option<ExpressionId>> {
        let effect = match (known, arguments) {
            (KnownTerm::EffectRefNew | KnownTerm::ControlMonadSTInternalNew, [value]) => {
                EffectExpression::ReferenceNew(*value)
            }
            (KnownTerm::EffectRefRead | KnownTerm::ControlMonadSTInternalRead, [reference]) => {
                EffectExpression::ReferenceRead(*reference)
            }
            (KnownTerm::EffectRefWrite, [value, reference]) => {
                let (reference, value) = (*reference, *value);
                let statement = EffectStatement::ReferenceWrite { reference, value };
                let Some(effect) = self.unit_statement(statement)? else { return Ok(None) };
                effect
            }
            (KnownTerm::ControlMonadSTInternalWrite, [value, reference]) => {
                EffectExpression::ReferenceWrite { reference: *reference, value: *value }
            }
            (
                KnownTerm::EffectRefModify | KnownTerm::ControlMonadSTInternalModify,
                [function, reference],
            ) => return self.reference_modification(*function, *reference, ModifyResult::State),
            (
                KnownTerm::EffectRefModifyPrime | KnownTerm::ControlMonadSTInternalModifyPrime,
                [function, reference],
            ) => return self.reference_modification(*function, *reference, ModifyResult::Record),
            (
                KnownTerm::EffectRefModifyUnderscore
                | KnownTerm::ControlMonadSTInternalModifyUnderscore,
                [function, reference],
            ) => return self.reference_modification(*function, *reference, ModifyResult::Unit),
            _ => return Ok(None),
        };
        Ok(Some(self.expression(ExpressionKind::Effect { effect })))
    }

    fn reference_modification(
        &mut self,
        function: ExpressionId,
        reference: ExpressionId,
        result: ModifyResult,
    ) -> ConversionResult<Option<ExpressionId>> {
        let mut bindings = Vec::new();
        let (parameter, value) = match self.effect_continuation(function)? {
            Some(continuation) => continuation,
            None => {
                let function = self.let_bound_local(function, "modifyFunction", &mut bindings)?;
                let parameter = self.fresh_parameter("value".into())?;
                let value = self.expression(ExpressionKind::Local { parameter: parameter.clone() });
                (parameter, self.application(function, [value])?)
            }
        };
        let reference = if self.expression_is_stable(reference) {
            reference
        } else {
            self.let_bound_local(reference, "modifyReference", &mut bindings)?
        };

        let written_reference =
            self.expression(ExpressionKind::clone(&self.storage[reference].kind));
        let body = match result {
            ModifyResult::State => {
                let effect =
                    EffectExpression::ReferenceWrite { reference: written_reference, value };
                self.expression(ExpressionKind::Effect { effect })
            }
            ModifyResult::Unit => {
                let Some(unit) = self.unit_value()? else { return Ok(None) };
                let statement =
                    EffectStatement::ReferenceWrite { reference: written_reference, value };
                let effect = EffectExpression::Statement { statement, unit };
                self.expression(ExpressionKind::Effect { effect })
            }
            ModifyResult::Record => {
                let Some(unit) = self.unit_value()? else { return Ok(None) };
                self.record_modification(written_reference, value, unit)?
            }
        };
        let effect = EffectExpression::ReferenceRead(reference);
        let action = self.expression(ExpressionKind::Effect { effect });
        let effect = EffectExpression::Bind { action, parameter, body };
        let modification = self.expression(ExpressionKind::Effect { effect });

        if bindings.is_empty() {
            return Ok(Some(modification));
        }
        Ok(Some(self.expression(ExpressionKind::Let {
            recursive: false,
            bindings: bindings.into(),
            body: modification,
        })))
    }

    fn record_modification(
        &mut self,
        reference: ExpressionId,
        modified: ExpressionId,
        unit: ExpressionId,
    ) -> ConversionResult<ExpressionId> {
        let mut bindings = Vec::new();
        let modified = self.let_bound_local(modified, "modified", &mut bindings)?;
        let field = self.label_field("state".into());
        let state = self.expression(ExpressionKind::Project { record: modified, field });
        let statement = EffectStatement::ReferenceWrite { reference, value: state };
        let effect = EffectExpression::Statement { statement, unit };
        let action = self.expression(ExpressionKind::Effect { effect });
        let modified = self.expression(ExpressionKind::clone(&self.storage[modified].kind));
        let field = self.label_field("value".into());
        let value = self.expression(ExpressionKind::Project { record: modified, field });
        let effect = EffectExpression::Pure(value);
        let body = self.expression(ExpressionKind::Effect { effect });
        let parameter = self.fresh_parameter("unit".into())?;
        let effect = EffectExpression::Bind { action, parameter, body };
        let body = self.expression(ExpressionKind::Effect { effect });
        Ok(self.expression(ExpressionKind::Let {
            recursive: false,
            bindings: bindings.into(),
            body,
        }))
    }

    fn let_bound_local(
        &mut self,
        expression: ExpressionId,
        name: &str,
        bindings: &mut Vec<Binding>,
    ) -> ConversionResult<ExpressionId> {
        let parameter = self.fresh_parameter(name.into())?;
        let local = self.expression(ExpressionKind::Local { parameter: parameter.clone() });
        let source_order = bindings.len();
        bindings.push(Binding { parameter, expression, source_order, origin: None });
        Ok(local)
    }

    pub(super) fn effect_continuation(
        &mut self,
        continuation: ExpressionId,
    ) -> ConversionResult<Option<(Parameter, ExpressionId)>> {
        if let ExpressionKind::Abstraction { parameters, body } = &self.storage[continuation].kind
            && let [pattern] = parameters.as_ref()
        {
            let parameter = match &self.storage[*pattern].kind {
                PatternKind::Variable(parameter) => Some(Parameter::clone(parameter)),
                PatternKind::Named { parameter, pattern }
                    if matches!(self.storage[*pattern].kind, PatternKind::Wildcard) =>
                {
                    Some(Parameter::clone(parameter))
                }
                PatternKind::Named { .. }
                | PatternKind::Wildcard
                | PatternKind::Literal(_)
                | PatternKind::Array(_)
                | PatternKind::Record(_)
                | PatternKind::Constructor { .. } => None,
            };
            if let Some(parameter) = parameter {
                return Ok(Some((parameter, *body)));
            }
        }

        if !matches!(
            self.storage[continuation].kind,
            ExpressionKind::Global { .. } | ExpressionKind::Local { .. }
        ) {
            return Ok(None);
        }

        let parameter = self.fresh_parameter("bindValue".into())?;
        let local = ExpressionKind::Local { parameter: Parameter::clone(&parameter) };

        let value = self.expression(local);
        let body = self.application(continuation, [value])?;

        Ok(Some((parameter, body)))
    }

    fn discharged_partial_abstraction(&mut self, function: ExpressionId) -> Option<ExpressionId> {
        let ExpressionKind::Abstraction { parameters, body } = &self.storage[function].kind else {
            return None;
        };
        let (dictionary, parameters) = parameters.split_first()?;
        let dictionary = *dictionary;
        let parameters = parameters.to_vec();
        let body = *body;
        let dictionary = match &self.storage[dictionary].kind {
            PatternKind::Variable(parameter) => Some(Parameter::clone(parameter)),
            PatternKind::Wildcard => None,
            PatternKind::Named { .. }
            | PatternKind::Literal(_)
            | PatternKind::Array(_)
            | PatternKind::Record(_)
            | PatternKind::Constructor { .. } => return None,
        };
        let body = if parameters.is_empty() {
            body
        } else {
            self.expression(ExpressionKind::Abstraction { parameters: parameters.into(), body })
        };
        let Some(dictionary) =
            dictionary.filter(|dictionary| local_uses(&self.storage, body, dictionary.id) > 0)
        else {
            return Some(body);
        };
        let evidence = self.expression(ExpressionKind::TrivialEvidence);
        let binding =
            Binding { parameter: dictionary, expression: evidence, source_order: 0, origin: None };
        Some(self.expression(ExpressionKind::Let {
            recursive: false,
            bindings: [binding].into(),
            body,
        }))
    }

    fn uncurried_parameters(
        &mut self,
        mut expression: ExpressionId,
        arity: usize,
    ) -> Option<(Vec<PatternId>, ExpressionId)> {
        let mut parameters = Vec::with_capacity(arity);
        while parameters.len() < arity {
            let ExpressionKind::Abstraction { parameters: abstraction, body } =
                &self.storage[expression].kind
            else {
                return None;
            };
            let abstraction = abstraction.to_vec();
            let body = *body;
            let remaining = arity - parameters.len();
            let split = abstraction.len().min(remaining);
            parameters.extend_from_slice(&abstraction[..split]);
            expression = if split == abstraction.len() {
                body
            } else {
                self.expression(ExpressionKind::Abstraction {
                    parameters: abstraction[split..].into(),
                    body,
                })
            };
        }
        Some((parameters, expression))
    }

    fn application_spine(
        &self,
        mut function: ExpressionId,
        arguments: &[ExpressionId],
    ) -> (ExpressionId, Vec<ExpressionId>) {
        let mut groups = vec![arguments];
        while let ExpressionKind::Application { function: inner, arguments, .. } =
            &self.storage[function].kind
        {
            function = *inner;
            groups.push(arguments);
        }
        let arguments = groups.into_iter().rev().flatten().copied();
        (function, arguments.collect())
    }

    fn application_is_synthetic(&self, expression: ExpressionId) -> bool {
        matches!(
            self.storage[expression].kind,
            ExpressionKind::Application { synthetic: true, .. }
                | ExpressionKind::UncurriedApplication { synthetic: true, .. }
        )
    }

    fn flipped_application(
        &mut self,
        argument: ExpressionId,
        function: ExpressionId,
        synthetic: bool,
        result_type: Option<checking::TypeId>,
    ) -> ConversionResult<ExpressionId> {
        if self.expression_is_stable(argument) || self.expression_is_stable(function) {
            return self.application_with_synthetic(function, [argument], synthetic, result_type);
        }

        let argument_parameter = self.fresh_parameter("applyArgument".into())?;
        let function_parameter = self.fresh_parameter("applyFunction".into())?;
        let argument_local =
            self.expression(ExpressionKind::Local { parameter: argument_parameter.clone() });
        let function_local =
            self.expression(ExpressionKind::Local { parameter: function_parameter.clone() });
        let body = self.application_with_synthetic(
            function_local,
            [argument_local],
            synthetic,
            result_type,
        )?;
        let bindings = [
            Binding {
                parameter: argument_parameter,
                expression: argument,
                source_order: 0,
                origin: None,
            },
            Binding {
                parameter: function_parameter,
                expression: function,
                source_order: 1,
                origin: None,
            },
        ];
        Ok(self.expression(ExpressionKind::Let {
            recursive: false,
            bindings: bindings.into(),
            body,
        }))
    }

    fn function_composition(
        &mut self,
        mut outer: ExpressionId,
        mut inner: ExpressionId,
        argument: Option<ExpressionId>,
        flipped: bool,
        synthetic: bool,
    ) -> ConversionResult<ExpressionId> {
        let stabilize_functions = argument.is_none()
            || (flipped && !self.expression_is_stable(outer) && !self.expression_is_stable(inner));
        let mut bindings = Vec::new();
        let functions = if flipped {
            [(&mut inner, "composeInner"), (&mut outer, "composeOuter")]
        } else {
            [(&mut outer, "composeOuter"), (&mut inner, "composeInner")]
        };
        for (function, name) in functions {
            if stabilize_functions && !self.expression_is_stable(*function) {
                let parameter = self.fresh_parameter(name.into())?;
                let expression = *function;
                *function = self.expression(ExpressionKind::Local { parameter: parameter.clone() });
                bindings.push(Binding {
                    parameter,
                    expression,
                    source_order: bindings.len(),
                    origin: None,
                });
            }
        }

        let (argument, parameter) = if let Some(argument) = argument {
            (argument, None)
        } else {
            let parameter = self.fresh_parameter("composeArgument".into())?;
            let argument = self.expression(ExpressionKind::Local { parameter: parameter.clone() });
            (argument, Some(parameter))
        };
        let body = self.application_with_synthetic(inner, [argument], synthetic, None)?;
        let body = self.application_with_synthetic(outer, [body], synthetic, None)?;
        let body = if let Some(parameter) = parameter {
            self.parameter_abstraction([parameter], body)
        } else {
            body
        };
        if bindings.is_empty() {
            Ok(body)
        } else {
            Ok(self.expression(ExpressionKind::Let {
                recursive: false,
                bindings: bindings.into(),
                body,
            }))
        }
    }

    pub(super) fn expression_is_stable(&self, expression: ExpressionId) -> bool {
        matches!(
            self.storage[expression].kind,
            ExpressionKind::Literal { .. }
                | ExpressionKind::Constructor { .. }
                | ExpressionKind::Global { .. }
                | ExpressionKind::Local { .. }
        )
    }

    pub(super) fn unit_value(&mut self) -> ConversionResult<Option<ExpressionId>> {
        if self.unit_global.is_none() {
            let mut unit = None;
            if let Some(file_id) = self.queries.module_file("Data.Unit") {
                let indexed = self.indexed_module(file_id)?;
                let mut terms = indexed.items.iter_terms();
                if let Some((term_id, _)) =
                    terms.find(|(_, item)| item.name.as_deref() == Some("unit"))
                {
                    unit = Some(self.term_global(file_id, term_id)?);
                }
            }
            self.unit_global = Some(unit);
        }
        let Some(Some(global)) = &self.unit_global else { return Ok(None) };
        let global = Global::clone(global);
        Ok(Some(self.expression(ExpressionKind::Global { global })))
    }

    pub(super) fn known_instance_member_arguments<'a>(
        &self,
        known: KnownTerm,
        arguments: &'a [ExpressionId],
        member: KnownTerm,
        instance: KnownInstance,
    ) -> Option<&'a [ExpressionId]> {
        if known != member {
            return None;
        }
        let (record, arguments) = arguments.split_first()?;
        (self.known_instance(*record) == Some(instance)).then_some(arguments)
    }

    fn class_member_parent(&self, expression: ExpressionId) -> QueryResult<(FileId, TypeItemId)> {
        let ExpressionKind::Global { global: Global { id: GlobalId::Term(file_id, term_id), .. } } =
            self.storage[expression].kind
        else {
            unreachable!("invariant violated: known class member is not a term global")
        };
        let indexed = self.indexed_module(file_id)?;
        let IndexedTermItemKind::ClassMember { parent, .. } = indexed.items[term_id].kind else {
            unreachable!("invariant violated: known class member is not a class member")
        };
        Ok((file_id, parent))
    }

    fn known_primitive_instance(
        &self,
        expression: ExpressionId,
        instances: &[(KnownInstance, PrimitiveType)],
    ) -> Option<PrimitiveType> {
        let known = self.known_instance(expression)?;
        instances.iter().find_map(|&(instance, operand)| (instance == known).then_some(operand))
    }

    fn known_thunk_instance(
        &self,
        expression: ExpressionId,
        expected_class: Option<(FileId, TypeItemId)>,
    ) -> bool {
        let ExpressionKind::Global { global } = &self.storage[expression].kind else {
            return false;
        };
        let GlobalId::Instance(InstanceIdentity::Declared(instance_file, instance_id)) = global.id
        else {
            return false;
        };
        let Some(&class) =
            self.functional_core.known_thunk_instances.get(&(instance_file, instance_id))
        else {
            return false;
        };
        expected_class.is_none_or(|expected_class| class == expected_class)
    }
}

fn known_effect_loop(known: KnownTerm, arguments: &[ExpressionId]) -> Option<EffectLoop> {
    match (known, arguments) {
        (KnownTerm::EffectForE | KnownTerm::ControlMonadSTInternalFor, &[start, end, body]) => {
            Some(EffectLoop::Range { start, end, body })
        }
        (KnownTerm::EffectForeachE | KnownTerm::ControlMonadSTInternalForeach, &[array, body]) => {
            Some(EffectLoop::Each { array, body })
        }
        (KnownTerm::EffectWhileE | KnownTerm::ControlMonadSTInternalWhile, &[condition, body]) => {
            Some(EffectLoop::While { condition, body })
        }
        (KnownTerm::EffectUntilE, &[condition]) => Some(EffectLoop::Until { condition }),
        _ => None,
    }
}
