use std::str::Chars;

use syntax::SyntaxKind;

use crate::categories::LexerCategories;
use crate::lexed::{LexedBuilder, SyntaxKindInfo};
use crate::{Lexed, Position};

const EOF_CHAR: char = '\0';

pub(super) fn operator_kind(source: &str) -> SyntaxKind {
    match source {
        "∷" => SyntaxKind::DOUBLE_COLON,
        "←" => SyntaxKind::LEFT_ARROW,
        "→" => SyntaxKind::RIGHT_ARROW,
        "⇐" => SyntaxKind::LEFT_THICK_ARROW,
        "⇒" => SyntaxKind::RIGHT_THICK_ARROW,
        "∀" => SyntaxKind::FORALL,
        "=" => SyntaxKind::EQUAL,
        ":" => SyntaxKind::COLON,
        "::" => SyntaxKind::DOUBLE_COLON,
        "." => SyntaxKind::PERIOD,
        ".." => SyntaxKind::DOUBLE_PERIOD,
        "<-" => SyntaxKind::LEFT_ARROW,
        "->" => SyntaxKind::RIGHT_ARROW,
        "<=" => SyntaxKind::LEFT_THICK_ARROW,
        "=>" => SyntaxKind::RIGHT_THICK_ARROW,
        "|" => SyntaxKind::PIPE,
        "@" => SyntaxKind::AT,
        "-" => SyntaxKind::MINUS,
        "\\" => SyntaxKind::BACKSLASH,
        _ => SyntaxKind::OPERATOR,
    }
}

pub(super) fn lower_kind(source: &str) -> SyntaxKind {
    match source {
        "ado" => SyntaxKind::ADO,
        "as" => SyntaxKind::AS,
        "case" => SyntaxKind::CASE,
        "class" => SyntaxKind::CLASS,
        "data" => SyntaxKind::DATA,
        "derive" => SyntaxKind::DERIVE,
        "do" => SyntaxKind::DO,
        "else" => SyntaxKind::ELSE,
        "false" => SyntaxKind::FALSE,
        "forall" => SyntaxKind::FORALL,
        "foreign" => SyntaxKind::FOREIGN,
        "hiding" => SyntaxKind::HIDING,
        "if" => SyntaxKind::IF,
        "import" => SyntaxKind::IMPORT,
        "in" => SyntaxKind::IN,
        "infix" => SyntaxKind::INFIX,
        "infixl" => SyntaxKind::INFIXL,
        "infixr" => SyntaxKind::INFIXR,
        "instance" => SyntaxKind::INSTANCE,
        "let" => SyntaxKind::LET,
        "module" => SyntaxKind::MODULE,
        "newtype" => SyntaxKind::NEWTYPE,
        "nominal" => SyntaxKind::NOMINAL,
        "of" => SyntaxKind::OF,
        "phantom" => SyntaxKind::PHANTOM,
        "representational" => SyntaxKind::REPRESENTATIONAL,
        "role" => SyntaxKind::ROLE,
        "then" => SyntaxKind::THEN,
        "true" => SyntaxKind::TRUE,
        "type" => SyntaxKind::TYPE,
        "where" => SyntaxKind::WHERE,
        _ => SyntaxKind::LOWER,
    }
}

pub(super) struct Lexer<'s> {
    source: &'s str,
    chars: Chars<'s>,
    annotation: u32,
    qualifier: u32,
    qualifier_position: Position,
    current_position: Position,
    lexed: LexedBuilder<'s>,
    modes: Vec<Mode>,
    operand_start: bool,
    jsx: bool,
}

#[derive(Debug, Clone, Copy)]
enum Mode {
    PureScript { hole_depth: Option<u32> },
    Tag { closing: bool, expects_name: bool },
    Children,
}

