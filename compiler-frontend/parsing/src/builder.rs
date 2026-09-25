use std::sync::Arc;

use lexing::{Lexed, Position};
use syntax::{SyntaxKind, SyntaxValue, TreeOwner};
use syntree::Builder as SyntreeBuilder;

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
    builder: SyntreeBuilder<SyntaxValue>,
    errors: Vec<ParseError>,
}

impl<'l, 's> Builder<'l, 's> {
    fn new(lexed: &'l Lexed<'s>) -> Builder<'l, 's> {
        let info = lexed.info(0);
        let index = 0;
        let previous_end = 0;
        let annotation_end = info.annotation;
        let qualifier_end = info.qualifier;
        let token_end = info.token;
        let position = info.position;
        let annotated = false;
        let qualified = false;
        let builder = SyntreeBuilder::new();
        let errors = vec![];
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
        let tree = self
            .builder
            .build()
            .expect("invariant violated: parser must produce a balanced syntax tree");
        (ParsedModule::new(TreeOwner::new(tree)), self.errors)
    }

    fn start(&mut self, kind: SyntaxKind) {
        if kind != SyntaxKind::Node {
            self.builder
                .open(SyntaxValue::node(kind))
                .expect("critical violation: syntax tree capacity exceeded");
        }
    }

    fn annotate(&mut self) {
        if !self.annotated && self.previous_end < self.annotation_end {
            let length = (self.annotation_end - self.previous_end) as usize;
            self.start(SyntaxKind::Annotation);
            self.builder
                .token(SyntaxValue::token(SyntaxKind::TEXT), length)
                .expect("critical violation: syntax tree capacity exceeded");
            self.finish();
        }

        self.annotated = true;
    }

    fn qualify(&mut self) {
        if !self.qualified && self.annotation_end < self.qualifier_end {
            let length = (self.qualifier_end - self.annotation_end) as usize;
            self.start(SyntaxKind::Qualifier);
            self.builder
                .token(SyntaxValue::token(SyntaxKind::TEXT), length)
                .expect("critical violation: syntax tree capacity exceeded");
            self.finish();
        }

        self.qualified = true;
    }

    fn token(&mut self, kind: SyntaxKind) {
        if kind.is_layout_token() {
            self.builder
                .token_empty(SyntaxValue::token(kind))
                .expect("critical violation: syntax tree capacity exceeded");
            return;
        }

        self.annotate();

        if let Some(message) = self.lexed.error(self.index) {
            self.error(message);
        }

        self.qualify();

        if !matches!(kind, SyntaxKind::ERROR) {
            let length = (self.token_end - self.qualifier_end) as usize;
            self.builder
                .token(SyntaxValue::token(kind), length)
                .expect("critical violation: syntax tree capacity exceeded");
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
        self.builder
            .token_empty(SyntaxValue::token(SyntaxKind::ERROR))
            .expect("critical violation: syntax tree capacity exceeded");
        self.errors.push(ParseError { offset, position, message });
    }

    fn finish(&mut self) {
        self.builder
            .close()
            .expect("invariant violated: parser must produce a balanced syntax tree");
    }
}

pub(crate) fn build(lexed: &Lexed<'_>, output: Output) -> (ParsedModule, Vec<ParseError>) {
    let mut builder = Builder::new(lexed);
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
