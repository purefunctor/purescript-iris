use std::sync::Arc;

use lexing::{Lexed, Position};
use syntax::{SyntaxKind, TreeBuilder};

use crate::{ParseError, ParsedModule};

pub(crate) struct Output {
    pub(crate) events: Vec<Event>,
    pub(crate) errors: Vec<ParserError>,
}

#[derive(Debug, Clone)]
pub(crate) enum Event {
    Start { kind: SyntaxKind },
    Annotate,
    Qualify,
    Token { kind: SyntaxKind },
    Error,
    Finish,
}

#[derive(Debug, Clone)]
pub(crate) enum ParserError {
    Message(&'static str),
    Expected(SyntaxKind),
}

struct Builder<'l, 's> {
    lexed: &'l Lexed<'s>,
    index: usize,
    previous_end: u32,
    annotation_end: u32,
    qualifier_end: u32,
    token_end: u32,
    position: Position,
    annotated: bool,
    qualified: bool,
    builder: TreeBuilder,
    errors: Vec<ParseError>,
}

impl<'l, 's> Builder<'l, 's> {
    fn new(lexed: &'l Lexed<'s>, capacity: usize) -> Builder<'l, 's> {
        let info = lexed.info(0);
        let index = 0;
        let previous_end = 0;
        let annotation_end = info.annotation;
        let qualifier_end = info.qualifier;
        let token_end = info.token;
        let position = info.position;
        let annotated = false;
        let qualified = false;
        let builder = TreeBuilder::with_capacity(capacity);
        let errors = Vec::new();
        Builder {
            lexed,
            index,
            previous_end,
            annotation_end,
            qualifier_end,
            token_end,
            position,
            annotated,
            qualified,
            builder,
            errors,
        }
    }

    fn build(self) -> (ParsedModule, Vec<ParseError>) {
        (ParsedModule::new(self.builder.build()), self.errors)
    }

    fn start(&mut self, kind: SyntaxKind) {
        if kind != SyntaxKind::Node {
            self.builder.open(kind);
        }
    }

    fn annotate(&mut self) {
        if !self.annotated && self.previous_end < self.annotation_end {
            let length = self.annotation_end - self.previous_end;
            self.start(SyntaxKind::Annotation);
            self.builder.token(SyntaxKind::TEXT, length);
            self.finish();
        }

        self.annotated = true;
    }

    fn qualify(&mut self) {
        if !self.qualified && self.annotation_end < self.qualifier_end {
            let length = self.qualifier_end - self.annotation_end;
            self.start(SyntaxKind::Qualifier);
            self.builder.token(SyntaxKind::TEXT, length);
            self.finish();
        }

        self.qualified = true;
    }

    fn token(&mut self, kind: SyntaxKind) {
        if kind.is_layout_token() {
            self.builder.token(kind, 0);
            return;
        }

        self.annotate();

        if let Some(message) = self.lexed.error(self.index) {
            self.error(message);
        }

        self.qualify();

        if !matches!(kind, SyntaxKind::ERROR) {
            self.builder.token(kind, self.token_end - self.qualifier_end);
        }

        self.previous_end = self.token_end;
        self.index += 1;
        if self.index < self.lexed.len() {
            let info = self.lexed.info(self.index);
            self.annotation_end = info.annotation;
            self.qualifier_end = info.qualifier;
            self.token_end = info.token;
            self.position = info.position;
        }
        self.annotated = false;
        self.qualified = false;
    }

    fn error(&mut self, message: impl Into<Arc<str>>) {
        let offset = self.qualifier_end as usize;
        let position = self.position;
        let message = message.into();
        self.builder.token(SyntaxKind::ERROR, 0);
        self.errors.push(ParseError { offset, position, message });
    }

