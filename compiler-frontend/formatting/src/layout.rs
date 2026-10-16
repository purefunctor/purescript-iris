use std::collections::VecDeque;
use std::rc::Rc;

use unicode_width::UnicodeWidthStr;

#[derive(Clone)]
pub(crate) struct Document(Rc<Node>);

#[derive(Clone, Copy)]
pub(crate) enum Choice {
    FirstFitting,
    Fullest,
}

enum Node {
    Empty,
    Text(Box<str>, usize),
    Verbatim(Box<str>),
    LineComment(Box<str>, usize),
    Newlines(usize),
    Switch(Document, Document),
    Concat(Vec<Document>),
    Nest(usize, Document),
    Align(Document),
    Block(Document),
    Continued(Document),
    Hanging(Document, Document),
    HangPoint(Document),
    Flat(Document),
    StartsLine(Document),
    Select(Choice, Vec<Document>),
    Fill(Vec<Document>),
}

impl Document {
    fn new(node: Node) -> Document {
        Document(Rc::new(node))
    }

    pub(crate) fn empty() -> Document {
        Document::new(Node::Empty)
    }

    pub(crate) fn text(text: &str) -> Document {
        if text.is_empty() {
            return Document::empty();
        }
        Document::new(Node::Text(text.into(), text.width()))
    }

    pub(crate) fn verbatim(text: &str) -> Document {
        if !text.contains('\n') {
            return Document::text(text);
        }
        Document::new(Node::Verbatim(text.into()))
    }

    pub(crate) fn line_comment(text: &str) -> Document {
        Document::new(Node::LineComment(text.into(), text.width()))
    }

    pub(crate) fn newlines(count: usize) -> Document {
        Document::new(Node::Newlines(count))
    }

    pub(crate) fn switch(flat: Document, broken: Document) -> Document {
        Document::new(Node::Switch(flat, broken))
    }

    pub(crate) fn line() -> Document {
        Document::switch(Document::text(" "), Document::newlines(1))
    }

    pub(crate) fn line_or_empty() -> Document {
        Document::switch(Document::empty(), Document::newlines(1))
    }

    pub(crate) fn concat(documents: impl IntoIterator<Item = Document>) -> Document {
        let documents = documents.into_iter().filter(|document| !document.is_empty());
        let mut documents = documents.collect::<Vec<_>>();
        match documents.len() {
            0 => Document::empty(),
            1 => documents.pop().unwrap(),
            _ => Document::new(Node::Concat(documents)),
        }
    }

    pub(crate) fn select(choice: Choice, alternatives: Vec<Document>) -> Document {
        Document::new(Node::Select(choice, alternatives))
    }

    pub(crate) fn fill(items: Vec<Document>) -> Document {
        Document::new(Node::Fill(items))
    }

    pub(crate) fn is_empty(&self) -> bool {
        matches!(*self.0, Node::Empty)
    }

    pub(crate) fn append(self, other: Document) -> Document {
        Document::concat([self, other])
    }

    pub(crate) fn nest(self, width: usize) -> Document {
        if self.is_empty() || width == 0 {
            return self;
        }
        Document::new(Node::Nest(width, self))
    }

    pub(crate) fn align(self) -> Document {
        Document::new(Node::Align(self))
    }

    pub(crate) fn block(self) -> Document {
        Document::new(Node::Block(self))
    }

    pub(crate) fn continued(self) -> Document {
        Document::new(Node::Continued(self))
    }

    pub(crate) fn hanging(self, alone: Document) -> Document {
        Document::new(Node::Hanging(self, alone))
    }

    pub(crate) fn hang_point(self) -> Document {
        Document::new(Node::HangPoint(self))
    }

    pub(crate) fn flat(self) -> Document {
        Document::new(Node::Flat(self))
    }

    pub(crate) fn starts_line(self) -> Document {
        Document::new(Node::StartsLine(self))
    }

