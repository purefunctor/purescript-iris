mod algorithm;
mod error;
mod items;
mod source;
#[cfg(test)]
mod tests;

pub use error::*;
pub use items::*;
pub use source::*;

use std::collections::hash_map::Entry;
use std::ops;

use la_arena::Arena;
use rustc_hash::FxHashMap;
use smol_str::SmolStr;
use stabilizing::StabilizedModule;
use syntax::{SyntaxNodePtr, cst};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct IndexedModule {
    pub kind: ExportKind,
    pub names: IndexedNames,
    pub exports: IndexedExports,
    pub items: IndexedItems,
    pub imports: IndexedImports,
    pub pairs: IndexedPairs,
    pub errors: Vec<IndexingError>,
}

impl IndexedModule {
    pub fn term_item_ptr(
        &self,
        stabilized: &StabilizedModule,
        id: TermItemId,
    ) -> impl Iterator<Item = SyntaxNodePtr> {
        const fn aux<T: Copy>(expected_id: TermItemId) -> impl Fn(&(T, TermItemId)) -> Option<T> {
            move |(id, item_id)| if *item_id == expected_id { Some(*id) } else { None }
        }

        let declaration = self.pairs.declaration_to_term.iter().filter_map(aux(id));
        let constructor = self.pairs.constructor_to_term.iter().filter_map(aux(id));
        let class_member = self.pairs.class_member_to_term.iter().filter_map(aux(id));

        let declaration = declaration.filter_map(|id| stabilized.syntax_ptr(id));
        let constructor = constructor.filter_map(|id| stabilized.syntax_ptr(id));
        let class_member = class_member.filter_map(|id| stabilized.syntax_ptr(id));

        declaration.chain(constructor).chain(class_member)
    }

    pub fn type_item_ptr(
        &self,
        stabilized: &StabilizedModule,
        id: TypeItemId,
    ) -> impl Iterator<Item = SyntaxNodePtr> {
        const fn aux<T: Copy>(expected_id: TypeItemId) -> impl Fn(&(T, TypeItemId)) -> Option<T> {
            move |(id, item_id)| if *item_id == expected_id { Some(*id) } else { None }
        }

        let declaration = self.pairs.declaration_to_type.iter().filter_map(aux(id));
        declaration.filter_map(|id| stabilized.syntax_ptr(id))
    }

    pub fn data_constructors(&self, id: TypeItemId) -> impl Iterator<Item = TermItemId> + '_ {
        let constructors = match &self.items[id].kind {
            IndexedTypeItemKind::Data { constructors, .. }
            | IndexedTypeItemKind::Newtype { constructors, .. } => constructors.as_slice(),
            _ => &[],
        };

