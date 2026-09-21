//! Read-only environment for the type checking algorithm.
//!
//! See documentation for [`CheckContext`] for more information.

use std::cell::RefCell;
use std::ops::Deref;
use std::sync::Arc;

use building_types::QueryResult;
use files::FileId;
use indexing::{IndexedModule, InstanceSourceItemId, TermItemId, TypeItemId};
use itertools::Itertools;
use lowering::{GroupedModule, LoweredModule};
use resolving::ResolvedModule;
use rustc_hash::FxHashMap;
use smol_str::SmolStr;
use stabilizing::StabilizedModule;
use sugar::{Bracketed, Sectioned};

use crate::core::constraint::instances::InstanceCandidateOrigin;
use crate::core::{
    CheckedSynonym, Depth, ForallBinder, ForallBinderId, Name, RowField, RowType, RowTypeId, Type,
    TypeFlags, TypeId,
};
use crate::{CheckedModule, ExternalQueries};

/// The read-only environment threaded through the type checking algorithm.
///
/// This structure holds a reference to [`ExternalQueries`] for interning and
/// making build system queries; Arc references to query results for the current
/// module; and cached lookups for modules and items 'known' by the compiler.
pub struct CheckContext<'q, Q>
where
    Q: ExternalQueries,
{
    pub queries: &'q Q,
    pub core: Arc<CheckedCore>,

    pub id: FileId,
    pub stabilized: Arc<StabilizedModule>,
    pub indexed: Arc<IndexedModule>,
    pub lowered: Arc<LoweredModule>,
    pub grouped: Arc<GroupedModule>,
    pub bracketed: Arc<Bracketed>,
    pub sectioned: Arc<Sectioned>,
    pub resolved: Arc<ResolvedModule>,

    pub(crate) instance_positions: FxHashMap<InstanceCandidateOrigin, usize>,
    checked_dependencies: RefCell<FxHashMap<FileId, Arc<CheckedModule>>>,
    checked_synonyms: RefCell<FxHashMap<(FileId, TypeItemId), Option<CheckedSynonym>>>,
}

impl<'q, Q> Deref for CheckContext<'q, Q>
where
    Q: ExternalQueries,
{
    type Target = CheckedCore;

    fn deref(&self) -> &CheckedCore {
        &self.core
    }
}

