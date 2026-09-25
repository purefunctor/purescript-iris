use std::collections::hash_map::Entry;
use std::iter;

use rustc_hash::{FxHashMap, FxHashSet};
use smol_str::{SmolStr, ToSmolStr};
use stabilizing::{AstId, ExpectId, StabilizedModule};
use syntax::ast::AstNode;
use syntax::{SyntaxToken, cst};

use crate::items::*;
use crate::source::*;
use crate::{
    ExistingKind, ExportKind, ImplicitItems, ImportKind, IndexedExport, IndexedExports,
    IndexedImport, IndexedImports, IndexedItems, IndexedModuleExport, IndexedNames, IndexedPairs,
    IndexedTypeExport, IndexingError, ItemKind, TypeSelection,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpenItemGroup {
    Term(TermItemId),
    Type(TypeItemId),
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct State<'a> {
    source: &'a str,
    name: Option<SmolStr>,
    open: Option<OpenItemGroup>,
    pub(super) kind: ExportKind,
    pub(super) names: IndexedNames,
    pub(super) exports: IndexedExports,
    pub(super) items: IndexedItems,
    pub(super) imports: IndexedImports,
    pub(super) pairs: IndexedPairs,
    pub(super) errors: Vec<IndexingError>,
}

impl<'a> State<'a> {
    fn new(source: &'a str, name: Option<SmolStr>) -> State<'a> {
        State { source, name, ..Default::default() }
    }

    fn open_term_group(
        &mut self,
        name: &Option<SmolStr>,
    ) -> Option<(TermItemId, &mut IndexedTermItem)> {
        let OpenItemGroup::Term(id) = self.open? else {
            return None;
        };
        let item = &mut self.items.terms[id];
        if &item.name != name {
            return None;
        }
        Some((id, &mut self.items.terms[id]))
    }

    fn open_type_group(
        &mut self,
        name: &Option<SmolStr>,
    ) -> Option<(TypeItemId, &mut IndexedTypeItem)> {
        let OpenItemGroup::Type(id) = self.open? else {
            return None;
        };
        let item = &mut self.items.types[id];
        if &item.name != name {
            return None;
        }
        Some((id, &mut self.items.types[id]))
    }

    fn alloc_term(&mut self, item: IndexedTermItem) -> TermItemId {
        let id = self.items.terms.alloc(item);
        self.items.ordered_terms.push(OrderedTermItemId::Term(id));
        self.open = Some(OpenItemGroup::Term(id));
        id
    }

    fn alloc_type(&mut self, item: IndexedTypeItem) -> TypeItemId {
        let id = self.items.types.alloc(item);
        self.open = Some(OpenItemGroup::Type(id));
        id
    }

    fn alloc_instance(&mut self, item: IndexedInstanceItem) -> InstanceItemId {
        let id = self.items.instances.alloc(item);
        self.items.instance_sources.push(InstanceSourceItemId::Instance(id));
        self.items.ordered_terms.push(OrderedTermItemId::Instance(id));
        self.open = None;
        id
    }

    fn alloc_derive(&mut self, item: IndexedDeriveItem) -> DeriveItemId {
        let id = self.items.derives.alloc(item);
        self.items.instance_sources.push(InstanceSourceItemId::Derive(id));
        self.items.ordered_terms.push(OrderedTermItemId::Derive(id));
        self.open = None;
        id
    }
}

fn name_from_token(source: &str, token: Option<SyntaxToken>) -> Option<SmolStr> {
    token.map(|token| SmolStr::from(token.text(source)))
}

pub(super) fn index_module<'a>(
    source: &'a str,
    cst: &cst::Module,
    stabilized: &StabilizedModule,
) -> State<'a> {
    let name = cst.header().and_then(|cst| {
        let cst = cst.name()?;
        Some(cst.syntax().text(source).to_smolstr())
    });

    let mut state = State::new(source, name);

    if let Some(statements) = cst.statements() {
        for declaration in statements.children() {
            index_declaration(&mut state, stabilized, &declaration);
        }
    }

    validate_items(&mut state);

    if let Some(imports) = cst.imports() {
        for import in imports.children() {
            index_import(&mut state, stabilized, &import);
        }
    }

    if let Some(header) = cst.header()
        && let Some(exports) = header.exports()
    {
        state.kind = ExportKind::Explicit;
        index_exports(&mut state, stabilized, &exports);
    }

    state
}

