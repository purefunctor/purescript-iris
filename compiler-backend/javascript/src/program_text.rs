use std::cell::{Cell, RefCell};
use std::rc::Rc;

use functional::tree::Origin;
use oxc_ast::ast::CommentNewlines;
use oxc_ast::{Comment, CommentKind, CommentPosition};
use oxc_span::Span;
use smol_str::SmolStr;

/// Every generated node gets its own one-character line tagged with its origin, because Oxc drops a
/// mapping whose source position repeats the previous one and skips empty spans.
#[derive(Clone)]
pub(crate) struct ProgramText(Rc<ProgramTextState>);

struct ProgramTextState {
    origin: Cell<Option<Origin>>,
    function_name: RefCell<Option<SmolStr>>,
    contents: RefCell<ProgramTextContents>,
}

#[derive(Default)]
struct ProgramTextContents {
    text: String,
    lines: Vec<ProgramLine>,
    comments: Vec<Comment>,
}

enum ProgramLine {
    Node(ProgramNode),
    Comment,
}

#[derive(Clone)]
pub(crate) struct ProgramNode {
    pub(crate) origin: Option<Origin>,
    pub(crate) name: Option<SmolStr>,
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
        let start = self.0.contents.borrow_mut().push_line(" ", ProgramLine::Node(node));
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
        let mut contents = self.0.contents.borrow_mut();
        let start = contents.push_line(&format!("// {text}"), ProgramLine::Comment);
        let end = start + 3 + text.len() as u32;
        let attached_to = contents.push_line(" ", ProgramLine::Comment);

        let mut comment = Comment::new(start, end, CommentKind::Line);
        comment.attached_to = attached_to;
        comment.position = CommentPosition::Leading;
        comment.newlines = CommentNewlines::Leading | CommentNewlines::Trailing;
        contents.comments.push(comment);
        Span::empty(attached_to)
    }

    pub(crate) fn source_text(&self) -> String {
        String::clone(&self.0.contents.borrow().text)
    }

    pub(crate) fn comments(&self) -> Vec<Comment> {
        Vec::clone(&self.0.contents.borrow().comments)
    }

    pub(crate) fn line_node(&self, line: u32) -> ProgramNode {
        match &self.0.contents.borrow().lines[line as usize] {
            ProgramLine::Node(node) => ProgramNode::clone(node),
            ProgramLine::Comment => {
                unreachable!("invariant violated: generated code was mapped to a comment")
            }
        }
    }
}

impl ProgramTextContents {
    fn push_line(&mut self, line: &str, kind: ProgramLine) -> u32 {
        assert!(!line.contains('\n'), "invariant violated: program text line contains a newline");
        let start = self.text.len() as u32;
        self.text.push_str(line);
        self.text.push('\n');
        self.lines.push(kind);
        start
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
