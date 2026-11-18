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
                        builder.error(format!("Expected {kind:?}"));
                    }
                }
            }
            Event::Finish => builder.finish(),
        }
    }
    assert!(errors.next().is_none(), "invariant violated: unconsumed parser error");

    builder.build()
}