fn index_declaration(state: &mut State, stabilized: &StabilizedModule, cst: &cst::Declaration) {
    let declaration_id = stabilized.lookup_cst(cst).expect_id();
    match cst {
        cst::Declaration::ValueSignature(cst) => {
            let id = stabilized.lookup_cst(cst).expect_id();
            let term_id = index_value_signature(state, id, cst);
            state.pairs.declaration_to_term.push((declaration_id, term_id));
        }
        cst::Declaration::ValueEquation(cst) => {
            let id = stabilized.lookup_cst(cst).expect_id();
            let term_id = index_value_equation(state, id, cst);
            state.pairs.declaration_to_term.push((declaration_id, term_id));
        }
        cst::Declaration::InfixDeclaration(cst) => {
            let id = stabilized.lookup_cst(cst).expect_id();
            index_infix(state, declaration_id, id, cst);
        }
        cst::Declaration::TypeRoleDeclaration(cst) => {
            let id = stabilized.lookup_cst(cst).expect_id();
            index_type_role(state, id, cst);
        }
        cst::Declaration::InstanceChain(cst) => {
            let chain_id = stabilized.lookup_cst(cst).expect_id();
            for (position, cst) in cst.instance_declarations().enumerate() {
                let instance_id = stabilized.lookup_cst(&cst).expect_id();
                let item_id = index_instance(state, instance_id, &cst);
                debug_assert!(
                    state
                        .pairs
                        .instance_chain
                        .last()
                        .is_none_or(|(previous_id, _, _)| *previous_id < instance_id),
                    "invariant violated: instance IDs are not in source order",
                );
                state.pairs.instance_chain.push((instance_id, chain_id, position as u32));
                state.pairs.instance_to_item.push((instance_id, item_id));
                state.pairs.declaration_to_instance.push((declaration_id, item_id));
                if let Some(cst) = cst.instance_statements() {
                    for cst in cst.children() {
                        let m_id = stabilized.lookup_cst(&cst).expect_id();
                        state.pairs.instance_members.push((instance_id, m_id));
                    }
                }
            }
        }
        cst::Declaration::TypeSynonymSignature(cst) => {
            let id = stabilized.lookup_cst(cst).expect_id();
            let token = cst.name_token();
            let type_id = index_type_signature(
                state,
                id,
                token,
                |name, id| IndexedTypeItem {
                    name,
                    kind: IndexedTypeItemKind::Synonym { signature: Some(id), equation: None },
                    exported: false,
                },
                ItemKind::SynonymSignature,
                |item| {
                    if let IndexedTypeItemKind::Synonym { signature, .. } = &mut item.kind {
                        Some(signature)
                    } else {
                        None
                    }
                },
            );
            state.pairs.declaration_to_type.push((declaration_id, type_id));
        }
        cst::Declaration::TypeSynonymEquation(cst) => {
            let id = stabilized.lookup_cst(cst).expect_id();
            let token = cst.name_token();
            let type_id = index_type_declaration(
                state,
                id,
                token,
                |name, id| IndexedTypeItem {
                    name,
                    kind: IndexedTypeItemKind::Synonym { signature: None, equation: Some(id) },
                    exported: false,
                },
                ItemKind::SynonymEquation,
                |item| {
                    if let IndexedTypeItemKind::Synonym { equation, .. } = &mut item.kind {
                        Some(equation)
                    } else {
                        None
                    }
                },
            );
            state.pairs.declaration_to_type.push((declaration_id, type_id));
        }
        cst::Declaration::ClassSignature(cst) => {
            let id = stabilized.lookup_cst(cst).expect_id();
            let token = cst.name_token();
            let type_id = index_type_signature(
                state,
                id,
                token,
                |name, id| IndexedTypeItem {
                    name,
                    kind: IndexedTypeItemKind::Class {
                        signature: Some(id),
                        declaration: None,
                        members: vec![],
                    },
                    exported: false,
                },
                ItemKind::ClassSignature,
                |item| {
                    if let IndexedTypeItemKind::Class { signature, .. } = &mut item.kind {
                        Some(signature)
                    } else {
                        None
                    }
                },
            );
            state.pairs.declaration_to_type.push((declaration_id, type_id));
        }
        cst::Declaration::ClassDeclaration(cst) => {
            let id = stabilized.lookup_cst(cst).expect_id();
            let token = cst.class_head().and_then(|cst| cst.name_token());
            let type_id = index_type_declaration(
                state,
                id,
                token,
                |name, id| IndexedTypeItem {
                    name,
                    kind: IndexedTypeItemKind::Class {
                        signature: None,
                        declaration: Some(id),
                        members: vec![],
                    },
                    exported: false,
                },
                ItemKind::ClassDeclaration,
                |item| {
                    if let IndexedTypeItemKind::Class { declaration, .. } = &mut item.kind {
                        Some(declaration)
                    } else {
                        None
                    }
                },
            );
            if let Some(cst) = cst.class_statements() {
                for cst in cst.children() {
                    let member_id = stabilized.lookup_cst(&cst).expect_id();
                    let term_id = index_class_member(state, type_id, member_id, &cst);
                    if let IndexedTypeItemKind::Class { members, .. } =
                        &mut state.items.types[type_id].kind
                    {
                        members.push(term_id);
                    }
                    state.pairs.class_member_to_term.push((member_id, term_id));
                }
            }
            state.pairs.declaration_to_type.push((declaration_id, type_id));
        }
        cst::Declaration::ForeignImportDataDeclaration(cst) => {
            let id = stabilized.lookup_cst(cst).expect_id();
            let type_id = index_foreign_data(state, id, cst);
            state.pairs.declaration_to_type.push((declaration_id, type_id));
        }
        cst::Declaration::ForeignImportValueDeclaration(cst) => {
            let id = stabilized.lookup_cst(cst).expect_id();
            let term_id = index_foreign_value(state, id, cst);
            state.pairs.declaration_to_term.push((declaration_id, term_id));
        }
        cst::Declaration::NewtypeSignature(cst) => {
            let id = stabilized.lookup_cst(cst).expect_id();
            let token = cst.name_token();
            let type_id = index_type_signature(
                state,
                id,
                token,
                |name, id| IndexedTypeItem {
                    name,
                    kind: IndexedTypeItemKind::Newtype {
                        signature: Some(id),
                        equation: None,
                        role: None,
                        constructors: vec![],
                    },
                    exported: false,
                },
                ItemKind::NewtypeSignature,
                |item| {
                    if let IndexedTypeItemKind::Newtype { signature, .. } = &mut item.kind {
                        Some(signature)
                    } else {
                        None
                    }
                },
            );
            state.pairs.declaration_to_type.push((declaration_id, type_id));
        }
        cst::Declaration::NewtypeEquation(cst) => {
            let id = stabilized.lookup_cst(cst).expect_id();
            let token = cst.name_token();
            let type_id = index_type_declaration(
                state,
                id,
                token,
                |name, id| IndexedTypeItem {
                    name,
                    kind: IndexedTypeItemKind::Newtype {
                        signature: None,
                        equation: Some(id),
                        role: None,
                        constructors: vec![],
                    },
                    exported: false,
                },
                ItemKind::NewtypeEquation,
                |item| {
                    if let IndexedTypeItemKind::Newtype { equation, .. } = &mut item.kind {
                        Some(equation)
                    } else {
                        None
                    }
                },
            );
            for cst in cst.data_constructors() {
                let constructor_id = stabilized.lookup_cst(&cst).expect_id();
                let attached_type_id =
                    matches!(state.items.types[type_id].kind, IndexedTypeItemKind::Newtype { .. })
                        .then_some(type_id);
                let term_id = index_data_constructor(state, attached_type_id, constructor_id, &cst);
                if let IndexedTypeItemKind::Newtype { constructors, .. } =
                    &mut state.items.types[type_id].kind
                {
                    constructors.push(term_id);
                }
                state.pairs.constructor_to_term.push((constructor_id, term_id));
            }
            state.pairs.declaration_to_type.push((declaration_id, type_id));
        }
        cst::Declaration::DataSignature(cst) => {
            let id = stabilized.lookup_cst(cst).expect_id();
            let token = cst.name_token();
            let type_id = index_type_signature(
                state,
                id,
                token,
                |name, id| IndexedTypeItem {
                    name,
                    kind: IndexedTypeItemKind::Data {
                        signature: Some(id),
                        equation: None,
                        role: None,
                        constructors: vec![],
                    },
                    exported: false,
                },
                ItemKind::DataSignature,
                |item| {
                    if let IndexedTypeItemKind::Data { signature, .. } = &mut item.kind {
                        Some(signature)
                    } else {
                        None
                    }
                },
            );
            state.pairs.declaration_to_type.push((declaration_id, type_id));
        }
        cst::Declaration::DataEquation(cst) => {
            let id = stabilized.lookup_cst(cst).expect_id();
            let token = cst.name_token();
            let type_id = index_type_declaration(
                state,
                id,
                token,
                |name, id| IndexedTypeItem {
                    name,
                    kind: IndexedTypeItemKind::Data {
                        signature: None,
                        equation: Some(id),
                        role: None,
                        constructors: vec![],
                    },
                    exported: false,
                },
                ItemKind::DataEquation,
                |item| {
                    if let IndexedTypeItemKind::Data { equation, .. } = &mut item.kind {
                        Some(equation)
                    } else {
                        None
                    }
                },
            );
            for cst in cst.data_constructors() {
                let constructor_id = stabilized.lookup_cst(&cst).expect_id();
                let attached_type_id =
                    matches!(state.items.types[type_id].kind, IndexedTypeItemKind::Data { .. })
                        .then_some(type_id);
                let term_id = index_data_constructor(state, attached_type_id, constructor_id, &cst);
                if let IndexedTypeItemKind::Data { constructors, .. } =
                    &mut state.items.types[type_id].kind
                {
                    constructors.push(term_id);
                }
                state.pairs.constructor_to_term.push((constructor_id, term_id));
            }
            state.pairs.declaration_to_type.push((declaration_id, type_id));
        }
        cst::Declaration::DeriveDeclaration(cst) => {
            let id = stabilized.lookup_cst(cst).expect_id();
            let item_id = index_derive(state, id, cst);
            state.pairs.derive_to_item.push((id, item_id));
            state.pairs.declaration_to_derive.push((declaration_id, item_id));
        }
    }
}

