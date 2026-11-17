//! Scope graphs for local resolution.
//!
//! This module implements a [scope graph] for PureScript. Scope graphs are
//! a novel take on name resolution which allow resolution semantics to be
//! represented independent of the language using graphs and graph traversals.
//!
//! The scope graph is built while constructing the lowered source tree from the
//! CST. Local name resolution is performed eagerly and attached to that tree,
//! which simplifies associating information with resolved nodes. For instance,
//! knowing the type of a variable can be as easy as obtaining the type of a
//! [`BinderId`].
//!
//! [scope graph]: https://pl.ewi.tudelft.nl/research/projects/scope-graphs/
use std::ops;

use files::FileId;
use indexing::TermItemId;
use indexmap::IndexMap;
use la_arena::{Arena, Idx, RawIdx};
use rustc_hash::{FxBuildHasher, FxHashMap};
use smol_str::SmolStr;

use crate::source::*;
use crate::tree::LetBindingNameGroupId;

/// The result of resolving a term variable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TermVariableResolution {
    Binder(BinderId),
    Let(LetBindingNameGroupId),
    RecordPun(RecordPunId),
    Reference(FileId, TermItemId),
}

/// The result of resolving a type variable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeVariableResolution {
    Forall(TypeVariableBindingId),
    Implicit(ImplicitTypeVariable),
}

/// A locator for implicit type variables.
///
/// Unlike explicitly-quantified type variables, implicitly quantified
/// type variables do not have an [`AstId`] to anchor to. Instead, the
/// type variables themselves introduce the name into the scope.
///
/// Implicit type variables have multiple definition positions, much
/// like value signatures and equations. For example, both occurrences
/// of `f` here are equally valid definition positions:
///
/// ```purescript
/// instance Coerce (f a) (f b)
/// ```
///
/// We use the [`GraphNode::Implicit`] node to group these positions
/// together using the [`ImplicitBindings`], with the condition that
/// that we must store the [`GraphNodeId`] in this structure.
///
/// Rather than store [`SmolStr`] names directly, we intern them first
/// through the [`ImplicitBindings`] to keep the representation light
/// enough for use in the type checker.
///
/// [`AstId`]: stabilizing::AstId
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImplicitTypeVariable {
    /// Determines the 'position' of an implicit type variable.
    ///
    /// If [`true`], this type variable introduced the name into
    /// the scope. If [`false`], this is what the name resolves to.
    pub binding: bool,
    /// The id for the [`GraphNode::Implicit`] node.
    pub node: GraphNodeId,
    /// The id for the [`ImplicitBindings`] map.
    pub id: ImplicitBindingId,
}

/// See documentation for [`ImplicitTypeVariable`].
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ImplicitBindings {
    inner: IndexMap<SmolStr, Vec<TypeId>, FxBuildHasher>,
}

pub type ImplicitBindingId = Idx<SmolStr>;

impl ImplicitBindings {
    fn binding_id(index: usize) -> ImplicitBindingId {
        Idx::from_raw(RawIdx::from_u32(index as u32))
    }

    pub(crate) fn bind(&mut self, name: &SmolStr, id: TypeId) -> ImplicitBindingId {
        let entry = self.inner.entry(SmolStr::clone(name));
        let index = entry.index();
        entry.or_default().push(id);
        ImplicitBindings::binding_id(index)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &[TypeId])> {
        self.inner.iter().map(|(name, id)| (name.as_str(), id.as_slice()))
    }

    pub fn iter_indexed(&self) -> impl Iterator<Item = (&SmolStr, ImplicitBindingId)> + '_ {
        self.inner.iter().enumerate().map(|(index, (name, _))| {
            let binding_id = ImplicitBindings::binding_id(index);
            (name, binding_id)
        })
    }

    pub fn get(&self, name: &str) -> Option<ImplicitBindingId> {
        let (index, _, _) = self.inner.get_full(name)?;
        Some(ImplicitBindings::binding_id(index))
    }

    pub fn get_index(&self, index: ImplicitBindingId) -> Option<(&str, &[TypeId])> {
        let index = index.into_raw().into_u32() as usize;
        let (name, id) = self.inner.get_index(index)?;
        Some((name, id))
    }
}

/// A node in the [`LoweringGraph`].
#[derive(Debug, PartialEq, Eq)]
pub enum GraphNode {
    /// Names bound by patterns.
    Binder {
        parent: Option<GraphNodeId>,
        binders: FxHashMap<SmolStr, BinderId>,
        puns: FxHashMap<SmolStr, RecordPunId>,
    },
    /// Explicitly quantified type variables.
    Forall { parent: Option<GraphNodeId>, bindings: FxHashMap<SmolStr, TypeVariableBindingId> },
    /// Names bound by `let`.
    Let { parent: Option<GraphNodeId>, bindings: FxHashMap<SmolStr, LetBindingNameGroupId> },
    /// Implicitly quantified type variables.
    Implicit {
        parent: Option<GraphNodeId>,
        /// If this implicit scope is collecting type variables.
        collecting: bool,
        /// See documentation for [`ImplicitTypeVariable`].
        bindings: ImplicitBindings,
    },
}