impl<'s> Lexer<'s> {
    pub(super) fn new(source: &'s str, jsx: bool) -> Lexer<'s> {
        let chars = source.chars();
        let annotation = 0;
        let qualifier = 0;
        let lexed = LexedBuilder::new(source);
        let qualifier_position = Position { line: 1, column: 1 };
        let current_position = Position { line: 1, column: 1 };

        // This allows us to take the annotation and qualifier at the end
        // of `take_token` rather than at the beginning. This positioning
        // is important for handling "hanging" annotations and qualifiers.
        let mut lexer = Lexer {
            source,
            chars,
            annotation,
            qualifier,
            lexed,
            qualifier_position,
            current_position,
            modes: vec![Mode::PureScript { hole_depth: None }],
            operand_start: true,
            jsx,
        };
        lexer.take_annotation();
        lexer.take_qualifier();

        lexer
    }

    pub(super) fn finish(mut self) -> Lexed<'s> {
        // If take_qualifier advanced before finishing:
        if self.qualifier > self.annotation {
            self.push(SyntaxKind::END_OF_FILE, Some("Unexpected end of file"));
        } else {
            self.push(SyntaxKind::END_OF_FILE, None);
        }
        self.lexed.build()
    }

    pub(super) fn is_eof(&self) -> bool {
        self.chars.as_str().is_empty()
    }

    pub(super) fn take_token(&mut self) {
        match self.modes.last().copied().unwrap() {
            Mode::PureScript { .. } => self.take_pure_script_token(),
            Mode::Tag { .. } => self.take_tag_token(),
            Mode::Children => self.take_children_token(),
        }
        match self.modes.last().copied().unwrap() {
            Mode::PureScript { .. } => {
                self.take_annotation();
                self.take_qualifier();
            }
            Mode::Tag { .. } => {
                // A brace starts a hole here, even in value={-1}; it cannot
                // introduce a PureScript block comment until inside the hole.
                self.take_annotation_whitespace();
                self.annotation = self.consumed() as u32;
                self.qualifier = self.annotation;
                self.qualifier_position = self.current_position;
            }
            Mode::Children => {
                self.annotation = self.consumed() as u32;
                self.qualifier = self.annotation;
                self.qualifier_position = self.current_position;
            }
        }
    }
}

