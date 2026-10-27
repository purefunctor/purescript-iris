use building_types::QueryResult;

use crate::tree::{
    EffectExpression, EffectLoop, EffectPrimitive, ExpressionId, ReferenceModifyResult,
    ReferenceWriteResult,
};

use super::super::Context;

const EFFECT_REFERENCE: &str = "Effect.Ref";
const STATE_THREAD: &str = "Control.Monad.ST.Internal";

impl<'c, Q> Context<'c, Q>
where
    Q: checking::ExternalQueries,
{
    pub(super) fn known_effect_operation(
        &self,
        function: ExpressionId,
        arguments: &[ExpressionId],
    ) -> QueryResult<Option<EffectExpression>> {
        if let Some(primitive) = self.known_effect_primitive(function, arguments)? {
            return Ok(Some(EffectExpression::Primitive(primitive)));
        }
        if let Some(effect_loop) = self.known_effect_loop(function, arguments)? {
            return Ok(Some(EffectExpression::Loop(effect_loop)));
        }
        Ok(None)
    }

    fn known_effect_primitive(
        &self,
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
}
