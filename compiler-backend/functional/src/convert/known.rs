use building_types::QueryResult;
use indexing::IndexedTermItemKind;

use crate::tree::{ExpressionId, ExpressionKind, GlobalId};

use super::Context;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum KnownTerm {
    ControlApplicativePure,
    ControlApplicativeUnless,
    ControlApplicativeWhen,
    ControlApplyApply,
    ControlApplyApplyFirst,
    ControlApplyApplySecond,
    ControlBindBind,
    ControlBindDiscard,
    ControlCategoryIdentity,
    ControlMonadSTInternalFor,
    ControlMonadSTInternalForeach,
    ControlMonadSTInternalModify,
    ControlMonadSTInternalModifyPrime,
    ControlMonadSTInternalModifyUnderscore,
    ControlMonadSTInternalNew,
    ControlMonadSTInternalRead,
    ControlMonadSTInternalRun,
    ControlMonadSTInternalWhile,
    ControlMonadSTInternalWrite,
    ControlMonadSTUncurriedMkSTFn(usize),
    ControlMonadSTUncurriedRunSTFn(usize),
    ControlSemigroupoidCompose,
    ControlSemigroupoidComposeFlipped,
    DataArrayLength,
    DataArrayUnsafeIndex,
    DataEqEq,
    DataEqNotEq,
    DataEuclideanRingDiv,
    DataEuclideanRingMod,
    DataFunctionApply,
    DataFunctionApplyFlipped,
    DataFunctionUncurriedMkFn(usize),
    DataFunctionUncurriedRunFn(usize),
    DataFunctorMap,
    DataHeytingAlgebraConj,
    DataHeytingAlgebraDisj,
    DataHeytingAlgebraNot,
    DataIntBitsAnd,
    DataIntBitsComplement,
    DataIntBitsOr,
    DataIntBitsShl,
    DataIntBitsShr,
    DataIntBitsXor,
    DataIntBitsZshr,
    DataOrdCompare,
    DataOrdGreaterThan,
    DataOrdGreaterThanOrEq,
    DataOrdLessThan,
    DataOrdLessThanOrEq,
    DataRingNegate,
    DataRingSub,
    DataSemigroupAppend,
    DataSemigroupConcatArray,
    DataSemiringAdd,
    DataSemiringMul,
    DataStringCodePointsToCodePointArray,
    EffectForE,
    EffectForeachE,
    EffectRefModify,
    EffectRefModifyPrime,
    EffectRefModifyUnderscore,
    EffectRefNew,
    EffectRefRead,
    EffectRefWrite,
    EffectUncurriedMkEffectFn(usize),
    EffectUncurriedRunEffectFn(usize),
    EffectUnsafePerformEffect,
    EffectUntilE,
    EffectWhileE,
    IrisEffectAsyncAttempt,
    IrisEffectAsyncCatch,
    IrisEffectAsyncFail,
    IrisEffectAsyncLiftEffect,
    IrisEffectAsyncMapError,
    IrisEffectAsyncProvide,
    IrisEffectAsyncService,
    IrisEffectAsyncSync,
    IrisEffectGetService,
    IrisEffectSyncLiftEffect,
    IrisEffectSyncMake,
    IrisEffectSyncProvide,
    IrisEffectSyncRun,
    IrisEffectSyncService,
    PartialUnsafeUnsafePartial,
    RecordBuilderBuild,
    RecordBuilderBuildFromScratch,
    RecordBuilderDelete,
    RecordBuilderDisjointUnion,
    RecordBuilderInsert,
    RecordBuilderMerge,
    RecordBuilderModify,
    RecordBuilderNub,
    RecordBuilderRename,
    RecordBuilderUnion,
    RecordUnsafeUnsafeDelete,
    RecordUnsafeUnsafeGet,
    RecordUnsafeUnsafeHas,
    RecordUnsafeUnsafeSet,
    RecordUnsafeUnionUnsafeUnion,
    RecordUnsafeUnionUnsafeUnionFn,
    SafeCoerceCoerce,
    UnsafeCoerceUnsafeCoerce,
}

