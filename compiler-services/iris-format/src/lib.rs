//! Source-preserving PureScript formatting, independent of project configuration.

mod printer;
mod trivia;

use syntax::ast::AstNode;
use syntax::{SyntaxElement, SyntaxKind, SyntaxNode, cst};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FormatError {
    #[error("{line}:{column}: {message}")]
    InvalidSource { line: u32, column: u32, message: String },
    #[error("cannot format incomplete {0:?}")]
    IncompleteSyntax(SyntaxKind),
    #[error("cannot format {0:?}")]
    UnsupportedSyntax(SyntaxKind),
    #[error("unterminated block comment")]
    UnterminatedComment,
    #[error("formatter safety check failed: {0}; source was not changed")]
    SafetyCheck(&'static str),
}

/// Formats with two-space indentation and a soft limit of 100 columns.
///
/// Spelling, comments, and syntax must survive a round trip, and the result must
/// be a formatting fixed point. Invalid or unsafe output is never returned.
pub fn format_module(source: &str) -> Result<String, FormatError> {
    let original = parse(source)?;
    let formatted = printer::render(&original, source)?;
    let reparsed =
        parse(&formatted).map_err(|_| FormatError::SafetyCheck("output does not parse"))?;
    if structure(&original, source) != structure(&reparsed, &formatted) {
        return Err(FormatError::SafetyCheck("syntax changed"));
    }
    if lexical_content(source)? != lexical_content(&formatted)? {
        return Err(FormatError::SafetyCheck("tokens or comments changed"));
    }
    if printer::render(&reparsed, &formatted)? != formatted {
        return Err(FormatError::SafetyCheck("output is not idempotent"));
    }
    Ok(formatted)
}

fn parse(source: &str) -> Result<SyntaxNode, FormatError> {
    let lexed = lexing::lex(source);
    let tokens = lexing::layout(&lexed);
    let (parsed, errors) = parsing::parse(&lexed, &tokens);
    if let Some(error) = errors.first() {
        return Err(FormatError::InvalidSource {
            line: error.position.line,
            column: error.position.column,
            message: error.message.to_string(),
        });
    }
    let root = parsed.syntax_node();
    validate(&root)?;
    Ok(root)
}

fn validate(node: &SyntaxNode) -> Result<(), FormatError> {
    use SyntaxKind::*;
    // Recovery can leave required operands absent without a parser diagnostic.
    // Reprinting that same incomplete tree would otherwise pass the round-trip checks.
    let children = node.children().collect::<Vec<_>>();
    let count = |predicate: fn(SyntaxKind) -> bool| {
        children.iter().filter(|child| predicate(child.kind())).count()
    };
    let expressions = count(cst::Expression::can_cast);
    let types = count(cst::Type::can_cast);
    let binders = count(cst::Binder::can_cast);
    let has = |kind| children.iter().any(|child| child.kind() == kind);
    let first = node
        .children_with_tokens()
        .find(|child| child.kind() != Annotation && !child.kind().is_layout_token());
    let complete = match node.kind() {
        ExpressionOperatorChain | ExpressionInfixChain => {
            first.is_some_and(|child| cst::Expression::can_cast(child.kind()))
        }
        TypeOperatorChain => first.is_some_and(|child| cst::Type::can_cast(child.kind())),
        BinderOperatorChain => first.is_some_and(|child| cst::Binder::can_cast(child.kind())),
        NewtypeEquation => children
            .iter()
            .find(|child| child.kind() == DataConstructor)
            .is_some_and(|constructor| {
                constructor.children().filter(|child| cst::Type::can_cast(child.kind())).count()
                    == 1
            }),
        ValueSignature
        | LetBindingSignature
        | InstanceSignatureStatement
        | ClassMemberStatement
        | TypeSynonymSignature
        | ClassSignature
        | ForeignImportDataDeclaration
        | ForeignImportValueDeclaration
        | NewtypeSignature
        | DataSignature
        | TypeSynonymEquation
        | TypeRowItem
        | TypeRowTail
        | TypeParenthesized
        | TypeForall
        | TypeOperatorPair
        | ExpressionTypeArgument => types >= 1,
        TypeArrow | TypeConstrained | TypeKinded | TypeApplicationChain => types >= 2,
        TypeVariableBinding => {
            !node.children_with_tokens().any(|child| child.kind() == DOUBLE_COLON) || types == 1
        }
        ExpressionTyped => expressions == 1 && types == 1,
        BinderTyped => binders == 1 && types == 1,
        WhereExpression
        | ExpressionIf
        | ExpressionThen
        | ExpressionElse
        | ExpressionLetIn
        | ExpressionAdo
        | ExpressionNegate
        | ExpressionParenthesized
        | ExpressionTermArgument
        | ExpressionOperatorPair
        | ExpressionInfixPair
        | ExpressionTick
        | DoStatementDiscard
        | DoStatementBind
        | PatternGuardBinder
        | PatternGuardExpression
        | RecordUpdateLeaf => expressions >= 1,
        ExpressionLambda => expressions == 1 && has(FunctionBinders),
        ExpressionApplicationChain => expressions == 1,
        Unconditional | PatternGuarded | LetBindingPattern => has(WhereExpression),
        BinderNamed | BinderParenthesized | BinderOperatorPair => binders >= 1,
        RecordField => expressions + binders == 1,
        CaseTrunk => expressions >= 1,
        CaseBranchBinders => binders >= 1,
        ERROR => false,
        _ => true,
    };
    if !complete {
        return Err(FormatError::IncompleteSyntax(node.kind()));
    }
    for child in children {
        validate(&child)?;
    }
    Ok(())
}

#[derive(PartialEq, Eq)]
enum Structure<'s> {
    Open(SyntaxKind),
    Close,
    Token(SyntaxKind, &'s str),
}

fn structure<'s>(node: &SyntaxNode, source: &'s str) -> Vec<Structure<'s>> {
    fn visit<'s>(node: &SyntaxNode, source: &'s str, output: &mut Vec<Structure<'s>>) {
        if node.kind() == SyntaxKind::Annotation {
            return;
        }
        output.push(Structure::Open(node.kind()));
        for child in node.children_with_tokens() {
            match child {
                SyntaxElement::Node(node) => visit(&node, source, output),
                SyntaxElement::Token(token) if !token.kind().is_layout_token() => {
                    output.push(Structure::Token(token.kind(), token.text(source)));
                }
                SyntaxElement::Token(_) => {}
            }
        }
        output.push(Structure::Close);
    }
    let mut output = Vec::new();
    visit(node, source, &mut output);
    output
}

#[derive(PartialEq, Eq)]
struct LexicalContent<'s> {
    kind: SyntaxKind,
    qualifier: Option<&'s str>,
    text: &'s str,
    comments: Vec<&'s str>,
}

fn lexical_content(source: &str) -> Result<Vec<LexicalContent<'_>>, FormatError> {
    let lexed = lexing::lex(source);
    (0..lexed.len())
        .map(|index| {
            let info = lexed.info(index);
            let start = if index == 0 { 0 } else { lexed.info(index - 1).token as usize };
            let annotation = &source[start..info.annotation as usize];
            let qualifier = &source[info.annotation as usize..info.qualifier as usize];
            let comments = trivia::comments(annotation)?;
            Ok(LexicalContent {
                kind: lexed.kind(index),
                qualifier: (!qualifier.is_empty()).then_some(qualifier),
                text: &source[info.qualifier as usize..info.token as usize],
                comments: comments.into_iter().map(|comment| comment.text).collect(),
            })
        })
        .collect()
}
