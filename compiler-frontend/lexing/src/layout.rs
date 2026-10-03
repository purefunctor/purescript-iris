use syntax::{SyntaxKind, TokenSet};

use crate::Position;
use crate::lexed::Lexed;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Delimiter {
    Root,
    TopDecl,
    TopDeclHead,
    DeclGuard,
    Case,
    CaseBinders,
    CaseGuard,
    LambdaBinders,
    Paren,
    Brace,
    Square,
    If,
    Then,
    Property,
    Forall,
    Tick,
    Let,
    LetStmt,
    Where,
    Of,
    Do,
    Ado,
    Jsx,
    JsxExpression,
}

impl Delimiter {
    fn is_indented(&self) -> bool {
        matches!(
            self,
            Delimiter::Let
                | Delimiter::LetStmt
                | Delimiter::Where
                | Delimiter::Of
                | Delimiter::Do
                | Delimiter::Ado
        )
    }
}

pub(super) struct Layout<'s> {
    lexed: &'s Lexed<'s>,
    index: usize,
    stack: Vec<(Position, Delimiter)>,
    output: Vec<SyntaxKind>,
}

impl<'s> Layout<'s> {
    pub(super) fn new(lexed: &'s Lexed<'s>) -> Layout<'s> {
        let index = 0;
        let stack = vec![(Position { line: 1, column: 1 }, Delimiter::Root)];
        let output = Vec::with_capacity(lexed.len());
        Layout { lexed, index, stack, output }
    }

    pub(super) fn is_eof(&self) -> bool {
        self.lexed.kind(self.index) == SyntaxKind::END_OF_FILE
    }

    pub(super) fn take_token(&mut self) {
        let token = self.lexed.kind(self.index);
        let info = self.lexed.info(self.index);
        let qualified = info.annotation < info.qualifier;
        let position = info.position;
        let next = self.lexed.info(self.index + 1).position;
        self.insert(token, qualified, position, next);
        self.index += 1;
    }

    pub(super) fn finish(mut self) -> Vec<SyntaxKind> {
        while let Some((_, delimiter)) = self.stack.pop() {
            if delimiter.is_indented() {
                self.output.push(SyntaxKind::LAYOUT_END);
            }
        }
        self.output.push(SyntaxKind::END_OF_FILE);
        self.output
    }
}

impl<'s> Layout<'s> {
    fn insert(&mut self, token: SyntaxKind, qualified: bool, position: Position, next: Position) {
        Insert::new(self, token, qualified, position, next).invoke();
    }
}

struct Insert<'l, 's> {
    layout: &'l mut Layout<'s>,
    token: SyntaxKind,
    qualified: bool,
    position: Position,
    next: Position,
}

impl<'l, 's> Insert<'l, 's> {
    fn new(
        layout: &'l mut Layout<'s>,
        token: SyntaxKind,
        qualified: bool,
        position: Position,
        next: Position,
    ) -> Insert<'l, 's> {
        Insert { layout, token, qualified, position, next }
    }

