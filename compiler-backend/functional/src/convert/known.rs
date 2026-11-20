use building_types::QueryResult;
use files::FileId;
use indexing::{IndexedTermItemKind, TermItemId};
use itertools::Itertools;
use rustc_hash::FxHashMap;

use crate::tree::{ExpressionId, ExpressionKind, GlobalId};

use super::Context;

#[derive(Debug, PartialEq, Eq)]
pub struct FunctionalCore {
    pub(super) known_terms: FxHashMap<(FileId, TermItemId), KnownTerm>,
}

impl FunctionalCore {
    pub fn new(queries: &impl checking::ExternalQueries) -> QueryResult<FunctionalCore> {
        let known_terms = collect_known_terms(queries)?;
        Ok(FunctionalCore { known_terms })
    }
}

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

const KNOWN_TERMS: &[(&str, &str, KnownTerm)] = &[
    ("Control.Applicative", "pure", KnownTerm::ControlApplicativePure),
    ("Control.Applicative", "unless", KnownTerm::ControlApplicativeUnless),
    ("Control.Applicative", "when", KnownTerm::ControlApplicativeWhen),
    ("Control.Apply", "apply", KnownTerm::ControlApplyApply),
    ("Control.Apply", "applyFirst", KnownTerm::ControlApplyApplyFirst),
    ("Control.Apply", "applySecond", KnownTerm::ControlApplyApplySecond),
    ("Control.Bind", "bind", KnownTerm::ControlBindBind),
    ("Control.Bind", "discard", KnownTerm::ControlBindDiscard),
    ("Control.Category", "identity", KnownTerm::ControlCategoryIdentity),
    ("Control.Monad.ST.Internal", "for", KnownTerm::ControlMonadSTInternalFor),
    ("Control.Monad.ST.Internal", "foreach", KnownTerm::ControlMonadSTInternalForeach),
    ("Control.Monad.ST.Internal", "modify", KnownTerm::ControlMonadSTInternalModify),
    ("Control.Monad.ST.Internal", "modify'", KnownTerm::ControlMonadSTInternalModifyPrime),
    ("Control.Monad.ST.Internal", "modify_", KnownTerm::ControlMonadSTInternalModifyUnderscore),
    ("Control.Monad.ST.Internal", "new", KnownTerm::ControlMonadSTInternalNew),
    ("Control.Monad.ST.Internal", "read", KnownTerm::ControlMonadSTInternalRead),
    ("Control.Monad.ST.Internal", "run", KnownTerm::ControlMonadSTInternalRun),
    ("Control.Monad.ST.Internal", "while", KnownTerm::ControlMonadSTInternalWhile),
    ("Control.Monad.ST.Internal", "write", KnownTerm::ControlMonadSTInternalWrite),
    ("Control.Semigroupoid", "compose", KnownTerm::ControlSemigroupoidCompose),
    ("Control.Semigroupoid", "composeFlipped", KnownTerm::ControlSemigroupoidComposeFlipped),
    ("Data.Array", "length", KnownTerm::DataArrayLength),
    ("Data.Array", "unsafeIndex", KnownTerm::DataArrayUnsafeIndex),
    ("Data.Eq", "eq", KnownTerm::DataEqEq),
    ("Data.Eq", "notEq", KnownTerm::DataEqNotEq),
    ("Data.EuclideanRing", "div", KnownTerm::DataEuclideanRingDiv),
    ("Data.EuclideanRing", "mod", KnownTerm::DataEuclideanRingMod),
    ("Data.Function", "apply", KnownTerm::DataFunctionApply),
    ("Data.Function", "applyFlipped", KnownTerm::DataFunctionApplyFlipped),
    ("Data.Functor", "map", KnownTerm::DataFunctorMap),
    ("Data.HeytingAlgebra", "conj", KnownTerm::DataHeytingAlgebraConj),
    ("Data.HeytingAlgebra", "disj", KnownTerm::DataHeytingAlgebraDisj),
    ("Data.HeytingAlgebra", "not", KnownTerm::DataHeytingAlgebraNot),
    ("Data.Int.Bits", "and", KnownTerm::DataIntBitsAnd),
    ("Data.Int.Bits", "complement", KnownTerm::DataIntBitsComplement),
    ("Data.Int.Bits", "or", KnownTerm::DataIntBitsOr),
    ("Data.Int.Bits", "shl", KnownTerm::DataIntBitsShl),
    ("Data.Int.Bits", "shr", KnownTerm::DataIntBitsShr),
    ("Data.Int.Bits", "xor", KnownTerm::DataIntBitsXor),
    ("Data.Int.Bits", "zshr", KnownTerm::DataIntBitsZshr),
    ("Data.Ord", "compare", KnownTerm::DataOrdCompare),
    ("Data.Ord", "greaterThan", KnownTerm::DataOrdGreaterThan),
    ("Data.Ord", "greaterThanOrEq", KnownTerm::DataOrdGreaterThanOrEq),
    ("Data.Ord", "lessThan", KnownTerm::DataOrdLessThan),
    ("Data.Ord", "lessThanOrEq", KnownTerm::DataOrdLessThanOrEq),
    ("Data.Ring", "negate", KnownTerm::DataRingNegate),
    ("Data.Ring", "sub", KnownTerm::DataRingSub),
    ("Data.Semigroup", "append", KnownTerm::DataSemigroupAppend),
    ("Data.Semigroup", "concatArray", KnownTerm::DataSemigroupConcatArray),
    ("Data.Semiring", "add", KnownTerm::DataSemiringAdd),
    ("Data.Semiring", "mul", KnownTerm::DataSemiringMul),
    ("Data.String.CodePoints", "toCodePointArray", KnownTerm::DataStringCodePointsToCodePointArray),
    ("Effect", "forE", KnownTerm::EffectForE),
    ("Effect", "foreachE", KnownTerm::EffectForeachE),
    ("Effect", "untilE", KnownTerm::EffectUntilE),
    ("Effect", "whileE", KnownTerm::EffectWhileE),
    ("Effect.Ref", "modify", KnownTerm::EffectRefModify),
    ("Effect.Ref", "modify'", KnownTerm::EffectRefModifyPrime),
    ("Effect.Ref", "modify_", KnownTerm::EffectRefModifyUnderscore),
    ("Effect.Ref", "new", KnownTerm::EffectRefNew),
    ("Effect.Ref", "read", KnownTerm::EffectRefRead),
    ("Effect.Ref", "write", KnownTerm::EffectRefWrite),
    ("Effect.Unsafe", "unsafePerformEffect", KnownTerm::EffectUnsafePerformEffect),
    ("Iris.Effect", "getService", KnownTerm::IrisEffectGetService),
    ("Iris.Effect.Async", "attempt", KnownTerm::IrisEffectAsyncAttempt),
    ("Iris.Effect.Async", "catch", KnownTerm::IrisEffectAsyncCatch),
    ("Iris.Effect.Async", "fail", KnownTerm::IrisEffectAsyncFail),
    ("Iris.Effect.Async", "liftEffect", KnownTerm::IrisEffectAsyncLiftEffect),
    ("Iris.Effect.Async", "mapError", KnownTerm::IrisEffectAsyncMapError),
    ("Iris.Effect.Async", "provide", KnownTerm::IrisEffectAsyncProvide),
    ("Iris.Effect.Async", "service", KnownTerm::IrisEffectAsyncService),
    ("Iris.Effect.Async", "sync", KnownTerm::IrisEffectAsyncSync),
    ("Iris.Effect.Sync", "liftEffect", KnownTerm::IrisEffectSyncLiftEffect),
    ("Iris.Effect.Sync", "make", KnownTerm::IrisEffectSyncMake),
    ("Iris.Effect.Sync", "provide", KnownTerm::IrisEffectSyncProvide),
    ("Iris.Effect.Sync", "run", KnownTerm::IrisEffectSyncRun),
    ("Iris.Effect.Sync", "service", KnownTerm::IrisEffectSyncService),
    ("Partial.Unsafe", "unsafePartial", KnownTerm::PartialUnsafeUnsafePartial),
    ("Record.Builder", "build", KnownTerm::RecordBuilderBuild),
    ("Record.Builder", "buildFromScratch", KnownTerm::RecordBuilderBuildFromScratch),
    ("Record.Builder", "delete", KnownTerm::RecordBuilderDelete),
    ("Record.Builder", "disjointUnion", KnownTerm::RecordBuilderDisjointUnion),
    ("Record.Builder", "insert", KnownTerm::RecordBuilderInsert),
    ("Record.Builder", "merge", KnownTerm::RecordBuilderMerge),
    ("Record.Builder", "modify", KnownTerm::RecordBuilderModify),
    ("Record.Builder", "nub", KnownTerm::RecordBuilderNub),
    ("Record.Builder", "rename", KnownTerm::RecordBuilderRename),
    ("Record.Builder", "union", KnownTerm::RecordBuilderUnion),
    ("Record.Unsafe", "unsafeDelete", KnownTerm::RecordUnsafeUnsafeDelete),
    ("Record.Unsafe", "unsafeGet", KnownTerm::RecordUnsafeUnsafeGet),
    ("Record.Unsafe", "unsafeHas", KnownTerm::RecordUnsafeUnsafeHas),
    ("Record.Unsafe", "unsafeSet", KnownTerm::RecordUnsafeUnsafeSet),
    ("Record.Unsafe.Union", "unsafeUnion", KnownTerm::RecordUnsafeUnionUnsafeUnion),
    ("Record.Unsafe.Union", "unsafeUnionFn", KnownTerm::RecordUnsafeUnionUnsafeUnionFn),
    ("Safe.Coerce", "coerce", KnownTerm::SafeCoerceCoerce),
    ("Unsafe.Coerce", "unsafeCoerce", KnownTerm::UnsafeCoerceUnsafeCoerce),
];