fn index_value_signature(
    state: &mut State,
    id: ValueSignatureId,
    cst: &cst::ValueSignature,
) -> TermItemId {
    let name = name_from_token(state.source, cst.name_token());

    let Some((active_id, active)) = state.open_term_group(&name) else {
        let kind = IndexedTermItemKind::Value { signature: Some(id), equations: vec![] };
        return state.alloc_term(IndexedTermItem { name, kind, exported: false });
    };

    if let IndexedTermItemKind::Value { signature, .. } = &mut active.kind {
        if signature.is_some() {
            let kind = ItemKind::ValueSignature(id);
            let existing = ExistingKind::Term(active_id);
            state.errors.push(IndexingError::DuplicateItem { kind, existing });
        } else {
            *signature = Some(id);
        }
    } else {
        let kind = ItemKind::ValueSignature(id);
        let existing = ExistingKind::Term(active_id);
        state.errors.push(IndexingError::MismatchedItem { kind, existing });
    }

    active_id
}

fn index_value_equation(
    state: &mut State,
    id: ValueEquationId,
    cst: &cst::ValueEquation,
) -> TermItemId {
    let name = name_from_token(state.source, cst.name_token());

    let Some((active_id, active)) = state.open_term_group(&name) else {
        let kind = IndexedTermItemKind::Value { signature: None, equations: vec![id] };
        return state.alloc_term(IndexedTermItem { name, kind, exported: false });
    };

    if let IndexedTermItemKind::Value { equations, .. } = &mut active.kind {
        equations.push(id);
    } else {
        let kind = ItemKind::ValueEquation(id);
        let existing = ExistingKind::Term(active_id);
        state.errors.push(IndexingError::MismatchedItem { kind, existing });
    }

    active_id
}