pub type GraphNodeId = Idx<GraphNode>;

/// A scope graph for PureScript.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct LoweringGraph {
    pub(crate) inner: Arena<GraphNode>,
}

impl LoweringGraph {
    /// Initialise a traversal starting from a [`GraphNodeId`].
    pub fn traverse(&self, id: GraphNodeId) -> GraphIter<'_> {
        let inner = &self.inner;
        let next = Some(id);
        GraphIter { inner, next }
    }

    pub fn resolve_term(&self, id: GraphNodeId, name: &str) -> Option<TermVariableResolution> {
        let mut scopes = self.traverse(id);
        scopes.find_map(|(_, graph)| match graph {
            GraphNode::Binder { binders, puns, .. } => binders
                .get(name)
                .map(|id| TermVariableResolution::Binder(*id))
                .or_else(|| puns.get(name).map(|id| TermVariableResolution::RecordPun(*id))),
            GraphNode::Let { bindings, .. } => {
                bindings.get(name).map(|id| TermVariableResolution::Let(*id))
            }
            GraphNode::Forall { .. } | GraphNode::Implicit { .. } => None,
        })
    }

    pub fn resolve_type(&self, id: GraphNodeId, name: &str) -> Option<TypeVariableResolution> {
        let mut scopes = self.traverse(id);
        scopes.find_map(|(node, graph)| match graph {
            GraphNode::Forall { bindings, .. } => {
                bindings.get(name).copied().map(TypeVariableResolution::Forall)
            }
            GraphNode::Implicit { bindings, .. } => {
                let id = bindings.get(name)?;
                Some(TypeVariableResolution::Implicit(ImplicitTypeVariable {
                    binding: false,
                    node,
                    id,
                }))
            }
            GraphNode::Binder { .. } | GraphNode::Let { .. } => None,
        })
    }
}

impl ops::Index<GraphNodeId> for LoweringGraph {
    type Output = GraphNode;

    fn index(&self, index: GraphNodeId) -> &Self::Output {
        &self.inner[index]
    }
}

#[derive(Default, Debug, PartialEq, Eq)]
pub struct LoweringGraphNodes {
    pub(crate) binder_node: FxHashMap<BinderId, GraphNodeId>,
    pub(crate) expression_node: FxHashMap<ExpressionId, GraphNodeId>,
    pub(crate) record_pun_node: FxHashMap<RecordPunId, GraphNodeId>,
    pub(crate) type_node: FxHashMap<TypeId, GraphNodeId>,
    pub(crate) type_variable_binding_node: FxHashMap<TypeVariableBindingId, GraphNodeId>,
    pub(crate) let_node: FxHashMap<LetBindingNameGroupId, GraphNodeId>,
}

impl LoweringGraphNodes {
    pub fn binder_node(&self, id: BinderId) -> Option<GraphNodeId> {
        self.binder_node.get(&id).copied()
    }

    pub fn expression_node(&self, id: ExpressionId) -> Option<GraphNodeId> {
        self.expression_node.get(&id).copied()
    }

    pub fn record_pun_node(&self, id: RecordPunId) -> Option<GraphNodeId> {
        self.record_pun_node.get(&id).copied()
    }

    pub fn type_node(&self, id: TypeId) -> Option<GraphNodeId> {
        self.type_node.get(&id).copied()
    }

    pub fn type_variable_binding_node(&self, id: TypeVariableBindingId) -> Option<GraphNodeId> {
        self.type_variable_binding_node.get(&id).copied()
    }

    pub fn let_node(&self, id: LetBindingNameGroupId) -> Option<GraphNodeId> {
        self.let_node.get(&id).copied()
    }

    pub fn term_node(&self, resolution: TermVariableResolution) -> Option<GraphNodeId> {
        match resolution {
            TermVariableResolution::Binder(id) => self.binder_node(id),
            TermVariableResolution::Let(id) => self.let_node(id),
            TermVariableResolution::RecordPun(id) => self.record_pun_node(id),
            TermVariableResolution::Reference(_, _) => None,
        }
    }
}

/// An iterator that traverses the [`LoweringGraph`].
pub struct GraphIter<'a> {
    inner: &'a Arena<GraphNode>,
    next: Option<GraphNodeId>,
}

impl<'a> Iterator for GraphIter<'a> {
    type Item = (Idx<GraphNode>, &'a GraphNode);

    fn next(&mut self) -> Option<Self::Item> {
        let id = self.next.take()?;
        let item = &self.inner[id];
        match &item {
            GraphNode::Binder { parent, .. }
            | GraphNode::Forall { parent, .. }
            | GraphNode::Let { parent, .. }
            | GraphNode::Implicit { parent, .. } => {
                self.next = *parent;
            }
        };
        Some((id, item))
    }
}