    fn invoke(&mut self) {
        match self.token {
            SyntaxKind::JSX_OPEN => {
                self.insert_default();
                self.push_stack(self.position, Delimiter::Jsx);
            }

            SyntaxKind::JSX_CLOSE_OPEN
            | SyntaxKind::JSX_TAG_END
            | SyntaxKind::JSX_NAME
            | SyntaxKind::JSX_TEXT
            | SyntaxKind::JSX_EXPRESSION_START => {
                self.insert_token(self.token);
                if self.token == SyntaxKind::JSX_EXPRESSION_START {
                    self.push_stack(self.position, Delimiter::JsxExpression);
                }
            }

            SyntaxKind::JSX_SELF_CLOSE | SyntaxKind::JSX_CLOSE_END => {
                self.insert_token(self.token);
                self.pop_stack_if(|delimiter| delimiter == Delimiter::Jsx);
            }

            SyntaxKind::JSX_EXPRESSION_END => {
                while let Some((_, delimiter)) = self.layout.stack.last().copied() {
                    if delimiter == Delimiter::JsxExpression {
                        self.pop_stack();
                        break;
                    }
                    if delimiter == Delimiter::Jsx {
                        break;
                    }
                    self.pop_stack();
                    if delimiter.is_indented() {
                        self.insert_end();
                    }
                }
                self.insert_token(self.token);
            }

            SyntaxKind::EQUAL | SyntaxKind::STRING | SyntaxKind::RAW_STRING
                if self.in_jsx_markup() =>
            {
                self.insert_token(self.token);
            }

            SyntaxKind::DATA if !self.qualified => {
                self.insert_default();
                if self.is_top_declaration(self.position) {
                    self.push_stack(self.position, Delimiter::TopDecl);
                } else {
                    self.pop_stack_if(|delimiter| delimiter == Delimiter::Property);
                }
            }

            SyntaxKind::CLASS if !self.qualified => {
                self.insert_default();
                if self.is_top_declaration(self.position) {
                    self.push_stack(self.position, Delimiter::TopDeclHead);
                } else {
                    self.pop_stack_if(|delimiter| delimiter == Delimiter::Property);
                }
            }

            SyntaxKind::WHERE if !self.qualified => match &self.layout.stack[..] {
                [.., (_, Delimiter::TopDeclHead)] => {
                    self.pop_stack();
                    self.insert_token(self.token);
                    self.insert_start(Delimiter::Where);
                }
                [.., (_, Delimiter::Property)] => {
                    self.pop_stack();
                    self.insert_token(self.token);
                }
                _ => {
                    self.collapse_and_commit(Insert::where_p);
                    self.insert_token(self.token);
                    self.insert_start(Delimiter::Where);
                }
            },

            SyntaxKind::IN if !self.qualified => {
                let collapse = self.collapse(Insert::in_p);
                match collapse.preview(self.layout) {
                    [.., (_, Delimiter::Ado), (_, Delimiter::LetStmt)] => {
                        collapse.commit(self.layout);
                        self.pop_stack();
                        self.pop_stack();
                        self.insert_end();
                        self.insert_end();
                        self.insert_token(self.token);
                    }
                    [.., (_, delimiter)] if delimiter.is_indented() => {
                        collapse.commit(self.layout);
                        self.pop_stack();
                        self.insert_end();
                        self.insert_token(self.token);
                    }
                    _ => {
                        self.insert_default();
                        self.pop_stack_if(|delimiter| delimiter == Delimiter::Property);
                    }
                }
            }

            SyntaxKind::LET if !self.qualified => {
                self.insert_keyword_property(|this| match &this.layout.stack[..] {
                    [.., (position, Delimiter::Do)] if position.column == this.position.column => {
                        this.insert_start(Delimiter::LetStmt);
                    }
                    [.., (position, Delimiter::Ado)] if position.column == this.position.column => {
                        this.insert_start(Delimiter::LetStmt);
                    }
                    _ => {
                        this.insert_start(Delimiter::Let);
                    }
                });
            }

            SyntaxKind::DO => {
                self.insert_keyword_property(|this| {
                    this.insert_start(Delimiter::Do);
                });
            }

            SyntaxKind::ADO => {
                self.insert_keyword_property(|this| {
                    this.insert_start(Delimiter::Ado);
                });
            }

            SyntaxKind::CASE if !self.qualified => {
                self.insert_keyword_property(|this| {
                    this.push_stack(this.position, Delimiter::Case);
                });
            }

            SyntaxKind::OF if !self.qualified => {
                let collapse = self.collapse(Insert::indented_p);
                match collapse.preview(self.layout) {
                    [.., (_, Delimiter::Case)] => {
                        collapse.commit(self.layout);
                        self.pop_stack();
                        self.insert_token(self.token);
                        self.insert_start(Delimiter::Of);
                        self.push_stack(self.next, Delimiter::CaseBinders);
                    }
                    _ => {
                        self.insert_default();
                        self.pop_stack_if(|delimiter| delimiter == Delimiter::Property);
                    }
                }
            }

            SyntaxKind::IF if !self.qualified => {
                self.insert_keyword_property(|this| {
                    this.push_stack(this.position, Delimiter::If);
                });
            }

            SyntaxKind::THEN if !self.qualified => {
                let collapse = self.collapse(Insert::indented_p);
                match collapse.preview(self.layout) {
                    [.., (_, Delimiter::If)] => {
                        collapse.commit(self.layout);
                        self.pop_stack();
                        self.insert_token(self.token);
                        self.push_stack(self.position, Delimiter::Then);
                    }
                    _ => {
                        self.insert_default();
                        self.pop_stack_if(|delimiter| delimiter == Delimiter::Property);
                    }
                }
            }

            SyntaxKind::ELSE if !self.qualified => {
                let collapse = self.collapse(Insert::indented_p);
                match collapse.preview(self.layout) {
                    [.., (_, Delimiter::Then)] => {
                        collapse.commit(self.layout);
                        self.pop_stack();
                        self.insert_token(self.token);
                    }
                    _ => {
                        self.collapse_and_commit(Insert::offside_p);
                        if self.is_top_declaration(self.position) {
                            self.insert_token(self.token);
                        } else {
                            self.insert_sep();
                            self.insert_token(self.token);
                            self.pop_stack_if(|delimiter| delimiter == Delimiter::Property);
                        }
                    }
                }
            }

            SyntaxKind::FORALL => {
                self.insert_keyword_property(|this| {
                    this.push_stack(this.position, Delimiter::Forall);
                });
            }

            SyntaxKind::BACKSLASH => {
                self.insert_default();
                self.push_stack(self.position, Delimiter::LambdaBinders);
            }

            SyntaxKind::RIGHT_ARROW => {
                self.collapse_and_commit(Insert::arrow_p);
                self.pop_stack_if(|delimiter| {
                    matches!(
                        delimiter,
                        Delimiter::CaseBinders | Delimiter::CaseGuard | Delimiter::LambdaBinders
                    )
                });
                self.insert_token(self.token);
            }

            SyntaxKind::EQUAL => {
                let collapse = self.collapse(Insert::equals_p);
                match collapse.preview(self.layout) {
                    [.., (_, Delimiter::DeclGuard)] => {
                        collapse.commit(self.layout);
                        self.pop_stack();
                        self.insert_token(self.token);
                    }
                    _ => {
                        self.insert_default();
                    }
                }
            }

            SyntaxKind::PIPE => {
                let collapse = self.collapse(Insert::offside_end_p);
                match collapse.preview(self.layout) {
                    [.., (_, Delimiter::Of)] => {
                        collapse.commit(self.layout);
                        self.push_stack(self.position, Delimiter::CaseGuard);
                        self.insert_token(self.token);
                    }
                    [.., (_, Delimiter::Let | Delimiter::LetStmt | Delimiter::Where)] => {
                        collapse.commit(self.layout);
                        self.push_stack(self.position, Delimiter::DeclGuard);
                        self.insert_token(self.token);
                    }
                    _ => {
                        self.insert_default();
                    }
                }
            }

            SyntaxKind::TICK => {
                let collapse = self.collapse(Insert::indented_p);
                match collapse.preview(self.layout) {
                    [.., (_, Delimiter::Tick)] => {
                        collapse.commit(self.layout);
                        self.pop_stack();
                        self.insert_token(self.token);
                    }
                    _ => {
                        self.collapse_and_commit(Insert::offside_end_p);
                        self.insert_sep();
                        self.insert_token(self.token);
                        self.push_stack(self.position, Delimiter::Tick);
                    }
                }
            }

            SyntaxKind::COMMA => {
                self.collapse_and_commit(Insert::indented_p);
                if let Some((_, Delimiter::Brace)) = self.layout.stack.last() {
                    self.insert_token(self.token);
                    self.push_stack(self.position, Delimiter::Property);
                } else {
                    self.insert_token(self.token);
                }
            }

            SyntaxKind::PERIOD => {
                self.insert_default();
                if let Some((_, Delimiter::Forall)) = self.layout.stack.last() {
                    self.pop_stack();
                } else {
                    self.push_stack(self.position, Delimiter::Property);
                }
            }

            SyntaxKind::LEFT_PARENTHESIS => {
                self.insert_default();
                self.push_stack(self.position, Delimiter::Paren);
            }

            SyntaxKind::LEFT_CURLY => {
                self.insert_default();
                self.push_stack(self.position, Delimiter::Brace);
                self.push_stack(self.position, Delimiter::Property);
            }

            SyntaxKind::LEFT_SQUARE => {
                self.insert_default();
                self.push_stack(self.position, Delimiter::Square);
            }

            SyntaxKind::RIGHT_PARENTHESIS => {
                self.collapse_and_commit(Insert::indented_p);
                self.pop_stack_if(|delimiter| delimiter == Delimiter::Paren);
                self.insert_token(self.token);
            }

            SyntaxKind::RIGHT_CURLY => {
                self.collapse_and_commit(Insert::indented_p);
                self.pop_stack_if(|delimiter| delimiter == Delimiter::Property);
                self.pop_stack_if(|delimiter| delimiter == Delimiter::Brace);
                self.insert_token(self.token);
            }

            SyntaxKind::RIGHT_SQUARE => {
                self.collapse_and_commit(Insert::indented_p);
                self.pop_stack_if(|delimiter| delimiter == Delimiter::Square);
                self.insert_token(self.token);
            }

            SyntaxKind::STRING | SyntaxKind::RAW_STRING => {
                self.insert_default();
                self.pop_stack_if(|delimiter| delimiter == Delimiter::Property);
            }

            _ if LOWER.contains(self.token) && !self.qualified => {
                self.insert_default();
                self.pop_stack_if(|delimiter| delimiter == Delimiter::Property);
            }

            _ if OPERATOR.contains(self.token) => {
                self.collapse_and_commit(Insert::offside_end_p);
                self.insert_sep();
                self.insert_token(self.token);
            }

            _ => {
                self.insert_default();
            }
        }
    }