impl KnownTerm {
    fn classify(module_name: &str, item_name: &str) -> Option<KnownTerm> {
        let term = match (module_name, item_name) {
            ("Control.Applicative", "pure") => KnownTerm::ControlApplicativePure,
            ("Control.Applicative", "unless") => KnownTerm::ControlApplicativeUnless,
            ("Control.Applicative", "when") => KnownTerm::ControlApplicativeWhen,
            ("Control.Apply", "apply") => KnownTerm::ControlApplyApply,
            ("Control.Apply", "applyFirst") => KnownTerm::ControlApplyApplyFirst,
            ("Control.Apply", "applySecond") => KnownTerm::ControlApplyApplySecond,
            ("Control.Bind", "bind") => KnownTerm::ControlBindBind,
            ("Control.Bind", "discard") => KnownTerm::ControlBindDiscard,
            ("Control.Category", "identity") => KnownTerm::ControlCategoryIdentity,
            ("Control.Monad.ST.Internal", "for") => KnownTerm::ControlMonadSTInternalFor,
            ("Control.Monad.ST.Internal", "foreach") => KnownTerm::ControlMonadSTInternalForeach,
            ("Control.Monad.ST.Internal", "modify") => KnownTerm::ControlMonadSTInternalModify,
            ("Control.Monad.ST.Internal", "modify'") => {
                KnownTerm::ControlMonadSTInternalModifyPrime
            }
            ("Control.Monad.ST.Internal", "modify_") => {
                KnownTerm::ControlMonadSTInternalModifyUnderscore
            }
            ("Control.Monad.ST.Internal", "new") => KnownTerm::ControlMonadSTInternalNew,
            ("Control.Monad.ST.Internal", "read") => KnownTerm::ControlMonadSTInternalRead,
            ("Control.Monad.ST.Internal", "run") => KnownTerm::ControlMonadSTInternalRun,
            ("Control.Monad.ST.Internal", "while") => KnownTerm::ControlMonadSTInternalWhile,
            ("Control.Monad.ST.Internal", "write") => KnownTerm::ControlMonadSTInternalWrite,
            ("Control.Monad.ST.Uncurried", _) => {
                return numbered_arity(item_name, "mkSTFn")
                    .map(KnownTerm::ControlMonadSTUncurriedMkSTFn)
                    .or_else(|| {
                        numbered_arity(item_name, "runSTFn")
                            .map(KnownTerm::ControlMonadSTUncurriedRunSTFn)
                    });
            }
            ("Control.Semigroupoid", "compose") => KnownTerm::ControlSemigroupoidCompose,
            ("Control.Semigroupoid", "composeFlipped") => {
                KnownTerm::ControlSemigroupoidComposeFlipped
            }
            ("Data.Array", "length") => KnownTerm::DataArrayLength,
            ("Data.Array", "unsafeIndex") => KnownTerm::DataArrayUnsafeIndex,
            ("Data.Eq", "eq") => KnownTerm::DataEqEq,
            ("Data.Eq", "notEq") => KnownTerm::DataEqNotEq,
            ("Data.EuclideanRing", "div") => KnownTerm::DataEuclideanRingDiv,
            ("Data.EuclideanRing", "mod") => KnownTerm::DataEuclideanRingMod,
            ("Data.Function", "apply") => KnownTerm::DataFunctionApply,
            ("Data.Function", "applyFlipped") => KnownTerm::DataFunctionApplyFlipped,
            ("Data.Function.Uncurried", _) => {
                return numbered_arity(item_name, "mkFn")
                    .map(KnownTerm::DataFunctionUncurriedMkFn)
                    .or_else(|| {
                        numbered_arity(item_name, "runFn")
                            .map(KnownTerm::DataFunctionUncurriedRunFn)
                    });
            }
            ("Data.Functor", "map") => KnownTerm::DataFunctorMap,
            ("Data.HeytingAlgebra", "conj") => KnownTerm::DataHeytingAlgebraConj,
            ("Data.HeytingAlgebra", "disj") => KnownTerm::DataHeytingAlgebraDisj,
            ("Data.HeytingAlgebra", "not") => KnownTerm::DataHeytingAlgebraNot,
            ("Data.Int.Bits", "and") => KnownTerm::DataIntBitsAnd,
            ("Data.Int.Bits", "complement") => KnownTerm::DataIntBitsComplement,
            ("Data.Int.Bits", "or") => KnownTerm::DataIntBitsOr,
            ("Data.Int.Bits", "shl") => KnownTerm::DataIntBitsShl,
            ("Data.Int.Bits", "shr") => KnownTerm::DataIntBitsShr,
            ("Data.Int.Bits", "xor") => KnownTerm::DataIntBitsXor,
            ("Data.Int.Bits", "zshr") => KnownTerm::DataIntBitsZshr,
            ("Data.Ord", "compare") => KnownTerm::DataOrdCompare,
            ("Data.Ord", "greaterThan") => KnownTerm::DataOrdGreaterThan,
            ("Data.Ord", "greaterThanOrEq") => KnownTerm::DataOrdGreaterThanOrEq,
            ("Data.Ord", "lessThan") => KnownTerm::DataOrdLessThan,
            ("Data.Ord", "lessThanOrEq") => KnownTerm::DataOrdLessThanOrEq,
            ("Data.Ring", "negate") => KnownTerm::DataRingNegate,
            ("Data.Ring", "sub") => KnownTerm::DataRingSub,
            ("Data.Semigroup", "append") => KnownTerm::DataSemigroupAppend,
            ("Data.Semigroup", "concatArray") => KnownTerm::DataSemigroupConcatArray,
            ("Data.Semiring", "add") => KnownTerm::DataSemiringAdd,
            ("Data.Semiring", "mul") => KnownTerm::DataSemiringMul,
            ("Data.String.CodePoints", "toCodePointArray") => {
                KnownTerm::DataStringCodePointsToCodePointArray
            }
            ("Effect", "forE") => KnownTerm::EffectForE,
            ("Effect", "foreachE") => KnownTerm::EffectForeachE,
            ("Effect", "untilE") => KnownTerm::EffectUntilE,
            ("Effect", "whileE") => KnownTerm::EffectWhileE,
            ("Effect.Ref", "modify") => KnownTerm::EffectRefModify,
            ("Effect.Ref", "modify'") => KnownTerm::EffectRefModifyPrime,
            ("Effect.Ref", "modify_") => KnownTerm::EffectRefModifyUnderscore,
            ("Effect.Ref", "new") => KnownTerm::EffectRefNew,
            ("Effect.Ref", "read") => KnownTerm::EffectRefRead,
            ("Effect.Ref", "write") => KnownTerm::EffectRefWrite,
            ("Effect.Uncurried", _) => {
                return numbered_arity(item_name, "mkEffectFn")
                    .map(KnownTerm::EffectUncurriedMkEffectFn)
                    .or_else(|| {
                        numbered_arity(item_name, "runEffectFn")
                            .map(KnownTerm::EffectUncurriedRunEffectFn)
                    });
            }
            ("Effect.Unsafe", "unsafePerformEffect") => KnownTerm::EffectUnsafePerformEffect,
            ("Iris.Effect", "getService") => KnownTerm::IrisEffectGetService,
            ("Iris.Effect.Async", "attempt") => KnownTerm::IrisEffectAsyncAttempt,
            ("Iris.Effect.Async", "catch") => KnownTerm::IrisEffectAsyncCatch,
            ("Iris.Effect.Async", "fail") => KnownTerm::IrisEffectAsyncFail,
            ("Iris.Effect.Async", "liftEffect") => KnownTerm::IrisEffectAsyncLiftEffect,
            ("Iris.Effect.Async", "mapError") => KnownTerm::IrisEffectAsyncMapError,
            ("Iris.Effect.Async", "provide") => KnownTerm::IrisEffectAsyncProvide,
            ("Iris.Effect.Async", "service") => KnownTerm::IrisEffectAsyncService,
            ("Iris.Effect.Async", "sync") => KnownTerm::IrisEffectAsyncSync,
            ("Iris.Effect.Sync", "liftEffect") => KnownTerm::IrisEffectSyncLiftEffect,
            ("Iris.Effect.Sync", "make") => KnownTerm::IrisEffectSyncMake,
            ("Iris.Effect.Sync", "provide") => KnownTerm::IrisEffectSyncProvide,
            ("Iris.Effect.Sync", "run") => KnownTerm::IrisEffectSyncRun,
            ("Iris.Effect.Sync", "service") => KnownTerm::IrisEffectSyncService,
            ("Partial.Unsafe", "unsafePartial") => KnownTerm::PartialUnsafeUnsafePartial,
            ("Record.Builder", "build") => KnownTerm::RecordBuilderBuild,
            ("Record.Builder", "buildFromScratch") => KnownTerm::RecordBuilderBuildFromScratch,
            ("Record.Builder", "delete") => KnownTerm::RecordBuilderDelete,
            ("Record.Builder", "disjointUnion") => KnownTerm::RecordBuilderDisjointUnion,
            ("Record.Builder", "insert") => KnownTerm::RecordBuilderInsert,
            ("Record.Builder", "merge") => KnownTerm::RecordBuilderMerge,
            ("Record.Builder", "modify") => KnownTerm::RecordBuilderModify,
            ("Record.Builder", "nub") => KnownTerm::RecordBuilderNub,
            ("Record.Builder", "rename") => KnownTerm::RecordBuilderRename,
            ("Record.Builder", "union") => KnownTerm::RecordBuilderUnion,
            ("Record.Unsafe", "unsafeDelete") => KnownTerm::RecordUnsafeUnsafeDelete,
            ("Record.Unsafe", "unsafeGet") => KnownTerm::RecordUnsafeUnsafeGet,
            ("Record.Unsafe", "unsafeHas") => KnownTerm::RecordUnsafeUnsafeHas,
            ("Record.Unsafe", "unsafeSet") => KnownTerm::RecordUnsafeUnsafeSet,
            ("Record.Unsafe.Union", "unsafeUnion") => KnownTerm::RecordUnsafeUnionUnsafeUnion,
            ("Record.Unsafe.Union", "unsafeUnionFn") => KnownTerm::RecordUnsafeUnionUnsafeUnionFn,
            ("Safe.Coerce", "coerce") => KnownTerm::SafeCoerceCoerce,
            ("Unsafe.Coerce", "unsafeCoerce") => KnownTerm::UnsafeCoerceUnsafeCoerce,
            _ => return None,
        };
        Some(term)
    }