fn index_infix(
    state: &mut State,
    declaration_id: DeclarationId,
    infix_id: InfixId,
    cst: &cst::InfixDeclaration,
) {
    let type_token = cst.type_token();
    let operator_token = cst.operator_token();

    let name = name_from_token(state.source, operator_token);

    if type_token.is_some() {
        let kind = IndexedTypeItemKind::Operator { id: infix_id };
        let item = IndexedTypeItem { name, kind, exported: false };

        let type_id = state.alloc_type(item);
        state.pairs.declaration_to_type.push((declaration_id, type_id))
    } else {
        let kind = IndexedTermItemKind::Operator { id: infix_id };
        let item = IndexedTermItem { name, kind, exported: false };

        let term_id = state.alloc_term(item);
        state.pairs.declaration_to_term.push((declaration_id, term_id))
    }
}

fn index_type_role(state: &mut State, id: TypeRoleId, cst: &cst::TypeRoleDeclaration) {
    let name = name_from_token(state.source, cst.name_token());

    let Some((active_id, active)) = state.open_type_group(&name) else {
        return state.errors.push(IndexingError::InvalidRole { id, existing: None });
    };

    if let IndexedTypeItemKind::Data { role, .. }
    | IndexedTypeItemKind::Newtype { role, .. }
    | IndexedTypeItemKind::Foreign { role, .. } = &mut active.kind
    {
        if role.is_some() {
            state.errors.push(IndexingError::InvalidRole { id, existing: Some(active_id) });
        } else {
            *role = Some(id)
        }
    } else {
        state.errors.push(IndexingError::InvalidRole { id, existing: None });
    }
}

