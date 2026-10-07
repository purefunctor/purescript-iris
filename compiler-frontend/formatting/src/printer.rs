use std::cell::RefCell;
use std::collections::HashMap;

use lexing::Lexed;
use pretty::{Arena, DocAllocator, DocBuilder};
use syntax::{SyntaxElement, SyntaxKind, SyntaxNode};

use crate::{Config, FormatError, Gap, spacing, trivia};

use SyntaxKind::*;

type Doc<'arena> = DocBuilder<'arena, Arena<'arena>, ()>;

struct Tree {
    identifier: usize,
    kind: SyntaxKind,
    start: usize,
    end: usize,
    children: Vec<Tree>,
}

impl Tree {
    fn from_syntax(
        element: SyntaxElement,
        offsets: &[u32],
        next_identifier: &mut usize,
    ) -> Option<Tree> {
        let kind = element.kind();
        if matches!(kind, Annotation | Qualifier) {
            return None;
        }
        let identifier = *next_identifier;
        *next_identifier += 1;
        let offset = u32::from(element.text_range().start());
        match element {
            SyntaxElement::Token(_) => {
                let start = offsets.partition_point(|&value| value < offset);
                Some(Tree {
                    identifier,
                    kind,
                    start,
                    end: start + usize::from(!kind.is_layout_token()),
                    children: Vec::new(),
                })
            }
            SyntaxElement::Node(node) => {
                let children = node
                    .children_with_tokens()
                    .filter_map(|element| Tree::from_syntax(element, offsets, next_identifier))
                    .collect::<Vec<_>>();
                let mut atoms = children.iter().filter(|child| child.start < child.end);
                let first = atoms.next();
                let start = first
                    .map_or(offsets.partition_point(|&value| value < offset), |child| child.start);
                let end = atoms.next_back().or(first).map_or(start, |child| child.end);
                Some(Tree { identifier, kind, start, end, children })
            }
        }
    }

    fn elements(&self) -> Vec<&Tree> {
        self.children
            .iter()
            .filter(|child| !child.kind.is_layout_token() && child.kind != END_OF_FILE)
            .collect()
    }

    fn ends_offside(&self) -> bool {
        if matches!(
            self.kind,
            DoStatements
                | CaseBranches
                | LetBindingStatements
                | ClassStatements
                | InstanceStatements
        ) {
            return self.start < self.end;
        }
        self.elements().last().is_some_and(|child| child.ends_offside())
    }

    fn has_inline_do(&self) -> bool {
        match self.kind {
            ExpressionDo | ExpressionAdo => true,
            ExpressionLetIn | ExpressionIfThenElse | ExpressionCaseOf => false,
            WhereExpression => self.children.first().is_some_and(Tree::has_inline_do),
            _ => self.children.iter().any(Tree::has_inline_do),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Context {
    margin: usize,
    following: Option<usize>,
}

struct Printer<'arena, 'source> {
    arena: &'arena Arena<'arena>,
    lexed: &'source Lexed<'source>,
    contexts: &'source [Vec<SyntaxKind>],
    config: &'source Config,
    documents: RefCell<HashMap<(usize, Context), Doc<'arena>>>,
}

pub(super) fn render(
    root: &SyntaxNode,
    lexed: &Lexed<'_>,
    contexts: &[Vec<SyntaxKind>],
    config: &Config,
) -> Result<String, FormatError> {
    let offsets = (0..lexed.len()).map(|index| lexed.info(index).qualifier).collect::<Vec<_>>();
    let mut next_identifier = 0;
    let tree = Tree::from_syntax(root.clone().into(), &offsets, &mut next_identifier).unwrap();
    let arena = Arena::new();
    let printer =
        Printer { arena: &arena, lexed, contexts, config, documents: RefCell::new(HashMap::new()) };
    let document = printer
        .boundary(0, Gap::Tight)?
        .append(printer.tree(&tree, Context { margin: 0, following: Some(0) })?)
        .append(printer.boundary(lexed.end_of_file_index(), Gap::Hard(1))?);
    Ok(document.pretty(config.line_width).to_string())
}

impl<'arena> Printer<'arena, '_> {
    fn verbatim(&self, text: &str) -> Doc<'arena> {
        if !text.contains('\n') {
            return self.arena.text(text.to_owned());
        }
        let lines = text.split('\n').map(|line| self.arena.text(line.to_owned()));
        let document = self.arena.intersperse(lines, self.arena.hardline());
        // Hardlines use the following document's indentation. Nest the entire
        // literal at zero so empty interior lines also preserve their whitespace.
        self.arena.nesting(move |margin| document.clone().nest(-(margin as isize)).into_doc())
    }

    fn gap(&self, gap: Gap) -> Doc<'arena> {
        match gap {
            Gap::Tight => self.arena.nil(),
            Gap::Space => self.arena.space(),
            Gap::BrokenSpace => self.arena.space().flat_alt(self.arena.nil()),
            Gap::Soft => self.arena.line(),
            Gap::SoftEmpty => self.arena.line_(),
            Gap::Hard(lines) => {
                // Newlines use the next command's margin. Zero-nest the final
                // hardline too, so blank lines stay empty before the body resumes.
                let arena = self.arena;
                arena.nesting(move |margin| {
                    arena
                        .concat((0..lines).map(|_| arena.hardline()))
                        .nest(-(margin as isize))
                        .into_doc()
                })
            }
        }
    }