    fn is_class_member(self) -> bool {
        matches!(
            self,
            KnownTerm::ControlApplicativePure
                | KnownTerm::ControlApplyApply
                | KnownTerm::ControlBindBind
                | KnownTerm::ControlBindDiscard
                | KnownTerm::ControlCategoryIdentity
                | KnownTerm::ControlSemigroupoidCompose
                | KnownTerm::DataEqEq
                | KnownTerm::DataEuclideanRingDiv
                | KnownTerm::DataEuclideanRingMod
                | KnownTerm::DataFunctorMap
                | KnownTerm::DataHeytingAlgebraConj
                | KnownTerm::DataHeytingAlgebraDisj
                | KnownTerm::DataHeytingAlgebraNot
                | KnownTerm::DataOrdCompare
                | KnownTerm::DataRingSub
                | KnownTerm::DataSemigroupAppend
                | KnownTerm::DataSemiringAdd
                | KnownTerm::DataSemiringMul
        )
    }
}

const ARITIES: [&str; 11] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10"];

fn numbered_arity(item_name: &str, item_prefix: &str) -> Option<usize> {
    let digits = item_name.strip_prefix(item_prefix)?;
    ARITIES.iter().position(|arity| *arity == digits)
}

impl<'c, Q> Context<'c, Q>
where
    Q: checking::ExternalQueries,
{
    pub(super) fn known_term(&self, expression: ExpressionId) -> QueryResult<Option<KnownTerm>> {
        let ExpressionKind::Global { global } = &self.storage[expression].kind else {
            return Ok(None);
        };
        let GlobalId::Term(file_id, term_id) = global.id else { return Ok(None) };
        let module_name = self.source_module_name(file_id)?;
        let Some(known) = KnownTerm::classify(&module_name, &global.item_name) else {
            return Ok(None);
        };
        if !known.is_class_member() {
            return Ok(Some(known));
        }
        let indexed = self.indexed_module(file_id)?;
        let kind = &indexed.items[term_id].kind;
        Ok(matches!(kind, IndexedTermItemKind::ClassMember { .. }).then_some(known))
    }
}
