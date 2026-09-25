use std::marker::PhantomData;

use crate::{SyntaxKind, SyntaxNode, SyntaxNodePtr, SyntaxToken};

pub trait AstNode: Clone {
    fn can_cast(kind: SyntaxKind) -> bool;
    fn cast(node: SyntaxNode) -> Option<Self>;
    fn syntax(&self) -> &SyntaxNode;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AstPtr<N> {
    raw: SyntaxNodePtr,
    marker: PhantomData<fn() -> N>,
}

impl<N: AstNode> AstPtr<N> {
    pub(crate) fn from_raw(raw: SyntaxNodePtr) -> Self {
        Self { raw, marker: PhantomData }
    }

    pub fn new(node: &N) -> Self {
        Self { raw: SyntaxNodePtr::new(node.syntax()), marker: PhantomData }
    }

    pub fn to_node(&self, root: &SyntaxNode) -> N {
        N::cast(self.raw.to_node(root))
            .expect("invariant violated: AST pointer resolved to an unexpected node")
    }

    pub fn try_to_node(&self, root: &SyntaxNode) -> Option<N> {
        self.raw.try_to_node(root).and_then(N::cast)
    }

    pub fn syntax_node_ptr(&self) -> SyntaxNodePtr {
        self.raw
    }
}

pub struct AstChildren<N> {
    inner: crate::SyntaxNodeChildren,
    marker: PhantomData<N>,
}

impl<N: AstNode> AstChildren<N> {
    pub(crate) fn new(node: &SyntaxNode) -> Self {
        Self { inner: node.children(), marker: PhantomData }
    }
}

impl<N: AstNode> Iterator for AstChildren<N> {
    type Item = N;

    fn next(&mut self) -> Option<N> {
        self.inner.find_map(N::cast)
    }
}

pub mod support {
    use super::*;

    pub fn child<N: AstNode>(node: &SyntaxNode) -> Option<N> {
        let mut child = node.first_child();
        while let Some(node) = child {
            if let Some(node) = N::cast(node.clone()) {
                return Some(node);
            }
            child = node.next_sibling();
        }
        None
    }

    pub fn children<N: AstNode>(node: &SyntaxNode) -> AstChildren<N> {
        AstChildren::new(node)
    }

    pub fn token(node: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
        node.token(kind)
    }
}