    pub(crate) fn group(self) -> Document {
        Document::select(Choice::FirstFitting, vec![self.clone().flat(), self])
    }

    pub(crate) fn own_line_group(self) -> Document {
        Document::select(Choice::FirstFitting, vec![self.clone().flat(), self.starts_line()])
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Flat,
    Break,
}

#[derive(Clone, Default)]
struct Scope {
    nest: usize,
    floor: usize,
    hanging: Option<Document>,
}

#[derive(Clone, Copy)]
struct Cursor {
    column: usize,
    line: usize,
    at_line_start: bool,
}

#[derive(Clone)]
enum Task {
    Document(Document, Mode, Scope),
    FillItem { fill: Document, index: usize, wrapped: bool, scope: Scope },
    FillSeparator { fill: Document, index: usize, start_line: usize, wrapped: bool, scope: Scope },
    EndOfAlternative,
}

#[derive(Clone, Default)]
struct Stack(Option<Rc<Frame>>);

struct Frame {
    task: Task,
    next: Stack,
}

impl Stack {
    fn push(self, task: Task) -> Stack {
        Stack(Some(Rc::new(Frame { task, next: self })))
    }

    fn pop(&self) -> Option<(Task, Stack)> {
        self.0.as_ref().map(|frame| (frame.task.clone(), frame.next.clone()))
    }
}

impl Drop for Stack {
    fn drop(&mut self) {
        let mut next = self.0.take();
        while let Some(frame) = next {
            match Rc::try_unwrap(frame) {
                Ok(mut frame) => next = frame.next.0.take(),
                Err(_) => break,
            }
        }
    }
}

#[derive(Clone, Copy)]
struct Decision {
    node: *const Node,
    index: usize,
}

#[derive(Default)]
struct Decisions {
    pending: VecDeque<Decision>,
    taken: Option<Vec<Decision>>,
}

impl Decisions {
    fn recording() -> Decisions {
        Decisions { pending: VecDeque::new(), taken: Some(Vec::new()) }
    }

    fn decide(
        &mut self,
        document: &Document,
        decide: impl FnOnce() -> (usize, Vec<Decision>),
    ) -> usize {
        let node = Rc::as_ptr(&document.0);
        let index = match self.pending.pop_front() {
            Some(decision) if std::ptr::eq(decision.node, node) => decision.index,
            _ => {
                let (index, decisions) = decide();
                self.pending = decisions.into();
                index
            }
        };
        if let Some(taken) = &mut self.taken {
            taken.push(Decision { node, index });
        }
        index
    }

    fn measurement(self, content: usize) -> Measurement {
        Measurement { content, decisions: self.taken.unwrap_or_default() }
    }
}

struct Measurement {
    content: usize,
    decisions: Vec<Decision>,
}

enum Emit {
    Nothing,
    Text(Document, Mode),
    Newlines { count: usize, nest: usize, hanging: bool, mode: Mode },
    StartsLine,
    BlockStart,
    LineOpportunity,
    EndOfAlternative,
}

struct Renderer {
    width: usize,
    indent: usize,
}

pub(crate) fn render(document: &Document, width: usize, indent: usize) -> String {
    Renderer { width, indent }.render(document)
}

impl Renderer {
    fn render(&self, document: &Document) -> String {
        let mut output = String::new();
        let mut trailing_whitespace = 0;
        let mut pending_indentation = None;
        let mut cursor = Cursor { column: 0, line: 0, at_line_start: true };
        let task = Task::Document(document.clone(), Mode::Break, Scope::default());
        let mut stack = Stack::default().push(task);
        let mut decisions = Decisions::default();
        while let Some((task, next)) = stack.pop() {
            let (next, emit) = self.expand(task, &mut cursor, next, false, &mut decisions);
            stack = next;
            match emit {
                Emit::Text(document, _) => {
                    if let Some(indentation) = pending_indentation.take() {
                        output.extend(std::iter::repeat_n(' ', indentation));
                        trailing_whitespace += indentation;
                    }
                    let (text, width) = text_of(&document);
                    output.push_str(text);
                    if text.trim_start_matches(' ').is_empty()
                        && matches!(*document.0, Node::Text(..))
                    {
                        trailing_whitespace += text.len();
                    } else {
                        trailing_whitespace = 0;
                    }
                    advance(&mut cursor, text, width);
                }
                Emit::Newlines { count, nest, .. } => {
                    output.truncate(output.len() - trailing_whitespace);
                    trailing_whitespace = 0;
                    output.extend(std::iter::repeat_n('\n', count));
                    pending_indentation = Some(nest);
                    cursor =
                        Cursor { column: nest, line: cursor.line + count, at_line_start: true };
                }
                Emit::Nothing
                | Emit::StartsLine
                | Emit::BlockStart
                | Emit::LineOpportunity
                | Emit::EndOfAlternative => {}
            }
        }
        output
    }