    fn is_top_declaration(&self, token_position: Position) -> bool {
        matches!(&self.layout.stack[..],
            [(_, Delimiter::Root), (layout_position, Delimiter::Where)]
                if token_position.column == layout_position.column
        )
    }

    fn in_jsx_markup(&self) -> bool {
        self.layout
            .stack
            .iter()
            .rev()
            .find_map(|(_, delimiter)| match delimiter {
                Delimiter::Jsx => Some(true),
                Delimiter::JsxExpression => Some(false),
                _ => None,
            })
            .unwrap_or(false)
    }

    fn insert_default(&mut self) {
        self.collapse_and_commit(Self::offside_p);
        self.insert_sep();
        self.insert_token(self.token);
    }

    fn insert_start(&mut self, delimiter: Delimiter) {
        if let Some((past_position, _)) = self
            .layout
            .stack
            .iter()
            .rev()
            .take_while(|(_, delimiter)| *delimiter != Delimiter::JsxExpression)
            .find(|(_, delimiter)| delimiter.is_indented())
            && self.next.column <= past_position.column
        {
            return;
        }

        self.push_stack(self.next, delimiter);
        self.insert_token(SyntaxKind::LAYOUT_START);
    }

    fn insert_sep(&mut self) {
        match &self.layout.stack[..] {
            [.., (position, Delimiter::TopDecl)] if self.sep_p(*position) => {
                self.pop_stack();
                self.insert_separator();
            }
            [.., (position, Delimiter::TopDeclHead)] if self.sep_p(*position) => {
                self.pop_stack();
                self.insert_separator();
            }
            [.., (position, delimiter)] if self.indent_sep_p(*position, *delimiter) => {
                match delimiter {
                    Delimiter::Of => {
                        self.insert_separator();
                        self.push_stack(self.position, Delimiter::CaseBinders);
                    }
                    _ => {
                        self.insert_separator();
                    }
                }
            }
            _ => (),
        }
    }

