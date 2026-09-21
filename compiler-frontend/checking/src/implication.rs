//! Implements the implication tree for constraint collection.

use std::collections::VecDeque;
use std::ops::{Index, IndexMut};
use std::sync::Arc;

use smol_str::SmolStr;

use crate::core::TypeId;
use crate::error::ErrorCrumb;
use crate::evidence::{EvidenceBinderId, EvidenceVarId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Patterns {
    pub patterns: Arc<[SmolStr]>,
    pub crumbs: Arc<[ErrorCrumb]>,
}

/// A unique identifier for an implication scope.
pub type ImplicationId = u32;

#[derive(Clone, Copy)]
pub struct GivenConstraint {
    pub constraint: TypeId,
    pub evidence: EvidenceBinderId,
}

#[derive(Clone)]
pub struct WantedConstraint {
    pub constraint: TypeId,
    pub evidence: EvidenceVarId,
    pub crumbs: Arc<[ErrorCrumb]>,
}

/// A node in the implication tree.
#[derive(Default)]
pub struct Implication {
    pub given: Vec<GivenConstraint>,
    pub wanted: VecDeque<WantedConstraint>,
    pub patterns: Vec<Patterns>,
    pub children: Vec<ImplicationId>,
    pub parent: Option<ImplicationId>,
}

impl Implication {
    pub fn new(parent: Option<ImplicationId>) -> Implication {
        Implication { parent, ..Implication::default() }
    }
}

/// Keeps track of implications for the type checker.
pub struct Implications {
    nodes: Vec<Implication>,
    current: ImplicationId,
}

impl Implications {
    pub fn new() -> Self {
        Implications { nodes: vec![Implication::new(None)], current: 0 }
    }

    pub fn current(&self) -> ImplicationId {
        self.current
    }

    pub fn current_mut(&mut self) -> &mut Implication {
        &mut self.nodes[self.current as usize]
    }

    pub fn push(&mut self) -> ImplicationId {
        let parent = self.current;
        let id = self.nodes.len() as ImplicationId;
        self.nodes.push(Implication::new(Some(parent)));
        self.nodes[parent as usize].children.push(id);
        self.current = id;
        id
    }

    pub fn pop(&mut self, implication: ImplicationId) {
        debug_assert_eq!(implication, self.current);
        let parent =
            self[implication].parent.expect("invariant violated: missing implication parent");
        self.current = parent;
    }
}

impl Default for Implications {
    fn default() -> Implications {
        Implications::new()
    }
}

impl Index<ImplicationId> for Implications {
    type Output = Implication;

    fn index(&self, index: ImplicationId) -> &Self::Output {
        &self.nodes[index as usize]
    }
}

impl IndexMut<ImplicationId> for Implications {
    fn index_mut(&mut self, index: ImplicationId) -> &mut Self::Output {
        &mut self.nodes[index as usize]
    }
}