type Item<T> = fn(Option<SmolStr>, AstId<T>) -> IndexedTypeItem;
type Extract<T> = fn(&mut IndexedTypeItem) -> Option<&mut Option<AstId<T>>>;
type MakeItemKind<T> = fn(AstId<T>) -> ItemKind;

fn index_type_signature<T: AstNode>(
    state: &mut State,
    id: AstId<T>,
    token: Option<SyntaxToken>,
    item: Item<T>,
    kind: MakeItemKind<T>,
    extract: Extract<T>,
) -> TypeItemId {
    let name = name_from_token(state.source, token);

    let Some((active_id, active)) = state.open_type_group(&name) else {
        return state.alloc_type(item(name, id));
    };

    let Some(signature) = extract(active) else {
        let kind = kind(id);
        let existing = ExistingKind::Type(active_id);
        state.errors.push(IndexingError::MismatchedItem { kind, existing });
        return active_id;
    };

    if signature.is_some() {
        let kind = kind(id);
        let existing = ExistingKind::Type(active_id);
        state.errors.push(IndexingError::DuplicateItem { kind, existing });
    } else {
        *signature = Some(id)
    }

    active_id
}

fn index_type_declaration<T: AstNode>(
    state: &mut State,
    id: AstId<T>,
    token: Option<SyntaxToken>,
    item: Item<T>,
    kind: MakeItemKind<T>,
    extract: Extract<T>,
) -> TypeItemId {
    let name = name_from_token(state.source, token);

    let Some((active_id, active)) = state.open_type_group(&name) else {
        return state.alloc_type(item(name, id));
    };

    let Some(equation) = extract(active) else {
        let kind = kind(id);
        let existing = ExistingKind::Type(active_id);
        state.errors.push(IndexingError::MismatchedItem { kind, existing });
        return active_id;
    };

    if equation.is_some() {
        let kind = kind(id);
        let existing = ExistingKind::Type(active_id);
        state.errors.push(IndexingError::DuplicateItem { kind, existing });
    } else {
        *equation = Some(id)
    }

    active_id
}

fn index_data_constructor(
    state: &mut State,
    type_id: Option<TypeItemId>,
    id: DataConstructorId,
    cst: &cst::DataConstructor,
) -> TermItemId {
    let name = name_from_token(state.source, cst.name_token());
    let kind = IndexedTermItemKind::Constructor { id, type_id };
    let item_id = state.items.terms.alloc(IndexedTermItem { name, kind, exported: false });
    state.items.ordered_terms.push(OrderedTermItemId::Term(item_id));
    item_id
}

fn index_class_member(
    state: &mut State,
    parent: TypeItemId,
    id: ClassMemberId,
    cst: &cst::ClassMemberStatement,
) -> TermItemId {
    let name = name_from_token(state.source, cst.name_token());
    let kind = IndexedTermItemKind::ClassMember { id, parent };
    let item_id = state.items.terms.alloc(IndexedTermItem { name, kind, exported: false });
    state.items.ordered_terms.push(OrderedTermItemId::Term(item_id));
    item_id
}

fn index_foreign_data(
    state: &mut State,
    id: ForeignDataId,
    cst: &cst::ForeignImportDataDeclaration,
) -> TypeItemId {
    let name = name_from_token(state.source, cst.name_token());
    let kind = IndexedTypeItemKind::Foreign { id, role: None };
    state.alloc_type(IndexedTypeItem { name, kind, exported: false })
}

fn index_foreign_value(
    state: &mut State,
    id: ForeignValueId,
    cst: &cst::ForeignImportValueDeclaration,
) -> TermItemId {
    let name = name_from_token(state.source, cst.name_token());
    state.alloc_term(IndexedTermItem {
        name,
        kind: IndexedTermItemKind::Foreign { id },
        exported: false,
    })
}

fn index_instance(
    state: &mut State,
    id: InstanceId,
    cst: &cst::InstanceDeclaration,
) -> InstanceItemId {
    let name = cst.instance_name().and_then(|n| {
        let token = n.name_token()?;
        let text = token.text(state.source);
        Some(SmolStr::from(text))
    });
    state.alloc_instance(IndexedInstanceItem { name, id })
}