    fn expand(
        &self,
        task: Task,
        cursor: &mut Cursor,
        stack: Stack,
        approximate: bool,
        decisions: &mut Decisions,
    ) -> (Stack, Emit) {
        let (document, mode, scope) = match task {
            Task::Document(document, mode, scope) => (document, mode, scope),
            Task::FillItem { fill, index, wrapped, scope } => {
                let items = fill_items(&fill);
                let last = index + 1 == items.len();
                let item = items[index].clone();
                let nest = if wrapped { scope.nest + self.indent } else { scope.nest };
                let item_scope = Scope { nest, ..scope.clone() };
                let item_scope = if last { item_scope } else { self.continued(item_scope) };
                let stack = if last {
                    stack
                } else {
                    let start_line = cursor.line;
                    let index = index + 1;
                    stack.push(Task::FillSeparator { fill, index, start_line, wrapped, scope })
                };
                let stack = stack.push(Task::Document(item, Mode::Break, item_scope));
                return (stack, Emit::Nothing);
            }
            Task::FillSeparator { fill, index, start_line, wrapped, scope } => {
                if approximate {
                    return (stack, Emit::LineOpportunity);
                }
                let items = fill_items(&fill);
                let separator = items[index].clone();
                let item = items[index + 1].clone();
                let nest = scope.nest + self.indent;
                let keeps_line = decisions.decide(&separator, || {
                    if cursor.line != start_line {
                        return (0, Vec::new());
                    }
                    let following = if index + 2 < items.len() {
                        let fill = fill.clone();
                        let index = index + 2;
                        stack.clone().push(Task::FillSeparator {
                            fill,
                            index,
                            start_line,
                            wrapped,
                            scope: scope.clone(),
                        })
                    } else {
                        stack.clone()
                    };
                    let alternative = separator.clone().flat().append(item.clone());
                    match self.measure(&alternative, scope.clone(), *cursor, &following) {
                        Some(measurement) => (1, measurement.decisions),
                        None => (0, Vec::new()),
                    }
                }) == 1;
                let hangs = match &scope.hanging {
                    Some(alone) if !keeps_line && matches!(*separator.0, Node::HangPoint(_)) => {
                        let ends_call = index + 2 == items.len();
                        (ends_call || !self.fits_on_next_line(&item, &scope, *cursor))
                            && !self.fits_on_next_line(alone, &scope, *cursor)
                    }
                    _ => false,
                };
                let scope = if hangs { Scope { hanging: None, ..scope } } else { scope };
                let separator_scope = Scope { nest, ..scope.clone() };
                let mode = if keeps_line { Mode::Flat } else { Mode::Break };
                let wrapped = wrapped || !keeps_line;
                let next = Task::FillItem { fill, index: index + 1, wrapped, scope };
                let stack = stack.push(next).push(Task::Document(separator, mode, separator_scope));
                return (stack, Emit::Nothing);
            }
            Task::EndOfAlternative => return (stack, Emit::EndOfAlternative),
        };
        let push = |stack: Stack, document: &Document, mode: Mode, scope: &Scope| {
            stack.push(Task::Document(document.clone(), mode, scope.clone()))
        };
        match &*document.0 {
            Node::Empty => (stack, Emit::Nothing),
            Node::Text(..) | Node::Verbatim(_) | Node::LineComment(..) => {
                (stack, Emit::Text(document.clone(), mode))
            }
            Node::Newlines(count) => {
                let hanging = scope.hanging.is_some();
                (stack, Emit::Newlines { count: *count, nest: scope.nest, hanging, mode })
            }
            Node::Switch(flat, broken) => {
                let chosen = if mode == Mode::Flat { flat } else { broken };
                (push(stack, chosen, mode, &scope), Emit::Nothing)
            }
            Node::Concat(documents) => {
                let stack = documents
                    .iter()
                    .rev()
                    .fold(stack, |stack, document| push(stack, document, mode, &scope));
                (stack, Emit::Nothing)
            }
            Node::Nest(width, inner) => {
                let scope = Scope { nest: scope.nest + width, ..scope };
                (push(stack, inner, mode, &scope), Emit::Nothing)
            }
            Node::Align(inner) => {
                cursor.at_line_start = true;
                let scope = Scope { nest: cursor.column, floor: 0, ..scope };
                (push(stack, inner, mode, &scope), Emit::Nothing)
            }
            Node::Block(inner) => {
                let nest = scope.nest.max(scope.floor) + self.indent;
                let scope = Scope { nest, hanging: None, ..scope };
                let emit = if mode == Mode::Break { Emit::BlockStart } else { Emit::Nothing };
                (push(stack, inner, mode, &scope), emit)
            }
            Node::Continued(inner) => {
                (push(stack, inner, mode, &self.continued(scope)), Emit::Nothing)
            }
            Node::Hanging(inner, alone) => {
                let scope = Scope { hanging: Some(alone.clone()), ..scope };
                (push(stack, inner, mode, &scope), Emit::Nothing)
            }
            Node::HangPoint(inner) => (push(stack, inner, mode, &scope), Emit::Nothing),
            Node::Flat(inner) => (push(stack, inner, Mode::Flat, &scope), Emit::Nothing),
            Node::StartsLine(inner) => (push(stack, inner, mode, &scope), Emit::StartsLine),
            Node::Select(choice, alternatives) => {
                let index = if mode == Mode::Flat {
                    0
                } else if approximate {
                    alternatives.len() - 1
                } else {
                    decisions.decide(&document, || {
                        self.choose(*choice, alternatives, &scope, *cursor, &stack)
                    })
                };
                (push(stack, &alternatives[index], mode, &scope), Emit::Nothing)
            }
            Node::Fill(items) => {
                if mode == Mode::Flat {
                    let stack = items
                        .iter()
                        .rev()
                        .fold(stack, |stack, item| push(stack, item, mode, &scope));
                    return (stack, Emit::Nothing);
                }
                let fill = document.clone();
                let task = Task::FillItem { fill, index: 0, wrapped: false, scope };
                (stack.push(task), Emit::Nothing)
            }
        }
    }

