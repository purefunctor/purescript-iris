use std::cell::RefCell;
use std::sync::mpsc;
use std::time::Duration;

use syntax::ast::{self, AstNode};
use syntax::{SyntaxKind, SyntaxNode, SyntaxValue, TreeOwner, cst};

thread_local! {
    static REENTRANT_ROOT: RefCell<Option<SyntaxNode>> = const { RefCell::new(None) };
}

#[derive(Clone)]
struct ReentrantAstNode {
    node: SyntaxNode,
}

impl AstNode for ReentrantAstNode {
    fn can_cast(kind: SyntaxKind) -> bool {
        REENTRANT_ROOT.with(|root| {
            assert_eq!(root.borrow().as_ref().unwrap().kind(), SyntaxKind::Module);
        });
        kind == SyntaxKind::ModuleHeader
    }

    fn cast(node: SyntaxNode) -> Option<ReentrantAstNode> {
        Self::can_cast(node.kind()).then_some(ReentrantAstNode { node })
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.node
    }
}

#[derive(Clone)]
struct FallibleAstNode {
    node: SyntaxNode,
}

impl AstNode for FallibleAstNode {
    fn can_cast(kind: SyntaxKind) -> bool {
        matches!(kind, SyntaxKind::ModuleHeader | SyntaxKind::ModuleImports)
    }

    fn cast(node: SyntaxNode) -> Option<FallibleAstNode> {
        let kind = node.kind();
        (Self::can_cast(kind) && kind == SyntaxKind::ModuleImports)
            .then_some(FallibleAstNode { node })
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.node
    }
}

#[derive(Clone)]
struct DivergentAstNode {
    node: SyntaxNode,
}

impl AstNode for DivergentAstNode {
    fn can_cast(_kind: SyntaxKind) -> bool {
        false
    }

    fn cast(node: SyntaxNode) -> Option<DivergentAstNode> {
        (node.kind() == SyntaxKind::ModuleHeader).then_some(DivergentAstNode { node })
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.node
    }
}

fn root_with_children() -> SyntaxNode {
    let mut builder = syntree::Builder::new_with();
    builder.open(SyntaxValue::node(SyntaxKind::Module)).unwrap();
    builder.open(SyntaxValue::node(SyntaxKind::ModuleHeader)).unwrap();
    builder.close().unwrap();
    builder.open(SyntaxValue::node(SyntaxKind::ModuleImports)).unwrap();
    builder.close().unwrap();
    builder.close().unwrap();
    SyntaxNode::new_root(TreeOwner::new(builder.build().unwrap()))
}

#[test]
fn child_cast_callback_can_reenter_tree_metadata() {
    let root = root_with_children();
    let (sender, receiver) = mpsc::channel();

    let handle = std::thread::spawn(move || {
        REENTRANT_ROOT.with(|stored| *stored.borrow_mut() = Some(root.clone()));
        let child = ast::support::child::<ReentrantAstNode>(&root).unwrap();
        sender.send(child.syntax().kind()).unwrap();
    });

    let kind = receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("custom AST callback deadlocked while re-entering tree metadata");
    assert_eq!(kind, SyntaxKind::ModuleHeader);
    handle.join().unwrap();
}

#[test]
fn child_cast_continues_after_matching_kind_returns_none() {
    let root = root_with_children();

    let child = ast::support::child::<FallibleAstNode>(&root).unwrap();

    assert_eq!(child.syntax().kind(), SyntaxKind::ModuleImports);
}

#[test]
fn child_cast_does_not_filter_with_can_cast() {
    let root = root_with_children();

    let child = ast::support::child::<DivergentAstNode>(&root).unwrap();

    assert_eq!(child.syntax().kind(), SyntaxKind::ModuleHeader);
}

#[test]
fn generated_casts_validate_the_node_kind() {
    let root = root_with_children();
    let header = root.first_child().unwrap();

    assert!(cst::ModuleImports::cast(header.clone()).is_none());
    assert!(cst::Declaration::cast(header).is_none());
}
