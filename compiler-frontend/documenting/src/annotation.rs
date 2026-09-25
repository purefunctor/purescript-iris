use stabilizing::StabilizedModule;
use syntax::ast::AstNode;
use syntax::{SyntaxElement, SyntaxKind, SyntaxNode, SyntaxNodePtr};

use indexing::{
    DataConstructorId, IndexedTermItem, IndexedTermItemKind, IndexedTypeItem, IndexedTypeItemKind,
};
use parsing::ParsedModule;
use stabilizing::AstId;

/// Resolves documentation for item nodes on demand, as only item nodes can
/// carry documentation that consumers observe.
pub struct Annotations<'a> {
    source: &'a str,
    root: &'a SyntaxNode,
}

impl<'a> Annotations<'a> {
    pub fn new(source: &'a str, root: &'a SyntaxNode) -> Annotations<'a> {
        Annotations { source, root }
    }

    fn documentation(&self, ptr: SyntaxNodePtr) -> Option<String> {
        let node = ptr.try_to_node(self.root)?;
        first_child_documentation(self.source, &node)
    }

    fn data_constructor_documentation(&self, ptr: SyntaxNodePtr) -> Option<String> {
        let node = ptr.try_to_node(self.root)?;
        data_constructor_documentation(self.source, &node)
    }
}

pub fn module_documentation(source: &str, parsed: &ParsedModule) -> String {
    parsed
        .cst()
        .header()
        .and_then(|header| header.annotation())
        .and_then(|annotation| annotation_documentation(source, annotation.syntax()))
        .unwrap_or_default()
}

pub fn term_documentation(
    stabilized: &StabilizedModule,
    annotations: &Annotations,
    item: &IndexedTermItem,
) -> String {
    match &item.kind {
        IndexedTermItemKind::ClassMember { id, .. } => {
            signature_equation_documentation(stabilized, annotations, &Some(*id), &Some(*id))
        }
        IndexedTermItemKind::Constructor { id, .. } => {
            data_constructor_item_documentation(stabilized, annotations, *id)
        }
        IndexedTermItemKind::Foreign { id } => {
            signature_equation_documentation(stabilized, annotations, &Some(*id), &Some(*id))
        }
        IndexedTermItemKind::Operator { id } => {
            signature_equation_documentation(stabilized, annotations, &Some(*id), &Some(*id))
        }
        IndexedTermItemKind::Value { signature, equations } => {
            let equation = equations.first().copied();
            signature_equation_documentation(stabilized, annotations, signature, &equation)
        }
    }
}

pub fn instance_documentation(
    stabilized: &StabilizedModule,
    annotations: &Annotations,
    id: indexing::InstanceId,
) -> String {
    signature_equation_documentation(stabilized, annotations, &Some(id), &Some(id))
}

pub fn derive_documentation(
    stabilized: &StabilizedModule,
    annotations: &Annotations,
    id: indexing::DeriveId,
) -> String {
    signature_equation_documentation(stabilized, annotations, &Some(id), &Some(id))
}

pub fn type_documentation(
    stabilized: &StabilizedModule,
    annotations: &Annotations,
    item: &IndexedTypeItem,
) -> String {
    match &item.kind {
        IndexedTypeItemKind::Data { signature, equation, .. } => {
            signature_equation_documentation(stabilized, annotations, signature, equation)
        }
        IndexedTypeItemKind::Newtype { signature, equation, .. } => {
            signature_equation_documentation(stabilized, annotations, signature, equation)
        }
        IndexedTypeItemKind::Synonym { signature, equation, .. } => {
            signature_equation_documentation(stabilized, annotations, signature, equation)
        }
        IndexedTypeItemKind::Class { signature, declaration, .. } => {
            signature_equation_documentation(stabilized, annotations, signature, declaration)
        }
        IndexedTypeItemKind::Foreign { id, .. } => {
            signature_equation_documentation(stabilized, annotations, &Some(*id), &Some(*id))
        }
        IndexedTypeItemKind::Operator { id } => {
            signature_equation_documentation(stabilized, annotations, &Some(*id), &Some(*id))
        }
    }
}

fn signature_equation_documentation<S, E>(
    stabilized: &StabilizedModule,
    annotations: &Annotations,
    signature: &Option<AstId<S>>,
    equation: &Option<AstId<E>>,
) -> String
where
    S: AstNode,
    E: AstNode,
{
    if let Some(id) = signature
        && let Some(ptr) = stabilized.syntax_ptr(*id)
        && let Some(documentation) = annotations.documentation(ptr)
        && !documentation.is_empty()
    {
        return documentation;
    }

    if let Some(id) = equation
        && let Some(ptr) = stabilized.syntax_ptr(*id)
    {
        return annotations.documentation(ptr).unwrap_or_default();
    }

    String::default()
}

fn data_constructor_item_documentation(
    stabilized: &StabilizedModule,
    annotations: &Annotations,
    id: DataConstructorId,
) -> String {
    stabilized
        .syntax_ptr(id)
        .and_then(|ptr| annotations.data_constructor_documentation(ptr))
        .unwrap_or_default()
}

