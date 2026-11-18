use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::Arc;

pub use text_size::{TextRange, TextSize};

use crate::SyntaxKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ElementCategory {
    Node,
    Token,
}

#[derive(Debug, PartialEq, Eq)]
struct Element {
    kind: SyntaxKind,
    category: ElementCategory,
    range: TextRange,
    subtree_end: u32,
    parent: u32,
}

const NO_PARENT: u32 = u32::MAX;

pub struct TreeBuilder {
    elements: Vec<Element>,
    offset: TextSize,
    parent: u32,
}

impl TreeBuilder {
    pub fn with_capacity(capacity: usize) -> TreeBuilder {
        let elements = Vec::with_capacity(capacity);
        TreeBuilder { elements, offset: TextSize::new(0), parent: NO_PARENT }
    }

    pub fn open(&mut self, kind: SyntaxKind) {
        self.parent = self.push(kind, ElementCategory::Node, self.offset);
    }

    pub fn token(&mut self, kind: SyntaxKind, length: u32) {
        let end = self
            .offset
            .checked_add(length.into())
            .expect("invariant violated: syntax tree byte span exceeds u32");
        self.push(kind, ElementCategory::Token, end);
        self.offset = end;
    }

    pub fn close(&mut self) {
        let subtree_end = self.elements.len() as u32;
        let node = self
            .elements
            .get_mut(self.parent as usize)
            .expect("invariant violated: syntax tree must be balanced");
        node.range = TextRange::new(node.range.start(), self.offset);
        node.subtree_end = subtree_end;
        self.parent = node.parent;
    }

    pub fn build(self) -> Arc<TreeOwner> {
        let root = self.elements.first().expect("invariant violated: syntax tree must have a root");
        assert!(
            self.parent == NO_PARENT
                && root.category == ElementCategory::Node
                && root.subtree_end as usize == self.elements.len(),
            "invariant violated: syntax tree must be balanced under a single root node"
        );
        Arc::new(TreeOwner { elements: self.elements.into_boxed_slice() })
    }

    fn push(&mut self, kind: SyntaxKind, category: ElementCategory, end: TextSize) -> u32 {
        let subtree_end = u32::try_from(self.elements.len() + 1)
            .expect("invariant violated: syntax tree exceeds u32 elements");
        let id = subtree_end - 1;
        let range = TextRange::new(self.offset, end);
        self.elements.push(Element { kind, category, range, subtree_end, parent: self.parent });
        id
    }
}

/// Owns the textless syntax structure shared by syntax handles.
///
/// Structural equality deliberately ignores source text. Consumers which derive
/// values from [`SyntaxNode::text`] or [`SyntaxToken::text`] must also track the
/// corresponding source as an input.
#[derive(Debug)]
pub struct TreeOwner {
    elements: Box<[Element]>,
}

impl TreeOwner {
    fn at(&self, id: u32) -> &Element {
        &self.elements[id as usize]
    }

    fn parent(&self, id: u32) -> Option<u32> {
        let parent = self.at(id).parent;
        (parent != NO_PARENT).then_some(parent)
    }

    fn subtree(&self, id: u32) -> Range<u32> {
        id..self.at(id).subtree_end
    }

    fn first_child(&self, id: u32) -> Option<u32> {
        let child = id + 1;
        (child < self.at(id).subtree_end).then_some(child)
    }

    fn next_sibling(&self, id: u32) -> Option<u32> {
        let parent = self.parent(id)?;
        self.next_sibling_within(id, self.at(parent).subtree_end)
    }

    fn next_sibling_within(&self, id: u32, parent_subtree_end: u32) -> Option<u32> {
        let sibling = self.at(id).subtree_end;
        (sibling < parent_subtree_end).then_some(sibling)
    }

