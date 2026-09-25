use std::sync::Arc;

use building_types::QueryResult;
use files::FileId;
use indexing::{IndexedTermItemKind, TypeItemId};
use itertools::Itertools;
use smol_str::format_smolstr;

use crate::tree::{
    BinaryOperator, Binding, EffectExpression, ExpressionId, ExpressionKind, GlobalId,
    InstanceIdentity, Literal, Parameter, PatternKind, RecordField, UnaryOperator,
};

use super::{Context, ConversionResult};

struct ClassMemberApplication<'a> {
    class: (FileId, TypeItemId),
    record: ExpressionId,
    arguments: &'a [ExpressionId],
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
        // Every known application is headed by a global or a constructor, so other heads skip
        // collecting the spine.
        let head = self.application_head(function);
        if !matches!(
            self.storage[head].kind,
            ExpressionKind::Global { .. } | ExpressionKind::Constructor { .. }
        ) {
            return Ok(self.expression(ExpressionKind::Application {
                function,
                arguments: arguments.into(),
                synthetic,
            }));
        }
        let (known_function, known_arguments) = self.application_spine(function, &arguments);
        if let ExpressionKind::Constructor { global } = &self.storage[known_function].kind {
            let global = global.clone();
            let GlobalId::Term(file_id, term_id) = global.id else {
                unreachable!("invariant violated: constructor has a non-term global identity")
            };
            let arity = self.constructor_arity(file_id, term_id)?;
            if known_arguments.len() == arity {
                let tag = self.expression(ExpressionKind::Literal {
                    literal: Literal::String(global.item_name.into()),
                });
                let mut fields = Vec::with_capacity(known_arguments.len() + 1);
                let field = self.label_field("tag".into());
                fields.push(RecordField { field, expression: tag });
                for (index, expression) in known_arguments.into_iter().enumerate() {
                    let field = self.label_field(format_smolstr!("_{}", index + 1));
                    fields.push(RecordField { field, expression });
                }
                return Ok(self.expression(ExpressionKind::Record { fields: fields.into() }));
            }
            synthetic = true;
        }
        if let Some(arity) =
            self.known_numbered_term_arity(known_function, "Data.Function.Uncurried", "mkFn")?
            && arity >= 1
            && let [function] = known_arguments.as_slice()
            && let Some(function) = self.uncurry_abstraction(*function, arity)
        {
            return Ok(function);
        }
        if let Some(arity) =
            self.known_numbered_term_arity(known_function, "Data.Function.Uncurried", "runFn")?
            && let Some((function, arguments)) = known_arguments.split_first()
            && arguments.len() == arity
        {
            return Ok(self.expression(ExpressionKind::UncurriedApplication {
                function: *function,
                arguments: arguments.into(),
                synthetic,
            }));
        }
        if self.known_term(known_function, "Data.Function", "apply")?
            && let [function, argument] = known_arguments.as_slice()
        {
            return self.application_with_synthetic(*function, [*argument], synthetic, result_type);
        }
        if self.known_term(known_function, "Data.Function", "applyFlipped")?
            && let [argument, function] = known_arguments.as_slice()
        {
            return self.flipped_application(*argument, *function, synthetic, result_type);
        }
        if let Some(composition) =
            self.known_composition_application(known_function, &known_arguments, synthetic)?
        {
            return Ok(composition);
        }
        if let Some(expression) =
            self.stylex_intrinsic(known_function, &known_arguments, result_type)?
        {
            return Ok(expression);
        }
        if let Some(effect) = self.known_effect_application(known_function, &known_arguments)? {
            return Ok(self.expression(ExpressionKind::Effect { effect }));
        }
        if let Some([argument]) = self.known_instance_member_arguments(
            known_function,
            &known_arguments,
            "Control.Category",
            "identity",
            "Control.Category",
            "categoryFn",
        )? {
            return Ok(*argument);
        }
        if self.known_term(known_function, "Unsafe.Coerce", "unsafeCoerce")?
            && let [argument] = known_arguments.as_slice()
        {
            return Ok(*argument);
        }
        if self.known_term(known_function, "Safe.Coerce", "coerce")?
            && let [_, argument] = known_arguments.as_slice()
        {
            return Ok(*argument);
        }
        if let Some(kind) = self.known_operator_application(known_function, &known_arguments)? {
            return Ok(self.expression(kind));
        }
        Ok(self.expression(ExpressionKind::Application {
            function,
            arguments: arguments.into(),
            synthetic,
        }))
    }

    fn known_composition_application(
        &mut self,
        function: ExpressionId,
        arguments: &[ExpressionId],
        synthetic: bool,
    ) -> ConversionResult<Option<ExpressionId>> {
        if let Some(arguments) = self.known_instance_member_arguments(
            function,
            arguments,
            "Control.Semigroupoid",
            "compose",
            "Control.Semigroupoid",
            "semigroupoidFn",
        )? && let [outer, inner, remaining @ ..] = arguments
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
        if self.known_term(function, "Control.Semigroupoid", "composeFlipped")?
            && let [dictionary, inner, outer, remaining @ ..] = arguments
            && let [] | [_] = remaining
            && self.known_named_instance(*dictionary, "Control.Semigroupoid", "semigroupoidFn")?
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
        &self,
        function: ExpressionId,
        arguments: &[ExpressionId],
    ) -> QueryResult<Option<ExpressionKind>> {
        if let Some([value]) = self.known_instance_member_arguments(
            function,
            arguments,
            "Data.HeytingAlgebra",
            "not",
            "Data.HeytingAlgebra",
            "heytingAlgebraBoolean",
        )? {
            return Ok(Some(ExpressionKind::Unary {
                operator: UnaryOperator::BooleanNot,
                value: *value,
            }));
        }
        if self.known_term(function, "Data.Ring", "negate")?
            && let [dictionary, value] = arguments
        {
            if self.known_named_instance(*dictionary, "Data.Ring", "ringInt")? {
                return Ok(Some(ExpressionKind::Unary {
                    operator: UnaryOperator::IntegerNegate,
                    value: *value,
                }));
            }
            if self.known_named_instance(*dictionary, "Data.Ring", "ringNumber")? {
                return Ok(Some(ExpressionKind::Unary {
                    operator: UnaryOperator::NumberNegate,
                    value: *value,
                }));
            }
        }
        if let Some([left, right]) = self.known_instance_member_arguments(
            function,
            arguments,
            "Data.Semiring",
            "add",
            "Data.Semiring",
            "semiringInt",
        )? {
            return Ok(Some(ExpressionKind::Binary {
                operator: BinaryOperator::IntegerAdd,
                left: *left,
                right: *right,
            }));
        }
        if let Some([left, right]) = self.known_instance_member_arguments(
            function,
            arguments,
            "Data.Ring",
            "sub",
            "Data.Ring",
            "ringInt",
        )? {
            return Ok(Some(ExpressionKind::Binary {
                operator: BinaryOperator::IntegerSubtract,
                left: *left,
                right: *right,
            }));
        }
        if let Some([left, right]) = self.known_instance_member_arguments(
            function,
            arguments,
            "Data.Semiring",
            "mul",
            "Data.Semiring",
            "semiringInt",
        )? {
            return Ok(Some(ExpressionKind::Binary {
                operator: BinaryOperator::IntegerMultiply,
                left: *left,
                right: *right,
            }));
        }
        Ok(None)
    }

    fn known_effect_application(
        &mut self,
        function: ExpressionId,
        arguments: &[ExpressionId],
    ) -> ConversionResult<Option<EffectExpression>> {
        if let Some([value]) = self.known_thunk_instance_member_arguments(
            function,
            arguments,
            "Control.Applicative",
            "pure",
        )? {
            return Ok(Some(EffectExpression::Pure(*value)));
        }
        if let Some([action, continuation]) =
            self.known_thunk_instance_member_arguments(function, arguments, "Control.Bind", "bind")?
            && let Some((parameter, body)) = self.effect_continuation(*continuation)?
        {
            return Ok(Some(EffectExpression::Bind { action: *action, parameter, body }));
        }
        if let Some([bind_dictionary, action, continuation]) = self
            .known_instance_member_arguments(
                function,
                arguments,
                "Control.Bind",
                "discard",
                "Control.Bind",
                "discardUnit",
            )?
            && self.known_thunk_instance(*bind_dictionary, None)?
            && let Some((parameter, body)) = self.effect_continuation(*continuation)?
        {
            return Ok(Some(EffectExpression::Bind { action: *action, parameter, body }));
        }
        if let Some([function, action]) =
            self.known_thunk_instance_member_arguments(function, arguments, "Data.Functor", "map")?
        {
            return Ok(Some(EffectExpression::Map { function: *function, action: *action }));
        }
        if let Some([function_action, argument_action]) = self
            .known_thunk_instance_member_arguments(function, arguments, "Control.Apply", "apply")?
        {
            return Ok(Some(EffectExpression::Apply {
                function_action: *function_action,
                argument_action: *argument_action,
            }));
        }
        Ok(None)
    }

    fn effect_continuation(
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

    fn uncurry_abstraction(
        &mut self,
        mut expression: ExpressionId,
        arity: usize,
    ) -> Option<ExpressionId> {
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
        Some(self.expression(ExpressionKind::UncurriedAbstraction {
            parameters: parameters.into(),
            body: expression,
        }))
    }

    fn application_head(&self, mut function: ExpressionId) -> ExpressionId {
        while let ExpressionKind::Application { function: inner, .. } = &self.storage[function].kind
        {
            function = *inner;
        }
        function
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
            Binding { parameter: argument_parameter, expression: argument, source_order: 0 },
            Binding { parameter: function_parameter, expression: function, source_order: 1 },
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
                bindings.push(Binding { parameter, expression, source_order: bindings.len() });
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

    fn known_term(
        &self,
        expression: ExpressionId,
        module_name: &str,
        item_name: &str,
    ) -> QueryResult<bool> {
        let ExpressionKind::Global { global } = &self.storage[expression].kind else {
            return Ok(false);
        };
        let GlobalId::Term(file_id, _) = global.id else { return Ok(false) };
        Ok(global.item_name == item_name && self.source_module_name(file_id)? == module_name)
    }

    fn known_numbered_term_arity(
        &self,
        expression: ExpressionId,
        module_name: &str,
        item_prefix: &str,
    ) -> QueryResult<Option<usize>> {
        let ExpressionKind::Global { global } = &self.storage[expression].kind else {
            return Ok(None);
        };
        let GlobalId::Term(file_id, _) = global.id else { return Ok(None) };
        let Some(arity) = global.item_name.strip_prefix(item_prefix) else {
            return Ok(None);
        };
        let Ok(arity) = arity.parse::<usize>() else { return Ok(None) };
        let canonical_name = format_smolstr!("{item_prefix}{arity}");
        if arity > 10 || global.item_name != canonical_name {
            return Ok(None);
        }
        Ok((self.source_module_name(file_id)? == module_name).then_some(arity))
    }

    fn known_instance_member_arguments<'a>(
        &self,
        expression: ExpressionId,
        arguments: &'a [ExpressionId],
        member_module_name: &str,
        member_name: &str,
        instance_module_name: &str,
        instance_name: &str,
    ) -> QueryResult<Option<&'a [ExpressionId]>> {
        let Some(ClassMemberApplication { record, arguments, .. }) = self
            .known_class_member_arguments(expression, arguments, member_module_name, member_name)?
        else {
            return Ok(None);
        };
        if !self.known_named_instance(record, instance_module_name, instance_name)? {
            return Ok(None);
        }
        Ok(Some(arguments))
    }

    fn known_thunk_instance_member_arguments<'a>(
        &self,
        expression: ExpressionId,
        arguments: &'a [ExpressionId],
        member_module_name: &str,
        member_name: &str,
    ) -> QueryResult<Option<&'a [ExpressionId]>> {
        let Some(ClassMemberApplication { class, record, arguments }) = self
            .known_class_member_arguments(expression, arguments, member_module_name, member_name)?
        else {
            return Ok(None);
        };
        if !self.known_thunk_instance(record, Some(class))? {
            return Ok(None);
        }
        Ok(Some(arguments))
    }

    fn known_class_member_arguments<'a>(
        &self,
        expression: ExpressionId,
        arguments: &'a [ExpressionId],
        module_name: &str,
        member_name: &str,
    ) -> QueryResult<Option<ClassMemberApplication<'a>>> {
        let ExpressionKind::Global { global } = &self.storage[expression].kind else {
            return Ok(None);
        };
        let GlobalId::Term(member_file, member_id) = global.id else {
            return Ok(None);
        };
        if global.item_name != member_name {
            return Ok(None);
        }
        let indexed = self.indexed_module(member_file)?;
        let IndexedTermItemKind::ClassMember { parent, .. } = indexed.items[member_id].kind else {
            return Ok(None);
        };
        if self.source_module_name(member_file)? != module_name {
            return Ok(None);
        }
        let Some((record, arguments)) = arguments.split_first() else {
            return Ok(None);
        };
        let class = (member_file, parent);
        Ok(Some(ClassMemberApplication { class, record: *record, arguments }))
    }

    fn known_named_instance(
        &self,
        expression: ExpressionId,
        module_name: &str,
        instance_name: &str,
    ) -> QueryResult<bool> {
        let ExpressionKind::Global { global } = &self.storage[expression].kind else {
            return Ok(false);
        };
        let GlobalId::Instance(identity) = global.id else { return Ok(false) };
        let instance_file = match identity {
            InstanceIdentity::Declared(file_id, _) | InstanceIdentity::Derived(file_id, _) => {
                file_id
            }
        };
        Ok(global.item_name == instance_name
            && self.source_module_name(instance_file)? == module_name)
    }

    fn known_thunk_instance(
        &self,
        expression: ExpressionId,
        expected_class: Option<(FileId, TypeItemId)>,
    ) -> QueryResult<bool> {
        let ExpressionKind::Global { global } = &self.storage[expression].kind else {
            return Ok(false);
        };
        let GlobalId::Instance(InstanceIdentity::Declared(instance_file, instance_id)) = global.id
        else {
            return Ok(false);
        };
        if self.queries.module_file("Effect") != Some(instance_file)
            && self.queries.module_file("Control.Monad.ST.Internal") != Some(instance_file)
        {
            return Ok(false);
        }

        let checked = if instance_file == self.file_id {
            Arc::clone(&self.checked)
        } else {
            self.queries.checked(instance_file)?
        };
        let instance = checked.lookup_instance(instance_id);
        let Some(instance) = instance else { return Ok(false) };
        if expected_class.is_some_and(|class| instance.resolution != class) {
            return Ok(false);
        }

        let indexed = self.indexed_module(instance_file)?;
        let canonical_instances = indexed.items.iter_instances().filter_map(|(_, candidate)| {
            let candidate_instance = checked.lookup_instance(candidate.id)?;
            (candidate_instance.resolution == instance.resolution).then_some(candidate.id)
        });
        let mut canonical_instances = canonical_instances.take(2);
        let Some(canonical_instance) = canonical_instances.next() else {
            return Ok(false);
        };
        Ok(canonical_instances.next().is_none() && instance_id == canonical_instance)
    }
}
