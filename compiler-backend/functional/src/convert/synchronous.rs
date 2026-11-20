use crate::optimize::local_uses;
use crate::tree::{
    Binding, EffectExpression, EffectStatement, ExpressionId, ExpressionKind, FieldIdentity,
    Parameter,
};

use super::known::{KnownInstance, KnownTerm};
use super::{Context, ConversionResult};

#[derive(Clone, Copy)]
enum SyncOperation {
    Pure,
    LiftEffect,
    Service,
    Provide,
    Map,
    Apply,
    Bind,
    When,
}

impl<'c, Q> Context<'c, Q>
where
    Q: checking::ExternalQueries,
{
    pub(super) fn known_service_application(
        &mut self,
        known: KnownTerm,
        arguments: &[ExpressionId],
    ) -> ConversionResult<Option<ExpressionId>> {
        if known == KnownTerm::IrisEffectGetService
            && let [dictionary, remaining @ ..] = arguments
        {
            let getter = self.service_getter(*dictionary)?;
            return self.application(getter, remaining.iter().copied()).map(Some);
        }
        Ok(None)
    }

    pub(super) fn service_getter(
        &mut self,
        dictionary: ExpressionId,
    ) -> ConversionResult<ExpressionId> {
        let get_service = FieldIdentity::Member(
            self.functional_core.iris_effect.get_service.0,
            self.functional_core.iris_effect.get_service.1,
        );
        if let ExpressionKind::Record { fields } = &self.storage[dictionary].kind
            && let [field] = fields.as_ref()
            && field.field.identity == get_service
        {
            return Ok(field.expression);
        }
        let field = self.member_field(self.functional_core.iris_effect.get_service)?;
        Ok(self.expression(ExpressionKind::Project { record: dictionary, field }))
    }

    pub(super) fn known_sync_application(
        &mut self,
        known: KnownTerm,
        arguments: &[ExpressionId],
    ) -> ConversionResult<Option<ExpressionId>> {
        if known == KnownTerm::IrisEffectSyncMake
            && let [program] = arguments
        {
            return Ok(Some(*program));
        }
        if known == KnownTerm::IrisEffectSyncRun
            && let [environment, program] = arguments
        {
            return self.sync_application(*program, *environment).map(Some);
        }

        for (function, arity, operation) in [
            (KnownTerm::IrisEffectSyncLiftEffect, 1, SyncOperation::LiftEffect),
            (KnownTerm::IrisEffectSyncService, 1, SyncOperation::Service),
            (KnownTerm::IrisEffectSyncProvide, 2, SyncOperation::Provide),
        ] {
            if arguments.len() == arity && known == function {
                return self.lower_sync(operation, arguments).map(Some);
            }
        }
        for (member, instance, arity, operation) in [
            (
                KnownTerm::ControlApplicativePure,
                KnownInstance::ApplicativeSync,
                1,
                SyncOperation::Pure,
            ),
            (KnownTerm::DataFunctorMap, KnownInstance::FunctorSync, 2, SyncOperation::Map),
            (KnownTerm::ControlApplyApply, KnownInstance::ApplySync, 2, SyncOperation::Apply),
            (KnownTerm::ControlBindBind, KnownInstance::BindSync, 2, SyncOperation::Bind),
        ] {
            if let Some(operands) =
                self.known_instance_member_arguments(known, arguments, member, instance)
                && operands.len() == arity
            {
                return self.lower_sync(operation, operands).map(Some);
            }
        }
        if let Some((action, continuation)) =
            self.known_discard_arguments(known, arguments, |context, dictionary| {
                context.known_instance(dictionary) == Some(KnownInstance::BindSync)
            })
        {
            return self.lower_sync(SyncOperation::Bind, &[action, continuation]).map(Some);
        }
        if let Some((condition, action)) =
            self.known_conditional_arguments(known, arguments, |context, dictionary| {
                context.known_instance(dictionary) == Some(KnownInstance::ApplicativeSync)
            })
            && let Some(unit) = self.unit_value()?
        {
            return self.lower_sync(SyncOperation::When, &[condition, action, unit]).map(Some);
        }
        Ok(None)
    }

    fn lower_sync(
        &mut self,
        operation: SyncOperation,
        operands: &[ExpressionId],
    ) -> ConversionResult<ExpressionId> {
        let (operands, bindings) =
            self.construction_operands(operands, "syncArgument", |kind| match operation {
                SyncOperation::Service => {
                    matches!(
                        kind,
                        ExpressionKind::SynthesizedEvidence { .. } | ExpressionKind::Record { .. }
                    )
                }
                _ => false,
            })?;

        let parameter = self.fresh_parameter("environment".into())?;
        let environment = self.expression(ExpressionKind::Local { parameter: parameter.clone() });
        let effect = match (operation, operands.as_slice()) {
            (SyncOperation::Pure, [value]) => EffectExpression::Pure(*value),
            (SyncOperation::Service, [dictionary]) => {
                let getter = self.service_getter(*dictionary)?;
                let service = self.sync_application(getter, environment)?;
                EffectExpression::Pure(service)
            }
            (SyncOperation::Map, [function, action]) => {
                let action = self.sync_application(*action, environment)?;
                EffectExpression::Map { function: *function, action }
            }
            (SyncOperation::Apply, [function, argument]) => {
                let function_action = self.sync_application(*function, environment)?;
                let argument_action = self.sync_application(*argument, environment)?;
                EffectExpression::Apply { function_action, argument_action }
            }
            (SyncOperation::Bind, [action, continuation]) => {
                let action = self.sync_application(*action, environment)?;
                let (parameter, continuation) = self.bound_continuation(*continuation)?;
                let body = self.sync_application(continuation, environment)?;
                EffectExpression::Bind { action, parameter, body }
            }
            (SyncOperation::When, [condition, action, unit]) => {
                let action = self.sync_application(*action, environment)?;
                let statement = EffectStatement::When { condition: *condition, action };
                EffectExpression::Statement { statement, unit: *unit }
            }
            (SyncOperation::LiftEffect, [action]) => {
                let body = self.parameter_abstraction([parameter], *action);
                return Ok(self.sync_bindings(bindings, body));
            }
            (SyncOperation::Provide, [provided, action]) => {
                let action = self.sync_application(*action, *provided)?;
                let body = self.parameter_abstraction([parameter], action);
                return Ok(self.sync_bindings(bindings, body));
            }
            _ => unreachable!("invariant violated: invalid synchronous effect application"),
        };
        let body = self.expression(ExpressionKind::Effect { effect });
        let body = self.parameter_abstraction([parameter], body);
        Ok(self.sync_bindings(bindings, body))
    }

    pub(super) fn construction_operands(
        &mut self,
        operands: &[ExpressionId],
        name: &str,
        retained: impl Fn(&ExpressionKind) -> bool,
    ) -> ConversionResult<(Vec<ExpressionId>, Vec<Binding>)> {
        let mut operands = operands.to_vec();
        let mut bindings = Vec::new();
        for operand in &mut operands {
            while let ExpressionKind::Let { recursive: false, bindings: nested, body } =
                self.storage[*operand].kind.clone()
            {
                for mut binding in nested.iter().cloned() {
                    binding.source_order = bindings.len();
                    bindings.push(binding);
                }
                *operand = body;
            }
            let kind = &self.storage[*operand].kind;
            if self.expression_is_stable(*operand)
                || matches!(kind, ExpressionKind::Abstraction { .. })
                || retained(kind)
            {
                continue;
            }
            let parameter = self.fresh_parameter(name.into())?;
            let origin = self.storage[*operand].origin;
            bindings.push(Binding {
                parameter: parameter.clone(),
                expression: *operand,
                source_order: bindings.len(),
                origin,
            });
            *operand = self.with_origin(origin, |context| {
                context.expression(ExpressionKind::Local { parameter })
            });
        }
        Ok((operands, bindings))
    }

    pub(super) fn bound_continuation(
        &mut self,
        continuation: ExpressionId,
    ) -> ConversionResult<(Parameter, ExpressionId)> {
        if let Some(continuation) = self.effect_continuation(continuation)? {
            return Ok(continuation);
        }
        let parameter = self.fresh_parameter("bindValue".into())?;
        let value = self.expression(ExpressionKind::Local { parameter: parameter.clone() });
        let body = self.application(continuation, [value])?;
        Ok((parameter, body))
    }

    pub(super) fn sync_application(
        &mut self,
        function: ExpressionId,
        argument: ExpressionId,
    ) -> ConversionResult<ExpressionId> {
        if let ExpressionKind::Let { recursive: false, bindings, body } =
            self.storage[function].kind.clone()
        {
            let body = self.sync_application(body, argument)?;
            return Ok(self.sync_bindings(bindings.to_vec(), body));
        }
        if matches!(self.storage[function].kind, ExpressionKind::Abstraction { .. })
            && let Some((parameter, body)) = self.effect_continuation(function)?
        {
            if self.expression_is_stable(argument)
                && local_uses(&self.storage, body, parameter.id) == 0
            {
                return Ok(body);
            }
            let binding =
                Binding { parameter, expression: argument, source_order: 0, origin: None };
            return Ok(self.sync_bindings(vec![binding], body));
        }
        self.application(function, [argument])
    }

    pub(super) fn sync_bindings(
        &mut self,
        bindings: Vec<Binding>,
        body: ExpressionId,
    ) -> ExpressionId {
        if bindings.is_empty() {
            body
        } else {
            self.expression(ExpressionKind::Let {
                recursive: false,
                bindings: bindings.into(),
                body,
            })
        }
    }
}