    fn prev_sibling(&self, id: u32) -> Option<u32> {
        let parent = self.parent(id)?;
        if parent + 1 == id {
            return None;
        }
        let mut sibling = id - 1;
        while self.at(sibling).parent != parent {
            sibling = self.at(sibling).parent;
        }
        Some(sibling)
    }

    fn children(&self, id: u32) -> impl Iterator<Item = u32> {
        let subtree_end = self.at(id).subtree_end;
        std::iter::successors(self.first_child(id), move |&child| {
            self.next_sibling_within(child, subtree_end)
        })
    }
}

impl PartialEq for TreeOwner {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other) || self.elements == other.elements
    }
}

impl Eq for TreeOwner {}

#[derive(Clone)]
pub struct SyntaxNode {
    owner: Arc<TreeOwner>,
    id: u32,
}

#[derive(Clone)]
pub struct SyntaxToken {
    owner: Arc<TreeOwner>,
    id: u32,
}

macro_rules! handle_impls {
    ($ty:ident) => {
        impl PartialEq for $ty {
            fn eq(&self, other: &Self) -> bool {
                Arc::ptr_eq(&self.owner, &other.owner) && self.id == other.id
            }
        }

        impl Eq for $ty {}

        impl Hash for $ty {
            fn hash<H: Hasher>(&self, state: &mut H) {
                Arc::as_ptr(&self.owner).hash(state);
                self.id.hash(state);
            }
        }
    };
}

handle_impls!(SyntaxNode);
handle_impls!(SyntaxToken);

fn element(owner: &Arc<TreeOwner>, id: u32) -> SyntaxElement {
    match owner.at(id).category {
        ElementCategory::Node => SyntaxNode { owner: Arc::clone(owner), id }.into(),
        ElementCategory::Token => SyntaxToken { owner: Arc::clone(owner), id }.into(),
    }
}

impl SyntaxNode {
    pub fn new_root(owner: Arc<TreeOwner>) -> SyntaxNode {
        SyntaxNode { owner, id: 0 }
    }

    pub fn owner(&self) -> Arc<TreeOwner> {
        Arc::clone(&self.owner)
    }

    pub fn kind(&self) -> SyntaxKind {
        self.owner.at(self.id).kind
    }

    pub fn text_range(&self) -> TextRange {
        self.owner.at(self.id).range
    }

