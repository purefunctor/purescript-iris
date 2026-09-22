use building_types::QueryProxy;
use lsp_types::{SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokens};
use syntax::{SyntaxKind, SyntaxToken, TextRange, WalkEvent};

use crate::position::PositionConverter;
use crate::{AnalyzerContext, AnalyzerError};

pub const TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::NAMESPACE,
    SemanticTokenType::TYPE,
    SemanticTokenType::CLASS,
    SemanticTokenType::ENUM_MEMBER,
    SemanticTokenType::TYPE_PARAMETER,
    SemanticTokenType::PARAMETER,
    SemanticTokenType::VARIABLE,
    SemanticTokenType::PROPERTY,
    SemanticTokenType::FUNCTION,
    SemanticTokenType::METHOD,
    SemanticTokenType::KEYWORD,
    SemanticTokenType::STRING,
    SemanticTokenType::NUMBER,
    SemanticTokenType::OPERATOR,
];

pub const TOKEN_MODIFIERS: &[SemanticTokenModifier] = &[SemanticTokenModifier::DECLARATION];

const NAMESPACE: u32 = 0;
const TYPE: u32 = 1;
const CLASS: u32 = 2;
const ENUM_MEMBER: u32 = 3;
const TYPE_PARAMETER: u32 = 4;
const PARAMETER: u32 = 5;
const VARIABLE: u32 = 6;
const PROPERTY: u32 = 7;
const FUNCTION: u32 = 8;
const METHOD: u32 = 9;
const KEYWORD: u32 = 10;
const STRING: u32 = 11;
const NUMBER: u32 = 12;
const OPERATOR: u32 = 13;

const DECLARATION: u32 = 1;

#[derive(Debug, Clone, Copy)]
struct TokenClassification {
    token_type: u32,
    token_modifiers_bitset: u32,
}

impl TokenClassification {
    const fn new(token_type: u32) -> TokenClassification {
        TokenClassification { token_type, token_modifiers_bitset: 0 }
    }

    const fn declaration(token_type: u32) -> TokenClassification {
        TokenClassification { token_type, token_modifiers_bitset: DECLARATION }
    }
}

pub fn implementation(
    context: &AnalyzerContext<impl crate::AnalyzerHost>,
    uri: lsp_types::Url,
) -> Result<Option<SemanticTokens>, AnalyzerError> {
    let current_file = {
        let uri = uri.as_str();
        context.file_id(uri).ok_or(AnalyzerError::NonFatal)?
    };

    let content = context.queries().content(current_file)?;
    let (parsed, _) = context.queries().parsed(current_file)?;
    let root = parsed.syntax_node();
    let positions = PositionConverter::new(&content, context.position_encoding());
    let mut data = vec![];
    let mut previous = lsp_types::Position::new(0, 0);

    for event in root.preorder_with_tokens() {
        let WalkEvent::Enter(element) = event else { continue };
        let Some(token) = element.into_token() else { continue };
        let Some(classification) = classify(&token) else { continue };

        push_token_ranges(&mut data, &mut previous, &positions, token.text_range(), classification);
    }

    Ok(Some(SemanticTokens { result_id: None, data }))
}

fn push_token_ranges(
    tokens: &mut Vec<SemanticToken>,
    previous: &mut lsp_types::Position,
    positions: &PositionConverter<'_>,
    range: TextRange,
    classification: TokenClassification,
) {
    let start: usize = range.start().into();
    let end: usize = range.end().into();
    let text = &positions.content()[start..end];

    let mut segment_start = range.start();
    for line in text.split_inclusive('\n') {
        let token_text = line.trim_end_matches(['\r', '\n']);
        let segment_end = segment_start + syntax::TextSize::new(token_text.len() as u32);
        if segment_start < segment_end
            && let Some(range) =
                positions.text_range_to_protocol(TextRange::new(segment_start, segment_end))
        {
            let delta_line = range.start.line - previous.line;
            let delta_start = if delta_line == 0 {
                range.start.character - previous.character
            } else {
                range.start.character
            };
            tokens.push(SemanticToken {
                delta_line,
                delta_start,
                length: range.end.character - range.start.character,
                token_type: classification.token_type,
                token_modifiers_bitset: classification.token_modifiers_bitset,
            });
            *previous = range.start;
        }
        segment_start += syntax::TextSize::new(line.len() as u32);
    }
}