fn index_derive(state: &mut State, id: DeriveId, cst: &cst::DeriveDeclaration) -> DeriveItemId {
    let name = cst.instance_name().and_then(|n| {
        let token = n.name_token()?;
        let text = token.text(state.source);
        Some(SmolStr::from(text))
    });
    state.alloc_derive(IndexedDeriveItem { name, id })
}

fn validate_items(state: &mut State) {
    for (id, item) in state.items.terms.iter() {
        let Some(name) = &item.name else { continue };
        if let Some(existing_id) = state.names.terms.insert(SmolStr::clone(name), id) {
            let kind = ItemKind::Term(id);
            let existing = ExistingKind::Term(existing_id);
            state.errors.push(IndexingError::DuplicateItem { kind, existing });
        }
    }

    for (id, item) in state.items.types.iter() {
        let Some(name) = &item.name else { continue };
        if let Some(existing_id) = state.names.types.insert(SmolStr::clone(name), id) {
            let kind = ItemKind::Type(id);
            let existing = ExistingKind::Type(existing_id);
            state.errors.push(IndexingError::DuplicateItem { kind, existing });
        }
    }
}

// Imports

fn index_import(state: &mut State, stabilized: &StabilizedModule, cst: &cst::ImportStatement) {
    let id = stabilized.lookup_cst(cst).expect_id();

    let name = extract_name(state.source, cst);
    let alias = extract_alias(state.source, cst);

    let mut import = IndexedImport::new(name, alias);

    if let Some(cst) = cst.import_list() {
        if cst.hiding().is_some() {
            import.kind = ImportKind::Hidden;
        } else {
            import.kind = ImportKind::Explicit;
        }
        for cst in cst.children() {
            index_import_items(state, stabilized, &mut import, &cst);
        }
    }

    state.imports.insert(id, import);
}

fn index_import_items(
    state: &mut State,
    stabilized: &StabilizedModule,
    import: &mut IndexedImport,
    cst: &cst::ImportItem,
) {
    let id = stabilized.lookup_cst(cst).expect_id();
    match cst {
        cst::ImportItem::ImportValue(v) => {
            let Some(token) = v.name_token() else { return };
            let name = token.text(state.source);
            index_term_import(state, import, name, id);
        }
        cst::ImportItem::ImportClass(c) => {
            let Some(token) = c.name_token() else { return };
            let name = token.text(state.source);
            index_type_import(state, import, name, id, None);
        }
        cst::ImportItem::ImportType(t) => {
            let Some(token) = t.name_token() else { return };
            let name = token.text(state.source);
            index_type_import(state, import, name, id, t.type_items());
        }
        cst::ImportItem::ImportOperator(o) => {
            let Some(token) = o.name_token() else { return };
            let name = token.text(state.source);
            let name = operator_name(name);
            index_term_import(state, import, name, id);
        }
        cst::ImportItem::ImportTypeOperator(o) => {
            let Some(token) = o.name_token() else { return };
            let name = token.text(state.source);
            let name = operator_name(name);
            index_type_import(state, import, name, id, None);
        }
    }
}

fn index_term_import(state: &mut State, import: &mut IndexedImport, name: &str, id: ImportItemId) {
    let name = SmolStr::from(name);
    if let Some(&existing) = import.terms.get(&name) {
        state.errors.push(IndexingError::DuplicateImport { duplicate: id, existing });
    } else {
        import.terms.insert(name, id);
    }
}

fn index_type_import(
    state: &mut State,
    import: &mut IndexedImport,
    name: &str,
    id: ImportItemId,
    items: Option<cst::TypeItems>,
) {
    let name = SmolStr::from(name);
    let items = items.map(|items| index_type_items(state, id, items));
    if let Some((existing_id, existing_items)) = import.types.get_mut(&name) {
        let existing = *existing_id;
        *existing_items = merge_implicit_items(existing_items.take(), items);
        state.errors.push(IndexingError::DuplicateImport { duplicate: id, existing });
    } else {
        import.types.insert(name, (id, items));
    }
}

fn merge_implicit_items(
    existing: Option<ImplicitItems>,
    incoming: Option<ImplicitItems>,
) -> Option<ImplicitItems> {
    match (existing, incoming) {
        (None, items) | (items, None) => items,
        (Some(ImplicitItems::Everything), _) | (_, Some(ImplicitItems::Everything)) => {
            Some(ImplicitItems::Everything)
        }
        (Some(ImplicitItems::Enumerated(existing)), Some(ImplicitItems::Enumerated(incoming))) => {
            let merged: FxHashSet<_> = iter::chain(existing, incoming).collect();
            Some(ImplicitItems::Enumerated(Box::from_iter(merged)))
        }
    }
}