    pub fn text<'a>(&self, source: &'a str) -> &'a str {
        let range = self.text_range();
        let start: usize = range.start().into();
        let end: usize = range.end().into();
        &source[start..end]
    }

    pub fn parent(&self) -> Option<SyntaxNode> {
        let id = self.owner.parent(self.id)?;
        Some(SyntaxNode { owner: Arc::clone(&self.owner), id })
    }

    pub fn ancestors(&self) -> impl Iterator<Item = SyntaxNode> {
        std::iter::successors(Some(self.clone()), SyntaxNode::parent)
    }

    pub fn parent_ancestors(&self) -> impl Iterator<Item = SyntaxNode> {
        std::iter::successors(self.parent(), SyntaxNode::parent)
    }

    pub fn children(&self) -> SyntaxNodeChildren {
        SyntaxNodeChildren {
            owner: Arc::clone(&self.owner),
            next: self.owner.first_child(self.id),
            subtree_end: self.owner.at(self.id).subtree_end,
        }
    }

    pub fn children_with_tokens(&self) -> SyntaxElementChildren {
        SyntaxElementChildren {
            owner: Arc::clone(&self.owner),
            next: self.owner.first_child(self.id),
            subtree_end: self.owner.at(self.id).subtree_end,
        }
    }

    pub(crate) fn token(&self, kind: SyntaxKind) -> Option<SyntaxToken> {
        let id = self.owner.children(self.id).find(|&id| {
            let child = self.owner.at(id);
            child.category == ElementCategory::Token && child.kind == kind
        })?;
        Some(SyntaxToken { owner: Arc::clone(&self.owner), id })
    }

    pub(crate) fn token_in_set(&self, set: crate::TokenSet) -> Option<SyntaxToken> {
        let id = self.owner.children(self.id).find(|&id| {
            let child = self.owner.at(id);
            child.category == ElementCategory::Token && set.contains(child.kind)
        })?;
        Some(SyntaxToken { owner: Arc::clone(&self.owner), id })
    }

    pub fn first_child(&self) -> Option<SyntaxNode> {
        let id = self
            .owner
            .children(self.id)
            .find(|&id| self.owner.at(id).category == ElementCategory::Node)?;
        Some(SyntaxNode { owner: Arc::clone(&self.owner), id })
    }

    pub fn first_child_or_token(&self) -> Option<SyntaxElement> {
        let id = self.owner.first_child(self.id)?;
        Some(element(&self.owner, id))
    }

    pub fn split_leading_annotation(&self) -> (Option<TextRange>, Option<TextRange>) {
        let owner = &self.owner;
        let first = owner.first_child(self.id);
        let annotation = first.filter(|&child| owner.at(child).kind == SyntaxKind::Annotation);
        let start = if annotation.is_some() {
            first.and_then(|child| owner.next_sibling(child))
        } else {
            first
        };
        let children = start.map(|start| {
            TextRange::new(owner.at(start).range.start(), owner.at(self.id).range.end())
        });
        (annotation.map(|annotation| owner.at(annotation).range), children)
    }

    pub fn first_token(&self) -> Option<SyntaxToken> {
        let id = self
            .owner
            .subtree(self.id)
            .find(|&id| self.owner.at(id).category == ElementCategory::Token)?;
        Some(SyntaxToken { owner: Arc::clone(&self.owner), id })
    }

    pub fn next_sibling_or_token(&self) -> Option<SyntaxElement> {
        let id = self.owner.next_sibling(self.id)?;
        Some(element(&self.owner, id))
    }

    pub fn prev_sibling_or_token(&self) -> Option<SyntaxElement> {
        let id = self.owner.prev_sibling(self.id)?;
        Some(element(&self.owner, id))
    }

    pub fn next_sibling(&self) -> Option<SyntaxNode> {
        node_sibling(self, true)
    }

    pub fn prev_sibling(&self) -> Option<SyntaxNode> {
        node_sibling(self, false)
    }

    pub fn preorder(&self) -> Preorder {
        Preorder(Walk::new(self))
    }

    /// Returns compact node metadata in preorder without materializing token events.
    pub fn preorder_pointers(&self) -> PreorderPointers {
        PreorderPointers { owner: Arc::clone(&self.owner), ids: self.owner.subtree(self.id) }
    }

    pub fn preorder_with_tokens(&self) -> PreorderWithTokens {
        PreorderWithTokens(Walk::new(self))
    }

    pub fn tokens(&self) -> SyntaxTokens {
        SyntaxTokens { owner: Arc::clone(&self.owner), ids: self.owner.subtree(self.id) }
    }

    pub fn token_at_offset(&self, offset: TextSize) -> TokenAtOffset<SyntaxToken> {
        token_at_offset(self, offset)
    }

    pub fn debug<'a>(&'a self, source: &'a str) -> DebugSyntax<'a> {
        DebugSyntax { node: self, source }
    }
}

impl SyntaxToken {
    pub fn kind(&self) -> SyntaxKind {
        self.owner.at(self.id).kind
    }

    pub fn text_range(&self) -> TextRange {
        self.owner.at(self.id).range
    }

