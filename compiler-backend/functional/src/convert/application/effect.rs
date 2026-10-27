use building_types::QueryResult;

use crate::tree::{
    ArrayEnd, EffectExpression, EffectLoop, EffectPrimitive, ExpressionId, ExpressionKind,
    ReferenceModifyResult, ReferenceWriteResult,
};

use super::super::{Context, ConversionResult};

const EFFECT_REFERENCE: &str = "Effect.Ref";
const STATE_THREAD: &str = "Control.Monad.ST.Internal";
const STATE_THREAD_ARRAY: &str = "Data.Array.ST";

const UNCURRIED_EFFECT_MODULES: &[(&str, &str, &str)] = &[
    ("Effect.Uncurried", "runEffectFn", "mkEffectFn"),
    ("Control.Monad.ST.Uncurried", "runSTFn", "mkSTFn"),
];

impl<'c, Q> Context<'c, Q>
where
    Q: checking::ExternalQueries,
{
    pub(in crate::convert) fn known_effect_value(
        &mut self,
        expression: ExpressionId,
    ) -> QueryResult<ExpressionId> {
        if self.known_term(expression, STATE_THREAD_ARRAY, "new")? {
            let effect = EffectExpression::Primitive(EffectPrimitive::NewArray);
            self.storage.replace_expression_kind(expression, ExpressionKind::Effect { effect });
        }
        Ok(expression)
    }

    pub(super) fn known_effect_operation(
        &mut self,
        function: ExpressionId,
        arguments: &[ExpressionId],
    ) -> QueryResult<Option<EffectExpression>> {
        for item_name in ["unsafeFreeze", "unsafeThaw"] {
            if self.known_term(function, STATE_THREAD_ARRAY, item_name)?
                && let [array] = *arguments
            {
                return Ok(Some(EffectExpression::Pure(array)));
            }
        }
        if let Some(primitive) = self.known_effect_primitive(function, arguments)? {
            return Ok(Some(EffectExpression::Primitive(primitive)));
        }
        if let Some(effect_loop) = self.known_effect_loop(function, arguments)? {
            return Ok(Some(EffectExpression::Loop(effect_loop)));
        }
        Ok(None)
    }

    fn known_effect_primitive(
        &mut self,
        function: ExpressionId,
        arguments: &[ExpressionId],
    ) -> QueryResult<Option<EffectPrimitive>> {
        for module_name in [EFFECT_REFERENCE, STATE_THREAD] {
            if self.known_term(function, module_name, "new")?
                && let [value] = *arguments
            {
                return Ok(Some(EffectPrimitive::NewReference { value }));
            }
            if self.known_term(function, module_name, "read")?
                && let [reference] = *arguments
            {
                return Ok(Some(EffectPrimitive::ReadReference { reference }));
            }
            if self.known_term(function, module_name, "write")?
                && let [value, reference] = *arguments
            {
                let result = if module_name == STATE_THREAD {
                    ReferenceWriteResult::Value
                } else {
                    ReferenceWriteResult::Unit
                };
                return Ok(Some(EffectPrimitive::WriteReference { value, reference, result }));
            }
            for (item_name, result) in [
                ("modify", ReferenceModifyResult::State),
                ("modify'", ReferenceModifyResult::Record),
                ("modify_", ReferenceModifyResult::Unit),
            ] {
                if self.known_term(function, module_name, item_name)?
                    && let [function, reference] = *arguments
                {
                    return Ok(Some(EffectPrimitive::ModifyReference {
                        function,
                        reference,
                        result,
                    }));
                }
            }
        }
        for (item_name, values_item_name, end) in
            [("push", "pushAll", ArrayEnd::End), ("unshift", "unshiftAll", ArrayEnd::Start)]
        {
            if self.known_term(function, STATE_THREAD_ARRAY, item_name)?
                && let [value, array] = *arguments
            {
                let values = self.expression(ExpressionKind::Array { elements: [value].into() });
                return Ok(Some(EffectPrimitive::InsertArrayValues { values, array, end }));
            }
            if self.known_term(function, STATE_THREAD_ARRAY, values_item_name)?
                && let [values, array] = *arguments
            {
                return Ok(Some(EffectPrimitive::InsertArrayValues { values, array, end }));
            }
        }
        for item_name in ["freeze", "thaw", "clone"] {
            if self.known_term(function, STATE_THREAD_ARRAY, item_name)?
                && let [array] = *arguments
            {
                return Ok(Some(EffectPrimitive::CopyArray { array }));
            }
        }
        for &(module_name, run_prefix, _) in UNCURRIED_EFFECT_MODULES {
            if let Some(arity) =
                self.known_numbered_term_arity(function, module_name, run_prefix)?
                && let Some((function, arguments)) = arguments.split_first()
                && arguments.len() == arity
            {
                let function = *function;
                let arguments = arguments.into();
                return Ok(Some(EffectPrimitive::UncurriedCall { function, arguments }));
            }
        }
        Ok(None)
    }

    fn known_effect_loop(
        &self,
        function: ExpressionId,
        arguments: &[ExpressionId],
    ) -> QueryResult<Option<EffectLoop>> {
        for (module_name, range, each, while_) in
            [("Effect", "forE", "foreachE", "whileE"), (STATE_THREAD, "for", "foreach", "while")]
        {
            if self.known_term(function, module_name, range)?
                && let [start, end, body] = *arguments
            {
                return Ok(Some(EffectLoop::Range { start, end, body }));
            }
            if self.known_term(function, module_name, each)?
                && let [array, body] = *arguments
            {
                return Ok(Some(EffectLoop::Each { array, body }));
            }
            if self.known_term(function, module_name, while_)?
                && let [condition, body] = *arguments
            {
                return Ok(Some(EffectLoop::While { condition, body }));
            }
        }
        if self.known_term(function, "Effect", "untilE")?
            && let [condition] = *arguments
        {
            return Ok(Some(EffectLoop::Until { condition }));
        }
        Ok(None)
    }

    pub(super) fn known_effect_runner(
        &mut self,
        function: ExpressionId,
        arguments: &[ExpressionId],
    ) -> ConversionResult<Option<ExpressionId>> {
        if (self.known_term(function, STATE_THREAD, "run")?
            || self.known_term(function, "Effect.Unsafe", "unsafePerformEffect")?)
            && let [effect] = *arguments
        {
            return Ok(Some(self.run_effect(effect)));
        }
        for &(module_name, _, make_prefix) in UNCURRIED_EFFECT_MODULES {
            if let Some(arity) =
                self.known_numbered_term_arity(function, module_name, make_prefix)?
                && arity >= 1
                && let [function] = *arguments
                && let Some(function) = self.uncurry_abstraction(function, arity)
            {
                let ExpressionKind::UncurriedAbstraction { parameters, body } =
                    &self.storage[function].kind
                else {
                    unreachable!("invariant violated: uncurried abstraction has another kind")
                };
                let parameters = parameters.clone();
                let body = self.run_effect(*body);
                let abstraction = ExpressionKind::UncurriedAbstraction { parameters, body };
                self.storage.replace_expression_kind(function, abstraction);
                return Ok(Some(function));
            }
        }
        Ok(None)
    }

    fn run_effect(&mut self, effect: ExpressionId) -> ExpressionId {
        self.expression(ExpressionKind::UncurriedApplication {
            function: effect,
            arguments: [].into(),
            synthetic: false,
        })
    }
}