fn classify(token: &SyntaxToken) -> Option<TokenClassification> {
    let classification = match token.kind() {
        SyntaxKind::CHAR | SyntaxKind::RAW_STRING | SyntaxKind::STRING => {
            TokenClassification::new(STRING)
        }
        SyntaxKind::INTEGER | SyntaxKind::NUMBER => TokenClassification::new(NUMBER),
        SyntaxKind::OPERATOR
        | SyntaxKind::OPERATOR_NAME
        | SyntaxKind::DOUBLE_PERIOD_OPERATOR_NAME => TokenClassification::new(OPERATOR),
        SyntaxKind::ADO
        | SyntaxKind::CASE
        | SyntaxKind::CLASS
        | SyntaxKind::DATA
        | SyntaxKind::DERIVE
        | SyntaxKind::DO
        | SyntaxKind::ELSE
        | SyntaxKind::FALSE
        | SyntaxKind::FORALL
        | SyntaxKind::FOREIGN
        | SyntaxKind::IF
        | SyntaxKind::IMPORT
        | SyntaxKind::IN
        | SyntaxKind::INFIX
        | SyntaxKind::INFIXL
        | SyntaxKind::INFIXR
        | SyntaxKind::INSTANCE
        | SyntaxKind::LET
        | SyntaxKind::MODULE
        | SyntaxKind::NEWTYPE
        | SyntaxKind::OF
        | SyntaxKind::THEN
        | SyntaxKind::TRUE
        | SyntaxKind::TYPE
        | SyntaxKind::WHERE => TokenClassification::new(KEYWORD),
        SyntaxKind::AS
        | SyntaxKind::HIDING
        | SyntaxKind::NOMINAL
        | SyntaxKind::PHANTOM
        | SyntaxKind::REPRESENTATIONAL
        | SyntaxKind::ROLE => classify_contextual_keyword(token)?,
        SyntaxKind::TEXT => match token.parent().kind() {
            SyntaxKind::Qualifier => TokenClassification::new(NAMESPACE),
            SyntaxKind::LabelName => TokenClassification::new(PROPERTY),
            _ => return None,
        },
        SyntaxKind::LOWER | SyntaxKind::UPPER => classify_name(token)?,
        _ => return None,
    };

    Some(classification)
}

fn classify_contextual_keyword(token: &SyntaxToken) -> Option<TokenClassification> {
    let parent = token.parent();
    match parent.kind() {
        SyntaxKind::ImportAlias
        | SyntaxKind::ImportList
        | SyntaxKind::InfixDeclaration
        | SyntaxKind::TypeRole
        | SyntaxKind::TypeRoleDeclaration => Some(TokenClassification::new(KEYWORD)),
        _ => classify_name(token),
    }
}

fn classify_name(token: &SyntaxToken) -> Option<TokenClassification> {
    for node in token.parent_ancestors() {
        let classification = match node.kind() {
            SyntaxKind::ERROR => return classify_unresolved_name(token),
            SyntaxKind::Qualifier | SyntaxKind::ModuleName => TokenClassification::new(NAMESPACE),
            SyntaxKind::LabelName => TokenClassification::new(PROPERTY),
            SyntaxKind::TypeVariable => TokenClassification::new(TYPE_PARAMETER),
            SyntaxKind::TypeVariableBinding => TokenClassification::declaration(TYPE_PARAMETER),
            SyntaxKind::BinderVariable | SyntaxKind::BinderNamed => {
                TokenClassification::declaration(PARAMETER)
            }
            SyntaxKind::ExpressionVariable | SyntaxKind::RecordPun => {
                TokenClassification::new(VARIABLE)
            }
            SyntaxKind::ExpressionConstructor | SyntaxKind::BinderConstructor => {
                TokenClassification::new(ENUM_MEMBER)
            }
            SyntaxKind::TypeConstructor => TokenClassification::new(TYPE),
            SyntaxKind::ValueSignature
            | SyntaxKind::ValueEquation
            | SyntaxKind::LetBindingSignature
            | SyntaxKind::LetBindingEquation
            | SyntaxKind::ForeignImportValueDeclaration => {
                TokenClassification::declaration(FUNCTION)
            }
            SyntaxKind::ClassMemberStatement => TokenClassification::declaration(METHOD),
            SyntaxKind::InstanceSignatureStatement | SyntaxKind::InstanceEquationStatement => {
                TokenClassification::declaration(METHOD)
            }
            SyntaxKind::DataConstructor => TokenClassification::declaration(ENUM_MEMBER),
            SyntaxKind::ClassHead | SyntaxKind::ClassSignature => {
                TokenClassification::declaration(CLASS)
            }
            SyntaxKind::InstanceHead => TokenClassification::new(CLASS),
            SyntaxKind::DataSignature
            | SyntaxKind::DataEquation
            | SyntaxKind::NewtypeSignature
            | SyntaxKind::NewtypeEquation
            | SyntaxKind::TypeSynonymSignature
            | SyntaxKind::TypeSynonymEquation
            | SyntaxKind::ForeignImportDataDeclaration
            | SyntaxKind::TypeRoleDeclaration => TokenClassification::declaration(TYPE),
            SyntaxKind::FunctionalDependencyDetermined
            | SyntaxKind::FunctionalDependencyDetermines => {
                TokenClassification::new(TYPE_PARAMETER)
            }
            SyntaxKind::ImportValue | SyntaxKind::ExportValue => TokenClassification::new(FUNCTION),
            SyntaxKind::ImportClass | SyntaxKind::ExportClass => TokenClassification::new(CLASS),
            SyntaxKind::ImportType | SyntaxKind::ExportType => TokenClassification::new(TYPE),
            SyntaxKind::TypeItemsList => TokenClassification::new(ENUM_MEMBER),
            SyntaxKind::ImportOperator
            | SyntaxKind::ImportTypeOperator
            | SyntaxKind::ExportOperator
            | SyntaxKind::ExportTypeOperator
            | SyntaxKind::TermOperator
            | SyntaxKind::TypeOperator => TokenClassification::new(OPERATOR),
            _ => continue,
        };
        return Some(classification);
    }

    classify_unresolved_name(token)
}

fn classify_unresolved_name(token: &SyntaxToken) -> Option<TokenClassification> {
    match token.kind() {
        SyntaxKind::LOWER => Some(TokenClassification::new(VARIABLE)),
        SyntaxKind::UPPER => Some(TokenClassification::new(TYPE)),
        _ => None,
    }
}
