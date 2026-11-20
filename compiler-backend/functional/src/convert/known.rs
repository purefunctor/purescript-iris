use building_types::QueryResult;
use files::FileId;
use indexing::{IndexedTermItemKind, InstanceId, InstanceSourceItemId, TermItemId, TypeItemId};
use itertools::Itertools;
use rustc_hash::FxHashMap;
use smol_str::SmolStr;

use crate::tree::{ExpressionId, ExpressionKind, GlobalId, InstanceIdentity};

use super::Context;
use super::stylex::IrisStyleXCore;

#[derive(Debug, PartialEq, Eq)]
pub struct FunctionalCore {
    pub(super) iris_effect: IrisEffectCore,
    pub(super) iris_stylex: IrisStyleXCore,
    pub(super) known_terms: FxHashMap<(FileId, TermItemId), KnownTerm>,
    pub(super) known_instances: FxHashMap<InstanceIdentity, KnownInstance>,
    pub(super) known_thunk_instances: FxHashMap<(FileId, InstanceId), (FileId, TypeItemId)>,
    pub(super) known_unit_term: Option<(FileId, TermItemId)>,
    pub(super) known_asynchronous_constructors: Option<KnownAsynchronousConstructors>,
}

impl FunctionalCore {
    pub fn new(queries: &impl checking::ExternalQueries) -> QueryResult<FunctionalCore> {
        let iris_effect = IrisEffectCore::collect(queries)?;
        let iris_stylex = IrisStyleXCore::collect(queries)?;
        let known_terms = collect_known_terms(queries)?;
        let known_instances = collect_known_instances(queries)?;
        let known_thunk_instances = collect_known_thunk_instances(queries)?;
        let known_unit_term = find_term(queries, "Data.Unit", "unit")?;
        let known_asynchronous_constructors = KnownAsynchronousConstructors::collect(queries)?;
        Ok(FunctionalCore {
            iris_effect,
            iris_stylex,
            known_terms,
            known_instances,
            known_thunk_instances,
            known_unit_term,
            known_asynchronous_constructors,
        })
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

fn find_term(
    queries: &impl checking::ExternalQueries,
    module_name: &str,
    term_name: &str,
) -> QueryResult<Option<(FileId, TermItemId)>> {
    let Some(file_id) = queries.module_file(module_name) else { return Ok(None) };
    let indexed = queries.indexed(file_id)?;
    let mut terms = indexed.items.iter_terms();
    let term = terms.find(|(_, item)| item.name.as_deref() == Some(term_name));
    Ok(term.map(|(term_id, _)| (file_id, term_id)))
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct IrisEffectCore {
    pub(super) get_service: (FileId, TermItemId),
}

impl IrisEffectCore {
    fn collect(queries: &impl checking::ExternalQueries) -> QueryResult<IrisEffectCore> {
        let file_id = queries
            .module_file("Iris.Effect")
            .unwrap_or_else(|| unreachable!("invariant violated: Iris.Effect not found"));
        let resolved = queries.resolved(file_id)?;
        let get_service = resolved
            .exports
            .lookup_term("getService")
            .unwrap_or_else(|| unreachable!("invariant violated: getService not in Iris.Effect"));
        Ok(IrisEffectCore { get_service })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum KnownInstance {
    ApplicativeAsync,
    ApplicativeSync,
    ApplyAsync,
    ApplySync,
    BindAsync,
    BindSync,
    CategoryBuilder,
    CategoryFunction,
    DiscardUnit,
    EqBoolean,
    EqChar,
    EqInt,
    EqNumber,
    EqString,
    EuclideanRingInt,
    EuclideanRingNumber,
    FunctorAsync,
    FunctorSync,
    HeytingAlgebraBoolean,
    OrdBoolean,
    OrdChar,
    OrdInt,
    OrdNumber,
    OrdString,
    RingInt,
    RingNumber,
    SemigroupArray,
    SemigroupString,
    SemigroupoidBuilder,
    SemigroupoidFunction,
    SemiringInt,
    SemiringNumber,
}

const KNOWN_INSTANCES: &[(KnownInstance, &str, &str, &str, &str)] = &[
    (
        KnownInstance::ApplicativeAsync,
        "Control.Applicative",
        "Applicative",
        "Iris.Effect.Async",
        "Async",
    ),
    (
        KnownInstance::ApplicativeSync,
        "Control.Applicative",
        "Applicative",
        "Iris.Effect.Sync",
        "Sync",
    ),
    (KnownInstance::ApplyAsync, "Control.Apply", "Apply", "Iris.Effect.Async", "Async"),
    (KnownInstance::ApplySync, "Control.Apply", "Apply", "Iris.Effect.Sync", "Sync"),
    (KnownInstance::BindAsync, "Control.Bind", "Bind", "Iris.Effect.Async", "Async"),
    (KnownInstance::BindSync, "Control.Bind", "Bind", "Iris.Effect.Sync", "Sync"),
    (KnownInstance::CategoryBuilder, "Control.Category", "Category", "Record.Builder", "Builder"),
    (KnownInstance::CategoryFunction, "Control.Category", "Category", "Prim", "Function"),
    (KnownInstance::DiscardUnit, "Control.Bind", "Discard", "Data.Unit", "Unit"),
    (KnownInstance::EqBoolean, "Data.Eq", "Eq", "Prim", "Boolean"),
    (KnownInstance::EqChar, "Data.Eq", "Eq", "Prim", "Char"),
    (KnownInstance::EqInt, "Data.Eq", "Eq", "Prim", "Int"),
    (KnownInstance::EqNumber, "Data.Eq", "Eq", "Prim", "Number"),
    (KnownInstance::EqString, "Data.Eq", "Eq", "Prim", "String"),
    (KnownInstance::EuclideanRingInt, "Data.EuclideanRing", "EuclideanRing", "Prim", "Int"),
    (KnownInstance::EuclideanRingNumber, "Data.EuclideanRing", "EuclideanRing", "Prim", "Number"),
    (KnownInstance::FunctorAsync, "Data.Functor", "Functor", "Iris.Effect.Async", "Async"),
    (KnownInstance::FunctorSync, "Data.Functor", "Functor", "Iris.Effect.Sync", "Sync"),
    (
        KnownInstance::HeytingAlgebraBoolean,
        "Data.HeytingAlgebra",
        "HeytingAlgebra",
        "Prim",
        "Boolean",
    ),
    (KnownInstance::OrdBoolean, "Data.Ord", "Ord", "Prim", "Boolean"),
    (KnownInstance::OrdChar, "Data.Ord", "Ord", "Prim", "Char"),
    (KnownInstance::OrdInt, "Data.Ord", "Ord", "Prim", "Int"),
    (KnownInstance::OrdNumber, "Data.Ord", "Ord", "Prim", "Number"),
    (KnownInstance::OrdString, "Data.Ord", "Ord", "Prim", "String"),
    (KnownInstance::RingInt, "Data.Ring", "Ring", "Prim", "Int"),
    (KnownInstance::RingNumber, "Data.Ring", "Ring", "Prim", "Number"),
    (KnownInstance::SemigroupArray, "Data.Semigroup", "Semigroup", "Prim", "Array"),
    (KnownInstance::SemigroupString, "Data.Semigroup", "Semigroup", "Prim", "String"),
    (
        KnownInstance::SemigroupoidBuilder,
        "Control.Semigroupoid",
        "Semigroupoid",
        "Record.Builder",
        "Builder",
    ),
    (
        KnownInstance::SemigroupoidFunction,
        "Control.Semigroupoid",
        "Semigroupoid",
        "Prim",
        "Function",
    ),
    (KnownInstance::SemiringInt, "Data.Semiring", "Semiring", "Prim", "Int"),
    (KnownInstance::SemiringNumber, "Data.Semiring", "Semiring", "Prim", "Number"),
];

fn collect_known_instances(
    queries: &impl checking::ExternalQueries,
) -> QueryResult<FxHashMap<InstanceIdentity, KnownInstance>> {
    let mut instances = FxHashMap::default();
    for &(known, class_module, class_name, head_module, head_name) in KNOWN_INSTANCES {
        let Some(class) = find_exported_class(queries, class_module, class_name)? else {
            continue;
        };
        let Some(head) = find_exported_type(queries, head_module, head_name)? else {
            continue;
        };
        let mut candidates = Vec::new();
        for file_id in [class.0, head.0].into_iter().unique() {
            let indexed = queries.indexed(file_id)?;
            let checked = queries.checked(file_id)?;
            for &source in indexed.items.instance_sources() {
                let (identity, instance) = match source {
                    InstanceSourceItemId::Instance(item) => {
                        let id = indexed.items[item].id;
                        let identity = InstanceIdentity::Declared(file_id, id);
                        let instance = checked.lookup_instance(id);
                        (identity, instance)
                    }
                    InstanceSourceItemId::Derive(item) => {
                        let id = indexed.items[item].id;
                        let identity = InstanceIdentity::Derived(file_id, id);
                        let instance = checked.lookup_derived_instance(id);
                        (identity, instance)
                    }
                };
                let Some(instance) = instance else { continue };
                if instance.resolution == class && instance_head(queries, &instance) == Some(head) {
                    candidates.push(identity);
                }
            }
        }
        if let [identity] = *candidates {
            instances.insert(identity, known);
        }
    }
    Ok(instances)
}

fn instance_head(
    queries: &impl checking::ExternalQueries,
    instance: &checking::core::CheckedInstance,
) -> Option<(FileId, TypeItemId)> {
    let [argument] = *instance.head.as_ref()?.type_arguments else { return None };
    let mut current = argument;
    loop {
        match *queries.lookup_type(current) {
            checking::Type::Application(function, _)
            | checking::Type::KindApplication(function, _)
            | checking::Type::Kinded(function, _) => current = function,
            checking::Type::Constructor(file_id, type_id) => return Some((file_id, type_id)),
            _ => return None,
        }
    }
}

fn find_exported_class(
    queries: &impl checking::ExternalQueries,
    module_name: &str,
    class_name: &str,
) -> QueryResult<Option<(FileId, TypeItemId)>> {
    let Some(file_id) = queries.module_file(module_name) else { return Ok(None) };
    let resolved = queries.resolved(file_id)?;
    Ok(resolved.exports.lookup_class(class_name))
}

fn find_exported_type(
    queries: &impl checking::ExternalQueries,
    module_name: &str,
    type_name: &str,
) -> QueryResult<Option<(FileId, TypeItemId)>> {
    let Some(file_id) = queries.module_file(module_name) else { return Ok(None) };
    let resolved = queries.resolved(file_id)?;
    Ok(resolved.exports.lookup_type(type_name))
}

const THUNK_MODULES: [&str; 2] = ["Effect", "Control.Monad.ST.Internal"];

fn collect_known_thunk_instances(
    queries: &impl checking::ExternalQueries,
) -> QueryResult<FxHashMap<(FileId, InstanceId), (FileId, TypeItemId)>> {
    let mut known_thunk_instances = FxHashMap::default();
    for module_name in THUNK_MODULES {
        let Some(file_id) = queries.module_file(module_name) else { continue };
        let indexed = queries.indexed(file_id)?;
        let checked = queries.checked(file_id)?;
        let instances = indexed.items.iter_instances().filter_map(|(_, item)| {
            let instance = checked.lookup_instance(item.id)?;
            Some((instance.resolution, item.id))
        });
        let instances = instances.into_group_map();
        for (class, instances) in instances {
            if let [instance_id] = *instances {
                known_thunk_instances.insert((file_id, instance_id), class);
            }
        }
    }
    Ok(known_thunk_instances)
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct KnownAsynchronousConstructors {
    pub(super) pure: SmolStr,
    pub(super) throw: SmolStr,
    pub(super) run: SmolStr,
    pub(super) bind: SmolStr,
    pub(super) catch: SmolStr,
    pub(super) left: SmolStr,
    pub(super) right: SmolStr,
}

impl KnownAsynchronousConstructors {
    fn collect(
        queries: &impl checking::ExternalQueries,
    ) -> QueryResult<Option<KnownAsynchronousConstructors>> {
        let Some(pure) = find_constructor(queries, "Iris.Effect.Fiber", "Pure")? else {
            return Ok(None);
        };
        let Some(throw) = find_constructor(queries, "Iris.Effect.Fiber", "Throw")? else {
            return Ok(None);
        };
        let Some(run) = find_constructor(queries, "Iris.Effect.Fiber", "Run")? else {
            return Ok(None);
        };
        let Some(bind) = find_constructor(queries, "Iris.Effect.Fiber", "Bind")? else {
            return Ok(None);
        };
        let Some(catch) = find_constructor(queries, "Iris.Effect.Fiber", "Catch")? else {
            return Ok(None);
        };
        let Some(left) = find_constructor(queries, "Data.Either", "Left")? else {
            return Ok(None);
        };
        let Some(right) = find_constructor(queries, "Data.Either", "Right")? else {
            return Ok(None);
        };
        Ok(Some(KnownAsynchronousConstructors { pure, throw, run, bind, catch, left, right }))
    }
}

fn find_constructor(
    queries: &impl checking::ExternalQueries,
    module_name: &str,
    constructor_name: &str,
) -> QueryResult<Option<SmolStr>> {
    let Some(file_id) = queries.module_file(module_name) else { return Ok(None) };
    let indexed = queries.indexed(file_id)?;
    let mut terms = indexed.items.iter_terms();
    let constructor = terms.find(|&(term_id, item)| {
        item.name.as_deref() == Some(constructor_name)
            && indexed.constructor_type(term_id).is_some()
    });
    Ok(constructor.and_then(|(_, item)| item.name.clone()))
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

    pub(super) fn known_instance(&self, expression: ExpressionId) -> Option<KnownInstance> {
        let ExpressionKind::Global { global } = &self.storage[expression].kind else {
            return None;
        };
        let GlobalId::Instance(identity) = global.id else { return None };
        self.functional_core.known_instances.get(&identity).copied()
    }
}
