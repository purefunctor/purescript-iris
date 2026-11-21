use std::sync::Arc;

use building_types::Severity;
use lexing::{Lexed, Position};
use smol_str::{SmolStr, ToSmolStr};
use syntax::ast::AstNode;
use syntax::{SyntaxKind, SyntaxNode, TreeOwner, cst};

mod builder;
mod parser;

#[derive(Debug, PartialEq, Eq)]
pub struct ParseError {
    pub offset: usize,
    pub position: Position,
    pub message: Arc<str>,
}

impl ParseError {
    pub fn severity(&self) -> Severity {
        Severity::Error
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedModule {
    owner: Arc<TreeOwner>,
}

impl ParsedModule {
    pub(crate) fn new(owner: Arc<TreeOwner>) -> ParsedModule {
        ParsedModule { owner }
    }

    pub fn syntax_node(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.owner.clone())
    }

    pub fn cst(&self) -> cst::Module {
        let node = self.syntax_node();
        cst::Module::cast(node).expect("invariant violated: expected cst::Module")
    }

    pub fn module_name(&self, source: &str) -> Option<SmolStr> {
        Some(self.cst().header()?.name()?.syntax().text(source).to_smolstr())
    }
}

pub type FullParsedModule = (ParsedModule, Arc<[ParseError]>);

pub fn parse(lexed: &Lexed<'_>, tokens: &[SyntaxKind]) -> FullParsedModule {
    let mut parser = parser::Parser::new(tokens);
    parser::module(&mut parser);

    let output = parser.finish();
    let (parsed, errors) = builder::build(lexed, output);

    (parsed, Arc::from(errors))
}
