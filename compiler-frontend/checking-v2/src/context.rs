use std::cell::RefCell;
use std::sync::Arc;

use building_types::QueryResult;
use files::FileId;
use indexing::{IndexedModule, TermItemId, TypeItemId};
use lowering::{GroupedModule, LoweredModule};
use resolving::ResolvedModule;
use rustc_hash::FxHashMap;
use smol_str::SmolStr;

use crate::types::{
    ForallBinder, ForallBinderId, Rigid, RigidId, RowField, RowType, RowTypeId, SynonymApplication,
    SynonymApplicationId, TextId, Type, TypeFlags, TypeId,
};
use crate::{CheckedModule, ExternalQueries};

pub struct CheckContext<'q, Q>
where
    Q: ExternalQueries,
{
    pub queries: &'q Q,
    pub id: FileId,
    pub indexed: Arc<IndexedModule>,
    pub lowered: Arc<LoweredModule>,
    pub grouped: Arc<GroupedModule>,
    pub resolved: Arc<ResolvedModule>,
    pub bracketed: Arc<sugar::Bracketed>,
    pub sectioned: Arc<sugar::Sectioned>,
    pub known: KnownItems,
    pub types: KnownTypes,
    dependencies: RefCell<FxHashMap<FileId, Arc<CheckedModule>>>,
    lowered_dependencies: RefCell<FxHashMap<FileId, Arc<LoweredModule>>>,
}

pub type Item = (FileId, TypeItemId);
pub type TermItem = (FileId, TermItemId);

#[derive(Debug, Clone, Copy)]
pub struct KnownItems {
    pub type_: Item,
    pub function: Item,
    pub array: Item,
    pub record: Item,
    pub number: Item,
    pub int: Item,
    pub string: Item,
    pub char: Item,
    pub boolean: Item,
    pub constraint: Item,
    pub symbol: Item,
    pub row: Item,
    pub partial: Item,
    pub row_union: Option<Item>,
    pub row_nub: Option<Item>,
    pub row_lacks: Option<Item>,
    pub row_cons: Option<Item>,
    pub row_list: Option<Item>,
    pub row_list_cons: Option<Item>,
    pub row_list_nil: Option<Item>,
    pub row_to_list: Option<Item>,
    pub symbol_append: Option<Item>,
    pub symbol_compare: Option<Item>,
    pub symbol_cons: Option<Item>,
    pub int_add: Option<Item>,
    pub int_compare: Option<Item>,
    pub int_mul: Option<Item>,
    pub int_to_string: Option<Item>,
    pub ordering: Option<Item>,
    pub ordering_lt: Option<Item>,
    pub ordering_eq: Option<Item>,
    pub ordering_gt: Option<Item>,
    pub boolean_true: Option<Item>,
    pub boolean_false: Option<Item>,
    pub coercible: Option<Item>,
    pub type_error_warn: Option<Item>,
    pub type_error_fail: Option<Item>,
    pub type_error_text: Option<Item>,
    pub type_error_quote: Option<Item>,
    pub type_error_quote_label: Option<Item>,
    pub type_error_beside: Option<Item>,
    pub type_error_above: Option<Item>,
    pub is_symbol: Option<Item>,
    pub reflectable: Option<Item>,
    pub eq: Option<Item>,
    pub ord: Option<Item>,
    pub functor: Option<Item>,
    pub newtype: Option<Item>,
}

#[derive(Debug, Clone, Copy)]
pub struct KnownTypes {
    pub type_: TypeId,
    pub function: TypeId,
    pub array: TypeId,
    pub record: TypeId,
    pub number: TypeId,
    pub int: TypeId,
    pub string: TypeId,
    pub char: TypeId,
    pub boolean: TypeId,
    pub constraint: TypeId,
    pub symbol: TypeId,
    pub row: TypeId,
    pub row_type: TypeId,
    pub unknown: TypeId,
}

struct ModuleLookup {
    resolved: Option<Arc<ResolvedModule>>,
}

