use std::cell::{Cell, RefCell};
use std::rc::Rc;

use functional::tree::Origin;
use oxc_ast::ast::CommentNewlines;
use oxc_ast::{Comment, CommentKind, CommentPosition};
use oxc_span::Span;
use smol_str::SmolStr;

/// Every generated node gets its own two-character position tagged with its origin, because Oxc
/// drops a mapping whose source position repeats the previous one, skips empty spans, and maps
/// the end of a span as well as its start.
#[derive(Clone)]
pub(crate) struct ProgramText(Rc<ProgramTextState>);

struct ProgramTextState {
    origin: Cell<Option<Origin>>,
    function_name: RefCell<Option<SmolStr>>,
    contents: RefCell<ProgramTextContents>,
}

#[derive(Default)]
struct ProgramTextContents {
    positions: Vec<ProgramPosition>,
    comment_text: String,
    comments: Vec<Comment>,
}

enum ProgramPosition {
    Node(ProgramNode),
    Comment,
}

const POSITION_WIDTH: u32 = 2;

pub(crate) struct ProgramNode {
    pub(crate) origin: Option<Origin>,
    pub(crate) name: Option<SmolStr>,
}

pub(crate) struct ProgramSource {
    pub(crate) text: String,
    pub(crate) comments: Vec<Comment>,
    positions: Vec<ProgramPosition>,
}

pub(crate) struct OriginScope {
    text: ProgramText,
    enclosing: Option<Origin>,
}

pub(crate) struct FunctionNameScope {
    text: ProgramText,
    enclosing: Option<SmolStr>,
}

impl ProgramText {
    pub(crate) fn new() -> ProgramText {
        let state = ProgramTextState {
            origin: Cell::new(None),
            function_name: RefCell::new(None),
            contents: RefCell::default(),
        };
        ProgramText(Rc::new(state))
    }

    pub(crate) fn span(&self) -> Span {
        self.named_span(None)
    }

    pub(crate) fn named_span(&self, name: Option<SmolStr>) -> Span {
        let node = ProgramNode { origin: self.0.origin.get(), name };
        let start = self.0.contents.borrow_mut().push_position(ProgramPosition::Node(node));
        Span::new(start, start + 1)
    }

    pub(crate) fn function_name(&self) -> Option<SmolStr> {
        Option::clone(&self.0.function_name.borrow())
    }

    pub(crate) fn enter_origin(&self, origin: Option<Origin>) -> OriginScope {
        let enclosing = self.0.origin.get();
        if origin.is_some() {
            self.0.origin.set(origin);
        }
        OriginScope { text: ProgramText::clone(self), enclosing }
    }

    pub(crate) fn enter_function_name(&self, name: Option<SmolStr>) -> FunctionNameScope {
        let enclosing = self.0.function_name.replace(name);
        FunctionNameScope { text: ProgramText::clone(self), enclosing }
    }

    pub(crate) fn line_comment(&self, text: &str) -> Span {
        assert!(
            !text.contains('\n'),
            "invariant violated: program text comment contains a newline"
        );
        let mut contents = self.0.contents.borrow_mut();
        let start = contents.comment_text.len() as u32;
        contents.comment_text.push_str("// ");
        contents.comment_text.push_str(text);
        let end = contents.comment_text.len() as u32;
        let attached_to = contents.push_position(ProgramPosition::Comment);

        let mut comment = Comment::new(start, end, CommentKind::Line);
        comment.attached_to = attached_to;
        comment.position = CommentPosition::Leading;
        comment.newlines = CommentNewlines::Leading | CommentNewlines::Trailing;
        contents.comments.push(comment);
        Span::empty(attached_to)
    }

    pub(crate) fn finish(&self) -> ProgramSource {
        let ProgramTextContents { positions, comment_text, comments } =
            self.0.contents.take();
        let comment_offset = positions.len() as u32 * POSITION_WIDTH;
        let mut text = " ".repeat(comment_offset as usize);
        text.push_str(&comment_text);
        let comments = comments.into_iter().map(|mut comment| {
            comment.span =
                Span::new(comment.span.start + comment_offset, comment.span.end + comment_offset);
            comment
        });
        let comments = comments.collect();
        ProgramSource { text, comments, positions }
    }
}

impl ProgramTextContents {
    fn push_position(&mut self, position: ProgramPosition) -> u32 {
        let start = self.positions.len() as u32 * POSITION_WIDTH;
        self.positions.push(position);
        start
    }
}

impl ProgramSource {
    pub(crate) fn node(&self, column: u32) -> &ProgramNode {
        match &self.positions[(column / POSITION_WIDTH) as usize] {
            ProgramPosition::Node(node) => node,
            ProgramPosition::Comment => {
                unreachable!("invariant violated: generated code was mapped to a comment")
            }
        }
    }
}

impl Drop for OriginScope {
    fn drop(&mut self) {
        self.text.0.origin.set(self.enclosing);
    }
}

impl Drop for FunctionNameScope {
    fn drop(&mut self) {
        self.text.0.function_name.replace(self.enclosing.take());
    }
}