fn index_type_items(state: &mut State, id: ImportItemId, items: cst::TypeItems) -> ImplicitItems {
    match items {
        cst::TypeItems::TypeItemsAll(_) => ImplicitItems::Everything,
        cst::TypeItems::TypeItemsList(cst) => {
            let mut names = FxHashSet::default();
            let enumerated = cst.name_tokens().map(|token| {
                let name = token.text(state.source);
                let name = SmolStr::from(name);
                if !names.insert(SmolStr::clone(&name)) {
                    state
                        .errors
                        .push(IndexingError::DuplicateImport { duplicate: id, existing: id });
                }
                name
            });
            let enumerated = enumerated.collect();
            ImplicitItems::Enumerated(enumerated)
        }
    }
}

fn extract_name(source: &str, cst: &cst::ImportStatement) -> Option<SmolStr> {
    let cst = cst.module_name()?;
    Some(cst.syntax().text(source).to_smolstr())
}

fn extract_alias(source: &str, cst: &cst::ImportStatement) -> Option<SmolStr> {
    let cst = cst.import_alias()?;
    let cst = cst.module_name()?;
    Some(cst.syntax().text(source).to_smolstr())
}

// Exports

fn index_exports(state: &mut State, stabilized: &StabilizedModule, cst: &cst::ExportList) {
    let mut terms = FxHashMap::default();
    let mut types = FxHashMap::default();

    for cst in cst.children() {
        let id = stabilized.lookup_cst(&cst).expect_id();
        match cst {
            cst::ExportItem::ExportValue(cst) => {
                let Some(name) = cst.name_token() else { continue };
                let name = name.text(state.source);
                index_export_term(state, &mut terms, name, id);
            }
            cst::ExportItem::ExportClass(cst) => {
                let Some(name) = cst.name_token() else { continue };
                let name = name.text(state.source);
                index_export_type(state, &mut types, name, id, None);
            }
            cst::ExportItem::ExportType(cst) => {
                let Some(name) = cst.name_token() else { continue };
                let name = name.text(state.source);
                let items = cst.type_items();
                index_export_type(state, &mut types, name, id, items);
            }
            cst::ExportItem::ExportOperator(cst) => {
                let Some(name) = cst.name_token() else { continue };
                let name = name.text(state.source);
                let name = operator_name(name);
                index_export_term(state, &mut terms, name, id);
            }
            cst::ExportItem::ExportTypeOperator(cst) => {
                let Some(name) = cst.name_token() else { continue };
                let name = name.text(state.source);
                let name = operator_name(name);
                index_export_type(state, &mut types, name, id, None);
            }
            cst::ExportItem::ExportModule(cst) => {
                index_module_export(state, id, &cst);
            }
        }
    }

    for (_, item) in state.items.types.iter_mut() {
        let Some(name) = &item.name else { continue };
        if let Some((id, implicit)) = types.get(name) {
            item.exported = true;
            if let Some(implicit) = implicit {
                let constructors: &[TermItemId] = match &item.kind {
                    IndexedTypeItemKind::Data { constructors, .. }
                    | IndexedTypeItemKind::Newtype { constructors, .. } => constructors,
                    _ => &[],
                };

                match implicit {
                    ImplicitItems::Everything => {
                        for term_id in constructors.iter().copied() {
                            state.items.terms[term_id].exported = true;
                        }
                    }
                    ImplicitItems::Enumerated(names) => {
                        for name in names {
                            let term_id = constructors.iter().copied().find(|term_id| {
                                let item = &state.items.terms[*term_id];
                                item.name.as_deref() == Some(name.as_str())
                            });

                            if let Some(term_id) = term_id {
                                state.items.terms[term_id].exported = true;
                                mark_exported_term(&mut state.errors, &mut terms, name, *id);
                            } else {
                                state.errors.push(IndexingError::InvalidExport { id: *id });
                            }
                        }
                    }
                }
            }
            let members: &[TermItemId] = match &item.kind {
                IndexedTypeItemKind::Class { members, .. } => members,
                _ => &[],
            };
            for &term_id in members {
                state.items.terms[term_id].exported = true;
            }
        }
    }

    for (_, item) in state.items.terms.iter_mut() {
        let Some(name) = &item.name else { continue };
        item.exported = item.exported || terms.contains_key(name);
    }
}

