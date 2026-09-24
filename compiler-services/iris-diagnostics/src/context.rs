use std::cell::RefCell;

use checking::CheckedModule;
use checking::core::TypeId;
use checking::core::pretty::{Pretty, PrettyState};
use checking::error::ErrorCrumb;
use indexing::IndexedModule;
use lowering::LoweredModule;
use stabilizing::StabilizedModule;
use syntax::ast::{AstNode, AstPtr};
use syntax::{SyntaxElement, SyntaxKind, SyntaxNode, SyntaxNodePtr, TextRange};

use crate::Span;

pub trait ExternalQueries: checking::ExternalQueries {
    fn lookup_checking_smol_str(&self, id: checking::core::SmolStrId) -> &smol_str::SmolStr;
}

impl<T> ExternalQueries for T
where
    T: checking::ExternalQueries + ?Sized,
{
    fn lookup_checking_smol_str(&self, id: checking::core::SmolStrId) -> &smol_str::SmolStr {
        self.lookup_smol_str(id)
    }
}

fn is_trivia(element: &SyntaxElement) -> bool {
    match element {
        SyntaxElement::Node(node) => matches!(node.kind(), SyntaxKind::Annotation),
        SyntaxElement::Token(token) => {
            token.text_range().is_empty()
                || matches!(token.kind(), SyntaxKind::LAYOUT_SEPARATOR | SyntaxKind::ELSE)
        }
    }
}

fn first_significant_range(node: &SyntaxNode) -> Option<TextRange> {
    for child in node.children_with_tokens() {
        if is_trivia(&child) {
            continue;
        }
        match child {
            SyntaxElement::Token(token) => return Some(token.text_range()),
            SyntaxElement::Node(node) => {
                if let Some(range) = first_significant_range(&node) {
                    return Some(range);
                }
            }
        }
    }
    None
}

fn last_significant_range(node: &SyntaxNode) -> Option<TextRange> {
    let mut significant = None;
    for child in node.children_with_tokens() {
        if is_trivia(&child) {
            continue;
        }
        match child {
            SyntaxElement::Token(token) => significant = Some(token.text_range()),
            SyntaxElement::Node(node) => {
                if let Some(range) = last_significant_range(&node) {
                    significant = Some(range);
                }
            }
        }
    }
    significant
}

fn significant_ranges(node: &SyntaxNode) -> Option<TextRange> {
    let start = first_significant_range(node)?;
    let end = last_significant_range(node)?;
    Some(start.cover(end))
}

