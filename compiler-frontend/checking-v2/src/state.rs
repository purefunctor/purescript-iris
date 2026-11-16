use std::mem;

use files::FileId;
use indexing::{TermItemId, TypeItemId};
use la_arena::{Arena, Idx};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::error::{CheckingError, ErrorCrumb, ErrorKind};
use crate::tree::{Evidence, EvidenceBinderId, EvidenceSlotId, TermReference};
use crate::types::{ForallBinderId, Level, Name, TypeId, UnificationId};
use crate::{CheckedModule, CheckedSynonym};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnificationEntry {
    pub level: Level,
    pub kind: TypeId,
    pub solution: Option<TypeId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypeVariableKey {
    Forall(lowering::TypeVariableBindingId),
    Implicit(lowering::GraphNodeId, lowering::ImplicitBindingId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TermVariableKey {
    Binder(lowering::BinderId),
    Let(lowering::LetBindingNameGroupId),
    Pun(lowering::RecordPunId),
    Section(lowering::ExpressionId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GroupMember {
    Term(TermItemId),
    Let(lowering::LetBindingNameGroupId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalVariable {
    pub type_id: TypeId,
    pub reference: TermReference,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hole {
    pub name: smol_str::SmolStr,
    pub type_id: TypeId,
    pub crumbs: Vec<ErrorCrumb>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wildcard {
    pub type_id: TypeId,
    pub crumbs: Vec<ErrorCrumb>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Given {
    pub constraint: TypeId,
    pub evidence: Evidence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GivenScope {
    pub parent: Option<GivenScopeId>,
    pub givens: Vec<Given>,
}

pub type GivenScopeId = Idx<GivenScope>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wanted {
    pub constraint: TypeId,
    pub slot: EvidenceSlotId,
    pub scope: Option<GivenScopeId>,
    pub crumbs: Vec<ErrorCrumb>,
    pub work: u32,
}

pub struct CheckState {
    pub checked: CheckedModule,
    pub file_id: FileId,
    pub unifications: Vec<UnificationEntry>,
    pub solutions: u32,
    pub unique: u32,
    pub level: Level,
    pub type_variables: FxHashMap<TypeVariableKey, TypeId>,
    pub term_variables: FxHashMap<TermVariableKey, LocalVariable>,
    pub holes: Vec<Hole>,
    pub wildcards: Vec<Wildcard>,
    pub pending_terms: FxHashMap<TermItemId, TypeId>,
    pub pending_types: FxHashMap<TypeItemId, TypeId>,
    pub pending_synonyms: FxHashMap<TypeItemId, CheckedSynonym>,
    pub undefined_members: FxHashSet<GroupMember>,
    pub implicit_binders: Vec<ForallBinderId>,
    pub crumbs: Vec<ErrorCrumb>,
    pub given_scopes: Arena<GivenScope>,
    pub given_scope: Option<GivenScopeId>,
    pub wanted: Vec<Wanted>,
    pub evidence_binders: u32,
}

impl CheckState {
    pub fn new(file_id: FileId) -> CheckState {
        CheckState {
            checked: CheckedModule::default(),
            file_id,
            unifications: Vec::new(),
            solutions: 0,
            unique: 0,
            level: Level(0),
            type_variables: FxHashMap::default(),
            term_variables: FxHashMap::default(),
            holes: Vec::new(),
            wildcards: Vec::new(),
            pending_terms: FxHashMap::default(),
            pending_types: FxHashMap::default(),
            pending_synonyms: FxHashMap::default(),
            undefined_members: FxHashSet::default(),
            implicit_binders: Vec::new(),
            crumbs: Vec::new(),
            given_scopes: Arena::default(),
            given_scope: None,
            wanted: Vec::new(),
            evidence_binders: 0,
        }
    }

    pub fn fresh_name(&mut self) -> Name {
        let unique = self.unique;
        self.unique += 1;
        Name { file: self.file_id, unique }
    }

    pub fn fresh_unification_id(&mut self, kind: TypeId) -> UnificationId {
        let index = self.unifications.len() as u32;
        let level = self.level;
        self.unifications.push(UnificationEntry { level, kind, solution: None });
        UnificationId(index)
    }

    pub fn unification(&self, UnificationId(index): UnificationId) -> UnificationEntry {
        let entry = self.unifications.get(index as usize);
        *entry.expect("invariant violated: unification out of range")
    }

    pub fn unification_mut(
        &mut self,
        UnificationId(index): UnificationId,
    ) -> &mut UnificationEntry {
        let entry = self.unifications.get_mut(index as usize);
        entry.expect("invariant violated: unification out of range")
    }

    pub fn fresh_evidence_binder(&mut self) -> EvidenceBinderId {
        let index = self.evidence_binders;
        self.evidence_binders += 1;
        EvidenceBinderId(index)
    }

    pub fn emit_wanted(&mut self, constraint: TypeId) -> EvidenceSlotId {
        let slot = self.checked.tree.allocate_evidence_slot();
        self.emit_wanted_into(constraint, slot, 0);
        slot
    }

    pub fn emit_wanted_into(&mut self, constraint: TypeId, slot: EvidenceSlotId, work: u32) {
        let scope = self.given_scope;
        let crumbs = self.crumbs.clone();
        self.wanted.push(Wanted { constraint, slot, scope, crumbs, work });
    }

    pub fn insert_error(&mut self, kind: ErrorKind) {
        let crumbs = self.crumbs.clone();
        self.insert_error_at(kind, crumbs);
    }

    pub fn insert_error_at(&mut self, kind: ErrorKind, crumbs: Vec<ErrorCrumb>) {
        let error = CheckingError { kind, crumbs };
        if !self.checked.errors.contains(&error) {
            self.checked.errors.push(error);
        }
    }

    pub fn with_crumb<T>(
        &mut self,
        crumb: ErrorCrumb,
        action: impl FnOnce(&mut CheckState) -> T,
    ) -> T {
        self.crumbs.push(crumb);
        let result = action(self);
        self.crumbs.pop();
        result
    }

    pub fn with_level<T>(&mut self, action: impl FnOnce(&mut CheckState) -> T) -> T {
        let level = self.level;
        self.level = level.increment();
        let result = action(self);
        self.level = level;
        result
    }

    pub fn with_givens<T>(
        &mut self,
        givens: Vec<Given>,
        action: impl FnOnce(&mut CheckState) -> T,
    ) -> T {
        if givens.is_empty() {
            return action(self);
        }
        let parent = self.given_scope;
        let scope = self.given_scopes.alloc(GivenScope { parent, givens });
        self.given_scope = Some(scope);
        let result = self.with_group_visible(action);
        self.given_scope = parent;
        result
    }

    pub fn with_undefined_members<T>(
        &mut self,
        members: &[GroupMember],
        action: impl FnOnce(&mut CheckState) -> T,
    ) -> T {
        self.undefined_members.extend(members.iter().copied());
        let result = action(self);
        for member in members {
            self.undefined_members.remove(member);
        }
        result
    }

    pub fn with_group_visible<T>(&mut self, action: impl FnOnce(&mut CheckState) -> T) -> T {
        let undefined = mem::take(&mut self.undefined_members);
        let result = action(self);
        self.undefined_members = undefined;
        result
    }

    pub fn givens_in_scope(&self, scope: Option<GivenScopeId>) -> Vec<Given> {
        let mut givens = Vec::new();
        let mut current = scope;
        while let Some(scope_id) = current {
            let scope = &self.given_scopes[scope_id];
            givens.extend(scope.givens.iter().map(Given::clone));
            current = scope.parent;
        }
        givens
    }
}