    fn boundary(&self, index: usize, gap: Gap) -> Result<Doc<'arena>, FormatError> {
        let (comments, whitespace) = trivia(self.lexed.annotation(index).unwrap_or_default())?;
        let mut document = self.arena.nil();
        let mut forced_line = false;
        for (comment_index, comment) in comments.iter().enumerate() {
            let lines = comment.whitespace.matches('\n').count().min(2);
            let separator = if index == 0 && comment_index == 0 {
                Gap::Tight
            } else if lines > 0 || forced_line {
                let required = if comment_index == 0 {
                    if let Gap::Hard(required) = gap { required } else { 1 }
                } else {
                    1
                };
                Gap::Hard(lines.max(required))
            } else {
                Gap::Space
            };
            document = document.append(self.gap(separator)).append(self.verbatim(&comment.text));
            forced_line = comment.line;
        }
        let gap = if !comments.is_empty() {
            if comments.last().is_some_and(|comment| comment.forces_line()) {
                let lines = whitespace.matches('\n').count().clamp(1, 2);
                Gap::Hard(lines)
            } else if matches!(gap, Gap::Tight | Gap::Space | Gap::BrokenSpace) {
                Gap::Space
            } else {
                gap
            }
        } else {
            gap
        };
        let document = document.append(self.gap(gap));
        if index > 0
            && matches!(gap, Gap::Soft | Gap::SoftEmpty)
            && matches!(
                self.lexed.kind(index - 1),
                COMMA
                    | PIPE
                    | EQUAL
                    | RIGHT_ARROW
                    | LEFT_ARROW
                    | RIGHT_THICK_ARROW
                    | LEFT_THICK_ARROW
                    | DOUBLE_COLON
                    | COLON
                    | PERIOD
                    | OPERATOR
                    | MINUS
            )
        {
            let (comments, _) = trivia(self.lexed.annotation(index - 1).unwrap_or_default())?;
            if comments.last().is_some_and(|comment| comment.forces_line()) {
                // If a comment moved the separator to a new line, let the
                // operand fit beside it instead of inheriting the chain's break.
                return Ok(document.group());
            }
        }
        Ok(document)
    }

    fn fixed_gap(&self, index: usize) -> Gap {
        let previous = self.lexed.kind(index - 1);
        let current = self.lexed.kind(index);
        let context = &self.contexts[index];
        if (previous == COMMA
            && matches!(
                self.contexts[index - 1].first(),
                Some(PatternGuarded | ClassFunctionalDependencies)
            ))
            || (current == PIPE && context.contains(&ClassFunctionalDependencies))
            || (previous == RIGHT_THICK_ARROW && context.contains(&InstanceHead))
            || (previous == LEFT_THICK_ARROW && context.contains(&ClassHead))
        {
            return Gap::Soft;
        }
        spacing(self.lexed, self.contexts, index)
    }

    fn closes(&self, tree: &Tree) -> bool {
        matches!(
            self.lexed.kind(tree.start),
            COMMA | RIGHT_PARENTHESIS | RIGHT_SQUARE | RIGHT_CURLY | IN | THEN | ELSE
        )
    }

    fn sequence(
        &self,
        elements: &[&Tree],
        context: Context,
        separator: impl Fn(usize, &Tree, &Tree) -> Gap,
    ) -> Result<Doc<'arena>, FormatError> {
        let mut document = self.arena.nil();
        let mut previous = None;
        for (position, &element) in elements.iter().enumerate() {
            if element.start == element.end {
                continue;
            }
            if let Some(previous) = previous {
                // Layout bodies own the break before their first atom.
                if !is_block(element.kind) && element.kind != Conditionals {
                    document = document.append(
                        self.boundary(element.start, separator(position, previous, element))?,
                    );
                }
            }
            let following = elements[position + 1..].iter().find(|tree| tree.start < tree.end);
            let following = following.map_or(context.following, |tree| {
                if self.closes(tree) { None } else { Some(context.margin) }
            });
            document = document.append(self.tree(element, Context { following, ..context })?);
            previous = Some(element);
        }
        Ok(document)
    }

    fn fixed(&self, elements: &[&Tree], context: Context) -> Result<Doc<'arena>, FormatError> {
        let elements = elements.iter().copied().filter(|tree| tree.start < tree.end);
        let elements = elements.collect::<Vec<_>>();
        if elements.len() <= 1 {
            return self.sequence(&elements, context, |_, _, _| Gap::Tight);
        }
        self.chain(
            &elements,
            1,
            context,
            context.margin + self.config.indent_width,
            |_, _, current| self.fixed_gap(current.start),
        )
    }

    fn statements(&self, elements: &[&Tree], context: Context) -> Result<Doc<'arena>, FormatError> {
        self.sequence(elements, context, |_, _, current| {
            let annotation = self.lexed.annotation(current.start).unwrap_or_default();
            let whitespace = annotation.len() - annotation.trim_start().len();
            Gap::Hard(annotation[..whitespace].matches('\n').count().clamp(1, 2))
        })
    }

    fn block(
        &self,
        elements: &[&Tree],
        context: Context,
        margin: usize,
    ) -> Result<Doc<'arena>, FormatError> {
        let Some(first) = elements.iter().find(|element| element.start < element.end) else {
            return Ok(self.arena.nil());
        };
        let body = self.statements(elements, Context { margin, following: Some(margin) })?;
        Ok(self
            .boundary(first.start, Gap::Hard(1))?
            .append(body)
            .nest((margin - context.margin) as isize))
    }

    fn continuation(
        &self,
        head: &[&Tree],
        tail: &[&Tree],
        context: Context,
    ) -> Result<Doc<'arena>, FormatError> {
        if let [expression] = tail
            && expression.kind == WhereExpression
        {
            return self.where_expression(head, &expression.elements(), context);
        }
        let Some(first) = tail.first() else {
            return self.fixed(head, context);
        };
        let margin = context.margin + self.config.indent_width;
        let head = self.fixed(head, Context { following: Some(margin), ..context })?.group();
        if tail.iter().any(|tree| tree.has_inline_do()) {
            let inline = self.fixed(tail, context)?;
            let broken = self.fixed(tail, Context { margin, ..context })?;
            return Ok(head.append(self.attach(first.start, Gap::Soft, inline, broken)?));
        }
        let tail = self.fixed(tail, Context { margin, ..context })?;
        Ok(head
            .append(
                self.boundary(first.start, Gap::Soft)?
                    .append(tail)
                    .group()
                    .nest(self.config.indent_width as isize),
            )
            .group())
    }

    fn attach(
        &self,
        index: usize,
        gap: Gap,
        inline: Doc<'arena>,
        broken: Doc<'arena>,
    ) -> Result<Doc<'arena>, FormatError> {
        let arena = self.arena;
        let indent = self.config.indent_width;
        let (comments, _) = trivia(self.lexed.annotation(index).unwrap_or_default())?;
        if matches!(gap, Gap::Hard(_))
            || comments.last().is_some_and(|comment| comment.forces_line())
        {
            // A mandatory break can nest the whole separator, keeping standalone
            // comments aligned with the continuation they precede.
            return Ok(self.boundary(index, gap)?.append(broken).nest(indent as isize));
        }
        if matches!(gap, Gap::Tight | Gap::Space | Gap::BrokenSpace) {
            return Ok(self.boundary(index, gap)?.append(inline));
        }
        // Group only the separator: the block's mandatory newlines and indivisible
        // body atoms must not force its keyword off a header that fits. A break
        // temporarily lands at column zero, which no nonempty inline prefix can
        // reach. Restore the chosen body's planned margin after that decision.
        let separator = self.boundary(index, gap)?.group();
        Ok(arena.nesting(move |margin| {
            let inline = inline.clone();
            let broken = broken.clone();
            let body = arena.column(move |column| {
                if column == 0 {
                    arena
                        .text(" ".repeat(margin + indent))
                        .append(broken.clone())
                        .nest((margin + indent) as isize)
                        .into_doc()
                } else {
                    inline.clone().nest(margin as isize).into_doc()
                }
            });
            separator.clone().append(body).nest(-(margin as isize)).into_doc()
        }))
    }

    fn application(
        &self,
        elements: &[&Tree],
        head_length: usize,
        context: Context,
    ) -> Result<Doc<'arena>, FormatError> {
        self.chain(
            elements,
            head_length,
            context,
            context.margin + self.config.indent_width,
            |_, _, _| Gap::Soft,
        )
    }

    fn chain(
        &self,
        elements: &[&Tree],
        head_length: usize,
        context: Context,
        margin: usize,
        separator: impl Fn(usize, &Tree, &Tree) -> Gap,
    ) -> Result<Doc<'arena>, FormatError> {
        if let Some((body, header)) = elements.split_last()
            && is_block(body.kind)
        {
            // A body's mandatory lines must not force an otherwise fitting
            // declaration header or block keyword to break.
            return Ok(self
                .chain(header, head_length, context, margin, separator)?
                .append(self.tree(body, context)?));
        }
        if elements.is_empty() {
            return Ok(self.arena.nil());
        }
        let (head, tail) = elements.split_at(head_length.min(elements.len()));
        if tail.is_empty() {
            return self.fixed(head, context);
        }
        let head_context = Context { following: Some(margin), ..context };
        let head_document = if let [keyword, constraints] = head
            && keyword.kind == CLASS
            && constraints.kind == ClassConstraints
            && constraints.children.first().is_some_and(|tree| tree.kind == LEFT_PARENTHESIS)
        {
            self.delimited(constraints, head_context, Some((keyword, tail[0])))?
        } else {
            self.fixed(head, head_context)?
        };
        let gap = |position: usize| {
            let previous = if position == 0 { head.last().unwrap() } else { tail[position - 1] };
            let element = tail[position];
            if previous.ends_offside() && !self.closes(element) {
                Gap::Hard(1)
            } else {
                separator(head.len() + position, previous, element)
            }
        };
        let fixed_gaps =
            (0..tail.len()).all(|position| !matches!(gap(position), Gap::Soft | Gap::SoftEmpty));
        if fixed_gaps
            || elements.iter().any(|tree| tree.has_inline_do())
            || head.last().is_some_and(|tree| tree.kind == ClassConstraints)
        {
            let mut inline_suffix = self.arena.nil();
            let mut broken_suffix = self.arena.nil();
            for position in (0..tail.len()).rev() {
                let element = tail[position];
                let gap = gap(position);
                let following =
                    if position + 1 < tail.len() { Some(margin) } else { context.following };
                let inline =
                    self.tree(element, Context { following, ..context })?.append(inline_suffix);
                let broken =
                    self.tree(element, Context { margin, following })?.append(broken_suffix);
                if is_block(element.kind) || element.kind == Conditionals {
                    inline_suffix = inline;
                    broken_suffix = broken;
                } else {
                    inline_suffix = self.attach(element.start, gap, inline, broken.clone())?;
                    broken_suffix = self.boundary(element.start, gap)?.group().append(broken);
                }
            }
            return Ok(head_document.append(inline_suffix));
        }
        let body =
            self.sequence(tail, Context { margin, ..context }, |position, _, _| gap(position))?;
        let body = if matches!(gap(0), Gap::Soft) { body } else { body.group() };
        Ok(head_document
            .append(
                self.boundary(tail[0].start, gap(0))?
                    .append(body)
                    .nest((margin - context.margin) as isize),
            )
            .group())
    }

    fn delimited(
        &self,
        tree: &Tree,
        context: Context,
        prefix: Option<(&Tree, &Tree)>,
    ) -> Result<Doc<'arena>, FormatError> {
        let elements = tree.elements();
        let opening = elements
            .iter()
            .position(|tree| matches!(tree.kind, LEFT_PARENTHESIS | LEFT_SQUARE | LEFT_CURLY));
        let closing = elements
            .iter()
            .rposition(|tree| matches!(tree.kind, RIGHT_PARENTHESIS | RIGHT_SQUARE | RIGHT_CURLY));
        let Some((opening, closing)) = opening.zip(closing) else {
            return self.fixed(&elements, context);
        };
        // Coordinates are relative to the opener, including inline openers.
        // Items start after the two-column `{ ` or `, ` prefix; only their
        // continuations use the configured indentation width.
        let item_context = Context { margin: 2, following: None };
        let punctuation_context = Context { margin: 0, following: None };
        let mut document = self.tree(elements[opening], punctuation_context)?;
        let items = elements[opening + 1..closing].iter().flat_map(|element| {
            if element.kind == TypeRowTail { element.elements() } else { vec![*element] }
        });
        let items = items.collect::<Vec<_>>();
        for (position, element) in items.iter().enumerate() {
            let punctuation = matches!(element.kind, COMMA | PIPE);
            let gap = if position == 0 {
                if elements[opening].kind == LEFT_PARENTHESIS {
                    Gap::BrokenSpace
                } else {
                    Gap::Space
                }
            } else if element.kind == COMMA {
                Gap::SoftEmpty
            } else if element.kind == PIPE {
                Gap::Soft
            } else {
                self.fixed_gap(element.start)
            };
            let boundary = self.boundary(element.start, gap)?;
            let body = if punctuation {
                boundary.append(self.tree(element, punctuation_context)?)
            } else if position > 0 && items[position - 1].kind == PIPE {
                // A row tail can start beside the opener or after a comment,
                // so its continuation must follow the type's actual column.
                boundary.append(self.tree(element, punctuation_context)?.align()).nest(2)
            } else {
                boundary.append(self.tree(element, item_context)?).nest(2)
            };
            document = document.append(body);
        }
        let edge = if items.is_empty() {
            Gap::Tight
        } else if matches!(elements[opening].kind, LEFT_CURLY | LEFT_SQUARE) {
            Gap::Soft
        } else {
            Gap::SoftEmpty
        };
        document = document
            .append(self.boundary(elements[closing].start, edge)?)
            .append(self.tree(elements[closing], punctuation_context)?);
        if prefix.is_none() && !matches!(tree.kind, ExportList | TypeItemsList) {
            document = document.group();
        }
        let mut document = document.align();
        if let Some((prefix, head)) = prefix {
            let keyword = self.tree(prefix, context)?;
            let broken = keyword.clone().append(
                self.boundary(elements[opening].start, Gap::Hard(1))?
                    .append(document.clone())
                    .nest(self.config.indent_width as isize),
            );
            document = keyword.append(
                self.boundary(elements[opening].start, Gap::Soft)?
                    .append(document)
                    .nest(self.config.indent_width as isize),
            );
            // Fit the complete class head before allowing its parameters to wrap.
            let probe =
                self.arena.fail().flat_alt(self.fixed(&[prefix, tree, head], context)?).group();
            let width = self.config.line_width;
            let inline = document.group();
            document = self.arena.column(move |column| {
                let mut rendered = String::new();
                if probe.render_fmt(width.saturating_sub(column), &mut rendered).is_ok() {
                    inline.clone().into_doc()
                } else {
                    broken.clone().into_doc()
                }
            });
        }
        if opening > 0 {
            document = self
                .fixed(&elements[..opening], context)?
                .append(self.boundary(elements[opening].start, Gap::Space)?)
                .append(document);
        }
        if closing + 1 < elements.len() {
            let margin = context.margin + self.config.indent_width;
            document = document.append(
                self.boundary(
                    elements[closing + 1].start,
                    self.fixed_gap(elements[closing + 1].start),
                )?
                .append(self.fixed(&elements[closing + 1..], Context { margin, ..context })?)
                .nest(self.config.indent_width as isize),
            );
        }
        Ok(document)
    }

    fn arrows(&self, tree: &Tree, context: Context) -> Result<Doc<'arena>, FormatError> {
        let mut elements = tree.elements();
        while elements.last().is_some_and(|tree| matches!(tree.kind, TypeArrow | TypeConstrained)) {
            let last = elements.pop().unwrap();
            elements.extend(last.elements());
        }
        self.chain(&elements, 1, context, context.margin, |position, _, current| {
            if position % 2 == 0 { Gap::Soft } else { self.fixed_gap(current.start) }
        })
    }

    fn operators(&self, elements: &[&Tree], context: Context) -> Result<Doc<'arena>, FormatError> {
        if elements.len() == 1 {
            return self.tree(elements[0], context);
        }
        let margin = context.margin + self.config.indent_width;
        let head = self
            .tree(elements[0], Context { margin, following: Some(margin) })?
            .nest(self.config.indent_width as isize);
        let operand_document = |operand: &Tree, context: Context| {
            let document = self.tree(operand, context)?;
            let document = if operand.has_inline_do() { document } else { document.align() };
            Ok::<_, FormatError>(document)
        };
        let pair = |operator: &Tree, operand: &Tree, context: Context| {
            Ok::<_, FormatError>(
                self.tree(operator, Context { following: Some(margin), ..context })?
                    .append(self.boundary(operand.start, Gap::Space)?)
                    .append(operand_document(operand, context)?),
            )
        };
        let gap = |position: usize| {
            if elements[position - 1].ends_offside() { Gap::Hard(1) } else { Gap::Soft }
        };
        if elements.iter().any(|tree| tree.has_inline_do()) {
            let mut inline_suffix = self.arena.nil();
            let mut broken_suffix = self.arena.nil();
            for position in (1..elements.len()).step_by(2).rev() {
                let operator = elements[position];
                let operand = elements[position + 1];
                let following =
                    if position + 2 < elements.len() { Some(margin) } else { context.following };
                let inline = operand_document(operand, Context { following, ..context })?
                    .append(inline_suffix);
                let broken =
                    operand_document(operand, Context { margin, following })?.append(broken_suffix);
                let inline = self
                    .tree(operator, Context { following: Some(margin), ..context })?
                    .append(self.attach(operand.start, Gap::Space, inline, broken.clone())?);
                let broken = self
                    .tree(operator, Context { margin, following: Some(margin) })?
                    .append(self.boundary(operand.start, Gap::Space)?.group())
                    .append(broken);
                inline_suffix =
                    self.attach(operator.start, gap(position), inline, broken.clone())?;
                broken_suffix =
                    self.boundary(operator.start, gap(position))?.group().append(broken);
            }
            return Ok(head.append(inline_suffix));
        }
        let mut document = head;
        for position in (1..elements.len()).step_by(2) {
            let operator = elements[position];
            let operand = elements[position + 1];
            let following =
                if position + 2 < elements.len() { Some(margin) } else { context.following };
            document = document.append(
                self.boundary(operator.start, gap(position))?
                    .append(pair(operator, operand, Context { margin, following })?)
                    .nest(self.config.indent_width as isize),
            );
        }
        Ok(document.group())
    }

    fn conditional(
        &self,
        elements: &[&Tree],
        context: Context,
    ) -> Result<Doc<'arena>, FormatError> {
        let mut document = self.arena.nil();
        for clause in elements.chunks(2) {
            if clause[0].kind != IF {
                document = document.append(self.boundary(clause[0].start, Gap::Soft)?);
            }
            document = document.append(self.continuation(
                &clause[..1],
                &clause[1..],
                Context { following: None, ..context },
            )?);
        }
        Ok(document.group())
    }

    fn where_expression(
        &self,
        head: &[&Tree],
        elements: &[&Tree],
        context: Context,
    ) -> Result<Doc<'arena>, FormatError> {
        let Some(position) = elements.iter().position(|tree| tree.kind == WHERE) else {
            return if head.is_empty() {
                self.fixed(elements, context)
            } else {
                self.continuation(head, elements, context)
            };
        };
        let margin = context.margin.max(context.following.unwrap_or(0) + self.config.indent_width);
        let expression_context =
            Context { following: Some(margin - self.config.indent_width), ..context };
        let expression = if head.is_empty() {
            self.fixed(&elements[..position], expression_context)?
        } else {
            self.continuation(head, &elements[..position], expression_context)?
        };
        let binding_context = Context { margin, following: Some(margin) };
        let bindings = self.block(&elements[position + 1].elements(), binding_context, margin)?;
        Ok(expression.append(
            self.boundary(elements[position].start, Gap::Hard(1))?
                .append(self.tree(elements[position], binding_context)?)
                .append(bindings)
                .nest((margin - context.margin) as isize),
        ))
    }

    fn tree(&self, tree: &Tree, context: Context) -> Result<Doc<'arena>, FormatError> {
        if let Some(document) = self.documents.borrow().get(&(tree.identifier, context)) {
            return Ok(document.clone());
        }
        // Inline and broken alternatives revisit nested blocks at the same margins.
        // Sharing their arena documents avoids an exponential construction tree.
        let document = self.tree_document(tree, context)?;
        self.documents.borrow_mut().insert((tree.identifier, context), document.clone());
        Ok(document)
    }

    fn tree_document(&self, tree: &Tree, context: Context) -> Result<Doc<'arena>, FormatError> {
        if tree.children.is_empty() {
            if tree.start == tree.end {
                return Ok(self.arena.nil());
            }
            return Ok(self.verbatim(&format!(
                "{}{}",
                self.lexed.qualifier(tree.start).unwrap_or_default(),
                self.config.token_text(self.lexed, &self.contexts[tree.start], tree.start)
            )));
        }
        let mut elements = tree.elements();
        match tree.kind {
            Module => self.sequence(&elements, context, |_, _, _| Gap::Hard(2)),
            ModuleImports | ModuleStatements => self.statements(&elements, context),
            ImportType | ExportType => {
                let document = self.sequence(&elements, context, |_, _, current| {
                    if current.kind == TypeItemsList {
                        Gap::BrokenSpace
                    } else {
                        self.fixed_gap(current.start)
                    }
                })?;
                Ok(document.group())
            }
            DoStatements | CaseBranches | LetBindingStatements | ClassStatements
            | InstanceStatements | Conditionals => {
                // A layout body must exceed the planned following continuation,
                // so `use do … argument` cannot absorb the later argument.
                let margin =
                    context.margin.max(context.following.unwrap_or(0)) + self.config.indent_width;
                self.block(&elements, context, margin)
            }
            ExpressionApplicationChain
            | TypeApplicationChain
            | ExpressionRecordUpdate
            | RecordUpdateBranch
            | BinderConstructor
            | InstanceHead
            | ClassHead
            | FunctionBinders => self.application(&elements, 1, context),
            DataConstructor => Ok(self.application(&elements, 1, context)?.align()),
            ExpressionOperatorChain
            | ExpressionInfixChain
            | TypeOperatorChain
            | BinderOperatorChain => {
                let elements = elements.into_iter().flat_map(|tree| {
                    if matches!(
                        tree.kind,
                        ExpressionOperatorPair
                            | ExpressionInfixPair
                            | TypeOperatorPair
                            | BinderOperatorPair
                    ) {
                        tree.elements()
                    } else {
                        vec![tree]
                    }
                });
                let elements = elements.collect::<Vec<_>>();
                self.operators(&elements, context)
            }
            TypeArrow | TypeConstrained => self.arrows(tree, context),
            FunctionalDependencyDetermined | FunctionalDependencyDetermines => self.chain(
                &elements,
                1,
                context,
                context.margin + self.config.indent_width,
                |_, _, current| {
                    if current.kind == RIGHT_ARROW { Gap::Soft } else { Gap::Space }
                },
            ),
            DataEquation => {
                if let Some(separator) = elements.iter().position(|tree| tree.kind == EQUAL) {
                    self.chain(
                        &elements,
                        separator,
                        context,
                        context.margin + self.config.indent_width,
                        |_, _, current| {
                            if matches!(current.kind, EQUAL | PIPE) {
                                Gap::Soft
                            } else {
                                self.fixed_gap(current.start)
                            }
                        },
                    )
                } else {
                    self.fixed(&elements, context)
                }
            }
            TypeForall => {
                let position = elements.iter().position(|tree| tree.kind == PERIOD).unwrap();
                let head = self.fixed(&elements[..=position], context)?.group();
                Ok(head
                    .append(self.boundary(elements[position + 1].start, Gap::Soft)?)
                    .append(self.fixed(&elements[position + 1..], context)?)
                    .group())
            }
            ExpressionArray
            | ExpressionRecord
            | BinderArray
            | BinderRecord
            | TypeRecord
            | TypeRow
            | RecordUpdates
            | ExpressionParenthesized
            | BinderParenthesized
            | TypeParenthesized
            | ExportList
            | ImportList
            | TypeItemsList
            | InstanceConstraints
            | ClassConstraints => self.delimited(tree, context, None),
            ExpressionIfThenElse => self.conditional(&elements, context),
            ValueEquation
            | LetBindingEquation
            | LetBindingPattern
            | InstanceEquationStatement
            | CaseBranch => {
                if let Some(last) = elements.pop_if(|tree| tree.kind == Unconditional) {
                    elements.extend(last.elements());
                }
                if let Some(separator) =
                    elements.iter().position(|tree| matches!(tree.kind, EQUAL | RIGHT_ARROW))
                {
                    self.continuation(&elements[..=separator], &elements[separator + 1..], context)
                } else {
                    self.fixed(&elements, context)
                }
            }
            ValueSignature
            | LetBindingSignature
            | InstanceSignatureStatement
            | ClassMemberStatement
            | TypeSynonymSignature
            | DataSignature
            | NewtypeSignature
            | ClassSignature
            | ForeignImportDataDeclaration
            | ForeignImportValueDeclaration
            | ExpressionTyped
            | BinderTyped
            | TypeKinded
            | TypeRowItem
            | RecordField
            | RecordUpdateLeaf
            | DoStatementBind
            | PatternGuardBinder
            | ExpressionLambda
            | PatternGuarded
            | TypeSynonymEquation
            | NewtypeEquation => {
                if matches!(elements.first().map(|tree| tree.kind), Some(LEFT_PARENTHESIS)) {
                    return self.delimited(tree, context, None);
                }
                let separator = elements.iter().position(|tree| {
                    matches!(
                        tree.kind,
                        DOUBLE_COLON | COLON | EQUAL | LEFT_ARROW | RIGHT_ARROW | PERIOD
                    )
                });
                if let Some(separator) = separator {
                    self.continuation(&elements[..=separator], &elements[separator + 1..], context)
                } else {
                    self.fixed(&elements, context)
                }
            }
            WhereExpression => self.where_expression(&[], &elements, context),
            ExpressionLetIn | ExpressionAdo => {
                let position = elements.iter().position(|tree| tree.kind == IN).unwrap();
                let has_body = elements[1..position].iter().any(|tree| tree.start < tree.end);
                let head =
                    self.fixed(&elements[..position], Context { following: None, ..context })?;
                let result_margin = if tree.kind == ExpressionAdo {
                    context.margin + self.config.indent_width
                } else {
                    context.margin
                };
                let result_context = Context { margin: result_margin, ..context };
                let tail = if tree.kind == ExpressionLetIn {
                    let body_context =
                        Context { margin: context.margin + self.config.indent_width, ..context };
                    self.tree(elements[position], context)?.append(
                        self.boundary(elements[position + 1].start, Gap::Hard(1))?
                            .append(self.fixed(&elements[position + 1..], body_context)?)
                            .nest(self.config.indent_width as isize),
                    )
                } else {
                    self.continuation(
                        &elements[position..=position],
                        &elements[position + 1..],
                        result_context,
                    )?
                };
                Ok(head
                    .append(
                        self.boundary(
                            elements[position].start,
                            if has_body { Gap::Hard(1) } else { Gap::Space },
                        )?
                        .append(tail)
                        .nest((result_margin - context.margin) as isize),
                    )
                    .group())
            }
            InstanceChain => self.sequence(&elements, context, |_, _, _| Gap::Hard(1)),
            InstanceDeclaration | ClassDeclaration | DeriveDeclaration => {
                if elements.first().is_some_and(|tree| tree.kind == ELSE)
                    && tree.children.iter().any(|tree| tree.kind == LAYOUT_SEPARATOR)
                {
                    let tail = self
                        .boundary(elements[1].start, Gap::Hard(1))?
                        .append(self.fixed(&elements[1..], context)?);
                    Ok(self.tree(elements[0], context)?.append(tail))
                } else if let Some(head) =
                    elements.iter().position(|tree| matches!(tree.kind, InstanceHead | ClassHead))
                    && elements[..head]
                        .iter()
                        .any(|tree| matches!(tree.kind, InstanceConstraints | ClassConstraints))
                {
                    self.chain(
                        &elements,
                        head,
                        context,
                        context.margin + self.config.indent_width,
                        |_, _, current| self.fixed_gap(current.start),
                    )
                } else {
                    self.fixed(&elements, context)
                }
            }
            ModuleHeader | ImportStatement => self.application(&elements, 2, context),
            _ => self.fixed(&elements, context),
        }
    }
}

fn is_block(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        DoStatements | CaseBranches | LetBindingStatements | ClassStatements | InstanceStatements
    )
}