    fn insert_keyword_property(&mut self, callback: impl Fn(&mut Self)) {
        self.insert_default();
        if let Some((_, Delimiter::Property)) = self.layout.stack.last() {
            self.pop_stack();
        } else {
            callback(self)
        }
    }

    fn insert_separator(&mut self) {
        self.layout.output.push(SyntaxKind::LAYOUT_SEPARATOR);
    }

    fn insert_end(&mut self) {
        self.layout.output.push(SyntaxKind::LAYOUT_END);
    }

    fn insert_token(&mut self, token: SyntaxKind) {
        self.layout.output.push(token);
    }

    fn push_stack(&mut self, position: Position, delimiter: Delimiter) {
        self.layout.stack.push((position, delimiter));
    }

    fn pop_stack(&mut self) {
        self.layout.stack.pop();
    }

    fn pop_stack_if(&mut self, predicate: impl Fn(Delimiter) -> bool) {
        match self.layout.stack.last() {
            Some((_, delimiter)) => {
                if predicate(*delimiter) {
                    self.layout.stack.pop();
                }
            }
            None => unreachable!(),
        };
    }

    fn collapse(&self, predicate: impl Fn(&Self, Position, Delimiter) -> bool) -> Collapse {
        let mut stack_len = self.layout.stack.len();
        let mut end_tokens = 0;
        for (position, delimiter) in self.layout.stack.iter().rev() {
            if predicate(self, *position, *delimiter) {
                stack_len = stack_len.saturating_sub(1);
                if delimiter.is_indented() {
                    end_tokens += 1;
                }
            } else {
                break;
            }
        }
        Collapse { stack_len, end_tokens }
    }