fn index_export_term(
    state: &mut State,
    terms: &mut FxHashMap<SmolStr, ExportItemId>,
    name: &str,
    id: ExportItemId,
) {
    let name = SmolStr::from(name);
    let item = state.names.terms.lookup(&name);
    state.exports.terms.push(IndexedExport { id, name: SmolStr::clone(&name), item });

    mark_exported_term_name(&mut state.errors, terms, name, id);
}

fn mark_exported_term(
    errors: &mut Vec<IndexingError>,
    terms: &mut FxHashMap<SmolStr, ExportItemId>,
    name: &str,
    id: ExportItemId,
) {
    let name = SmolStr::from(name);
    mark_exported_term_name(errors, terms, name, id);
}

fn mark_exported_term_name(
    errors: &mut Vec<IndexingError>,
    terms: &mut FxHashMap<SmolStr, ExportItemId>,
    name: SmolStr,
    id: ExportItemId,
) {
    match terms.entry(name) {
        Entry::Occupied(o) => {
            let existing = *o.get();
            errors.push(IndexingError::DuplicateExport { id, existing });
        }
        Entry::Vacant(v) => {
            v.insert(id);
        }
    }
}

fn index_export_type(
    state: &mut State,
    types: &mut FxHashMap<SmolStr, (ExportItemId, Option<ImplicitItems>)>,
    name: &str,
    id: ExportItemId,
    items: Option<cst::TypeItems>,
) {
    let name = SmolStr::from(name);
    let item = state.names.types.lookup(&name);
    let selection = items.map(|items| type_selection(state.source, items));

    state.exports.types.push(IndexedTypeExport {
        id,
        name: SmolStr::clone(&name),
        item,
        selection: selection.clone(),
    });

    match types.entry(name) {
        Entry::Occupied(o) => {
            let (existing, _) = *o.get();
            state.errors.push(IndexingError::DuplicateExport { id, existing });
        }
        Entry::Vacant(v) => {
            let items = selection.map(implicit_items_from_selection);
            v.insert((id, items));
        }
    }
}

fn type_selection(source: &str, cst: cst::TypeItems) -> TypeSelection {
    match cst {
        cst::TypeItems::TypeItemsAll(_) => TypeSelection::Everything,
        cst::TypeItems::TypeItemsList(cst) => {
            let enumerated = cst.name_tokens().map(|token| {
                let name = token.text(source);
                SmolStr::from(name)
            });
            let enumerated = enumerated.collect();
            TypeSelection::Enumerated(enumerated)
        }
    }
}

fn implicit_items_from_selection(selection: TypeSelection) -> ImplicitItems {
    match selection {
        TypeSelection::Everything => ImplicitItems::Everything,
        TypeSelection::Enumerated(items) => ImplicitItems::Enumerated(items),
    }
}

fn operator_name(name: &str) -> &str {
    name.trim_start_matches("(").trim_end_matches(")")
}

fn index_module_export(state: &mut State, id: ExportItemId, cst: &cst::ExportModule) {
    if let Some(n) = extracted_exported_module(state.source, cst) {
        state.exports.modules.push(IndexedModuleExport { id, name: SmolStr::clone(&n) });
        if state.name.as_ref() == Some(&n) {
            state.kind = ExportKind::ExplicitSelf;
        } else {
            // PureScript supports the following export forms:
            //
            // 1. Using the alias:
            //
            // ```purescript
            // module Main (module Maybe) where
            //
            // import Data.Maybe as Maybe
            // ```
            //
            // 2. Using the name:
            //
            // ```purescript
            // module Main (module Data.Maybe) where
            //
            // import Data.Maybe (isJust)
            // ```
            //
            // Modules can only be exported using its full name if it's not aliased.
            // As a result, the following export form is invalid:
            //
            // ```purescript
            // module Main (module Data.Maybe) where
            //
            // import Data.Maybe as Maybe
            // ```
            for items in state.imports.values_mut() {
                let alias = items.alias.as_deref();
                let name = items.name.as_deref();

                let using_alias = alias == Some(&n);
                let using_name = alias.is_none() && name == Some(&n);

                if using_alias || using_name {
                    items.exported = true;
                }
            }
        }
    }
}

fn extracted_exported_module(source: &str, cst: &cst::ExportModule) -> Option<SmolStr> {
    let cst = cst.module_name()?;
    Some(cst.syntax().text(source).to_smolstr())
}