fn pointers_span<Q, I>(context: &DiagnosticsContext<'_, Q>, iterator: I) -> Option<Span>
where
    Q: ExternalQueries,
    I: IntoIterator<Item = SyntaxNodePtr>,
    I::IntoIter: DoubleEndedIterator,
{
    let mut iter = iterator.into_iter();

    let start_ptr = iter.next()?;
    let end_ptr = iter.next_back().unwrap_or(start_ptr);

    let start_span = context.span_from_syntax_ptr(&start_ptr)?;
    let end_span = context.span_from_syntax_ptr(&end_ptr)?;

    Some(Span::new(start_span.start, end_span.end))
}

pub struct DiagnosticsContext<'a, Q>
where
    Q: ExternalQueries,
{
    pub queries: &'a Q,
    pub content: &'a str,
    pub root: &'a SyntaxNode,
    pub stabilized: &'a StabilizedModule,
    pub indexed: &'a IndexedModule,
    pub lowered: &'a LoweredModule,
    pub checked: &'a CheckedModule,
    pretty_state: RefCell<PrettyState<'a, Q>>,
}

impl<'a, Q> DiagnosticsContext<'a, Q>
where
    Q: ExternalQueries,
{
    pub fn new(
        queries: &'a Q,
        content: &'a str,
        root: &'a SyntaxNode,
        stabilized: &'a StabilizedModule,
        indexed: &'a IndexedModule,
        lowered: &'a LoweredModule,
        checked: &'a CheckedModule,
    ) -> DiagnosticsContext<'a, Q> {
        DiagnosticsContext {
            queries,
            content,
            root,
            stabilized,
            indexed,
            lowered,
            checked,
            pretty_state: RefCell::new(Pretty::new(queries, checked).state()),
        }
    }

    pub fn render_type(&self, id: TypeId) -> smol_str::SmolStr {
        self.pretty_state.borrow_mut().render(id)
    }

    pub fn span_from_syntax_ptr(&self, ptr: &SyntaxNodePtr) -> Option<Span> {
        let node = ptr.try_to_node(self.root)?;
        self.span_from_syntax_node(&node)
    }

    pub fn span_from_ast_ptr<N: AstNode>(&self, ptr: &AstPtr<N>) -> Option<Span> {
        let node = ptr.try_to_node(self.root)?;
        self.span_from_syntax_node(node.syntax())
    }

    pub(crate) fn span_from_ast_ptr_child<N, C>(
        &self,
        ptr: &AstPtr<N>,
        child: impl FnOnce(&N) -> Option<C>,
    ) -> Option<Span>
    where
        N: AstNode,
        C: AstNode,
    {
        let node = ptr.try_to_node(self.root)?;
        let child = child(&node)?;
        self.span_from_syntax_node(child.syntax())
    }

    fn span_from_syntax_node(&self, node: &SyntaxNode) -> Option<Span> {
        let range = significant_ranges(node)?;
        Some(Span::new(range.start().into(), range.end().into()))
    }

    pub fn text_of(&self, span: Span) -> &'a str {
        &self.content[span.start as usize..span.end as usize]
    }

    pub fn module_span(&self) -> Span {
        let range = if let Some(range) = significant_ranges(self.root) {
            range
        } else {
            self.root.text_range()
        };

        let start = range.start().into();
        let end = range.end().into();

        Span::new(start, end)
    }

    pub fn span_from_error_crumb(&self, crumb: &ErrorCrumb) -> Option<Span> {
        let ptr = match crumb {
            ErrorCrumb::ConstructorArgument(id) => self.stabilized.syntax_ptr(*id)?,
            ErrorCrumb::InferringKind(id) | ErrorCrumb::CheckingKind(id) => {
                self.stabilized.syntax_ptr(*id)?
            }
            ErrorCrumb::InferringBinder(id) | ErrorCrumb::CheckingBinder(id) => {
                self.stabilized.syntax_ptr(*id)?
            }
            ErrorCrumb::InferringExpression(id) | ErrorCrumb::CheckingExpression(id) => {
                self.stabilized.syntax_ptr(*id)?
            }
            ErrorCrumb::TermDeclaration(id) => {
                self.indexed.term_item_ptr(self.stabilized, *id).next()?
            }
            ErrorCrumb::TypeDeclaration(id) => {
                self.indexed.type_item_ptr(self.stabilized, *id).next()?
            }
            ErrorCrumb::InstanceDeclaration(id) => {
                let source = self.indexed.items[*id].id;
                self.stabilized.syntax_ptr(source)?
            }
            ErrorCrumb::DeriveDeclaration(id) => {
                let source = self.indexed.items[*id].id;
                self.stabilized.syntax_ptr(source)?
            }
            ErrorCrumb::InferringDoBind(id)
            | ErrorCrumb::InferringDoDiscard(id)
            | ErrorCrumb::CheckingDoLet(id) => self.stabilized.syntax_ptr(*id)?,
            ErrorCrumb::InferringAdoMap(id)
            | ErrorCrumb::InferringAdoApply(id)
            | ErrorCrumb::CheckingAdoLet(id) => self.stabilized.syntax_ptr(*id)?,
            ErrorCrumb::CheckingLetName(id) => {
                let group = self.lowered.tree.get_let_binding_group(*id);

                let signature = group
                    .signature
                    .as_slice()
                    .iter()
                    .filter_map(|signature| self.stabilized.syntax_ptr(*signature));

                let equations = group
                    .equations
                    .as_ref()
                    .iter()
                    .filter_map(|equation| self.stabilized.syntax_ptr(*equation));

                return pointers_span(self, signature.chain(equations));
            }
        };
        self.span_from_syntax_ptr(&ptr)
    }

    pub fn primary_span_from_crumbs(&self, crumbs: &[ErrorCrumb]) -> Span {
        let primary = crumbs.iter().rev().find_map(|crumb| self.span_from_error_crumb(crumb));
        if let Some(primary) = primary { primary } else { self.module_span() }
    }
}
