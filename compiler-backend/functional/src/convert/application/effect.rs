use building_types::QueryResult;

use crate::tree::{
    EffectExpression, EffectPrimitive, ExpressionId, ReferenceModifyResult, ReferenceWriteResult,
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
}