        constructors.iter().copied()
    }

    pub fn constructor_type(&self, id: TermItemId) -> Option<TypeItemId> {
        let index = id.into_raw().into_u32() as usize;
        if index >= self.items.terms.len() {
            return None;
        }
        let IndexedTermItemKind::Constructor { type_id, .. } = self.items[id].kind else {
            return None;
        };
        type_id
    }

    pub fn class_members(&self, id: TypeItemId) -> impl Iterator<Item = TermItemId> + '_ {
        let members = match &self.items[id].kind {
            IndexedTypeItemKind::Class { members, .. } => members.as_slice(),
            _ => &[],
        };

        members.iter().copied()
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct IndexedNames {
    pub terms: NameIndex<TermItemId>,
    pub types: NameIndex<TypeItemId>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct NameIndex<ItemId> {
    first: FxHashMap<SmolStr, ItemId>,
    entries: Vec<(SmolStr, ItemId)>,
}

impl<ItemId> Default for NameIndex<ItemId> {
    fn default() -> Self {
        NameIndex { first: FxHashMap::default(), entries: Vec::default() }
    }
}

impl<ItemId> NameIndex<ItemId>
where
    ItemId: Copy + Eq,
{
    pub fn lookup(&self, name: &str) -> Option<ItemId> {
        self.first.get(name).copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&SmolStr, ItemId)> {
        self.entries.iter().map(|(name, id)| (name, *id))
    }

    pub(crate) fn insert(&mut self, name: SmolStr, id: ItemId) -> Option<ItemId> {
        let existing = match self.first.entry(SmolStr::clone(&name)) {
            Entry::Occupied(entry) => {
                let id = entry.get();
                Some(*id)
            }
            Entry::Vacant(entry) => {
                entry.insert(id);
                None
            }
        };

        self.entries.push((name, id));
        existing.filter(|existing| *existing != id)
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct IndexedExports {
    pub terms: Vec<IndexedExport<TermItemId>>,
    pub types: Vec<IndexedTypeExport>,
    pub modules: Vec<IndexedModuleExport>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct IndexedExport<ItemId> {
    pub id: ExportItemId,
    pub name: SmolStr,
    pub item: Option<ItemId>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct IndexedTypeExport {
    pub id: ExportItemId,
    pub name: SmolStr,
    pub item: Option<TypeItemId>,
    pub selection: Option<TypeSelection>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct IndexedModuleExport {
    pub id: ExportItemId,
    pub name: SmolStr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeSelection {
    Everything,
    Enumerated(Box<[SmolStr]>),
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct IndexedItems {
    terms: Arena<IndexedTermItem>,
    types: Arena<IndexedTypeItem>,
    instances: Arena<IndexedInstanceItem>,
    derives: Arena<IndexedDeriveItem>,
    instance_sources: Vec<InstanceSourceItemId>,
    ordered_terms: Vec<OrderedTermItemId>,
}

impl IndexedItems {
    pub fn iter_terms(&self) -> impl Iterator<Item = (TermItemId, &IndexedTermItem)> {
        self.terms.iter()
    }

    pub fn iter_types(&self) -> impl Iterator<Item = (TypeItemId, &IndexedTypeItem)> {
        self.types.iter()
    }

    pub fn iter_instances(&self) -> impl Iterator<Item = (InstanceItemId, &IndexedInstanceItem)> {
        self.instances.iter()
    }

    pub fn iter_derives(&self) -> impl Iterator<Item = (DeriveItemId, &IndexedDeriveItem)> {
        self.derives.iter()
    }

    pub fn instance_sources(&self) -> &[InstanceSourceItemId] {
        &self.instance_sources
    }

    pub fn ordered_terms(&self) -> &[OrderedTermItemId] {
        &self.ordered_terms
    }
}

impl ops::Index<TermItemId> for IndexedItems {
    type Output = IndexedTermItem;

    fn index(&self, index: TermItemId) -> &IndexedTermItem {
        &self.terms[index]
    }
}

impl ops::Index<TypeItemId> for IndexedItems {
    type Output = IndexedTypeItem;

    fn index(&self, index: TypeItemId) -> &IndexedTypeItem {
        &self.types[index]
    }
}

impl ops::Index<InstanceItemId> for IndexedItems {
    type Output = IndexedInstanceItem;

    fn index(&self, index: InstanceItemId) -> &IndexedInstanceItem {
        &self.instances[index]
    }
}

impl ops::Index<DeriveItemId> for IndexedItems {
    type Output = IndexedDeriveItem;

    fn index(&self, index: DeriveItemId) -> &IndexedDeriveItem {
        &self.derives[index]
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub enum ExportKind {
    #[default]
    /// module Main where
    Implicit,
    /// module Main (value, Type, ...) where
    Explicit,
    /// module Main (module Main, ...) where
    ExplicitSelf,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ImportKind {
    #[default]
    /// import Lib
    Implicit,
    /// import Lib (value, Type, ...)
    Explicit,
    /// import Lib hiding (value, Type, ...)
    Hidden,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ImplicitItems {
    Everything,
    Enumerated(Box<[SmolStr]>),
}

pub type ImportedTerms = FxHashMap<SmolStr, ImportItemId>;
pub type ImportedTypes = FxHashMap<SmolStr, (ImportItemId, Option<ImplicitItems>)>;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct IndexedImport {
    pub name: Option<SmolStr>,
    pub alias: Option<SmolStr>,
    pub kind: ImportKind,
    pub terms: ImportedTerms,
    pub types: ImportedTypes,
    pub exported: bool,
}

pub type IndexedImports = FxHashMap<ImportId, IndexedImport>;

impl IndexedImport {
    pub(crate) fn new(name: Option<SmolStr>, alias: Option<SmolStr>) -> IndexedImport {
        IndexedImport { name, alias, ..Default::default() }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct IndexedPairs {
    derive_to_item: Vec<(DeriveId, DeriveItemId)>,
    instance_chain: Vec<(InstanceId, InstanceChainId, u32)>,
    instance_to_item: Vec<(InstanceId, InstanceItemId)>,
    instance_members: Vec<(InstanceId, InstanceMemberId)>,

    declaration_to_term: Vec<(DeclarationId, TermItemId)>,
    declaration_to_type: Vec<(DeclarationId, TypeItemId)>,
    declaration_to_instance: Vec<(DeclarationId, InstanceItemId)>,
    declaration_to_derive: Vec<(DeclarationId, DeriveItemId)>,
    constructor_to_term: Vec<(DataConstructorId, TermItemId)>,
    class_member_to_term: Vec<(ClassMemberId, TermItemId)>,
}

impl IndexedPairs {
    pub fn derive_to_item(&self, id: DeriveId) -> Option<DeriveItemId> {
        lookup_pair(&self.derive_to_item, id)
    }

    pub fn instance_to_item(&self, id: InstanceId) -> Option<InstanceItemId> {
        lookup_pair(&self.instance_to_item, id)
    }

    pub fn declaration_to_term(&self, id: DeclarationId) -> Option<TermItemId> {
        lookup_pair(&self.declaration_to_term, id)
    }

    pub fn declaration_to_type(&self, id: DeclarationId) -> Option<TypeItemId> {
        lookup_pair(&self.declaration_to_type, id)
    }

    pub fn declaration_to_instance(&self, id: DeclarationId) -> Option<InstanceItemId> {
        lookup_pair(&self.declaration_to_instance, id)
    }

    pub fn declaration_to_derive(&self, id: DeclarationId) -> Option<DeriveItemId> {
        lookup_pair(&self.declaration_to_derive, id)
    }

    pub fn constructor_to_term(&self, id: DataConstructorId) -> Option<TermItemId> {
        lookup_pair(&self.constructor_to_term, id)
    }

    pub fn class_member_to_term(&self, id: ClassMemberId) -> Option<TermItemId> {
        lookup_pair(&self.class_member_to_term, id)
    }

    fn has_keys_in_source_order(&self) -> bool {
        is_sorted_by_key(&self.derive_to_item)
            && is_sorted_by_key(&self.instance_to_item)
            && is_sorted_by_key(&self.declaration_to_term)
            && is_sorted_by_key(&self.declaration_to_type)
            && is_sorted_by_key(&self.declaration_to_instance)
            && is_sorted_by_key(&self.declaration_to_derive)
            && is_sorted_by_key(&self.constructor_to_term)
            && is_sorted_by_key(&self.class_member_to_term)
    }

    pub fn instance_chain_id(&self, id: InstanceId) -> Option<InstanceChainId> {
        self.instance_chain_metadata(id).map(|(chain_id, _)| chain_id)
    }

    pub fn instance_chain_position(&self, id: InstanceId) -> Option<u32> {
        self.instance_chain_metadata(id).map(|(_, position)| position)
    }

    pub fn instance_chain_metadata(&self, id: InstanceId) -> Option<(InstanceChainId, u32)> {
        self.instance_chain.binary_search_by_key(&id, |(instance_id, _, _)| *instance_id).ok().map(
            |index| {
                let (_, chain_id, position) = self.instance_chain[index];
                (chain_id, position)
            },
        )
    }
}

/// Returns the first value for the key, as instance chains pair one declaration with many items.
fn lookup_pair<K: Ord + Copy, V: Copy>(pairs: &[(K, V)], id: K) -> Option<V> {
    let index = pairs.partition_point(|(key, _)| *key < id);
    pairs.get(index).and_then(|(key, value)| if *key == id { Some(*value) } else { None })
}

/// Pairs are pushed in source order and stabilized IDs are allocated in preorder.
fn is_sorted_by_key<K: Ord, V>(pairs: &[(K, V)]) -> bool {
    pairs.is_sorted_by_key(|(key, _)| key)
}

pub fn index_module(
    source: &str,
    cst: &cst::Module,
    stabilized: &StabilizedModule,
) -> IndexedModule {
    let algorithm::State { kind, names, exports, items, imports, pairs, errors, .. } =
        algorithm::index_module(source, cst, stabilized);
    debug_assert!(
        pairs.has_keys_in_source_order(),
        "invariant violated: pair keys are not in source order"
    );
    IndexedModule { kind, names, exports, items, imports, pairs, errors }
}