impl<'q, Q> CheckContext<'q, Q>
where
    Q: ExternalQueries,
{
    pub fn new(queries: &'q Q, id: FileId) -> QueryResult<CheckContext<'q, Q>> {
        let core = queries.checked_core()?;
        let stabilized = queries.stabilized(id)?;
        let indexed = queries.indexed(id)?;
        let lowered = queries.lowered(id)?;
        let grouped = queries.grouped(id)?;
        let bracketed = queries.bracketed(id)?;
        let sectioned = queries.sectioned(id)?;
        let resolved = queries.resolved(id)?;

        let mut instance_positions = FxHashMap::default();
        for (position, item) in indexed.items.instance_sources().iter().enumerate() {
            let origin = match *item {
                InstanceSourceItemId::Instance(item) => {
                    InstanceCandidateOrigin::Instance(id, indexed.items[item].id)
                }
                InstanceSourceItemId::Derive(item) => {
                    InstanceCandidateOrigin::Derive(id, indexed.items[item].id)
                }
            };
            instance_positions.entry(origin).or_insert(position);
        }

        Ok(CheckContext {
            queries,
            core,
            id,
            stabilized,
            indexed,
            lowered,
            grouped,
            bracketed,
            sectioned,
            resolved,
            instance_positions,
            checked_dependencies: RefCell::default(),
            checked_synonyms: RefCell::default(),
        })
    }

    pub(crate) fn checked_dependency(&self, file_id: FileId) -> QueryResult<Arc<CheckedModule>> {
        debug_assert_ne!(file_id, self.id);

        let checked = self.checked_dependencies.borrow().get(&file_id).cloned();
        if let Some(checked) = checked {
            return Ok(checked);
        }

        let checked = self.queries.checked(file_id)?;
        self.checked_dependencies.borrow_mut().insert(file_id, Arc::clone(&checked));
        Ok(checked)
    }

    pub(crate) fn checked_synonym_dependency(
        &self,
        file_id: FileId,
        type_id: TypeItemId,
    ) -> QueryResult<Option<CheckedSynonym>> {
        debug_assert_ne!(file_id, self.id);

        let key = (file_id, type_id);
        let checked_synonym = self.checked_synonyms.borrow().get(&key).cloned();
        if let Some(checked_synonym) = checked_synonym {
            return Ok(checked_synonym);
        }

        let checked = self.checked_dependency(file_id)?;
        let checked_synonym = checked.lookup_synonym(type_id);
        self.checked_synonyms.borrow_mut().insert(key, checked_synonym.clone());
        Ok(checked_synonym)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct CheckedCore {
    pub prim: PrimCore,
    pub prim_int: PrimIntCore,
    pub prim_boolean: PrimBooleanCore,
    pub prim_ordering: PrimOrderingCore,
    pub prim_symbol: PrimSymbolCore,
    pub prim_effect: PrimEffectCore,
    pub prim_row: PrimRowCore,
    pub prim_row_list: PrimRowListCore,
    pub prim_coerce: PrimCoerceCore,
    pub prim_type_error: PrimTypeErrorCore,
    pub iris_effect: IrisEffectCore,
    pub known_types: KnownTypesCore,
    pub known_terms: KnownTermsCore,
    pub known_reflectable: KnownReflectableCore,
    pub known_generic: Option<KnownGeneric>,
    pub prim_indexed: Arc<IndexedModule>,
    pub prim_resolved: Arc<ResolvedModule>,
}

impl CheckedCore {
    pub fn new(queries: &impl ExternalQueries) -> QueryResult<CheckedCore> {
        let prim = PrimCore::collect(queries)?;
        let prim_int = PrimIntCore::collect(queries)?;
        let prim_boolean = PrimBooleanCore::collect(queries)?;
        let prim_ordering = PrimOrderingCore::collect(queries)?;
        let prim_symbol = PrimSymbolCore::collect(queries)?;
        let prim_effect = PrimEffectCore::collect(queries)?;
        let prim_row = PrimRowCore::collect(queries)?;
        let prim_row_list = PrimRowListCore::collect(queries)?;
        let prim_coerce = PrimCoerceCore::collect(queries)?;
        let prim_type_error = PrimTypeErrorCore::collect(queries)?;
        let iris_effect = IrisEffectCore::collect(queries)?;
        let known_types = KnownTypesCore::collect(queries)?;
        let known_terms = KnownTermsCore::collect(queries)?;
        let known_reflectable = KnownReflectableCore::collect(queries)?;
        let known_generic = KnownGeneric::collect(queries)?;

        let prim_id = queries.prim_id();
        let prim_indexed = queries.indexed(prim_id)?;
        let prim_resolved = queries.resolved(prim_id)?;

        Ok(CheckedCore {
            prim,
            prim_int,
            prim_boolean,
            prim_ordering,
            prim_symbol,
            prim_effect,
            prim_row,
            prim_row_list,
            prim_coerce,
            prim_type_error,
            iris_effect,
            known_types,
            known_terms,
            known_reflectable,
            known_generic,
            prim_indexed,
            prim_resolved,
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct IrisEffectCore {
    pub sync: TypeId,
    pub asynchronous: TypeId,
}

impl IrisEffectCore {
    fn collect(queries: &impl ExternalQueries) -> QueryResult<IrisEffectCore> {
        let file_id = queries
            .module_file("Iris.Effect")
            .unwrap_or_else(|| unreachable!("invariant violated: Iris.Effect not found"));

        let resolved = queries.resolved(file_id)?;
        let lookup = PrimLookup::new(&resolved, queries, "Iris.Effect");

        Ok(IrisEffectCore {
            sync: lookup.type_constructor("Sync"),
            asynchronous: lookup.type_constructor("Async"),
        })
    }
}

impl<'q, Q> CheckContext<'q, Q>
where
    Q: ExternalQueries,
{
    /// Creates an [`Type::Unknown`] type with a descriptive label.
    pub fn unknown(&self, label: &str) -> TypeId {
        let label = self.queries.intern_smol_str(SmolStr::new(label));
        self.queries.intern_type(Type::Unknown(label))
    }

    /// Interns a [`Type::Application`] node.
    pub fn intern_application(&self, function: TypeId, argument: TypeId) -> TypeId {
        if let Type::Application(constructor, member) = *self.lookup_type(function)
            && constructor == self.prim.effect_cons
        {
            return self.intern_effect_set_application(member, argument);
        }
        self.queries.intern_type(Type::Application(function, argument))
    }

    fn intern_effect_set_application(&self, member: TypeId, mut tail: TypeId) -> TypeId {
        let mut members = vec![member];

        while let Type::Application(function, next_tail) = *self.lookup_type(tail) {
            let Type::Application(constructor, next_member) = *self.lookup_type(function) else {
                break;
            };
            if constructor != self.prim.effect_cons {
                break;
            }
            members.push(next_member);
            tail = next_tail;
        }

        members.sort_unstable();
        members.dedup();
        members.into_iter().rev().fold(tail, |effects, effect| {
            let constructor =
                self.queries.intern_type(Type::Application(self.prim.effect_cons, effect));
            self.queries.intern_type(Type::Application(constructor, effects))
        })
    }

    /// Interns a [`Type::KindApplication`] node.
    pub fn intern_kind_application(&self, function: TypeId, argument: TypeId) -> TypeId {
        self.queries.intern_type(Type::KindApplication(function, argument))
    }

    /// Interns a [`Type::Forall`] node.
    pub fn intern_forall(&self, binder_id: ForallBinderId, inner: TypeId) -> TypeId {
        self.queries.intern_type(Type::Forall(binder_id, inner))
    }

    /// Interns a [`Type::Constrained`] node.
    pub fn intern_constrained(&self, constraint: TypeId, inner: TypeId) -> TypeId {
        self.queries.intern_type(Type::Constrained(constraint, inner))
    }

    /// Interns a [`Type::Function`] node.
    pub fn intern_function(&self, argument: TypeId, result: TypeId) -> TypeId {
        self.queries.intern_type(Type::Function(argument, result))
    }

    /// Interns a [`Type::Function`] given a list of arguments.
    pub fn intern_function_list(&self, arguments: &[TypeId], result: TypeId) -> TypeId {
        let arguments = arguments.iter().copied();
        self.intern_function_iter(arguments, result)
    }

    /// Interns a list of [`Type::Constrained`] over a type.
    pub fn intern_constrained_list(&self, constraints: &[TypeId], constrained: TypeId) -> TypeId {
        constraints.iter().rev().fold(constrained, |constrained, &constraint| {
            self.intern_constrained(constraint, constrained)
        })
    }

    /// Interns a list of [`Type::Forall`] over a type.
    pub fn intern_forall_iter<I>(&self, binders: I, inner: TypeId) -> TypeId
    where
        I: IntoIterator<Item = ForallBinder>,
        I::IntoIter: DoubleEndedIterator,
    {
        binders.into_iter().rev().fold(inner, |inner, binder| {
            let binder_id = self.intern_forall_binder(binder);
            self.intern_forall(binder_id, inner)
        })
    }

    /// Interns a right-associated function chain from iterator arguments to result.
    pub fn intern_function_iter<I>(&self, arguments: I, result: TypeId) -> TypeId
    where
        I: IntoIterator<Item = TypeId>,
        I::IntoIter: DoubleEndedIterator,
    {
        arguments
            .into_iter()
            .rfold(result, |result, argument| self.intern_function(argument, result))
    }

    /// Interns a [`Type::Kinded`] node.
    pub fn intern_kinded(&self, inner: TypeId, kind: TypeId) -> TypeId {
        self.queries.intern_type(Type::Kinded(inner, kind))
    }

    /// Interns a [`Type::Row`] node for an already-interned row.
    pub fn intern_row_id(&self, row_id: RowTypeId) -> TypeId {
        self.queries.intern_type(Type::Row(row_id))
    }

    /// Materializes a row-kind [`TypeId`] from fields and an optional tail.
    pub fn intern_row(
        &self,
        fields: impl IntoIterator<Item = RowField>,
        tail: Option<TypeId>,
    ) -> TypeId {
        let mut fields = fields.into_iter().collect_vec();
        fields.sort_by(|a, b| a.label.cmp(&b.label));

        if fields.is_empty()
            && let Some(tail) = tail
        {
            return tail;
        }

        let fields = Arc::from(fields);
        let row = match tail {
            Some(tail) => RowType::from_open(fields, tail),
            None => RowType::from_closed(fields),
        };

        let row_id = self.queries.intern_row_type(row);
        self.intern_row_id(row_id)
    }

    /// Interns a [`Type::Rigid`] node.
    pub fn intern_rigid(&self, name: Name, depth: Depth, kind: TypeId) -> TypeId {
        self.queries.intern_type(Type::Rigid(name, depth, kind))
    }

    /// Interns a [`Type::Application`]-based function.
    ///
    /// The types `Function a b` and `a -> b` are equivalent, represented by
    /// [`Type::Application`] and [`Type::Function`] respectively. Normalising
    /// into the application-based form is generally more useful, such as in
    /// the following example:
    ///
    /// ```text
    /// unify(?function_a b, a -> b)
    ///   unify(?function_a b, Function a b) = [ ?function_a := Function a ]
    /// ```
    pub fn intern_function_application(&self, argument: TypeId, result: TypeId) -> TypeId {
        let function_argument = self.intern_application(self.prim.function, argument);
        self.intern_application(function_argument, result)
    }

    /// Looks up the [`Type`] for the given [`TypeId`].
    pub fn lookup_type(&self, id: TypeId) -> &'q Type {
        self.queries.lookup_type(id)
    }

    pub fn lookup_type_flags(&self, id: TypeId) -> TypeFlags {
        self.queries.lookup_type_flags(id)
    }

    /// Looks up the [`ForallBinder`] for the given [`ForallBinderId`].
    pub fn lookup_forall_binder(&self, id: ForallBinderId) -> ForallBinder {
        self.queries.lookup_forall_binder(id)
    }

    /// Looks up the [`RowType`] for the given [`RowTypeId`].
    pub fn lookup_row_type(&self, id: RowTypeId) -> &'q RowType {
        self.queries.lookup_row_type(id)
    }

    /// Interns a [`ForallBinder`], returning its [`ForallBinderId`].
    pub fn intern_forall_binder(&self, binder: ForallBinder) -> ForallBinderId {
        self.queries.intern_forall_binder(binder)
    }

    /// Interns a [`RowType`], returning its [`RowTypeId`].
    pub fn intern_row_type(&self, row: RowType) -> RowTypeId {
        self.queries.intern_row_type(row)
    }
}

struct PrimLookup<'r, 'q, Q>
where
    Q: ExternalQueries,
{
    resolved: &'r ResolvedModule,
    queries: &'q Q,
    module_name: &'static str,
}

impl<'r, 'q, Q: ExternalQueries> PrimLookup<'r, 'q, Q> {
    fn new(resolved: &'r ResolvedModule, queries: &'q Q, module_name: &'static str) -> Self {
        PrimLookup { resolved, queries, module_name }
    }

    fn type_item(&self, name: &str) -> TypeItemId {
        let (_, type_id) = self.resolved.exports.lookup_type(name).unwrap_or_else(|| {
            unreachable!("invariant violated: {name} not in {}", self.module_name)
        });
        type_id
    }

    fn type_constructor(&self, name: &str) -> TypeId {
        let (file_id, type_id) = self.resolved.exports.lookup_type(name).unwrap_or_else(|| {
            unreachable!("invariant violated: {name} not in {}", self.module_name)
        });
        self.queries.intern_type(Type::Constructor(file_id, type_id))
    }

    fn class_item(&self, name: &str) -> TypeItemId {
        let (_, type_id) = self.resolved.exports.lookup_class(name).unwrap_or_else(|| {
            unreachable!("invariant violated: {name} not in {}", self.module_name)
        });
        type_id
    }

    fn class_constructor(&self, name: &str) -> TypeId {
        let (file_id, type_id) = self.resolved.exports.lookup_class(name).unwrap_or_else(|| {
            unreachable!("invariant violated: {name} not in {}", self.module_name)
        });
        self.queries.intern_type(Type::Constructor(file_id, type_id))
    }

    fn intern(&self, ty: Type) -> TypeId {
        self.queries.intern_type(ty)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct PrimCore {
    pub prim_id: FileId,
    pub t: TypeId,
    pub type_to_type: TypeId,
    pub function: TypeId,
    pub function_item: TypeItemId,
    pub array: TypeId,
    pub record: TypeId,
    pub number: TypeId,
    pub int: TypeId,
    pub string: TypeId,
    pub char: TypeId,
    pub boolean: TypeId,
    pub partial: TypeId,
    pub constraint: TypeId,
    pub symbol: TypeId,
    pub row: TypeId,
    pub row_type: TypeId,
    pub effects: TypeId,
    pub effect_nil: TypeId,
    pub effect_cons: TypeId,
}

impl PrimCore {
    fn collect(queries: &impl ExternalQueries) -> QueryResult<PrimCore> {
        let prim_id = queries.prim_id();
        let resolved = queries.resolved(prim_id)?;
        let lookup = PrimLookup::new(&resolved, queries, "Prim");

        let t = lookup.type_constructor("Type");
        let type_to_type = lookup.intern(Type::Function(t, t));

        let row = lookup.type_constructor("Row");
        let row_type = lookup.intern(Type::Application(row, t));

        let function = lookup.type_constructor("Function");
        let function_item = lookup.type_item("Function");

        Ok(PrimCore {
            prim_id,
            t,
            type_to_type,
            function,
            function_item,
            array: lookup.type_constructor("Array"),
            record: lookup.type_constructor("Record"),
            number: lookup.type_constructor("Number"),
            int: lookup.type_constructor("Int"),
            string: lookup.type_constructor("String"),
            char: lookup.type_constructor("Char"),
            boolean: lookup.type_constructor("Boolean"),
            partial: lookup.class_constructor("Partial"),
            constraint: lookup.type_constructor("Constraint"),
            symbol: lookup.type_constructor("Symbol"),
            row,
            row_type,
            effects: lookup.type_constructor("Effects"),
            effect_nil: lookup.type_constructor("EffectNil"),
            effect_cons: lookup.type_constructor("EffectCons"),
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct PrimEffectCore {
    pub file_id: FileId,
    pub abort: TypeItemId,
    pub abort_identity: TypeItemId,
    pub union: TypeItemId,
    pub remove: TypeItemId,
    pub subset: TypeItemId,
    pub runnable: TypeItemId,
}

impl PrimEffectCore {
    fn collect(queries: &impl ExternalQueries) -> QueryResult<PrimEffectCore> {
        let file_id = queries
            .module_file("Prim.Effect")
            .unwrap_or_else(|| unreachable!("invariant violated: Prim.Effect not found"));

        let resolved = queries.resolved(file_id)?;
        let lookup = PrimLookup::new(&resolved, queries, "Prim.Effect");

        Ok(PrimEffectCore {
            file_id,
            abort: lookup.type_item("Abort"),
            abort_identity: lookup.class_item("AbortIdentity"),
            union: lookup.class_item("Union"),
            remove: lookup.class_item("Remove"),
            subset: lookup.class_item("Subset"),
            runnable: lookup.class_item("Runnable"),
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct PrimIntCore {
    pub file_id: FileId,
    pub add: TypeItemId,
    pub mul: TypeItemId,
    pub compare: TypeItemId,
    pub to_string: TypeItemId,
}

impl PrimIntCore {
    fn collect(queries: &impl ExternalQueries) -> QueryResult<PrimIntCore> {
        let file_id = queries
            .module_file("Prim.Int")
            .unwrap_or_else(|| unreachable!("invariant violated: Prim.Int not found"));

        let resolved = queries.resolved(file_id)?;
        let lookup = PrimLookup::new(&resolved, queries, "Prim.Int");

        Ok(PrimIntCore {
            file_id,
            add: lookup.class_item("Add"),
            mul: lookup.class_item("Mul"),
            compare: lookup.class_item("Compare"),
            to_string: lookup.class_item("ToString"),
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct PrimBooleanCore {
    pub true_: TypeId,
    pub false_: TypeId,
}

impl PrimBooleanCore {
    fn collect(queries: &impl ExternalQueries) -> QueryResult<PrimBooleanCore> {
        let file_id = queries
            .module_file("Prim.Boolean")
            .unwrap_or_else(|| unreachable!("invariant violated: Prim.Boolean not found"));

        let resolved = queries.resolved(file_id)?;
        let lookup = PrimLookup::new(&resolved, queries, "Prim.Boolean");

        Ok(PrimBooleanCore {
            true_: lookup.type_constructor("True"),
            false_: lookup.type_constructor("False"),
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct PrimOrderingCore {
    pub lt: TypeId,
    pub eq: TypeId,
    pub gt: TypeId,
}

impl PrimOrderingCore {
    fn collect(queries: &impl ExternalQueries) -> QueryResult<PrimOrderingCore> {
        let file_id = queries
            .module_file("Prim.Ordering")
            .unwrap_or_else(|| unreachable!("invariant violated: Prim.Ordering not found"));

        let resolved = queries.resolved(file_id)?;
        let lookup = PrimLookup::new(&resolved, queries, "Prim.Ordering");

        Ok(PrimOrderingCore {
            lt: lookup.type_constructor("LT"),
            eq: lookup.type_constructor("EQ"),
            gt: lookup.type_constructor("GT"),
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct PrimSymbolCore {
    pub file_id: FileId,
    pub append: TypeItemId,
    pub compare: TypeItemId,
    pub cons: TypeItemId,
}

impl PrimSymbolCore {
    fn collect(queries: &impl ExternalQueries) -> QueryResult<PrimSymbolCore> {
        let file_id = queries
            .module_file("Prim.Symbol")
            .unwrap_or_else(|| unreachable!("invariant violated: Prim.Symbol not found"));

        let resolved = queries.resolved(file_id)?;
        let lookup = PrimLookup::new(&resolved, queries, "Prim.Symbol");

        Ok(PrimSymbolCore {
            file_id,
            append: lookup.class_item("Append"),
            compare: lookup.class_item("Compare"),
            cons: lookup.class_item("Cons"),
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct PrimRowCore {
    pub file_id: FileId,
    pub union: TypeItemId,
    pub cons: TypeItemId,
    pub lacks: TypeItemId,
    pub nub: TypeItemId,
}

impl PrimRowCore {
    fn collect(queries: &impl ExternalQueries) -> QueryResult<PrimRowCore> {
        let file_id = queries
            .module_file("Prim.Row")
            .unwrap_or_else(|| unreachable!("invariant violated: Prim.Row not found"));

        let resolved = queries.resolved(file_id)?;
        let lookup = PrimLookup::new(&resolved, queries, "Prim.Row");

        Ok(PrimRowCore {
            file_id,
            union: lookup.class_item("Union"),
            cons: lookup.class_item("Cons"),
            lacks: lookup.class_item("Lacks"),
            nub: lookup.class_item("Nub"),
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct PrimRowListCore {
    pub file_id: FileId,
    pub row_to_list: TypeItemId,
    pub cons: TypeId,
    pub nil: TypeId,
}

impl PrimRowListCore {
    fn collect(queries: &impl ExternalQueries) -> QueryResult<PrimRowListCore> {
        let file_id = queries
            .module_file("Prim.RowList")
            .unwrap_or_else(|| unreachable!("invariant violated: Prim.RowList not found"));

        let resolved = queries.resolved(file_id)?;
        let lookup = PrimLookup::new(&resolved, queries, "Prim.RowList");

        Ok(PrimRowListCore {
            file_id,
            row_to_list: lookup.class_item("RowToList"),
            cons: lookup.type_constructor("Cons"),
            nil: lookup.type_constructor("Nil"),
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct PrimCoerceCore {
    pub file_id: FileId,
    pub coercible: TypeItemId,
}

impl PrimCoerceCore {
    fn collect(queries: &impl ExternalQueries) -> QueryResult<PrimCoerceCore> {
        let file_id = queries
            .module_file("Prim.Coerce")
            .unwrap_or_else(|| unreachable!("invariant violated: Prim.Coerce not found"));

        let resolved = queries.resolved(file_id)?;
        let (_, coercible) = resolved
            .exports
            .lookup_class("Coercible")
            .unwrap_or_else(|| unreachable!("invariant violated: Coercible not in Prim.Coerce"));

        Ok(PrimCoerceCore { file_id, coercible })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct PrimTypeErrorCore {
    pub file_id: FileId,
    pub warn: TypeItemId,
    pub fail: TypeItemId,
    pub text: TypeId,
    pub quote: TypeId,
    pub quote_label: TypeId,
    pub beside: TypeId,
    pub above: TypeId,
}

impl PrimTypeErrorCore {
    fn collect(queries: &impl ExternalQueries) -> QueryResult<PrimTypeErrorCore> {
        let file_id = queries
            .module_file("Prim.TypeError")
            .unwrap_or_else(|| unreachable!("invariant violated: Prim.TypeError not found"));

        let resolved = queries.resolved(file_id)?;
        let lookup = PrimLookup::new(&resolved, queries, "Prim.TypeError");

        Ok(PrimTypeErrorCore {
            file_id,
            warn: lookup.class_item("Warn"),
            fail: lookup.class_item("Fail"),
            text: lookup.type_constructor("Text"),
            quote: lookup.type_constructor("Quote"),
            quote_label: lookup.type_constructor("QuoteLabel"),
            beside: lookup.type_constructor("Beside"),
            above: lookup.type_constructor("Above"),
        })
    }
}

fn fetch_known_term(
    queries: &impl ExternalQueries,
    m: &str,
    n: &str,
) -> QueryResult<Option<(FileId, TermItemId)>> {
    let Some(file_id) = queries.module_file(m) else {
        return Ok(None);
    };
    let resolved = queries.resolved(file_id)?;
    let Some((file_id, term_id)) = resolved.exports.lookup_term(n) else {
        return Ok(None);
    };
    Ok(Some((file_id, term_id)))
}

fn fetch_known_class(
    queries: &impl ExternalQueries,
    m: &str,
    n: &str,
) -> QueryResult<Option<(FileId, TypeItemId)>> {
    let Some(file_id) = queries.module_file(m) else {
        return Ok(None);
    };
    let resolved = queries.resolved(file_id)?;
    let Some((file_id, type_id)) = resolved.exports.lookup_class(n) else {
        return Ok(None);
    };
    Ok(Some((file_id, type_id)))
}

fn fetch_known_constructor(
    queries: &impl ExternalQueries,
    m: &str,
    n: &str,
) -> QueryResult<Option<TypeId>> {
    let Some(file_id) = queries.module_file(m) else {
        return Ok(None);
    };
    let resolved = queries.resolved(file_id)?;
    let Some((file_id, type_id)) = resolved.exports.lookup_type(n) else {
        return Ok(None);
    };
    Ok(Some(queries.intern_type(Type::Constructor(file_id, type_id))))
}

#[derive(Debug, PartialEq, Eq)]
pub struct KnownTypesCore {
    pub eq: Option<(FileId, TypeItemId)>,
    pub eq1: Option<(FileId, TypeItemId)>,
    pub ord: Option<(FileId, TypeItemId)>,
    pub ord1: Option<(FileId, TypeItemId)>,
    pub functor: Option<(FileId, TypeItemId)>,
    pub bifunctor: Option<(FileId, TypeItemId)>,
    pub contravariant: Option<(FileId, TypeItemId)>,
    pub profunctor: Option<(FileId, TypeItemId)>,
    pub foldable: Option<(FileId, TypeItemId)>,
    pub bifoldable: Option<(FileId, TypeItemId)>,
    pub traversable: Option<(FileId, TypeItemId)>,
    pub bitraversable: Option<(FileId, TypeItemId)>,
    pub newtype: Option<(FileId, TypeItemId)>,
    pub generic: Option<(FileId, TypeItemId)>,
}

impl KnownTypesCore {
    fn collect(queries: &impl ExternalQueries) -> QueryResult<KnownTypesCore> {
        let eq = fetch_known_class(queries, "Data.Eq", "Eq")?;
        let eq1 = fetch_known_class(queries, "Data.Eq", "Eq1")?;
        let ord = fetch_known_class(queries, "Data.Ord", "Ord")?;
        let ord1 = fetch_known_class(queries, "Data.Ord", "Ord1")?;
        let functor = fetch_known_class(queries, "Data.Functor", "Functor")?;
        let bifunctor = fetch_known_class(queries, "Data.Bifunctor", "Bifunctor")?;
        let contravariant =
            fetch_known_class(queries, "Data.Functor.Contravariant", "Contravariant")?;
        let profunctor = fetch_known_class(queries, "Data.Profunctor", "Profunctor")?;
        let foldable = fetch_known_class(queries, "Data.Foldable", "Foldable")?;
        let bifoldable = fetch_known_class(queries, "Data.Bifoldable", "Bifoldable")?;
        let traversable = fetch_known_class(queries, "Data.Traversable", "Traversable")?;
        let bitraversable = fetch_known_class(queries, "Data.Bitraversable", "Bitraversable")?;
        let newtype = fetch_known_class(queries, "Data.Newtype", "Newtype")?;
        let generic = fetch_known_class(queries, "Data.Generic.Rep", "Generic")?;
        Ok(KnownTypesCore {
            eq,
            eq1,
            ord,
            ord1,
            functor,
            bifunctor,
            contravariant,
            profunctor,
            foldable,
            bifoldable,
            traversable,
            bitraversable,
            newtype,
            generic,
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct KnownReflectableCore {
    pub is_symbol: Option<(FileId, TypeItemId)>,
    pub reflectable: Option<(FileId, TypeItemId)>,
    pub ordering: Option<TypeId>,
}

impl KnownReflectableCore {
    fn collect(queries: &impl ExternalQueries) -> QueryResult<KnownReflectableCore> {
        let is_symbol = fetch_known_class(queries, "Data.Symbol", "IsSymbol")?;
        let reflectable = fetch_known_class(queries, "Data.Reflectable", "Reflectable")?;
        let ordering = fetch_known_constructor(queries, "Data.Ordering", "Ordering")?;
        Ok(KnownReflectableCore { is_symbol, reflectable, ordering })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct KnownGeneric {
    pub no_constructors: TypeId,
    pub constructor: TypeId,
    pub sum: TypeId,
    pub product: TypeId,
    pub no_arguments: TypeId,
    pub argument: TypeId,
    pub constructor_value: (FileId, TermItemId),
    pub in_left: (FileId, TermItemId),
    pub in_right: (FileId, TermItemId),
    pub product_value: (FileId, TermItemId),
    pub no_arguments_value: (FileId, TermItemId),
    pub argument_value: (FileId, TermItemId),
    pub to: (FileId, TermItemId),
    pub from: (FileId, TermItemId),
}

impl KnownGeneric {
    fn collect(queries: &impl ExternalQueries) -> QueryResult<Option<KnownGeneric>> {
        let Some(no_constructors) =
            fetch_known_constructor(queries, "Data.Generic.Rep", "NoConstructors")?
        else {
            return Ok(None);
        };
        let Some(constructor) =
            fetch_known_constructor(queries, "Data.Generic.Rep", "Constructor")?
        else {
            return Ok(None);
        };
        let Some(sum) = fetch_known_constructor(queries, "Data.Generic.Rep", "Sum")? else {
            return Ok(None);
        };
        let Some(product) = fetch_known_constructor(queries, "Data.Generic.Rep", "Product")? else {
            return Ok(None);
        };
        let Some(no_arguments) =
            fetch_known_constructor(queries, "Data.Generic.Rep", "NoArguments")?
        else {
            return Ok(None);
        };
        let Some(argument) = fetch_known_constructor(queries, "Data.Generic.Rep", "Argument")?
        else {
            return Ok(None);
        };
        let Some(constructor_value) = fetch_known_term(queries, "Data.Generic.Rep", "Constructor")?
        else {
            return Ok(None);
        };
        let Some(in_left) = fetch_known_term(queries, "Data.Generic.Rep", "Inl")? else {
            return Ok(None);
        };
        let Some(in_right) = fetch_known_term(queries, "Data.Generic.Rep", "Inr")? else {
            return Ok(None);
        };
        let Some(product_value) = fetch_known_term(queries, "Data.Generic.Rep", "Product")? else {
            return Ok(None);
        };
        let Some(no_arguments_value) =
            fetch_known_term(queries, "Data.Generic.Rep", "NoArguments")?
        else {
            return Ok(None);
        };
        let Some(argument_value) = fetch_known_term(queries, "Data.Generic.Rep", "Argument")?
        else {
            return Ok(None);
        };
        let Some(to) = fetch_known_term(queries, "Data.Generic.Rep", "to")? else {
            return Ok(None);
        };
        let Some(from) = fetch_known_term(queries, "Data.Generic.Rep", "from")? else {
            return Ok(None);
        };
        Ok(Some(KnownGeneric {
            no_constructors,
            constructor,
            sum,
            product,
            no_arguments,
            argument,
            constructor_value,
            in_left,
            in_right,
            product_value,
            no_arguments_value,
            argument_value,
            to,
            from,
        }))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct KnownTermsCore {
    pub otherwise: Option<(FileId, TermItemId)>,
    pub map: Option<(FileId, TermItemId)>,
    pub bimap: Option<(FileId, TermItemId)>,
    pub cmap: Option<(FileId, TermItemId)>,
    pub dimap: Option<(FileId, TermItemId)>,
    pub foldr: Option<(FileId, TermItemId)>,
    pub foldl: Option<(FileId, TermItemId)>,
    pub fold_map: Option<(FileId, TermItemId)>,
    pub bifoldr: Option<(FileId, TermItemId)>,
    pub bifoldl: Option<(FileId, TermItemId)>,
    pub bifold_map: Option<(FileId, TermItemId)>,
    pub append: Option<(FileId, TermItemId)>,
    pub mempty: Option<(FileId, TermItemId)>,
    pub pure: Option<(FileId, TermItemId)>,
    pub apply: Option<(FileId, TermItemId)>,
    pub traverse: Option<(FileId, TermItemId)>,
    pub sequence: Option<(FileId, TermItemId)>,
    pub bitraverse: Option<(FileId, TermItemId)>,
    pub bisequence: Option<(FileId, TermItemId)>,
    pub eq: Option<(FileId, TermItemId)>,
    pub eq1: Option<(FileId, TermItemId)>,
    pub compare: Option<(FileId, TermItemId)>,
    pub compare1: Option<(FileId, TermItemId)>,
    pub ordering_lt: Option<(FileId, TermItemId)>,
    pub ordering_eq: Option<(FileId, TermItemId)>,
    pub ordering_gt: Option<(FileId, TermItemId)>,
}

impl KnownTermsCore {
    fn collect(queries: &impl ExternalQueries) -> QueryResult<KnownTermsCore> {
        let otherwise = fetch_known_term(queries, "Data.Boolean", "otherwise")?;
        let map = fetch_known_term(queries, "Data.Functor", "map")?;
        let bimap = fetch_known_term(queries, "Data.Bifunctor", "bimap")?;
        let cmap = fetch_known_term(queries, "Data.Functor.Contravariant", "cmap")?;
        let dimap = fetch_known_term(queries, "Data.Profunctor", "dimap")?;
        let foldr = fetch_known_term(queries, "Data.Foldable", "foldr")?;
        let foldl = fetch_known_term(queries, "Data.Foldable", "foldl")?;
        let fold_map = fetch_known_term(queries, "Data.Foldable", "foldMap")?;
        let bifoldr = fetch_known_term(queries, "Data.Bifoldable", "bifoldr")?;
        let bifoldl = fetch_known_term(queries, "Data.Bifoldable", "bifoldl")?;
        let bifold_map = fetch_known_term(queries, "Data.Bifoldable", "bifoldMap")?;
        let append = fetch_known_term(queries, "Data.Semigroup", "append")?;
        let mempty = fetch_known_term(queries, "Data.Monoid", "mempty")?;
        let pure = fetch_known_term(queries, "Control.Applicative", "pure")?;
        let apply = fetch_known_term(queries, "Control.Apply", "apply")?;
        let traverse = fetch_known_term(queries, "Data.Traversable", "traverse")?;
        let sequence = fetch_known_term(queries, "Data.Traversable", "sequence")?;
        let bitraverse = fetch_known_term(queries, "Data.Bitraversable", "bitraverse")?;
        let bisequence = fetch_known_term(queries, "Data.Bitraversable", "bisequence")?;
        let eq = fetch_known_term(queries, "Data.Eq", "eq")?;
        let eq1 = fetch_known_term(queries, "Data.Eq", "eq1")?;
        let compare = fetch_known_term(queries, "Data.Ord", "compare")?;
        let compare1 = fetch_known_term(queries, "Data.Ord", "compare1")?;
        let ordering_lt = fetch_known_term(queries, "Data.Ordering", "LT")?;
        let ordering_eq = fetch_known_term(queries, "Data.Ordering", "EQ")?;
        let ordering_gt = fetch_known_term(queries, "Data.Ordering", "GT")?;
        Ok(KnownTermsCore {
            otherwise,
            map,
            bimap,
            cmap,
            dimap,
            foldr,
            foldl,
            fold_map,
            bifoldr,
            bifoldl,
            bifold_map,
            append,
            mempty,
            pure,
            apply,
            traverse,
            sequence,
            bitraverse,
            bisequence,
            eq,
            eq1,
            compare,
            compare1,
            ordering_lt,
            ordering_eq,
            ordering_gt,
        })
    }
}