    pub fn text<'a>(&self, source: &'a str) -> &'a str {
        let range = self.text_range();
        let start: usize = range.start().into();
        let end: usize = range.end().into();
        &source[start..end]
    }

    pub fn parent(&self) -> SyntaxNode {
        let id = self
            .owner
            .parent(self.id)
            .expect("invariant violated: syntax token must have a parent");
        SyntaxNode { owner: Arc::clone(&self.owner), id }
    }

    pub fn parent_ancestors(&self) -> impl Iterator<Item = SyntaxNode> {
        std::iter::successors(Some(self.parent()), SyntaxNode::parent)
    }

    pub fn next_sibling_or_token(&self) -> Option<SyntaxElement> {
        let id = self.owner.next_sibling(self.id)?;
        Some(element(&self.owner, id))
    }

    pub fn prev_sibling_or_token(&self) -> Option<SyntaxElement> {
        let id = self.owner.prev_sibling(self.id)?;
        Some(element(&self.owner, id))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SyntaxElement {
    Node(SyntaxNode),
    Token(SyntaxToken),
}

impl SyntaxElement {
    pub fn into_node(self) -> Option<SyntaxNode> {
        if let SyntaxElement::Node(value) = self { Some(value) } else { None }
    }

    pub fn into_token(self) -> Option<SyntaxToken> {
        if let SyntaxElement::Token(value) = self { Some(value) } else { None }
    }

    pub fn as_node(&self) -> Option<&SyntaxNode> {
        if let SyntaxElement::Node(value) = self { Some(value) } else { None }
    }

    pub fn as_token(&self) -> Option<&SyntaxToken> {
        if let SyntaxElement::Token(value) = self { Some(value) } else { None }
    }

    pub fn kind(&self) -> SyntaxKind {
        match self {
            SyntaxElement::Node(n) => n.kind(),
            SyntaxElement::Token(t) => t.kind(),
        }
    }

    pub fn text_range(&self) -> TextRange {
        match self {
            SyntaxElement::Node(n) => n.text_range(),
            SyntaxElement::Token(t) => t.text_range(),
        }
    }
}

impl From<SyntaxNode> for SyntaxElement {
    fn from(value: SyntaxNode) -> Self {
        Self::Node(value)
    }
}

impl From<SyntaxToken> for SyntaxElement {
    fn from(value: SyntaxToken) -> Self {
        Self::Token(value)
    }
}

/// Iterates child nodes lazily, as most callers stop at the first match.
pub struct SyntaxNodeChildren {
    owner: Arc<TreeOwner>,
    next: Option<u32>,
    subtree_end: u32,
}

impl Iterator for SyntaxNodeChildren {
    type Item = SyntaxNode;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(id) = self.next {
            self.next = self.owner.next_sibling_within(id, self.subtree_end);
            if self.owner.at(id).category == ElementCategory::Node {
                return Some(SyntaxNode { owner: Arc::clone(&self.owner), id });
            }
        }
        None
    }
}

/// Iterates child elements lazily, as most callers stop at the first match.
pub struct SyntaxElementChildren {
    owner: Arc<TreeOwner>,
    next: Option<u32>,
    subtree_end: u32,
}

impl Iterator for SyntaxElementChildren {
    type Item = SyntaxElement;

    fn next(&mut self) -> Option<Self::Item> {
        let id = self.next?;
        self.next = self.owner.next_sibling_within(id, self.subtree_end);
        Some(element(&self.owner, id))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WalkEvent<T> {
    Enter(T),
    Leave(T),
}

impl<T> WalkEvent<T> {
    fn map<U>(self, transform: impl FnOnce(T) -> U) -> WalkEvent<U> {
        match self {
            WalkEvent::Enter(value) => WalkEvent::Enter(transform(value)),
            WalkEvent::Leave(value) => WalkEvent::Leave(transform(value)),
        }
    }
}

struct Walk {
    owner: Arc<TreeOwner>,
    root: u32,
    next: Option<WalkEvent<u32>>,
}

impl Walk {
    fn new(node: &SyntaxNode) -> Walk {
        Walk { owner: node.owner(), root: node.id, next: Some(WalkEvent::Enter(node.id)) }
    }
}

impl Iterator for Walk {
    type Item = WalkEvent<u32>;

    fn next(&mut self) -> Option<Self::Item> {
        let event = self.next.take()?;
        self.next = match event {
            WalkEvent::Enter(id) => match self.owner.first_child(id) {
                Some(child) => Some(WalkEvent::Enter(child)),
                None => Some(WalkEvent::Leave(id)),
            },
            WalkEvent::Leave(id) if id == self.root => None,
            WalkEvent::Leave(id) => match self.owner.next_sibling(id) {
                Some(sibling) => Some(WalkEvent::Enter(sibling)),
                None => Some(WalkEvent::Leave(self.owner.at(id).parent)),
            },
        };
        Some(event)
    }
}

pub struct Preorder(Walk);

impl Iterator for Preorder {
    type Item = WalkEvent<SyntaxNode>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let event = self.0.next()?;
            let owner = &self.0.owner;
            let (WalkEvent::Enter(id) | WalkEvent::Leave(id)) = event;
            if owner.at(id).category == ElementCategory::Node {
                return Some(event.map(|id| SyntaxNode { owner: Arc::clone(owner), id }));
            }
        }
    }
}

