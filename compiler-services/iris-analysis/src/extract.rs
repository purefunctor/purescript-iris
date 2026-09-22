use files::FileId;
use indexing::{IndexedTermItemKind, IndexedTypeItemKind, TermItemId, TypeItemId};
use stabilizing::{AstId, StabilizedModule};
use syntax::ast::AstNode;
use syntax::{SyntaxKind, SyntaxNode, SyntaxNodePtr, TextRange};

use crate::{AnalyzerError, AnalyzerQueries};

#[derive(Debug, Default)]
pub struct AnnotationSyntaxRange {
    pub annotation: Option<TextRange>,
    pub syntax: Option<TextRange>,
}

impl AnnotationSyntaxRange {
    pub fn from_ptr(root: &SyntaxNode, ptr: &SyntaxNodePtr) -> AnnotationSyntaxRange {
        ptr.try_to_node(root).map(|node| Self::from_node(&node)).unwrap_or_default()
    }

    pub fn from_node(node: &SyntaxNode) -> AnnotationSyntaxRange {
        let mut children = node.children_with_tokens().peekable();

        let annotation = children.next_if(|child| {
            let kind = child.kind();
            matches!(kind, SyntaxKind::Annotation)
        });

        let start = children.peek().map(|child| child.text_range());
        let end = children.last().map(|child| child.text_range());

        let annotation = annotation.map(|child| child.text_range());
        let syntax = start.zip(end).map(|(start, end)| start.cover(end));

        AnnotationSyntaxRange { annotation, syntax }
    }
}

pub fn extract_annotation(source: &str, range: TextRange) -> String {
    let text = &source[usize::from(range.start())..usize::from(range.end())];

    let mut annotation = String::default();

    {
        let lines = text.lines().filter_map(|line| {
            let trimmed = line.trim_start_matches("-- |");
            if line != trimmed { Some(trimmed.trim_matches(' ')) } else { None }
        });

        let mut lines = lines.peekable();
        if let Some(line) = lines.next() {
            annotation.push_str(line);
        }

        lines.for_each(|line| {
            annotation.push('\n');
            annotation.push_str(line);
        });
    }

    annotation
}

pub fn extract_syntax(source: &str, range: TextRange) -> String {
    source[usize::from(range.start())..usize::from(range.end())].to_string()
}

impl AnnotationSyntaxRange {
    pub fn of_file(
        engine: &impl AnalyzerQueries,
        file_id: FileId,
    ) -> Result<AnnotationSyntaxRange, AnalyzerError> {
        let (parsed, _) = engine.parsed(file_id)?;

        let header = parsed.cst().header().ok_or(AnalyzerError::NonFatal)?;
        let header = header.syntax();

        let annotation = header
            .children()
            .find(|child| matches!(child.kind(), SyntaxKind::Annotation))
            .map(|annotation| annotation.text_range());

        let syntax = {
            let module_token = header
                .children_with_tokens()
                .find(|element| matches!(element.kind(), SyntaxKind::MODULE))
                .ok_or(AnalyzerError::NonFatal)?;
            let where_token = header
                .children_with_tokens()
                .find(|element| matches!(element.kind(), SyntaxKind::WHERE))
                .ok_or(AnalyzerError::NonFatal)?;

            let start = module_token.text_range().start();
            let end = where_token.text_range().end();

            Some(TextRange::new(start, end))
        };

        Ok(AnnotationSyntaxRange { annotation, syntax })
    }

    pub fn of_file_term(
        engine: &impl AnalyzerQueries,
        file_id: FileId,
        term_id: TermItemId,
    ) -> Result<AnnotationSyntaxRange, AnalyzerError> {
        let (parsed, _) = engine.parsed(file_id)?;
        let stabilized = engine.stabilized(file_id)?;
        let indexed = engine.indexed(file_id)?;

        let root = parsed.syntax_node();
        let item = &indexed.items[term_id];

        let range = match &item.kind {
            IndexedTermItemKind::ClassMember { id, .. } => {
                signature_equation_range(&stabilized, &root, &Some(*id), &Some(*id))
            }
            IndexedTermItemKind::Constructor { id, .. } => {
                signature_equation_range(&stabilized, &root, &Some(*id), &Some(*id))
            }
            IndexedTermItemKind::Foreign { id } => {
                signature_equation_range(&stabilized, &root, &Some(*id), &Some(*id))
            }
            IndexedTermItemKind::Operator { id } => {
                signature_equation_range(&stabilized, &root, &Some(*id), &Some(*id))
            }
            IndexedTermItemKind::Value { signature, equations } => {
                let equation = equations.first().copied();
                signature_equation_range(&stabilized, &root, signature, &equation)
            }
        };

        range.ok_or(AnalyzerError::NonFatal)
    }

    pub fn of_file_type(
        engine: &impl AnalyzerQueries,
        file_id: FileId,
        type_id: TypeItemId,
    ) -> Result<AnnotationSyntaxRange, AnalyzerError> {
        let (parsed, _) = engine.parsed(file_id)?;
        let stabilized = engine.stabilized(file_id)?;
        let indexed = engine.indexed(file_id)?;

        let root = parsed.syntax_node();
        let item = &indexed.items[type_id];

        let range = match &item.kind {
            IndexedTypeItemKind::Data { signature, equation, .. } => {
                signature_equation_range(&stabilized, &root, signature, equation)
            }
            IndexedTypeItemKind::Newtype { signature, equation, .. } => {
                signature_equation_range(&stabilized, &root, signature, equation)
            }
            IndexedTypeItemKind::Synonym { signature, equation, .. } => {
                signature_equation_range(&stabilized, &root, signature, equation)
            }
            IndexedTypeItemKind::Class { signature, declaration, .. } => {
                signature_equation_range(&stabilized, &root, signature, declaration)
            }
            IndexedTypeItemKind::Foreign { id, .. } => {
                signature_equation_range(&stabilized, &root, &Some(*id), &Some(*id))
            }
            IndexedTypeItemKind::Operator { id } => {
                signature_equation_range(&stabilized, &root, &Some(*id), &Some(*id))
            }
        };

        range.ok_or(AnalyzerError::NonFatal)
    }
}

fn signature_equation_range<S, E>(
    stabilized: &StabilizedModule,
    root: &SyntaxNode,
    signature: &Option<AstId<S>>,
    equation: &Option<AstId<E>>,
) -> Option<AnnotationSyntaxRange>
where
    S: AstNode,
    E: AstNode,
{
    let signature = signature.and_then(|id| {
        let ptr = stabilized.syntax_ptr(id)?;
        Some(AnnotationSyntaxRange::from_ptr(root, &ptr))
    });

    let equation = || {
        let id = equation.as_ref()?;
        let ptr = stabilized.syntax_ptr(*id)?;
        let range = AnnotationSyntaxRange::from_ptr(root, &ptr);
        Some(AnnotationSyntaxRange { syntax: None, ..range })
    };

    signature.or_else(equation)
}