impl Lexer<'_> {
    fn starts_jsx(&self) -> bool {
        self.jsx
            && self.first() == '<'
            && self.operand_start
            && self.annotation == self.qualifier
            && (self.second() == '>' || jsx_name_start(self.second()))
    }

    fn take_pure_script_token(&mut self) {
        if self.starts_jsx() {
            self.take_jsx_open();
            return;
        }

        if let Some(Mode::PureScript { hole_depth: Some(depth) }) = self.modes.last().copied() {
            match (self.first(), depth) {
                ('}', 0) => {
                    self.take_kind(SyntaxKind::JSX_EXPRESSION_END);
                    self.modes.pop();
                    self.operand_start = false;
                    return;
                }
                ('{', _) => {
                    self.take_kind(SyntaxKind::LEFT_CURLY);
                    *self.modes.last_mut().unwrap() =
                        Mode::PureScript { hole_depth: Some(depth + 1) };
                    self.operand_start = true;
                    return;
                }
                ('}', _) => {
                    self.take_kind(SyntaxKind::RIGHT_CURLY);
                    *self.modes.last_mut().unwrap() =
                        Mode::PureScript { hole_depth: Some(depth - 1) };
                    self.operand_start = false;
                    return;
                }
                _ => {}
            }
        }

        let before = self.lexed.len();
        let property = before > 0 && self.lexed.last_kind() == SyntaxKind::PERIOD;
        let qualified = self.annotation < self.qualifier;
        self.take_token_impl();
        if self.lexed.len() > before {
            let kind = self.lexed.last_kind();
            let qualified_prefix = matches!(kind, SyntaxKind::DO | SyntaxKind::ADO)
                || syntax::names::OPERATOR.contains(kind);
            self.operand_start =
                !property && (!qualified || qualified_prefix) && token_permits_operand(kind);
        }
    }

    fn take_jsx_open(&mut self) {
        self.take();
        let closing = if self.first() == '/' {
            self.take();
            true
        } else {
            false
        };
        self.push(if closing { SyntaxKind::JSX_CLOSE_OPEN } else { SyntaxKind::JSX_OPEN }, None);
        self.modes.push(Mode::Tag { closing, expects_name: true });
        self.operand_start = true;
    }

    fn take_tag_token(&mut self) {
        let Mode::Tag { closing, expects_name } = self.modes.last().copied().unwrap() else {
            unreachable!()
        };
        match (self.first(), self.second()) {
            ('/', '>') if !closing => {
                self.take();
                self.take();
                self.push(SyntaxKind::JSX_SELF_CLOSE, None);
                self.modes.pop();
                self.operand_start = false;
            }
            ('>', _) => {
                self.take();
                self.push(
                    if closing { SyntaxKind::JSX_CLOSE_END } else { SyntaxKind::JSX_TAG_END },
                    None,
                );
                self.modes.pop();
                if closing {
                    if matches!(self.modes.last(), Some(Mode::Children)) {
                        self.modes.pop();
                    }
                    self.operand_start = false;
                } else {
                    self.modes.push(Mode::Children);
                }
            }
            ('=', _) if !expects_name => self.take_kind(SyntaxKind::EQUAL),
            ('"', _) if !expects_name => self.take_string(),
            ('{', _) if !expects_name && !closing => {
                self.take_kind(SyntaxKind::JSX_EXPRESSION_START);
                self.modes.push(Mode::PureScript { hole_depth: Some(0) });
                self.operand_start = true;
            }
            (character, _) if jsx_name_start(character) => {
                let start = self.consumed();
                self.take();
                self.take_while(jsx_name_continue);
                let name = &self.source[start..self.consumed()];
                let valid =
                    name.split('.').rev().skip(1).all(|component| {
                        component.chars().next().is_some_and(char::is_upper_start)
                    });
                self.push(
                    SyntaxKind::JSX_NAME,
                    (!valid).then_some("JSX name qualifiers must start with an uppercase letter"),
                );
                *self.modes.last_mut().unwrap() = Mode::Tag { closing, expects_name: false };
            }
            _ => {
                self.take();
                self.push(SyntaxKind::JSX_INVALID, Some("Invalid token in JSX tag"));
            }
        }
    }

    fn take_children_token(&mut self) {
        match (self.first(), self.second()) {
            ('<', _) => self.take_jsx_open(),
            ('{', _) => {
                self.take_kind(SyntaxKind::JSX_EXPRESSION_START);
                self.modes.push(Mode::PureScript { hole_depth: Some(0) });
                self.operand_start = true;
            }
            _ => {
                self.take();
                self.take_while(|character| character != '<' && character != '{');
                self.push(SyntaxKind::JSX_TEXT, None);
            }
        }
    }

    fn first(&self) -> char {
        let mut chars = self.chars.clone();
        chars.next().unwrap_or(EOF_CHAR)
    }

    fn second(&self) -> char {
        let mut chars = self.chars.clone();
        chars.next();
        chars.next().unwrap_or(EOF_CHAR)
    }

    fn consumed(&self) -> usize {
        self.source.len() - self.chars.as_str().len()
    }

    fn take(&mut self) -> char {
        let c = self.chars.next().unwrap_or(EOF_CHAR);
        if c == '\n' {
            self.current_position.line += 1;
            self.current_position.column = 1;
        } else {
            self.current_position.column += c.len_utf8() as u32;
        }
        c
    }

    fn take_while(&mut self, predicate: impl Fn(char) -> bool) {
        while predicate(self.first()) && !self.is_eof() {
            self.take();
        }
    }

    fn take_while_maximum(&mut self, predicate: impl Fn(char) -> bool, maximum: usize) {
        for _ in 0..maximum {
            if predicate(self.first()) && !self.is_eof() {
                self.take();
            } else {
                break;
            }
        }
    }

    fn take_annotation(&mut self) {
        loop {
            match self.first() {
                i if i.is_whitespace() => self.take_annotation_whitespace(),
                '-' if self.second() == '-' => self.take_annotation_line_comment(),
                '{' if self.second() == '-' => self.take_annotation_block_comment(),
                _ => break,
            }
        }
        self.annotation = self.consumed() as u32;
        self.qualifier_position = self.current_position;
    }

    fn take_annotation_whitespace(&mut self) {
        self.take_while(char::is_whitespace);
    }

    fn take_annotation_line_comment(&mut self) {
        self.take();
        self.take();
        self.take_while(|c| c != '\n');
    }

    fn take_annotation_block_comment(&mut self) {
        self.take();
        self.take();
        let mut level = 1;
        while !self.is_eof() {
            match (self.first(), self.second()) {
                ('{', '-') => {
                    level += 1;
                    self.take();
                    self.take();
                }
                ('-', '}') => {
                    level -= 1;
                    self.take();
                    self.take();
                }
                _ => (),
            }
            if level == 0 {
                break;
            }
            self.take();
        }
    }

    fn take_qualifier(&mut self) {
        while self.first().is_upper_start() {
            let checkpoint = self.chars.clone();
            let annotation = self.annotation;
            let qualifier = self.qualifier;
            let qualifier_position = self.qualifier_position;
            let current_position = self.current_position;

            self.take_while(char::is_name);
            if self.first() == '.' {
                self.take();
            } else {
                self.chars = checkpoint;
                self.annotation = annotation;
                self.qualifier = qualifier;
                self.qualifier_position = qualifier_position;
                self.current_position = current_position;
                break;
            }
        }
        self.qualifier = self.consumed() as u32;
    }
}