    fn collapse_and_commit(&mut self, predicate: impl Fn(&Self, Position, Delimiter) -> bool) {
        self.collapse(predicate).commit(self.layout);
    }

    fn indented_p(&self, _: Position, delimiter: Delimiter) -> bool {
        delimiter.is_indented()
    }

    fn offside_p(&self, position: Position, delimiter: Delimiter) -> bool {
        delimiter.is_indented() && self.position.column < position.column
    }

    fn offside_end_p(&self, position: Position, delimiter: Delimiter) -> bool {
        delimiter.is_indented() && self.position.column <= position.column
    }

    fn indent_sep_p(&self, position: Position, delimiter: Delimiter) -> bool {
        delimiter.is_indented() && self.sep_p(position)
    }

    fn sep_p(&self, position: Position) -> bool {
        self.position.column == position.column && self.position.line != position.line
    }

    fn where_p(&self, position: Position, delimiter: Delimiter) -> bool {
        if let Delimiter::Do = delimiter { true } else { self.offside_end_p(position, delimiter) }
    }

    fn in_p(&self, _: Position, delimiter: Delimiter) -> bool {
        if let Delimiter::Let | Delimiter::Ado = delimiter {
            false
        } else {
            delimiter.is_indented()
        }
    }

    fn arrow_p(&self, position: Position, delimiter: Delimiter) -> bool {
        match delimiter {
            Delimiter::Do => true,
            Delimiter::Of => false,
            _ => self.offside_end_p(position, delimiter),
        }
    }

    fn equals_p(&self, _: Position, delimiter: Delimiter) -> bool {
        matches!(delimiter, Delimiter::Where | Delimiter::Let | Delimiter::LetStmt)
    }
}

struct Collapse {
    stack_len: usize,
    end_tokens: usize,
}

impl Collapse {
    fn preview<'s>(&self, machine: &'s Layout) -> &'s [(Position, Delimiter)] {
        &machine.stack[..self.stack_len]
    }

    fn commit(self, machine: &mut Layout) {
        machine.stack.truncate(self.stack_len);
        for _ in 0..self.end_tokens {
            machine.output.push(SyntaxKind::LAYOUT_END);
        }
    }
}

const LOWER: TokenSet = TokenSet::new(&[
    SyntaxKind::ADO,
    SyntaxKind::AS,
    SyntaxKind::CASE,
    SyntaxKind::CLASS,
    SyntaxKind::DATA,
    SyntaxKind::DERIVE,
    SyntaxKind::DO,
    SyntaxKind::ELSE,
    SyntaxKind::FALSE,
    SyntaxKind::FORALL,
    SyntaxKind::FOREIGN,
    SyntaxKind::HIDING,
    SyntaxKind::IF,
    SyntaxKind::IMPORT,
    SyntaxKind::IN,
    SyntaxKind::INFIX,
    SyntaxKind::INFIXL,
    SyntaxKind::INFIXR,
    SyntaxKind::INSTANCE,
    SyntaxKind::LET,
    SyntaxKind::LOWER,
    SyntaxKind::MODULE,
    SyntaxKind::NEWTYPE,
    SyntaxKind::NOMINAL,
    SyntaxKind::OF,
    SyntaxKind::PHANTOM,
    SyntaxKind::REPRESENTATIONAL,
    SyntaxKind::ROLE,
    SyntaxKind::THEN,
    SyntaxKind::TRUE,
    SyntaxKind::TYPE,
    SyntaxKind::WHERE,
]);

const OPERATOR: TokenSet = TokenSet::new(&[
    SyntaxKind::OPERATOR,
    SyntaxKind::MINUS,
    SyntaxKind::COLON,
    SyntaxKind::DOUBLE_PERIOD,
    SyntaxKind::LEFT_THICK_ARROW,
]);