fn data_constructor_documentation(source: &str, node: &SyntaxNode) -> Option<String> {
    if let Some(documentation) = first_child_documentation(source, node) {
        return Some(documentation);
    }

    let separator = node.prev_sibling_or_token()?;
    if !matches!(separator.kind(), SyntaxKind::EQUAL | SyntaxKind::PIPE) {
        return None;
    }

    let annotation = match separator {
        SyntaxElement::Node(node) => node.prev_sibling_or_token()?,
        SyntaxElement::Token(token) => token.prev_sibling_or_token()?,
    };
    match annotation {
        SyntaxElement::Node(node) => annotation_documentation(source, &node),
        SyntaxElement::Token(_) => None,
    }
}

fn first_child_documentation(source: &str, node: &SyntaxNode) -> Option<String> {
    let first_child = node.first_child_or_token()?;
    match first_child {
        SyntaxElement::Node(node) => annotation_documentation(source, &node),
        SyntaxElement::Token(_) => None,
    }
}

fn annotation_documentation(source: &str, node: &SyntaxNode) -> Option<String> {
    if !matches!(node.kind(), SyntaxKind::Annotation) {
        return None;
    }

    let text = node.first_token()?.text(source);
    extract_annotation(text)
}

fn documentation_line_content(line: &str) -> Option<&str> {
    let line = line.trim_start();
    let line = line.strip_prefix("--")?;
    let line = line.trim_start_matches(' ');
    let line = line.strip_prefix('|')?;
    let line = line.strip_prefix(' ').unwrap_or(line);
    let line = line.trim_end();
    Some(line)
}

fn extract_annotation(text: &str) -> Option<String> {
    let mut annotation = String::default();

    let lines = text.lines().filter_map(documentation_line_content);

    let mut lines = lines.peekable();
    {
        let line = lines.next()?;
        annotation.push_str(line);
    }

    lines.for_each(|line| {
        annotation.push('\n');
        annotation.push_str(line);
    });

    Some(annotation)
}

#[cfg(test)]
mod tests {
    use indexing::IndexedTermItemKind;

    use super::*;

    #[test]
    fn multiline_annotation_extraction_preserves_line_semantics() {
        let text = concat!(
            "  -- | First line.  \n",
            "    -- |  Indented second line.\n",
            "    -- Ordinary comment.\n",
            "    -- | Third line.\n",
        );

        let documentation = extract_annotation(text);

        assert_eq!(
            documentation.as_deref(),
            Some("First line.\n Indented second line.\nThird line.")
        );
    }

    #[test]
    fn value_equation_documentation_used_when_signature_has_no_documentation() {
        let source = r#"module Main where

value :: Int
-- | Equation documentation.
value = 1
"#;

        let lexed = lexing::lex(source);
        let tokens = lexing::layout(&lexed);
        let (parsed, errors) = parsing::parse(&lexed, &tokens);
        assert!(errors.is_empty());

        let root = parsed.syntax_node();
        let cst = parsed.cst();

        let stabilized = stabilizing::stabilize_module(&root);
        let indexed = indexing::index_module(source, &cst, &stabilized);
        let annotations = Annotations::new(source, &root);

        let id = indexed.names.terms.lookup("value").unwrap();
        let item = &indexed.items[id];
        assert!(matches!(item.kind, IndexedTermItemKind::Value { .. }));

        let documentation = term_documentation(&stabilized, &annotations, item);
        assert_eq!(documentation, "Equation documentation.");
    }

    #[test]
    fn data_constructor_documentation_before_separators() {
        let source = r#"module Main where

data Maybe a
  -- | `Nothing` is `null`.
  = Nothing
  -- | `Just x` is the non-null value `x`.
  | Just a
"#;

        let lexed = lexing::lex(source);
        let tokens = lexing::layout(&lexed);
        let (parsed, errors) = parsing::parse(&lexed, &tokens);
        assert!(errors.is_empty());

        let root = parsed.syntax_node();
        let cst = parsed.cst();

        let stabilized = stabilizing::stabilize_module(&root);
        let indexed = indexing::index_module(source, &cst, &stabilized);
        let annotations = Annotations::new(source, &root);

        let documentation = indexed.items.iter_terms().filter_map(|(_, item)| {
            if !matches!(item.kind, IndexedTermItemKind::Constructor { .. }) {
                return None;
            }

            let name = item.name.as_deref()?;
            let documentation = term_documentation(&stabilized, &annotations, item);

            Some((name.to_string(), documentation))
        });

        let documentation: Vec<_> = documentation.collect();

        insta::assert_debug_snapshot!(documentation, @r###"
        [
            (
                "Nothing",
                "`Nothing` is `null`.",
            ),
            (
                "Just",
                "`Just x` is the non-null value `x`.",
            ),
        ]
        "###);
    }
}