impl ModuleLookup {
    fn new(queries: &impl ExternalQueries, name: &str) -> QueryResult<ModuleLookup> {
        let resolved = match queries.module_file(name) {
            Some(file_id) => Some(queries.resolved(file_id)?),
            None => None,
        };
        Ok(ModuleLookup { resolved })
    }

    fn type_item(&self, name: &str) -> Option<Item> {
        let resolved = self.resolved.as_ref()?;
        resolved.exports.lookup_type(name)
    }

    fn class_item(&self, name: &str) -> Option<Item> {
        let resolved = self.resolved.as_ref()?;
        resolved.exports.lookup_class(name)
    }
}

impl KnownItems {
    fn collect(queries: &impl ExternalQueries) -> QueryResult<KnownItems> {
        let prim = ModuleLookup::new(queries, "Prim")?;
        let prim_row = ModuleLookup::new(queries, "Prim.Row")?;
        let prim_row_list = ModuleLookup::new(queries, "Prim.RowList")?;
        let prim_symbol = ModuleLookup::new(queries, "Prim.Symbol")?;
        let prim_int = ModuleLookup::new(queries, "Prim.Int")?;
        let prim_ordering = ModuleLookup::new(queries, "Prim.Ordering")?;
        let prim_boolean = ModuleLookup::new(queries, "Prim.Boolean")?;
        let prim_coerce = ModuleLookup::new(queries, "Prim.Coerce")?;
        let prim_type_error = ModuleLookup::new(queries, "Prim.TypeError")?;
        let data_symbol = ModuleLookup::new(queries, "Data.Symbol")?;
        let data_reflectable = ModuleLookup::new(queries, "Data.Reflectable")?;
        let data_eq = ModuleLookup::new(queries, "Data.Eq")?;
        let data_ord = ModuleLookup::new(queries, "Data.Ord")?;
        let data_functor = ModuleLookup::new(queries, "Data.Functor")?;
        let data_newtype = ModuleLookup::new(queries, "Data.Newtype")?;

        let required = |name: &str| {
            let item = prim.type_item(name);
            item.unwrap_or_else(|| unreachable!("invariant violated: {name} not in Prim"))
        };

        let partial = prim.class_item("Partial");
        let partial = partial.unwrap_or_else(|| unreachable!("invariant violated: no Partial"));

        Ok(KnownItems {
            type_: required("Type"),
            function: required("Function"),
            array: required("Array"),
            record: required("Record"),
            number: required("Number"),
            int: required("Int"),
            string: required("String"),
            char: required("Char"),
            boolean: required("Boolean"),
            constraint: required("Constraint"),
            symbol: required("Symbol"),
            row: required("Row"),
            partial,
            row_union: prim_row.class_item("Union"),
            row_nub: prim_row.class_item("Nub"),
            row_lacks: prim_row.class_item("Lacks"),
            row_cons: prim_row.class_item("Cons"),
            row_list: prim_row_list.type_item("RowList"),
            row_list_cons: prim_row_list.type_item("Cons"),
            row_list_nil: prim_row_list.type_item("Nil"),
            row_to_list: prim_row_list.class_item("RowToList"),
            symbol_append: prim_symbol.class_item("Append"),
            symbol_compare: prim_symbol.class_item("Compare"),
            symbol_cons: prim_symbol.class_item("Cons"),
            int_add: prim_int.class_item("Add"),
            int_compare: prim_int.class_item("Compare"),
            int_mul: prim_int.class_item("Mul"),
            int_to_string: prim_int.class_item("ToString"),
            ordering: prim_ordering.type_item("Ordering"),
            ordering_lt: prim_ordering.type_item("LT"),
            ordering_eq: prim_ordering.type_item("EQ"),
            ordering_gt: prim_ordering.type_item("GT"),
            boolean_true: prim_boolean.type_item("True"),
            boolean_false: prim_boolean.type_item("False"),
            coercible: prim_coerce.class_item("Coercible"),
            type_error_warn: prim_type_error.class_item("Warn"),
            type_error_fail: prim_type_error.class_item("Fail"),
            type_error_text: prim_type_error.type_item("Text"),
            type_error_quote: prim_type_error.type_item("Quote"),
            type_error_quote_label: prim_type_error.type_item("QuoteLabel"),
            type_error_beside: prim_type_error.type_item("Beside"),
            type_error_above: prim_type_error.type_item("Above"),
            is_symbol: data_symbol.class_item("IsSymbol"),
            reflectable: data_reflectable.class_item("Reflectable"),
            eq: data_eq.class_item("Eq"),
            ord: data_ord.class_item("Ord"),
            functor: data_functor.class_item("Functor"),
            newtype: data_newtype.class_item("Newtype"),
        })
    }
}

