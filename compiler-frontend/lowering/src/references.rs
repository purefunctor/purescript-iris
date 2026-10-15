use std::ops::Range;

use files::FileId;
use indexing::{
    DeriveId, IndexedModule, IndexedTermItemKind, IndexedTypeItemKind, InfixId, InstanceId,
    InstanceSourceItemId, TermItemId, TypeItemId,
};

use crate::{
    BinderId, BinderKind, ExpressionId, ExpressionKind, LoweredTree, RecordPunId, TermItemKind,
    TermOperatorId, TermVariableResolution, TypeId, TypeItemKind, TypeKind, TypeOperatorId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TermReference {
    Expression(ExpressionId),
    RecordPun(RecordPunId),
    Binder(BinderId),
    Operator(TermOperatorId),
    Infix(InfixId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TypeReference {
    Type(TypeId),
    Operator(TypeOperatorId),
    Infix(InfixId),
    Instance(InstanceId),
    Derive(DeriveId),
}

#[derive(Debug, PartialEq, Eq)]
pub struct ReferenceIndex {
    terms: ItemReferences<(FileId, TermItemId), TermReference>,
    types: ItemReferences<(FileId, TypeItemId), TypeReference>,
}

impl ReferenceIndex {
    pub fn of_term(&self, file_id: FileId, term_id: TermItemId) -> &[TermReference] {
        self.terms.of((file_id, term_id))
    }

    pub fn of_type(&self, file_id: FileId, type_id: TypeItemId) -> &[TypeReference] {
        self.types.of((file_id, type_id))
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ItemReferences<Item, Reference> {
    items: Vec<(Item, Range<usize>)>,
    references: Vec<Reference>,
}

impl<Item, Reference> ItemReferences<Item, Reference>
where
    Item: Copy + Ord,
    Reference: Copy + Ord,
{
    fn new(mut entries: Vec<(Item, Reference)>) -> ItemReferences<Item, Reference> {
        entries.sort_unstable();

        let mut items: Vec<(Item, Range<usize>)> = Vec::new();
        let mut references = Vec::with_capacity(entries.len());
        for (item, reference) in entries {
            let index = references.len();
            match items.last_mut() {
                Some((last, range)) if *last == item => range.end = index + 1,
                _ => items.push((item, index..index + 1)),
            }
            references.push(reference);
        }

        ItemReferences { items, references }
    }

    fn of(&self, item: Item) -> &[Reference] {
        match self.items.binary_search_by_key(&item, |(item, _)| *item) {
            Ok(index) => &self.references[self.items[index].1.clone()],
            Err(_) => &[],
        }
    }
}

pub(crate) fn index_references(indexed: &IndexedModule, tree: &LoweredTree) -> ReferenceIndex {
    let mut terms = Vec::new();
    let mut types = Vec::new();

    for (id, kind) in tree.iter_expression() {
        let resolution = match kind {
            ExpressionKind::Constructor { resolution }
            | ExpressionKind::OperatorName { resolution } => *resolution,
            ExpressionKind::Variable {
                resolution: Some(TermVariableResolution::Reference(file_id, term_id)),
            } => Some((*file_id, *term_id)),
            _ => None,
        };
        if let Some(item) = resolution {
            terms.push((item, TermReference::Expression(id)));
        }
    }

    for (id, resolution) in tree.iter_expression_pun() {
        if let TermVariableResolution::Reference(file_id, term_id) = resolution {
            terms.push(((file_id, term_id), TermReference::RecordPun(id)));
        }
    }

    for (id, kind) in tree.iter_binder() {
        if let BinderKind::Constructor { resolution: Some(item), .. } = kind {
            terms.push((*item, TermReference::Binder(id)));
        }
    }

    for (id, file_id, term_id) in tree.iter_term_operator() {
        terms.push(((file_id, term_id), TermReference::Operator(id)));
    }

    for (term_id, indexed_item) in indexed.items.iter_terms() {
        let IndexedTermItemKind::Operator { id } = indexed_item.kind else { continue };
        let kind = tree.get_term_item_kind(term_id);
        if let Some(TermItemKind::Operator { resolution: Some(item), .. }) = kind {
            terms.push((*item, TermReference::Infix(id)));
        }
    }

    for (id, kind) in tree.iter_type() {
        if let TypeKind::Constructor { resolution: Some(item) }
        | TypeKind::Operator { resolution: Some(item) } = kind
        {
            types.push((*item, TypeReference::Type(id)));
        }
    }

    for (id, file_id, type_id) in tree.iter_type_operator() {
        types.push(((file_id, type_id), TypeReference::Operator(id)));
    }

    for (type_id, indexed_item) in indexed.items.iter_types() {
        let IndexedTypeItemKind::Operator { id } = indexed_item.kind else { continue };
        let kind = tree.get_type_item_kind(type_id);
        if let Some(TypeItemKind::Operator { resolution: Some(item), .. }) = kind {
            types.push((*item, TypeReference::Infix(id)));
        }
    }

    for &item_id in indexed.items.instance_sources() {
        match item_id {
            InstanceSourceItemId::Instance(id) => {
                let resolution = tree.get_instance_item(id).and_then(|item| item.resolution);
                if let Some(item) = resolution {
                    types.push((item, TypeReference::Instance(indexed.items[id].id)));
                }
            }
            InstanceSourceItemId::Derive(id) => {
                let resolution = tree.get_derive_item(id).and_then(|item| item.resolution);
                if let Some(item) = resolution {
                    types.push((item, TypeReference::Derive(indexed.items[id].id)));
                }
            }
        }
    }

    let terms = ItemReferences::new(terms);
    let types = ItemReferences::new(types);

    ReferenceIndex { terms, types }
}
