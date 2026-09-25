use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use parking_lot::Mutex;
use syntree::pointer::PointerUsize;
pub use text_size::{TextRange, TextSize};

use crate::SyntaxKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ElementCategory {
    Node,
    Token,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SyntaxValue {
    pub kind: SyntaxKind,
    pub category: ElementCategory,
}

impl SyntaxValue {
    pub const fn node(kind: SyntaxKind) -> SyntaxValue {
        SyntaxValue { kind, category: ElementCategory::Node }
    }

    pub const fn token(kind: SyntaxKind) -> SyntaxValue {
        SyntaxValue { kind, category: ElementCategory::Token }
    }
}

pub type Syntree = syntree::Tree<SyntaxValue, syntree::FlavorDefault>;

/// Owns the textless syntax structure shared by syntax handles.
///
/// The tree is immutable here, but Syntree's replaceable node values make it
/// non-`Sync`; the mutex permits sharing it across parallel queries.
///
/// Structural equality deliberately ignores source text. Consumers which derive
/// values from [`SyntaxNode::text`] or [`SyntaxToken::text`] must also track the
/// corresponding source as an input.
#[derive(Debug)]
pub struct TreeOwner {
    tree: Mutex<Syntree>,
    root: PointerUsize,
}

impl TreeOwner {
    pub fn new(tree: Syntree) -> Arc<TreeOwner> {
        let root = tree.first().expect("invariant violated: syntax tree must have a root").id();
        Arc::new(TreeOwner { tree: Mutex::new(tree), root })
    }
}

impl PartialEq for TreeOwner {
    fn eq(&self, other: &Self) -> bool {
        if std::ptr::eq(self, other) {
            return true;
        }
        if std::ptr::from_ref(self) < std::ptr::from_ref(other) {
            let this = self.tree.lock();
            let other = other.tree.lock();
            *this == *other
        } else {
            let other = other.tree.lock();
            let this = self.tree.lock();
            *this == *other
        }
    }
}

impl Eq for TreeOwner {}

#[derive(Clone)]
pub struct SyntaxNode {
    owner: Arc<TreeOwner>,
    id: PointerUsize,
}

#[derive(Clone)]
pub struct SyntaxToken {
    owner: Arc<TreeOwner>,
    id: PointerUsize,
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

fn range(node: &syntree::Node<'_, SyntaxValue, syntree::FlavorDefault>) -> TextRange {
    TextRange::new(node.span().start.into(), node.span().end.into())
}

fn element(owner: &Arc<TreeOwner>, id: PointerUsize, value: SyntaxValue) -> SyntaxElement {
    match value.category {
        ElementCategory::Node => SyntaxNode { owner: Arc::clone(owner), id }.into(),
        ElementCategory::Token => SyntaxToken { owner: Arc::clone(owner), id }.into(),
    }
}

impl SyntaxNode {
    pub fn new_root(owner: Arc<TreeOwner>) -> SyntaxNode {
        let id = owner.root;
        SyntaxNode { owner, id }
    }

    pub fn owner(&self) -> Arc<TreeOwner> {
        Arc::clone(&self.owner)
    }

    pub fn kind(&self) -> SyntaxKind {
        self.owner.tree.lock().get(self.id).unwrap().value().kind
    }

    pub fn text_range(&self) -> TextRange {
        range(&self.owner.tree.lock().get(self.id).unwrap())
    }

    pub fn text<'a>(&self, source: &'a str) -> &'a str {
        let range = self.text_range();
        let start: usize = range.start().into();
        let end: usize = range.end().into();
        &source[start..end]
    }

    pub fn parent(&self) -> Option<SyntaxNode> {
        let id = self.owner.tree.lock().get(self.id)?.parent()?.id();
        Some(SyntaxNode { owner: Arc::clone(&self.owner), id })
    }

    pub fn ancestors(&self) -> impl Iterator<Item = SyntaxNode> {
        std::iter::successors(Some(self.clone()), SyntaxNode::parent)
    }

    pub fn parent_ancestors(&self) -> impl Iterator<Item = SyntaxNode> {
        std::iter::successors(self.parent(), SyntaxNode::parent)
    }

    pub fn children(&self) -> SyntaxNodeChildren {
        SyntaxNodeChildren { owner: Arc::clone(&self.owner), next: self.first_element_id() }
    }

    pub fn children_with_tokens(&self) -> SyntaxElementChildren {
        SyntaxElementChildren { owner: Arc::clone(&self.owner), next: self.first_element_id() }
    }

    fn first_element_id(&self) -> Option<PointerUsize> {
        let tree = self.owner.tree.lock();
        Some(tree.get(self.id)?.first()?.id())
    }

    pub(crate) fn token(&self, kind: SyntaxKind) -> Option<SyntaxToken> {
        let tree = self.owner.tree.lock();
        let token = tree.get(self.id)?.children().find(|child| {
            let value = child.value();
            value.category == ElementCategory::Token && value.kind == kind
        })?;
        Some(SyntaxToken { owner: Arc::clone(&self.owner), id: token.id() })
    }

    pub(crate) fn token_in_set(&self, set: crate::TokenSet) -> Option<SyntaxToken> {
        let tree = self.owner.tree.lock();
        let token = tree.get(self.id)?.children().find(|child| {
            let value = child.value();
            value.category == ElementCategory::Token && set.contains(value.kind)
        })?;
        Some(SyntaxToken { owner: Arc::clone(&self.owner), id: token.id() })
    }

    pub fn first_child(&self) -> Option<SyntaxNode> {
        let tree = self.owner.tree.lock();
        let child = tree
            .get(self.id)?
            .children()
            .find(|child| child.value().category == ElementCategory::Node)?;
        Some(SyntaxNode { owner: Arc::clone(&self.owner), id: child.id() })
    }

    pub fn first_token(&self) -> Option<SyntaxToken> {
        let tree = self.owner.tree.lock();
        let node = tree.get(self.id)?;
        let id =
            node.walk().inside().find(|node| node.value().category == ElementCategory::Token)?.id();
        Some(SyntaxToken { owner: Arc::clone(&self.owner), id })
    }

    pub fn next_sibling_or_token(&self) -> Option<SyntaxElement> {
        sibling(&self.owner, self.id, true)
    }

    pub fn prev_sibling_or_token(&self) -> Option<SyntaxElement> {
        sibling(&self.owner, self.id, false)
    }

    pub fn next_sibling(&self) -> Option<SyntaxNode> {
        node_sibling(self, true)
    }

    pub fn prev_sibling(&self) -> Option<SyntaxNode> {
        node_sibling(self, false)
    }

    pub fn preorder(&self) -> Preorder {
        let tree = self.owner.tree.lock();
        let mut events = Vec::new();
        collect_node_events(&self.owner, tree.get(self.id).unwrap(), &mut events);
        Preorder(events.into_iter())
    }

    /// Returns compact node metadata in preorder without materializing token events.
    pub fn preorder_pointers(&self) -> PreorderPointers {
        let tree = self.owner.tree.lock();
        let root = tree.get(self.id).unwrap();
        let pointers = root.walk().inside().filter_map(|node| {
            let value = node.value();
            (value.category == ElementCategory::Node).then(|| SyntaxNodePtr::from_raw(&node))
        });
        PreorderPointers(pointers.collect::<Vec<_>>().into_iter())
    }

    pub fn preorder_with_tokens(&self) -> PreorderWithTokens {
        let tree = self.owner.tree.lock();
        let mut events = Vec::new();
        collect_events(&self.owner, tree.get(self.id).unwrap(), &mut events);
        PreorderWithTokens(events.into_iter())
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
        self.owner.tree.lock().get(self.id).unwrap().value().kind
    }

    pub fn text_range(&self) -> TextRange {
        range(&self.owner.tree.lock().get(self.id).unwrap())
    }

    pub fn text<'a>(&self, source: &'a str) -> &'a str {
        let range = self.text_range();
        let start: usize = range.start().into();
        let end: usize = range.end().into();
        &source[start..end]
    }

    pub fn parent(&self) -> SyntaxNode {
        let id = self.owner.tree.lock().get(self.id).unwrap().parent().unwrap().id();
        SyntaxNode { owner: Arc::clone(&self.owner), id }
    }

    pub fn parent_ancestors(&self) -> impl Iterator<Item = SyntaxNode> {
        std::iter::successors(Some(self.parent()), SyntaxNode::parent)
    }

    pub fn next_sibling_or_token(&self) -> Option<SyntaxElement> {
        sibling(&self.owner, self.id, true)
    }

    pub fn prev_sibling_or_token(&self) -> Option<SyntaxElement> {
        sibling(&self.owner, self.id, false)
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
    next: Option<PointerUsize>,
}