    fn finish(&mut self) {
        self.builder.close();
    }
}

fn element_capacity(lexed: &Lexed<'_>, events: &[Event]) -> usize {
    let elements = events.iter().filter(|event| match event {
        Event::Start { kind } => *kind != SyntaxKind::Node,
        Event::Token { .. } | Event::Error => true,
        Event::Annotate | Event::Qualify | Event::Finish => false,
    });
    let annotations = lexed.annotations().flatten();
    let qualifiers = (0..lexed.len()).filter_map(|index| lexed.qualifier(index));
    elements.count() + 2 * (annotations.count() + qualifiers.count())
}

fn expected_syntax_kind(kind: SyntaxKind) -> Option<&'static str> {
    let description = match kind {
        SyntaxKind::HOLE => "a typed hole",
        SyntaxKind::UPPER => "an uppercase identifier",
        SyntaxKind::LOWER => "a lowercase identifier",
        SyntaxKind::OPERATOR => "an operator",
        SyntaxKind::OPERATOR_NAME => "a parenthesised operator",
        SyntaxKind::AT => "'@'",
        SyntaxKind::BACKSLASH => "'\\'",
        SyntaxKind::COLON => "':'",
        SyntaxKind::DOUBLE_COLON => "'::'",
        SyntaxKind::COMMA => "','",
        SyntaxKind::EQUAL => "'='",
        SyntaxKind::MINUS => "'-'",
        SyntaxKind::PERIOD => "'.'",
        SyntaxKind::DOUBLE_PERIOD | SyntaxKind::DOUBLE_PERIOD_OPERATOR_NAME => "'..'",
        SyntaxKind::PIPE => "'|'",
        SyntaxKind::TICK => "'`'",
        SyntaxKind::UNDERSCORE => "'_'",
        SyntaxKind::LEFT_ARROW => "'<-'",
        SyntaxKind::RIGHT_ARROW => "'->'",
        SyntaxKind::LEFT_THICK_ARROW => "'<='",
        SyntaxKind::RIGHT_THICK_ARROW => "'=>'",
        SyntaxKind::LEFT_PARENTHESIS => "'('",
        SyntaxKind::RIGHT_PARENTHESIS => "')'",
        SyntaxKind::LEFT_CURLY => "'{'",
        SyntaxKind::RIGHT_CURLY => "'}'",
        SyntaxKind::LEFT_SQUARE => "'['",
        SyntaxKind::RIGHT_SQUARE => "']'",
        SyntaxKind::ADO => "'ado'",
        SyntaxKind::AS => "'as'",
        SyntaxKind::CASE => "'case'",
        SyntaxKind::CLASS => "'class'",
        SyntaxKind::DATA => "'data'",
        SyntaxKind::DERIVE => "'derive'",
        SyntaxKind::DO => "'do'",
        SyntaxKind::ELSE => "'else'",
        SyntaxKind::FORALL => "'forall'",
        SyntaxKind::FOREIGN => "'foreign'",
        SyntaxKind::HIDING => "'hiding'",
        SyntaxKind::IF => "'if'",
        SyntaxKind::IMPORT => "'import'",
        SyntaxKind::IN => "'in'",
        SyntaxKind::INFIX => "'infix'",
        SyntaxKind::INFIXL => "'infixl'",
        SyntaxKind::INFIXR => "'infixr'",
        SyntaxKind::INSTANCE => "'instance'",
        SyntaxKind::LET => "'let'",
        SyntaxKind::MODULE => "'module'",
        SyntaxKind::NEWTYPE => "'newtype'",
        SyntaxKind::NOMINAL => "'nominal'",
        SyntaxKind::OF => "'of'",
        SyntaxKind::PHANTOM => "'phantom'",
        SyntaxKind::REPRESENTATIONAL => "'representational'",
        SyntaxKind::ROLE => "'role'",
        SyntaxKind::THEN => "'then'",
        SyntaxKind::TYPE => "'type'",
        SyntaxKind::WHERE => "'where'",
        SyntaxKind::CHAR => "a character literal",
        SyntaxKind::INTEGER => "an integer literal",
        SyntaxKind::NUMBER => "a number literal",
        SyntaxKind::RAW_STRING | SyntaxKind::STRING => "a string literal",
        SyntaxKind::TRUE => "'true'",
        SyntaxKind::FALSE => "'false'",
        SyntaxKind::LAYOUT_START => "an indented block",
        SyntaxKind::LAYOUT_SEPARATOR => "a new line or ';' at this indentation",
        SyntaxKind::LAYOUT_END => "the end of this block",
        SyntaxKind::END_OF_FILE => "the end of the file",
        _ => return None,
    };
    Some(description)
}

pub(crate) fn build(lexed: &Lexed<'_>, output: Output) -> (ParsedModule, Vec<ParseError>) {
    let capacity = element_capacity(lexed, &output.events);
    let mut builder = Builder::new(lexed, capacity);
    let mut errors = output.errors.into_iter();

    for event in output.events {
        match event {
            Event::Start { kind } => builder.start(kind),
            Event::Annotate => builder.annotate(),
            Event::Qualify => builder.qualify(),
            Event::Token { kind } => builder.token(kind),
            Event::Error => {
                match errors.next().expect("invariant violated: missing parser error") {
                    ParserError::Message(message) => builder.error(message),
                    ParserError::Expected(kind) => {
                        if let Some(description) = expected_syntax_kind(kind) {
                            builder.error(format!("Expected {description}"));
                        } else {
                            builder.error(format!("Expected {kind:?}"));
                        }
                    }
                }
            }
            Event::Finish => builder.finish(),
        }
    }
    assert!(errors.next().is_none(), "invariant violated: unconsumed parser error");

    builder.build()
}
