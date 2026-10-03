mod categories;
mod layout;
mod lexed;
mod lexer;

use categories::LexerCategories;
pub use lexed::Lexed;
use syntax::SyntaxKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub line: u32,
    pub column: u32,
}

pub fn lex(source: &str) -> Lexed<'_> {
    lex_with_jsx(source, false)
}

pub fn lex_with_jsx(source: &str, jsx: bool) -> Lexed<'_> {
    let mut lexer = lexer::Lexer::new(source, jsx);
    while !lexer.is_eof() {
        lexer.take_token();
    }
    lexer.finish()
}

pub fn is_lower_name(source: &str) -> bool {
    let mut characters = source.chars();
    source != "_"
        && characters.next().is_some_and(char::is_lower_start)
        && characters.all(char::is_name)
        && lexer::lower_kind(source) == SyntaxKind::LOWER
}

pub fn is_upper_name(source: &str) -> bool {
    let mut characters = source.chars();
    characters.next().is_some_and(char::is_upper_start) && characters.all(char::is_name)
}

pub fn is_operator_name(source: &str) -> bool {
    !source.is_empty()
        && !source.starts_with("--")
        && source.chars().all(char::is_operator)
        && matches!(
            lexer::operator_kind(source),
            SyntaxKind::OPERATOR
                | SyntaxKind::COLON
                | SyntaxKind::MINUS
                | SyntaxKind::DOUBLE_PERIOD
                | SyntaxKind::LEFT_THICK_ARROW
        )
}

pub fn layout(lexed: &Lexed) -> Vec<SyntaxKind> {
    let mut layout = layout::Layout::new(lexed);
    while !layout.is_eof() {
        layout.take_token();
    }
    layout.finish()
}