impl KnownTypes {
    fn collect(queries: &impl ExternalQueries, known: &KnownItems) -> KnownTypes {
        let interners = queries.interners();
        let constructor =
            |(file_id, type_id): Item| interners.intern_type(Type::Constructor(file_id, type_id));

        let type_ = constructor(known.type_);
        let row = constructor(known.row);
        let row_type = interners.intern_type(Type::Application(row, type_));

        KnownTypes {
            type_,
            function: constructor(known.function),
            array: constructor(known.array),
            record: constructor(known.record),
            number: constructor(known.number),
            int: constructor(known.int),
            string: constructor(known.string),
            char: constructor(known.char),
            boolean: constructor(known.boolean),
            constraint: constructor(known.constraint),
            symbol: constructor(known.symbol),
            row,
            row_type,
            unknown: interners.intern_type(Type::Unknown),
        }
    }
}

impl<'q, Q> CheckContext<'q, Q>
where
    Q: ExternalQueries,
{
    pub fn new(queries: &'q Q, id: FileId) -> QueryResult<CheckContext<'q, Q>> {
        let indexed = queries.indexed(id)?;
        let lowered = queries.lowered(id)?;
        let grouped = queries.grouped(id)?;
        let resolved = queries.resolved(id)?;
        let bracketed = queries.bracketed(id)?;
        let sectioned = queries.sectioned(id)?;
        let known = KnownItems::collect(queries)?;
        let types = KnownTypes::collect(queries, &known);
        Ok(CheckContext {
            queries,
            id,
            indexed,
            lowered,
            grouped,
            resolved,
            bracketed,
            sectioned,
            known,
            types,
            dependencies: RefCell::default(),
            lowered_dependencies: RefCell::default(),
        })
    }

    pub fn checked_dependency(&self, file_id: FileId) -> QueryResult<Arc<CheckedModule>> {
        if let Some(checked) = self.dependencies.borrow().get(&file_id) {
            return Ok(Arc::clone(checked));
        }
        let checked = self.queries.checked_v2(file_id)?;
        self.dependencies.borrow_mut().insert(file_id, Arc::clone(&checked));
        Ok(checked)
    }

    pub fn lowered_dependency(&self, file_id: FileId) -> QueryResult<Arc<LoweredModule>> {
        if file_id == self.id {
            return Ok(Arc::clone(&self.lowered));
        }
        if let Some(lowered) = self.lowered_dependencies.borrow().get(&file_id) {
            return Ok(Arc::clone(lowered));
        }
        let lowered = self.queries.lowered(file_id)?;
        self.lowered_dependencies.borrow_mut().insert(file_id, Arc::clone(&lowered));
        Ok(lowered)
    }

    pub fn intern_type(&self, t: Type) -> TypeId {
        self.queries.interners().intern_type(t)
    }

    pub fn lookup_type(&self, id: TypeId) -> Type {
        self.queries.interners().lookup_type(id)
    }

    pub fn lookup_type_flags(&self, id: TypeId) -> TypeFlags {
        self.queries.interners().lookup_type_flags(id)
    }

    pub fn intern_forall_binder(&self, binder: ForallBinder) -> ForallBinderId {
        self.queries.interners().intern_forall_binder(binder)
    }

    pub fn lookup_forall_binder(&self, id: ForallBinderId) -> ForallBinder {
        self.queries.interners().lookup_forall_binder(id)
    }

    pub fn intern_rigid(&self, rigid: Rigid) -> RigidId {
        self.queries.interners().intern_rigid(rigid)
    }

    pub fn lookup_rigid(&self, id: RigidId) -> Rigid {
        self.queries.interners().lookup_rigid(id)
    }

    pub fn intern_row_type(&self, row: RowType) -> RowTypeId {
        self.queries.interners().intern_row_type(row)
    }

    pub fn lookup_row_type(&self, id: RowTypeId) -> RowType {
        RowType::clone(self.queries.interners().lookup_row_type(id))
    }

    pub fn intern_synonym_application(
        &self,
        application: SynonymApplication,
    ) -> SynonymApplicationId {
        self.queries.interners().intern_synonym_application(application)
    }

    pub fn lookup_synonym_application(&self, id: SynonymApplicationId) -> SynonymApplication {
        let application = self.queries.interners().lookup_synonym_application(id);
        SynonymApplication::clone(application)
    }

    pub fn intern_text(&self, text: SmolStr) -> TextId {
        self.queries.interners().intern_text(text)
    }

    pub fn lookup_text(&self, id: TextId) -> SmolStr {
        SmolStr::clone(self.queries.interners().lookup_text(id))
    }

    pub fn intern_application(&self, function: TypeId, argument: TypeId) -> TypeId {
        self.intern_type(Type::Application(function, argument))
    }

    pub fn intern_kind_application(&self, function: TypeId, argument: TypeId) -> TypeId {
        self.intern_type(Type::KindApplication(function, argument))
    }

    pub fn intern_function(&self, argument: TypeId, result: TypeId) -> TypeId {
        self.intern_type(Type::Function(argument, result))
    }

    pub fn intern_function_list(&self, arguments: &[TypeId], result: TypeId) -> TypeId {
        let function = |result, &argument| self.intern_function(argument, result);
        arguments.iter().rev().fold(result, function)
    }

    pub fn intern_forall(&self, binder_id: ForallBinderId, inner: TypeId) -> TypeId {
        self.intern_type(Type::Forall(binder_id, inner))
    }

    pub fn intern_forall_list(&self, binders: &[ForallBinderId], inner: TypeId) -> TypeId {
        let forall = |inner, &binder_id| self.intern_forall(binder_id, inner);
        binders.iter().rev().fold(inner, forall)
    }

    pub fn intern_constrained(&self, constraint: TypeId, constrained: TypeId) -> TypeId {
        self.intern_type(Type::Constrained(constraint, constrained))
    }

    pub fn intern_constrained_list(&self, constraints: &[TypeId], constrained: TypeId) -> TypeId {
        let constrain = |constrained, &constraint| self.intern_constrained(constraint, constrained);
        constraints.iter().rev().fold(constrained, constrain)
    }

    pub fn intern_kinded(&self, inner: TypeId, kind: TypeId) -> TypeId {
        self.intern_type(Type::Kinded(inner, kind))
    }

    pub fn intern_row(&self, fields: Vec<RowField>, tail: Option<TypeId>) -> TypeId {
        let mut fields = fields;
        fields.sort_by(|left, right| left.label.cmp(&right.label));
        let row = RowType { fields: Arc::from(fields), tail };
        let row_id = self.intern_row_type(row);
        self.intern_type(Type::Row(row_id))
    }

    pub fn intern_empty_row(&self) -> TypeId {
        self.intern_row(vec![], None)
    }

    pub fn intern_record(&self, row: TypeId) -> TypeId {
        self.intern_application(self.types.record, row)
    }

    pub fn intern_row_kind(&self, element: TypeId) -> TypeId {
        self.intern_application(self.types.row, element)
    }

    pub fn intern_constructor(&self, (file_id, type_id): Item) -> TypeId {
        self.intern_type(Type::Constructor(file_id, type_id))
    }

    pub fn intern_application_list(&self, function: TypeId, arguments: &[TypeId]) -> TypeId {
        let apply = |function, &argument| self.intern_application(function, argument);
        arguments.iter().fold(function, apply)
    }
}