    fn continued(&self, scope: Scope) -> Scope {
        Scope { floor: scope.floor.max(scope.nest + self.indent), ..scope }
    }

    fn choose(
        &self,
        choice: Choice,
        alternatives: &[Document],
        scope: &Scope,
        cursor: Cursor,
        rest: &Stack,
    ) -> (usize, Vec<Decision>) {
        let last = alternatives.len() - 1;
        let measure = |alternative| self.measure(alternative, scope.clone(), cursor, rest);
        let chosen = match choice {
            Choice::FirstFitting => (0..last).find_map(|index| {
                measure(&alternatives[index]).map(|measurement| (index, measurement))
            }),
            Choice::Fullest => {
                let mut fullest: Option<(usize, Measurement)> = None;
                for (index, alternative) in alternatives.iter().enumerate() {
                    let Some(measurement) = measure(alternative) else { continue };
                    if fullest.as_ref().is_none_or(|(_, most)| measurement.content > most.content) {
                        fullest = Some((index, measurement));
                    }
                }
                fullest
            }
        };
        chosen.map_or((last, Vec::new()), |(index, measurement)| (index, measurement.decisions))
    }

    fn measure(
        &self,
        alternative: &Document,
        scope: Scope,
        mut cursor: Cursor,
        rest: &Stack,
    ) -> Option<Measurement> {
        let task = Task::Document(alternative.clone(), Mode::Break, scope);
        let mut stack = rest.clone().push(Task::EndOfAlternative).push(task);
        let mut decisions = Decisions::recording();
        let mut content = 0;
        let mut counting = true;
        let mut emitted = false;
        let mut in_rest = false;
        while let Some((task, next)) = stack.pop() {
            let (next, emit) = self.expand(task, &mut cursor, next, in_rest, &mut decisions);
            stack = next;
            match emit {
                Emit::Nothing => {}
                Emit::EndOfAlternative => in_rest = true,
                Emit::BlockStart => {
                    if !in_rest && counting {
                        counting = false;
                        content = usize::MAX;
                    }
                }
                Emit::LineOpportunity => return Some(decisions.measurement(content)),
                Emit::StartsLine => {
                    if !in_rest && !cursor.at_line_start {
                        return None;
                    }
                }
                Emit::Text(document, mode) => {
                    let (text, width) = text_of(&document);
                    let breaks_line = match &*document.0 {
                        Node::LineComment(..) => true,
                        Node::Verbatim(_) => {
                            let first_line = text.split('\n').next().unwrap_or_default();
                            cursor.column += first_line.width();
                            if counting && !in_rest {
                                content += first_line.width();
                            }
                            true
                        }
                        _ => {
                            advance(&mut cursor, text, width);
                            if counting && !in_rest && text != " " {
                                content += width;
                            }
                            false
                        }
                    };
                    if cursor.column > self.width {
                        return None;
                    }
                    if breaks_line {
                        return (in_rest || mode == Mode::Break)
                            .then(|| decisions.measurement(content));
                    }
                    emitted |= !in_rest;
                }
                Emit::Newlines { nest, hanging, mode, .. } => {
                    if !in_rest && (mode == Mode::Flat || hanging) {
                        return None;
                    }
                    if in_rest || emitted {
                        return Some(decisions.measurement(content));
                    }
                    emitted = true;
                    cursor = Cursor { column: nest, line: cursor.line + 1, at_line_start: true };
                }
            }
        }
        Some(decisions.measurement(content))
    }

    fn fits_on_next_line(&self, document: &Document, scope: &Scope, cursor: Cursor) -> bool {
        let nest = scope.nest.max(scope.floor) + self.indent;
        let scope = Scope { nest, floor: 0, hanging: None };
        let cursor = Cursor { column: nest, line: cursor.line + 1, at_line_start: true };
        self.measure(&document.clone().flat(), scope, cursor, &Stack::default()).is_some()
    }
}

fn fill_items(fill: &Document) -> &[Document] {
    let Node::Fill(items) = &*fill.0 else {
        unreachable!("invariant violated: fill tasks refer to fill documents");
    };
    items
}

fn text_of(document: &Document) -> (&str, usize) {
    match &*document.0 {
        Node::Text(text, width) | Node::LineComment(text, width) => (text, *width),
        Node::Verbatim(text) => (text, 0),
        _ => unreachable!("invariant violated: only text documents are emitted"),
    }
}

fn advance(cursor: &mut Cursor, text: &str, width: usize) {
    match text.rsplit_once('\n') {
        Some((before, last)) => {
            cursor.line += before.matches('\n').count() + 1;
            cursor.column = last.width();
        }
        None => cursor.column += width,
    }
    if !text.is_empty() {
        cursor.at_line_start = false;
    }
}