impl Iterator for SyntaxNodeChildren {
    type Item = SyntaxNode;

    fn next(&mut self) -> Option<Self::Item> {
        let tree = self.owner.tree.lock();
        while let Some(id) = self.next {
            let node = tree.get(id)?;
            self.next = node.next().map(|node| node.id());
            if node.value().category == ElementCategory::Node {
                return Some(SyntaxNode { owner: Arc::clone(&self.owner), id });
            }
        }
        None
    }
}

/// Iterates child elements lazily, as most callers stop at the first match.
pub struct SyntaxElementChildren {
    owner: Arc<TreeOwner>,
    next: Option<PointerUsize>,
}

impl Iterator for SyntaxElementChildren {
    type Item = SyntaxElement;

    fn next(&mut self) -> Option<Self::Item> {
        let tree = self.owner.tree.lock();
        let node = tree.get(self.next?)?;
        self.next = node.next().map(|node| node.id());
        Some(element(&self.owner, node.id(), node.value()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WalkEvent<T> {
    Enter(T),
    Leave(T),
}

pub struct Preorder(std::vec::IntoIter<WalkEvent<SyntaxNode>>);

impl Iterator for Preorder {
    type Item = WalkEvent<SyntaxNode>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

pub struct PreorderPointers(std::vec::IntoIter<SyntaxNodePtr>);

impl Iterator for PreorderPointers {
    type Item = SyntaxNodePtr;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

pub struct PreorderWithTokens(std::vec::IntoIter<WalkEvent<SyntaxElement>>);

impl Iterator for PreorderWithTokens {
    type Item = WalkEvent<SyntaxElement>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

fn collect_events(
    owner: &Arc<TreeOwner>,
    node: syntree::Node<'_, SyntaxValue, syntree::FlavorDefault>,
    out: &mut Vec<WalkEvent<SyntaxElement>>,
) {
    let value = node.value();
    let current = element(owner, node.id(), value);
    out.push(WalkEvent::Enter(current.clone()));
    for child in node.children() {
        collect_events(owner, child, out);
    }
    out.push(WalkEvent::Leave(current));
}

fn collect_node_events(
    owner: &Arc<TreeOwner>,
    node: syntree::Node<'_, SyntaxValue, syntree::FlavorDefault>,
    out: &mut Vec<WalkEvent<SyntaxNode>>,
) {
    let current = (node.value().category == ElementCategory::Node)
        .then(|| SyntaxNode { owner: Arc::clone(owner), id: node.id() });
    if let Some(current) = &current {
        out.push(WalkEvent::Enter(current.clone()));
    }
    for child in node.children() {
        collect_node_events(owner, child, out);
    }
    if let Some(current) = current {
        out.push(WalkEvent::Leave(current));
    }
}

fn sibling(owner: &Arc<TreeOwner>, id: PointerUsize, next: bool) -> Option<SyntaxElement> {
    let tree = owner.tree.lock();
    let node = tree.get(id)?;
    let node = if next { node.next()? } else { node.prev()? };
    Some(element(owner, node.id(), node.value()))
}

fn node_sibling(node: &SyntaxNode, next: bool) -> Option<SyntaxNode> {
    let tree = node.owner.tree.lock();
    let mut sibling = tree.get(node.id)?;
    loop {
        sibling = if next { sibling.next()? } else { sibling.prev()? };
        if sibling.value().category == ElementCategory::Node {
            return Some(SyntaxNode { owner: Arc::clone(&node.owner), id: sibling.id() });
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
    let tree = node.owner.tree.lock();
    let root = tree.get(node.id).unwrap();
    let mut left = None;

    for raw in root.walk().inside() {
        if raw.value().category != ElementCategory::Token {
            continue;
        }

        let id = raw.id();
        let range = range(&raw);
        if !range.is_empty() && range.start() == offset {
            return left
                .filter(|(_, range): &(PointerUsize, TextRange)| range.end() == offset)
                .map_or_else(
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
    id: PointerUsize,
    kind: SyntaxKind,
    range: TextRange,
}

impl SyntaxNodePtr {
    pub fn new(node: &SyntaxNode) -> SyntaxNodePtr {
        let tree = node.owner.tree.lock();
        SyntaxNodePtr::from_raw(&tree.get(node.id).unwrap())
    }

    fn from_raw(node: &syntree::Node<'_, SyntaxValue, syntree::FlavorDefault>) -> SyntaxNodePtr {
        SyntaxNodePtr { id: node.id(), kind: node.value().kind, range: range(node) }
    }

    pub fn kind(&self) -> SyntaxKind {
        self.kind
    }

    pub fn try_to_node(&self, root: &SyntaxNode) -> Option<SyntaxNode> {
        let tree = root.owner.tree.lock();
        let node = tree.get(self.id)?;
        (node.value().category == ElementCategory::Node
            && node.value().kind == self.kind
            && range(&node) == self.range)
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
        let mut builder = syntree::Builder::new();
        let root = SyntaxValue::node(SyntaxKind::Module);
        builder.open(root).unwrap();
        let lower = SyntaxValue::token(SyntaxKind::LOWER);
        builder.token(lower, 1).unwrap();
        let separator = SyntaxValue::token(SyntaxKind::LAYOUT_SEPARATOR);
        builder.token_empty(separator).unwrap();
        let upper = SyntaxValue::token(SyntaxKind::UPPER);
        builder.token(upper, 2).unwrap();
        builder.close().unwrap();

        SyntaxNode::new_root(TreeOwner::new(builder.build().unwrap()))
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
    fn direct_child_lookup_skips_tokens() {
        let root = root_with_boundary_tokens();

        assert!(root.first_child().is_none());
        assert_eq!(root.token(SyntaxKind::UPPER).unwrap().kind(), SyntaxKind::UPPER);
    }

    #[test]
    fn preorder_finds_nodes_nested_under_raw_token_elements() {
        let mut builder = syntree::Builder::new();
        builder.open(SyntaxValue::node(SyntaxKind::Module)).unwrap();
        builder.open(SyntaxValue::token(SyntaxKind::TEXT)).unwrap();
        builder.open(SyntaxValue::node(SyntaxKind::ModuleHeader)).unwrap();
        builder.close().unwrap();
        builder.close().unwrap();
        builder.close().unwrap();
        let root = SyntaxNode::new_root(TreeOwner::new(builder.build().unwrap()));

        let events = root.preorder().map(|event| match event {
            WalkEvent::Enter(node) => WalkEvent::Enter(node.kind()),
            WalkEvent::Leave(node) => WalkEvent::Leave(node.kind()),
        });
        let events = events.collect::<Vec<_>>();

        assert_eq!(
            events,
            vec![
                WalkEvent::Enter(SyntaxKind::Module),
                WalkEvent::Enter(SyntaxKind::ModuleHeader),
                WalkEvent::Leave(SyntaxKind::ModuleHeader),
                WalkEvent::Leave(SyntaxKind::Module),
            ]
        );
    }
}
