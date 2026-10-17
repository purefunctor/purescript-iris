use std::sync::Arc;

use functional::tree::Origin;
use line_index::{LineIndex, WideEncoding};
use rustc_hash::FxHashMap;
use stabilizing::StabilizedModule;
use syntax::{SyntaxElement, SyntaxKind, SyntaxNode, TextSize};

use crate::module::Module;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OriginMapping {
    pub(crate) generated: Position,
    pub(crate) origin: Option<Origin>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceMapping {
    pub generated: Position,
    pub source: Option<Position>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceMap {
    mappings: Arc<[SourceMapping]>,
}

impl SourceMap {
    pub fn mappings(&self) -> &[SourceMapping] {
        &self.mappings
    }
}

pub fn source_map(
    module: &Module,
    root: &SyntaxNode,
    stabilized: &StabilizedModule,
    line_index: &LineIndex,
) -> SourceMap {
    let mut positions = FxHashMap::default();
    let mut mappings = Vec::<SourceMapping>::new();
    for mapping in module.files().index_mappings() {
        let source = mapping.origin.and_then(|origin| {
            *positions
                .entry(origin)
                .or_insert_with(|| origin_position(origin, root, stabilized, line_index))
        });
        let continues_previous = mappings.last().is_some_and(|previous| {
            previous.generated.line == mapping.generated.line && previous.source == source
        });
        if !continues_previous {
            mappings.push(SourceMapping { generated: mapping.generated, source });
        }
    }
    SourceMap { mappings: mappings.into() }
}

fn origin_position(
    origin: Origin,
    root: &SyntaxNode,
    stabilized: &StabilizedModule,
    line_index: &LineIndex,
) -> Option<Position> {
    let node = origin.syntax_ptr(stabilized)?.try_to_node(root)?;
    let offset = first_token_start(&node)?;
    let position = line_index.to_wide(WideEncoding::Utf16, line_index.line_col(offset))?;
    Some(Position { line: position.line, column: position.col })
}

fn first_token_start(node: &SyntaxNode) -> Option<TextSize> {
    node.children_with_tokens().find_map(|child| match child {
        SyntaxElement::Node(node) if node.kind() == SyntaxKind::Annotation => None,
        SyntaxElement::Node(node) => first_token_start(&node),
        SyntaxElement::Token(token) if token.text_range().is_empty() => None,
        SyntaxElement::Token(token) => Some(token.text_range().start()),
    })
}