impl Lexer<'_> {
    fn push(&mut self, kind: SyntaxKind, error: Option<&str>) {
        let info = SyntaxKindInfo {
            annotation: self.annotation,
            qualifier: self.qualifier,
            token: self.consumed() as u32,
            position: self.qualifier_position,
        };
        self.lexed.push(kind, info, error);
    }

    fn take_token_impl(&mut self) {
        match self.first() {
            i if i.is_whitespace() => {
                self.push(SyntaxKind::ERROR, Some("Expected a token"));
            }
            '-' if self.second() == '-' => {
                self.push(SyntaxKind::ERROR, Some("Expected a token"));
            }
            '{' if self.second() == '-' => {
                self.push(SyntaxKind::ERROR, Some("Expected a token"));
            }

            '(' => self.take_operator_name_or_left_parenthesis(),
            ')' => self.take_kind(SyntaxKind::RIGHT_PARENTHESIS),
            '{' => self.take_kind(SyntaxKind::LEFT_CURLY),
            '}' => self.take_kind(SyntaxKind::RIGHT_CURLY),
            '[' => self.take_kind(SyntaxKind::LEFT_SQUARE),
            ']' => self.take_kind(SyntaxKind::RIGHT_SQUARE),

            '`' => self.take_kind(SyntaxKind::TICK),
            ',' => self.take_kind(SyntaxKind::COMMA),
            '?' if !self.second().is_name() => self.take_operator(),
            '_' if !self.second().is_name() => self.take_kind(SyntaxKind::UNDERSCORE),

            '\'' => self.take_char(),
            '"' => self.take_string(),

            i => {
                if i == '?' {
                    self.take_hole();
                } else if i.is_lower_start() {
                    self.take_lower();
                } else if i.is_upper_start() {
                    self.take_upper();
                } else if i.is_operator() {
                    self.take_operator();
                } else if i.is_ascii_digit() {
                    self.take_integer_or_number();
                } else {
                    self.take();
                    self.push(SyntaxKind::ERROR, Some("Invalid token"));
                }
            }
        }
    }

    fn take_kind(&mut self, kind: SyntaxKind) {
        self.take();
        self.push(kind, None);
    }

    fn take_operator_kind(&mut self) -> SyntaxKind {
        let start = self.consumed();
        self.take_while(char::is_operator);
        let end = self.consumed();
        operator_kind(&self.source[start..end])
    }

    fn take_operator_name_or_left_parenthesis(&mut self) {
        let lp_position = self.current_position;
        self.take();

        let lp_token = self.consumed() as u32;
        let lp_info = SyntaxKindInfo {
            annotation: self.annotation,
            qualifier: self.qualifier,
            token: lp_token,
            position: lp_position,
        };

        // Complete operator names win over fragments: (<>) remains an operator,
        // while (<><span /></>) starts a parenthesized JSX expression.
        let operator_name =
            self.chars.clone().skip_while(|character| character.is_operator()).next() == Some(')');
        if self.jsx
            && self.first() == '<'
            && (self.second() == '>' || jsx_name_start(self.second()))
            && !operator_name
        {
            self.lexed.push(SyntaxKind::LEFT_PARENTHESIS, lp_info, None);
            self.operand_start = true;
            return;
        }

        if !self.first().is_operator() {
            return self.lexed.push(SyntaxKind::LEFT_PARENTHESIS, lp_info, None);
        }

        let op_position = self.current_position;
        let op_kind = self.take_operator_kind();
        if self.first() != ')' {
            self.lexed.push(SyntaxKind::LEFT_PARENTHESIS, lp_info, None);
            let op_info = SyntaxKindInfo {
                // annotation/qualifier are zero-width
                annotation: lp_info.token,
                qualifier: lp_info.token,
                token: self.consumed() as u32,
                position: op_position,
            };
            return self.lexed.push(op_kind, op_info, None);
        }

        self.take();
        let rp_token = self.consumed() as u32;
        let rp_info = SyntaxKindInfo {
            annotation: lp_info.annotation,
            qualifier: lp_info.qualifier,
            token: rp_token,
            position: lp_position,
        };

        let op_name_kind = match op_kind {
            SyntaxKind::DOUBLE_PERIOD => SyntaxKind::DOUBLE_PERIOD_OPERATOR_NAME,
            _ => SyntaxKind::OPERATOR_NAME,
        };

        self.lexed.push(op_name_kind, rp_info, None);
    }

    fn take_operator(&mut self) {
        let kind = self.take_operator_kind();
        self.push(kind, None);
    }

    fn take_char(&mut self) {
        self.take();
        while !self.is_eof() {
            match self.first() {
                '\'' => {
                    break;
                }
                '\\' if self.second() == '\\' || self.second() == '\'' => {
                    self.take();
                    self.take();
                }
                _ => {
                    self.take();
                }
            }
        }
        if self.first() == '\'' {
            self.take();
            self.push(SyntaxKind::CHAR, None);
        } else {
            self.push(SyntaxKind::CHAR, Some("Unterminated character literal"));
        }
    }

    fn take_string(&mut self) {
        let string = self.consumed();
        self.take_while_maximum(|c| c == '"', 8);
        let quotes = self.consumed() - string;

        match quotes {
            1 => {
                self.take_normal_string();
            }
            2 => {
                self.push(SyntaxKind::STRING, None);
            }
            3..=5 => {
                self.take_raw_string();
            }
            6..=8 => {
                self.push(SyntaxKind::RAW_STRING, None);
            }
            _ => {
                unreachable!()
            }
        }
    }

    fn take_normal_string(&mut self) {
        while !self.is_eof() {
            match self.first() {
                '"' => {
                    break;
                }
                '\\' if self.second().is_whitespace() => {
                    self.take();
                    self.take_while(char::is_whitespace);
                    if self.first() == '\\' {
                        self.take();
                    }
                }
                '\\' if self.second() == '\\' || self.second() == '"' => {
                    self.take();
                    self.take();
                }
                _ => {
                    self.take();
                }
            }
        }
        if self.first() == '"' {
            self.take();
            self.push(SyntaxKind::STRING, None);
        } else {
            self.push(SyntaxKind::STRING, Some("Unterminated single quoted string literal"));
        };
    }

    fn take_raw_string(&mut self) {
        let kind = loop {
            self.take_while(|c| c != '"');
            let string = self.consumed();
            self.take_while_maximum(|c| c == '"', 5);
            let quotes = self.consumed() - string;
            match quotes {
                0 => {
                    break Some("Unterminated triple quoted string literal");
                }
                1 | 2 => {
                    continue;
                }
                3..=5 => {
                    break None;
                }
                _ => {
                    unreachable!()
                }
            }
        };
        self.push(SyntaxKind::RAW_STRING, kind);
    }

    fn take_hole(&mut self) {
        assert_eq!(self.take(), '?');
        self.take_while(char::is_name);
        self.push(SyntaxKind::HOLE, None);
    }

    fn take_lower(&mut self) {
        let start = self.consumed();
        self.take_while(char::is_name);
        let end = self.consumed();
        let kind = lower_kind(&self.source[start..end]);
        self.push(kind, None);
    }

    fn take_upper(&mut self) {
        self.take();
        self.take_while(char::is_name);
        self.push(SyntaxKind::UPPER, None);
    }

    fn take_integer_or_number(&mut self) {
        let mut kind = SyntaxKind::INTEGER;

        let error = if self.first() == '0' && self.second() == '0' {
            Some("Too many leading zeros")
        } else {
            None
        };

        self.take_while(|c| c.is_ascii_digit() || c == '_');

        if self.first() == 'x' {
            self.take();
            self.take_while(|c| c.is_ascii_hexdigit());
            return self.push(SyntaxKind::INTEGER, error);
        }

        // lex(1..2) = [INTEGER, DOUBLE_PERIOD, INTEGER]
        if self.first() == '.' && self.second() == '.' {
            return self.push(SyntaxKind::INTEGER, error);
        }

        if self.first() == '.' {
            self.take();
            self.take_while(|c| c.is_ascii_digit() || c == '_');
            kind = SyntaxKind::NUMBER;
        }

        if self.first() == 'e' || self.first() == 'E' {
            self.take();
            if self.first() == '+' || self.first() == '-' {
                self.take();
            }
            self.take_while(|c| c.is_ascii_digit() || c == '_');
            kind = SyntaxKind::NUMBER;
        }

        self.push(kind, error);
    }
}

fn jsx_name_start(character: char) -> bool {
    character.is_lower_start() || character.is_upper_start()
}

fn jsx_name_continue(character: char) -> bool {
    character.is_name() || matches!(character, '-' | '.')
}

fn token_permits_operand(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::LEFT_PARENTHESIS
            | SyntaxKind::LEFT_CURLY
            | SyntaxKind::LEFT_SQUARE
            | SyntaxKind::BACKSLASH
            | SyntaxKind::TICK
            | SyntaxKind::COMMA
            | SyntaxKind::EQUAL
            | SyntaxKind::PIPE
            | SyntaxKind::RIGHT_ARROW
            | SyntaxKind::LEFT_ARROW
            | SyntaxKind::THEN
            | SyntaxKind::ELSE
            | SyntaxKind::IN
            | SyntaxKind::OF
            | SyntaxKind::CASE
            | SyntaxKind::IF
            | SyntaxKind::LET
            | SyntaxKind::DO
            | SyntaxKind::ADO
            | SyntaxKind::OPERATOR
            | SyntaxKind::MINUS
            | SyntaxKind::COLON
            | SyntaxKind::DOUBLE_PERIOD
            | SyntaxKind::LEFT_THICK_ARROW
    )
}