type KnownTermFamily = fn(usize) -> KnownTerm;

const NUMBERED_TERMS: &[(&str, &str, KnownTermFamily)] = &[
    ("Control.Monad.ST.Uncurried", "mkSTFn", KnownTerm::ControlMonadSTUncurriedMkSTFn),
    ("Control.Monad.ST.Uncurried", "runSTFn", KnownTerm::ControlMonadSTUncurriedRunSTFn),
    ("Data.Function.Uncurried", "mkFn", KnownTerm::DataFunctionUncurriedMkFn),
    ("Data.Function.Uncurried", "runFn", KnownTerm::DataFunctionUncurriedRunFn),
    ("Effect.Uncurried", "mkEffectFn", KnownTerm::EffectUncurriedMkEffectFn),
    ("Effect.Uncurried", "runEffectFn", KnownTerm::EffectUncurriedRunEffectFn),
];

impl KnownTerm {
    fn classify(module_name: &str, item_name: &str) -> Option<KnownTerm> {
        let named = KNOWN_TERMS
            .iter()
            .find(|&&(module, item, _)| module == module_name && item == item_name);
        if let Some(&(_, _, known)) = named {
            return Some(known);
        }
        NUMBERED_TERMS.iter().find_map(|&(module, prefix, known)| {
            if module != module_name {
                return None;
            }
            numbered_arity(item_name, prefix).map(known)
        })
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

fn collect_known_terms(
    queries: &impl checking::ExternalQueries,
) -> QueryResult<FxHashMap<(FileId, TermItemId), KnownTerm>> {
    let named = KNOWN_TERMS.iter().map(|&(module_name, _, _)| module_name);
    let numbered = NUMBERED_TERMS.iter().map(|&(module_name, _, _)| module_name);
    let mut terms = FxHashMap::default();
    for module_name in named.chain(numbered).unique() {
        let Some(file_id) = queries.module_file(module_name) else { continue };
        let indexed = queries.indexed(file_id)?;
        for (term_id, item) in indexed.items.iter_terms() {
            let Some(name) = item.name.as_deref() else { continue };
            let Some(known) = KnownTerm::classify(module_name, name) else { continue };
            if known.is_class_member()
                && !matches!(item.kind, IndexedTermItemKind::ClassMember { .. })
            {
                continue;
            }
            terms.insert((file_id, term_id), known);
        }
    }
    Ok(terms)
}

impl<'c, Q> Context<'c, Q>
where
    Q: checking::ExternalQueries,
{
    pub(super) fn known_term(&self, expression: ExpressionId) -> Option<KnownTerm> {
        let ExpressionKind::Global { global } = &self.storage[expression].kind else {
            return None;
        };
        let GlobalId::Term(file_id, term_id) = global.id else { return None };
        self.functional_core.known_terms.get(&(file_id, term_id)).copied()
    }
}