pub struct PreorderPointers {
    owner: Arc<TreeOwner>,
    ids: Range<u32>,
}

impl Iterator for PreorderPointers {
    type Item = SyntaxNodePtr;

    fn next(&mut self) -> Option<Self::Item> {
        let id = self.ids.find(|&id| self.owner.at(id).category == ElementCategory::Node)?;
        Some(SyntaxNodePtr::from_element(id, self.owner.at(id)))
    }
}

pub struct PreorderWithTokens(Walk);

impl Iterator for PreorderWithTokens {
    type Item = WalkEvent<SyntaxElement>;

    fn next(&mut self) -> Option<Self::Item> {
        let event = self.0.next()?;
        Some(event.map(|id| element(&self.0.owner, id)))
    }
}

pub struct SyntaxTokens {
    owner: Arc<TreeOwner>,
    ids: Range<u32>,
}

impl Iterator for SyntaxTokens {
    type Item = SyntaxToken;

    fn next(&mut self) -> Option<SyntaxToken> {
        let id = self.ids.find(|&id| self.owner.at(id).category == ElementCategory::Token)?;
        Some(SyntaxToken { owner: Arc::clone(&self.owner), id })
    }
}

fn node_sibling(node: &SyntaxNode, next: bool) -> Option<SyntaxNode> {
    let owner = &node.owner;
    let mut id = node.id;
    loop {
        id = if next { owner.next_sibling(id)? } else { owner.prev_sibling(id)? };
        if owner.at(id).category == ElementCategory::Node {
            return Some(SyntaxNode { owner: Arc::clone(owner), id });
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenAtOffset<T> {
    None,
    Single(T),
    Between(T, T),
}

impl<T> TokenAtOffset<T> {
    pub fn left_biased(self) -> Option<T> {
        match self {
            TokenAtOffset::None => None,
            TokenAtOffset::Single(token) | TokenAtOffset::Between(token, _) => Some(token),
        }
    }

    pub fn right_biased(self) -> Option<T> {
        match self {
            TokenAtOffset::None => None,
            TokenAtOffset::Single(token) | TokenAtOffset::Between(_, token) => Some(token),
        }
    }
}

fn token_at_offset(node: &SyntaxNode, offset: TextSize) -> TokenAtOffset<SyntaxToken> {
    let mut left = None;

    for id in node.owner.subtree(node.id) {
        let element = node.owner.at(id);
        if element.category != ElementCategory::Token {
            continue;
        }

        let range = element.range;
        if !range.is_empty() && range.start() == offset {
            return left.filter(|(_, range): &(u32, TextRange)| range.end() == offset).map_or_else(
                || TokenAtOffset::Single(SyntaxToken { owner: Arc::clone(&node.owner), id }),
                |(left, _)| {
                    TokenAtOffset::Between(
                        SyntaxToken { owner: Arc::clone(&node.owner), id: left },
                        SyntaxToken { owner: Arc::clone(&node.owner), id },
                    )
                },
            );
        }
        if range.contains(offset) {
            return TokenAtOffset::Single(SyntaxToken { owner: Arc::clone(&node.owner), id });
        }
        if !range.is_empty() && range.end() == offset {
            left = Some((id, range));
        }
    }

    left.map_or(TokenAtOffset::None, |(id, _)| {
        TokenAtOffset::Single(SyntaxToken { owner: Arc::clone(&node.owner), id })
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SyntaxNodePtr {
    id: u32,
    kind: SyntaxKind,
    range: TextRange,
}

impl SyntaxNodePtr {
    pub fn new(node: &SyntaxNode) -> SyntaxNodePtr {
        SyntaxNodePtr::from_element(node.id, node.owner.at(node.id))
    }

    fn from_element(id: u32, element: &Element) -> SyntaxNodePtr {
        SyntaxNodePtr { id, kind: element.kind, range: element.range }
    }

    pub fn kind(&self) -> SyntaxKind {
        self.kind
    }

    pub fn try_to_node(&self, root: &SyntaxNode) -> Option<SyntaxNode> {
        let element = root.owner.elements.get(self.id as usize)?;
        (element.category == ElementCategory::Node
            && element.kind == self.kind
            && element.range == self.range)
            .then(|| SyntaxNode { owner: Arc::clone(&root.owner), id: self.id })
    }

    pub fn to_node(&self, root: &SyntaxNode) -> SyntaxNode {
        self.try_to_node(root)
            .expect("invariant violated: syntax pointer does not belong to the supplied tree")
    }

    pub fn text_range(&self) -> TextRange {
        self.range
    }

    pub fn cast<N: crate::ast::AstNode>(self) -> Option<crate::ast::AstPtr<N>> {
        N::can_cast(self.kind).then(|| crate::ast::AstPtr::from_raw(self))
    }
}

pub struct DebugSyntax<'a> {
    node: &'a SyntaxNode,
    source: &'a str,
}

impl fmt::Debug for DebugSyntax<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn write(
            node: &SyntaxNode,
            source: &str,
            f: &mut fmt::Formatter<'_>,
            depth: usize,
        ) -> fmt::Result {
            writeln!(
                f,
                "{:indent$}{:?}@{:?}",
                "",
                node.kind(),
                node.text_range(),
                indent = depth * 2
            )?;
            for element in node.children_with_tokens() {
                match element {
                    SyntaxElement::Node(node) => write(&node, source, f, depth + 1)?,
                    SyntaxElement::Token(token) => {
                        let text = token.text(source);
                        if text.len() < 25 {
                            writeln!(
                                f,
                                "{:indent$}{:?}@{:?} {:?}",
                                "",
                                token.kind(),
                                token.text_range(),
                                text,
                                indent = (depth + 1) * 2
                            )?;
                        } else {
                            let end = (21..25).find(|&index| text.is_char_boundary(index)).unwrap();
                            let text = format!("{} ...", &text[..end]);
                            writeln!(
                                f,
                                "{:indent$}{:?}@{:?} {:?}",
                                "",
                                token.kind(),
                                token.text_range(),
                                text,
                                indent = (depth + 1) * 2
                            )?;
                        }
                    }
                }
            }
            Ok(())
        }
        write(self.node, self.source, f, 0)
    }
}

impl fmt::Debug for SyntaxNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SyntaxNode")
            .field("kind", &self.kind())
            .field("range", &self.text_range())
            .finish()
    }
}

impl fmt::Debug for SyntaxToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SyntaxToken")
            .field("kind", &self.kind())
            .field("range", &self.text_range())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root_with_boundary_tokens() -> SyntaxNode {
        let mut builder = TreeBuilder::with_capacity(4);
        builder.open(SyntaxKind::Module);
        builder.token(SyntaxKind::LOWER, 1);
        builder.token(SyntaxKind::LAYOUT_SEPARATOR, 0);
        builder.token(SyntaxKind::UPPER, 2);
        builder.close();

        SyntaxNode::new_root(builder.build())
    }

    #[test]
    fn token_at_offset_preserves_boundaries_around_empty_tokens() {
        let root = root_with_boundary_tokens();

        assert!(matches!(
            root.token_at_offset(TextSize::new(0)),
            TokenAtOffset::Single(token) if token.kind() == SyntaxKind::LOWER
        ));
        assert!(matches!(
            root.token_at_offset(TextSize::new(1)),
            TokenAtOffset::Between(left, right)
                if left.kind() == SyntaxKind::LOWER && right.kind() == SyntaxKind::UPPER
        ));
        assert!(matches!(
            root.token_at_offset(TextSize::new(2)),
            TokenAtOffset::Single(token) if token.kind() == SyntaxKind::UPPER
        ));
        assert!(matches!(
            root.token_at_offset(TextSize::new(3)),
            TokenAtOffset::Single(token) if token.kind() == SyntaxKind::UPPER
        ));
    }

    #[test]
    fn preorder_pointers_match_preorder_enter_events() {
        let root = root_with_boundary_tokens();
        let preorder = root.preorder().filter_map(|event| match event {
            WalkEvent::Enter(node) => Some(SyntaxNodePtr::new(&node)),
            WalkEvent::Leave(_) => None,
        });
        let pointers = root.preorder_pointers();

        assert!(preorder.eq(pointers));
    }

    #[test]
    fn tokens_preserve_empty_token_order_and_ranges() {
        let root = root_with_boundary_tokens();
        let tokens = root.tokens().map(|token| (token.kind(), token.text_range()));
        let tokens = tokens.collect::<Vec<_>>();

        assert_eq!(
            tokens,
            [
                (SyntaxKind::LOWER, TextRange::new(0.into(), 1.into())),
                (SyntaxKind::LAYOUT_SEPARATOR, TextRange::empty(1.into())),
                (SyntaxKind::UPPER, TextRange::new(1.into(), 3.into())),
            ]
        );
    }

    #[test]
    fn tokens_include_nested_tokens_and_stay_inside_the_subtree() {
        let mut builder = TreeBuilder::with_capacity(4);
        builder.open(SyntaxKind::Module);
        builder.open(SyntaxKind::ModuleHeader);
        builder.token(SyntaxKind::UPPER, 2);
        builder.close();
        builder.token(SyntaxKind::LOWER, 1);
        builder.close();
        let root = SyntaxNode::new_root(builder.build());

        let kinds = root.tokens().map(|token| token.kind()).collect::<Vec<_>>();
        assert_eq!(kinds, [SyntaxKind::UPPER, SyntaxKind::LOWER]);

        let header = root.first_child().unwrap();
        let kinds = header.tokens().map(|token| token.kind()).collect::<Vec<_>>();
        assert_eq!(kinds, [SyntaxKind::UPPER]);
    }

    #[test]
    fn direct_child_lookup_skips_tokens() {
        let root = root_with_boundary_tokens();

        assert!(root.first_child().is_none());
        assert_eq!(root.token(SyntaxKind::UPPER).unwrap().kind(), SyntaxKind::UPPER);
    }

    #[test]
    fn preorder_skips_tokens_and_stays_inside_the_subtree() {
        let mut builder = TreeBuilder::with_capacity(6);
        builder.open(SyntaxKind::Module);
        builder.token(SyntaxKind::LOWER, 1);
        builder.open(SyntaxKind::ModuleHeader);
        builder.token(SyntaxKind::UPPER, 2);
        builder.close();
        builder.open(SyntaxKind::ModuleImports);
        builder.close();
        builder.token(SyntaxKind::LOWER, 1);
        builder.close();
        let root = SyntaxNode::new_root(builder.build());

        let kinds = |node: &SyntaxNode| {
            let events = node.preorder().map(|event| event.map(|node| node.kind()));
            events.collect::<Vec<_>>()
        };

        assert_eq!(
            kinds(&root),
            [
                WalkEvent::Enter(SyntaxKind::Module),
                WalkEvent::Enter(SyntaxKind::ModuleHeader),
                WalkEvent::Leave(SyntaxKind::ModuleHeader),
                WalkEvent::Enter(SyntaxKind::ModuleImports),
                WalkEvent::Leave(SyntaxKind::ModuleImports),
                WalkEvent::Leave(SyntaxKind::Module),
            ]
        );
        assert_eq!(
            kinds(&root.first_child().unwrap()),
            [
                WalkEvent::Enter(SyntaxKind::ModuleHeader),
                WalkEvent::Leave(SyntaxKind::ModuleHeader),
            ]
        );
    }

    #[test]
    fn navigation_skips_subtrees_and_keeps_zero_width_siblings() {
        let mut builder = TreeBuilder::with_capacity(9);
        builder.open(SyntaxKind::Module);
        builder.open(SyntaxKind::Annotation);
        builder.token(SyntaxKind::TEXT, 2);
        builder.close();
        builder.open(SyntaxKind::ModuleHeader);
        builder.open(SyntaxKind::QualifiedName);
        builder.token(SyntaxKind::UPPER, 3);
        builder.close();
        builder.close();
        builder.token(SyntaxKind::ERROR, 0);
        builder.open(SyntaxKind::ModuleImports);
        builder.token(SyntaxKind::LOWER, 1);
        builder.close();
        builder.close();
        let root = SyntaxNode::new_root(builder.build());

        let children = root.children().collect::<Vec<_>>();
        let [annotation, header, imports] = children.as_slice() else { panic!("missing children") };
        let error = header.next_sibling_or_token().unwrap().into_token().unwrap();
        assert_eq!(error.kind(), SyntaxKind::ERROR);
        assert_eq!(error.text_range(), TextRange::empty(5.into()));
        assert_eq!(error.parent(), root);
        assert_eq!(annotation.next_sibling().as_ref(), Some(header));
        assert_eq!(header.next_sibling().as_ref(), Some(imports));
        assert_eq!(error.prev_sibling_or_token().unwrap().as_node(), Some(header));
        assert_eq!(imports.prev_sibling().as_ref(), Some(header));
        assert!(imports.next_sibling_or_token().is_none());
        assert_eq!(
            root.split_leading_annotation(),
            (Some(TextRange::new(0.into(), 2.into())), Some(TextRange::new(2.into(), 6.into())))
        );
        let pointer = SyntaxNodePtr::new(imports);
        assert_eq!(pointer.try_to_node(&root).as_ref(), Some(imports));
        assert!(pointer.try_to_node(&root_with_boundary_tokens()).is_none());
        assert!(matches!(
            root.token_at_offset(5.into()),
            TokenAtOffset::Between(left, right)
                if left.kind() == SyntaxKind::UPPER && right.kind() == SyntaxKind::LOWER
        ));
    }

    #[test]
    fn deep_tree_is_walked_without_recursion() {
        let depth = 32_768;
        let mut builder = TreeBuilder::with_capacity(depth + 1);
        for _ in 0..depth {
            builder.open(SyntaxKind::Module);
        }
        builder.token(SyntaxKind::LOWER, 3);
        for _ in 0..depth {
            builder.close();
        }
        let root = SyntaxNode::new_root(builder.build());

        let deepest = root.preorder_pointers().last().unwrap().to_node(&root);
        assert_eq!(deepest.parent_ancestors().count(), depth - 1);
        assert_eq!(root.preorder_with_tokens().count(), 2 * (depth + 1));
        assert_eq!(root.first_token().unwrap().text("abc"), "abc");
    }
}
