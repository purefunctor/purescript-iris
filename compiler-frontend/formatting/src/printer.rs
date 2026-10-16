use lexing::Lexed;
use syntax::{SyntaxElement, SyntaxKind, SyntaxNode};

use crate::layout::{self, Choice, Document};
use crate::{Config, FormatError, Gap, spacing, trivia};

use SyntaxKind::*;

struct Tree {
    kind: SyntaxKind,
    start: usize,
    end: usize,
    children: Vec<Tree>,
}

impl Tree {
    fn from_syntax(element: SyntaxElement, offsets: &[u32]) -> Option<Tree> {
        let kind = element.kind();
        if matches!(kind, Annotation | Qualifier) {
            return None;
        }
        let offset = u32::from(element.text_range().start());
        match element {
            SyntaxElement::Token(_) => {
                let start = offsets.partition_point(|&value| value < offset);
                let end = start + usize::from(!kind.is_layout_token());
                Some(Tree { kind, start, end, children: Vec::new() })
            }
            SyntaxElement::Node(node) => {
                let children = node
                    .children_with_tokens()
                    .filter_map(|element| Tree::from_syntax(element, offsets))
                    .collect::<Vec<_>>();
                let mut atoms = children.iter().filter(|child| !child.is_empty());
                let first = atoms.next();
                let start = first
                    .map_or(offsets.partition_point(|&value| value < offset), |child| child.start);
                let end = atoms.next_back().or(first).map_or(start, |child| child.end);
                Some(Tree { kind, start, end, children })
            }
        }
    }

    fn is_empty(&self) -> bool {
        self.start == self.end
    }

    fn elements(&self) -> Vec<&Tree> {
        self.children
            .iter()
            .filter(|child| !child.kind.is_layout_token() && child.kind != END_OF_FILE)
            .collect()
    }

    fn owns_break(&self) -> bool {
        is_block(self.kind) || self.kind == Conditionals
    }

    fn ends_offside(&self) -> bool {
        if is_block(self.kind) {
            return !self.is_empty();
        }
        self.elements().last().is_some_and(|child| child.ends_offside())
    }

    fn is_bracketed(&self) -> bool {
        match self.kind {
            ExpressionArray | ExpressionRecord | ExpressionParenthesized => true,
            ExpressionTermArgument => {
                self.elements().last().is_some_and(|child| child.is_bracketed())
            }
            _ => false,
        }
    }

    fn hangs_from_operator(&self) -> bool {
        matches!(self.kind, ExpressionLambda | ExpressionDo | ExpressionAdo | ExpressionCaseOf)
    }
}

struct Printer<'source> {
    lexed: &'source Lexed<'source>,
    contexts: &'source [Vec<SyntaxKind>],
    config: &'source Config,
}

pub(super) fn render(
    root: &SyntaxNode,
    lexed: &Lexed<'_>,
    contexts: &[Vec<SyntaxKind>],
    config: &Config,
) -> Result<String, FormatError> {
    let offsets = (0..lexed.len()).map(|index| lexed.info(index).qualifier).collect::<Vec<_>>();
    let tree = Tree::from_syntax(root.clone().into(), &offsets).unwrap();
    let printer = Printer { lexed, contexts, config };
    let document = Document::concat([
        printer.boundary(0, Gap::Tight)?,
        printer.tree(&tree)?,
        printer.boundary(lexed.end_of_file_index(), Gap::Hard(1))?,
    ]);
    Ok(layout::render(&document, config.line_width, config.indent_width))
}

impl Printer<'_> {
    fn indent(&self) -> usize {
        self.config.indent_width
    }

    fn token(&self, index: usize) -> Document {
        let qualifier = self.lexed.qualifier(index).unwrap_or_default();
        let text = self.config.token_text(self.lexed, &self.contexts[index], index);
        Document::verbatim(&format!("{qualifier}{text}"))
    }

    fn gap(&self, gap: Gap) -> Document {
        match gap {
            Gap::Tight => Document::empty(),
            Gap::Space => Document::text(" "),
            Gap::BrokenSpace => Document::switch(Document::empty(), Document::text(" ")),
            Gap::Soft => Document::line(),
            Gap::SoftEmpty => Document::line_or_empty(),
            Gap::Hard(lines) => Document::newlines(lines),
        }
    }

    fn forces_line(&self, index: usize) -> Result<bool, FormatError> {
        let (comments, _) = trivia(self.lexed.annotation(index).unwrap_or_default())?;
        Ok(comments.last().is_some_and(|comment| comment.forces_line()))
    }

    fn boundary(&self, index: usize, gap: Gap) -> Result<Document, FormatError> {
        let (comments, whitespace) = trivia(self.lexed.annotation(index).unwrap_or_default())?;
        let mut documents = Vec::new();
        let mut forced_line = false;
        for (comment_index, comment) in comments.iter().enumerate() {
            let lines = comment.whitespace.matches('\n').count().min(2);
            let separator = if index == 0 && comment_index == 0 {
                Gap::Tight
            } else if lines > 0 || forced_line {
                let required = match gap {
                    Gap::Hard(required) if comment_index == 0 => required,
                    _ => 1,
                };
                Gap::Hard(lines.max(required))
            } else {
                Gap::Space
            };
            documents.push(self.gap(separator));
            documents.push(if comment.line {
                Document::line_comment(&comment.text)
            } else {
                Document::verbatim(&comment.text)
            });
            forced_line = comment.line;
        }
        let gap = match comments.last() {
            None => gap,
            Some(comment) if comment.forces_line() => {
                Gap::Hard(whitespace.matches('\n').count().clamp(1, 2))
            }
            Some(_) if matches!(gap, Gap::Tight | Gap::Space | Gap::BrokenSpace) => Gap::Space,
            Some(_) => gap,
        };
        documents.push(self.gap(gap));
        let document = Document::concat(documents);
        let separates = index > 0
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
            );
        if separates && self.forces_line(index - 1)? {
            return Ok(document.group());
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

    fn element_gap(&self, previous: &Tree, element: &Tree) -> Gap {
        if previous.ends_offside() && !self.closes(element) {
            Gap::Hard(1)
        } else {
            self.fixed_gap(element.start)
        }
    }

    fn sequence(
        &self,
        elements: &[&Tree],
        separator: impl Fn(&Tree) -> Gap,
    ) -> Result<Document, FormatError> {
        let mut documents = Vec::new();
        for (position, element) in elements.iter().filter(|tree| !tree.is_empty()).enumerate() {
            if position > 0 && !element.owns_break() {
                documents.push(self.boundary(element.start, separator(element))?);
            }
            documents.push(self.tree(element)?);
        }
        Ok(Document::concat(documents))
    }

    fn statements(&self, elements: &[&Tree]) -> Result<Document, FormatError> {
        self.sequence(elements, |element| {
            let annotation = self.lexed.annotation(element.start).unwrap_or_default();
            let whitespace = annotation.len() - annotation.trim_start().len();
            Gap::Hard(annotation[..whitespace].matches('\n').count().clamp(1, 2))
        })
    }

    fn block(&self, elements: &[&Tree]) -> Result<Document, FormatError> {
        let Some(first) = elements.iter().find(|element| !element.is_empty()) else {
            return Ok(Document::empty());
        };
        let body = self.boundary(first.start, Gap::Hard(1))?.append(self.statements(elements)?);
        Ok(body.block())
    }

    fn spaced(&self, elements: &[&Tree]) -> Result<Document, FormatError> {
        let elements = elements.iter().copied().filter(|tree| !tree.is_empty());
        let elements = elements.collect::<Vec<_>>();
        let mut documents = Vec::new();
        let mut soft = false;
        for (position, element) in elements.iter().enumerate() {
            if position > 0 && !element.owns_break() {
                let gap = self.element_gap(elements[position - 1], element);
                soft |= matches!(gap, Gap::Soft | Gap::SoftEmpty);
                documents.push(self.boundary(element.start, gap)?.nest(self.indent()));
            }
            let document = self.tree(element)?;
            let last = position + 1 == elements.len();
            documents.push(if last { document } else { document.continued() });
        }
        let document = Document::concat(documents);
        Ok(if soft { document.group() } else { document })
    }

    fn application(&self, elements: &[&Tree], head_length: usize) -> Result<Document, FormatError> {
        let (head, tail) = elements.split_at(head_length.min(elements.len()));
        let tail = tail.iter().copied().filter(|tree| !tree.is_empty()).collect::<Vec<_>>();
        if tail.is_empty() {
            return self.spaced(head);
        }
        let mut documents = Vec::new();
        for (position, element) in tail.iter().enumerate() {
            let previous =
                if position == 0 { head.last().copied() } else { Some(tail[position - 1]) };
            let gap =
                if previous.is_some_and(Tree::ends_offside) { Gap::Hard(1) } else { Gap::Soft };
            documents.push(self.boundary(element.start, gap)?);
            documents.push(self.tree(element)?);
        }
        let tail = Document::concat(documents).nest(self.indent());
        Ok(self.spaced(head)?.append(tail).own_line_group())
    }

    fn call(&self, elements: &[&Tree]) -> Result<Document, FormatError> {
        let [function, arguments @ ..] = elements else {
            unreachable!("invariant violated: validated application has a function");
        };
        let mut items = vec![self.tree(function)?];
        for argument in arguments {
            let separator = self.boundary(argument.start, Gap::Soft)?;
            items.push(if argument.is_bracketed() { separator.hang_point() } else { separator });
            items.push(self.tree(argument)?);
        }
        Ok(Document::fill(items))
    }

    fn operators(&self, elements: &[&Tree]) -> Result<Document, FormatError> {
        let [first, pairs @ ..] = elements else {
            unreachable!("invariant violated: operator chains have operands");
        };
        let head = self.tree(first)?;
        if pairs.is_empty() {
            return Ok(head);
        }
        let mut prefix = vec![head.clone()];
        let mut leading = Vec::new();
        let mut last_operand = Document::empty();
        let mut last_pair = Document::empty();
        for pair in pairs.chunks(2) {
            let [operator, operand] = pair else {
                unreachable!("invariant violated: operators are followed by operands");
            };
            let operator_document = self.tree(operator)?;
            let operand_document = self.tree(operand)?;
            let before_operand = self.boundary(operand.start, Gap::Space)?;
            prefix.push(last_operand);
            prefix.push(self.boundary(operator.start, Gap::Space)?);
            prefix.push(operator_document.clone());
            prefix.push(before_operand.clone());
            last_operand = operand_document.clone();
            last_pair = Document::concat([
                operator_document.clone(),
                before_operand.clone(),
                operand_document.clone(),
            ]);
            let operand_document = if operand.hangs_from_operator() {
                operand_document
            } else {
                operand_document.align()
            };
            leading.push(Document::concat([
                self.boundary(operator.start, Gap::Soft)?,
                operator_document,
                before_operand,
                operand_document,
            ]));
        }
        let prefix = Document::concat(prefix);
        let flat = prefix.clone().append(last_operand.clone()).flat();
        let hanging = prefix.flat().append(last_operand.hanging(last_pair));
        let leading =
            head.nest(self.indent()).append(Document::concat(leading).nest(self.indent()));
        Ok(Document::select(Choice::FirstFitting, vec![flat, hanging, leading]))
    }

    fn arrows(&self, tree: &Tree) -> Result<Document, FormatError> {
        let mut elements = tree.elements();
        while elements.last().is_some_and(|tree| matches!(tree.kind, TypeArrow | TypeConstrained)) {
            let last = elements.pop().unwrap();
            elements.extend(last.elements());
        }
        let [first, rest @ ..] = elements.as_slice() else {
            unreachable!("invariant violated: arrow chains have operands");
        };
        let mut tail = Vec::new();
        for (position, element) in rest.iter().enumerate() {
            let gap = if position % 2 == 0 {
                self.element_gap(elements[position], element)
            } else {
                Gap::Soft
            };
            tail.push(self.boundary(element.start, gap)?);
            tail.push(self.tree(element)?);
        }
        let tail = Document::concat(tail).group();
        Ok(self.tree(first)?.append(tail).own_line_group())
    }

    fn delimited(&self, tree: &Tree, grouped: bool) -> Result<Document, FormatError> {
        let elements = tree.elements();
        let opening = elements
            .iter()
            .position(|tree| matches!(tree.kind, LEFT_PARENTHESIS | LEFT_SQUARE | LEFT_CURLY));
        let closing = elements
            .iter()
            .rposition(|tree| matches!(tree.kind, RIGHT_PARENTHESIS | RIGHT_SQUARE | RIGHT_CURLY));
        let Some((opening, closing)) = opening.zip(closing) else {
            return self.spaced(&elements);
        };
        let opener = elements[opening];
        let items = elements[opening + 1..closing].iter().flat_map(|element| {
            if element.kind == TypeRowTail { element.elements() } else { vec![*element] }
        });
        let items = items.collect::<Vec<_>>();
        let mut documents = vec![self.tree(opener)?];
        for (position, item) in items.iter().enumerate() {
            let gap = if position == 0 {
                if opener.kind == LEFT_PARENTHESIS { Gap::BrokenSpace } else { Gap::Space }
            } else if item.kind == COMMA {
                Gap::SoftEmpty
            } else if item.kind == PIPE {
                Gap::Soft
            } else {
                self.fixed_gap(item.start)
            };
            let boundary = self.boundary(item.start, gap)?;
            let document = self.tree(item)?;
            documents.push(if matches!(item.kind, COMMA | PIPE) {
                boundary.append(document)
            } else if position > 0 && items[position - 1].kind == PIPE {
                boundary.append(document.align()).nest(2)
            } else {
                boundary.append(document).nest(2)
            });
        }
        let edge = if items.is_empty() {
            Gap::Tight
        } else if matches!(opener.kind, LEFT_CURLY | LEFT_SQUARE) {
            Gap::Soft
        } else {
            Gap::SoftEmpty
        };
        documents.push(self.boundary(elements[closing].start, edge)?);
        documents.push(self.tree(elements[closing])?);
        let inner = Document::concat(documents);
        let mut document = if grouped {
            Document::select(
                Choice::FirstFitting,
                vec![inner.clone().flat(), inner.align().starts_line()],
            )
        } else {
            inner.align()
        };
        if opening > 0 {
            document = Document::concat([
                self.spaced(&elements[..opening])?,
                self.boundary(opener.start, Gap::Space)?,
                document,
            ]);
        }
        if let Some(next) = elements.get(closing + 1) {
            let suffix = self
                .boundary(next.start, self.fixed_gap(next.start))?
                .append(self.spaced(&elements[closing + 1..])?);
            document = document.append(suffix.nest(self.indent()));
        }
        Ok(document)
    }

    fn right_hand_side(&self, expression: &Tree) -> Result<Document, FormatError> {
        let document = self.tree(expression)?;
        let wrapped = self
            .boundary(expression.start, Gap::Hard(1))?
            .append(document.clone())
            .nest(self.indent());
        if self.forces_line(expression.start)? {
            return Ok(wrapped);
        }
        let attached = self.boundary(expression.start, Gap::Space)?.append(document);
        Ok(Document::select(Choice::Fullest, vec![attached, wrapped]))
    }

    fn equation(&self, head: &[&Tree], tail: &[&Tree]) -> Result<Document, FormatError> {
        if let [expression] = tail
            && expression.kind == WhereExpression
        {
            return self.where_expression(head, &expression.elements());
        }
        let [expression] = tail else {
            return self.spaced(&[head, tail].concat());
        };
        Ok(self.spaced(head)?.append(self.right_hand_side(expression)?))
    }

    fn where_expression(
        &self,
        head: &[&Tree],
        elements: &[&Tree],
    ) -> Result<Document, FormatError> {
        let (expression, where_keyword, bindings) = match elements {
            [expression] => return self.equation(head, &[*expression]),
            [expression, where_keyword, bindings] => (*expression, *where_keyword, *bindings),
            _ => unreachable!(
                "invariant violated: validated where expression has one or three elements"
            ),
        };
        let bindings = bindings.elements();
        let Some(first) = bindings.iter().find(|binding| !binding.is_empty()) else {
            unreachable!("invariant violated: validated where expression has bindings");
        };
        let expression = if head.is_empty() {
            self.tree(expression)?
        } else {
            self.equation(head, &[expression])?
        };
        let bindings = Document::concat([
            self.boundary(where_keyword.start, Gap::Hard(1))?,
            self.tree(where_keyword)?,
            self.boundary(first.start, Gap::Hard(1))?,
            self.statements(&bindings)?,
        ]);
        Ok(expression.append(bindings.nest(self.indent())))
    }

    fn lambda(&self, elements: &[&Tree]) -> Result<Document, FormatError> {
        let [backslash, binders, arrow, body] = elements[..] else {
            unreachable!("invariant violated: validated lambda has four elements");
        };
        let header = self.spaced(&[backslash, binders, arrow])?;
        let body_document = self.tree(body)?;
        let broken = self.boundary(body.start, Gap::Soft)?.append(body_document.clone()).block();
        let body = if self.forces_line(body.start)? {
            broken
        } else {
            let attached = self.boundary(body.start, Gap::Space)?.append(body_document.clone());
            Document::select(Choice::FirstFitting, vec![attached.hanging(body_document), broken])
        };
        Ok(header.append(body).group())
    }

    fn conditional(&self, elements: &[&Tree]) -> Result<Document, FormatError> {
        let mut documents = Vec::new();
        let mut clauses = elements.to_vec();
        let mut leading_else: Option<&Tree> = None;
        loop {
            let [if_keyword, condition, then_keyword, consequent, else_keyword, alternative] =
                clauses[..]
            else {
                unreachable!("invariant violated: validated conditional has six elements");
            };
            let head = match leading_else.take() {
                Some(keyword) => {
                    documents.push(self.boundary(keyword.start, Gap::Soft)?);
                    self.spaced(&[keyword, if_keyword])?
                }
                None => self.tree(if_keyword)?,
            };
            documents.push(head.append(self.right_hand_side(condition)?));
            documents.push(self.boundary(then_keyword.start, Gap::Soft)?);
            documents.push(self.tree(then_keyword)?.append(self.right_hand_side(consequent)?));
            if let [nested] = alternative.elements().as_slice()
                && nested.kind == ExpressionIfThenElse
            {
                let (comments, _) =
                    trivia(self.lexed.annotation(nested.start).unwrap_or_default())?;
                if !comments.iter().any(|comment| comment.forces_line()) {
                    leading_else = Some(else_keyword);
                    clauses = nested.elements();
                    continue;
                }
            }
            documents.push(self.boundary(else_keyword.start, Gap::Soft)?);
            documents.push(self.tree(else_keyword)?.append(self.right_hand_side(alternative)?));
            return Ok(Document::concat(documents).own_line_group());
        }
    }

    fn case_expression(&self, elements: &[&Tree]) -> Result<Document, FormatError> {
        let [case_keyword, trunk, of_keyword, branches] = elements[..] else {
            unreachable!("invariant violated: validated case expression has four elements");
        };
        let case_document = self.tree(case_keyword)?;
        let of_document = self.tree(of_keyword)?;
        let items = trunk.elements();
        let mut scrutinees = Vec::new();
        for (position, item) in items.iter().enumerate() {
            if position > 0 {
                let gap =
                    if item.kind == COMMA { Gap::SoftEmpty } else { self.fixed_gap(item.start) };
                scrutinees.push(self.boundary(item.start, gap)?);
            }
            let follows_comma = position > 0 && items[position - 1].kind == COMMA;
            let document = self.tree(item)?;
            scrutinees.push(if follows_comma { document.nest(2) } else { document });
        }
        let scrutinees = Document::concat(scrutinees);
        let header = Document::concat([
            case_document.clone(),
            self.boundary(trunk.start, Gap::Space)?,
            scrutinees.clone(),
            self.boundary(of_keyword.start, Gap::Space)?,
            of_document.clone(),
        ]);
        let trunk = self.boundary(trunk.start, Gap::Hard(1))?.append(scrutinees.group());
        let broken_header = Document::concat([
            case_document,
            trunk.nest(self.indent()),
            self.boundary(of_keyword.start, Gap::Hard(1))?,
            of_document,
        ]);
        let branches = self.tree(branches)?;
        Ok(Document::select(
            Choice::FirstFitting,
            vec![
                header.flat().append(branches.clone()),
                broken_header.starts_line().append(branches),
            ],
        ))
    }

    fn let_expression(&self, elements: &[&Tree]) -> Result<Document, FormatError> {
        let [let_keyword, bindings, in_keyword, result] = elements[..] else {
            unreachable!("invariant violated: validated let expression has four elements");
        };
        let result = self.boundary(result.start, Gap::Hard(1))?.append(self.tree(result)?);
        let document = Document::concat([
            self.tree(let_keyword)?,
            self.tree(bindings)?,
            self.boundary(in_keyword.start, Gap::Hard(1))?,
            self.tree(in_keyword)?,
            result.nest(self.indent()),
        ]);
        Ok(document.starts_line())
    }

    fn ado_expression(&self, elements: &[&Tree]) -> Result<Document, FormatError> {
        let [ado_keyword, statements, in_keyword, result] = elements[..] else {
            unreachable!("invariant violated: validated ado expression has four elements");
        };
        let statements = statements.elements();
        let first = statements.iter().find(|statement| !statement.is_empty());
        let body = match first {
            Some(first) => {
                self.boundary(first.start, Gap::Hard(1))?.append(self.statements(&statements)?)
            }
            None => Document::empty(),
        };
        let gap = if first.is_some() { Gap::Hard(1) } else { Gap::Space };
        let body = Document::concat([
            body,
            self.boundary(in_keyword.start, gap)?,
            self.tree(in_keyword)?,
            self.right_hand_side(result)?,
        ]);
        Ok(self.tree(ado_keyword)?.append(body.block()))
    }

    fn declaration(&self, elements: &[&Tree], head_length: usize) -> Result<Document, FormatError> {
        let (body, elements) = match elements.split_last() {
            Some((body, elements)) if is_block(body.kind) => (Some(*body), elements),
            _ => (None, elements),
        };
        let (head, tail) = elements.split_at(head_length);
        let mut documents = Vec::new();
        let mut clause = Vec::new();
        for (position, element) in tail.iter().enumerate() {
            let previous = if position == 0 { head[head.len() - 1] } else { tail[position - 1] };
            let target = if element.kind == ClassFunctionalDependencies || !clause.is_empty() {
                &mut clause
            } else {
                &mut documents
            };
            target.push(self.boundary(element.start, self.element_gap(previous, element))?);
            target.push(self.tree(element)?);
        }
        let tail = Document::concat(documents).nest(self.indent());
        let clause = Document::concat(clause).nest(self.indent()).group();
        let header = match head {
            [keyword, constraints] if constraints.kind == ClassConstraints => {
                let keyword = self.tree(keyword)?;
                let constraints_document = self.delimited(constraints, false)?;
                let inline = Document::concat([
                    keyword.clone(),
                    self.boundary(constraints.start, Gap::Space)?,
                    constraints_document.clone(),
                    tail.clone(),
                ]);
                let broken = Document::concat([
                    keyword,
                    self.boundary(constraints.start, Gap::Hard(1))?.nest(self.indent()),
                    constraints_document,
                    tail.group(),
                ]);
                Document::select(Choice::FirstFitting, vec![inline.flat(), broken])
            }
            _ => self.spaced(head)?.append(tail).group(),
        };
        let body = match body {
            Some(body) => self.tree(body)?,
            None => Document::empty(),
        };
        Ok(Document::concat([header, clause, body]))
    }

    fn tree(&self, tree: &Tree) -> Result<Document, FormatError> {
        if tree.children.is_empty() {
            return Ok(if tree.is_empty() { Document::empty() } else { self.token(tree.start) });
        }
        let mut elements = tree.elements();
        match tree.kind {
            Module => self.sequence(&elements, |_| Gap::Hard(2)),
            ModuleImports | ModuleStatements => self.statements(&elements),
            ImportType | ExportType => {
                let document = self.sequence(&elements, |current| {
                    if current.kind == TypeItemsList {
                        Gap::BrokenSpace
                    } else {
                        self.fixed_gap(current.start)
                    }
                })?;
                Ok(document.group())
            }
            DoStatements | CaseBranches | LetBindingStatements | ClassStatements
            | InstanceStatements | Conditionals => self.block(&elements),
            ExpressionApplicationChain => self.call(&elements),
            TypeApplicationChain
            | ExpressionRecordUpdate
            | RecordUpdateBranch
            | BinderConstructor
            | InstanceHead
            | ClassHead
            | FunctionBinders
            | DataConstructor => self.application(&elements, 1),
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
                self.operators(&elements.collect::<Vec<_>>())
            }
            TypeArrow | TypeConstrained => self.arrows(tree),
            FunctionalDependencyDetermined | FunctionalDependencyDetermines => {
                let mut documents = Vec::new();
                for (position, element) in elements.iter().enumerate() {
                    if position > 0 {
                        let gap = if element.kind == RIGHT_ARROW { Gap::Soft } else { Gap::Space };
                        documents.push(self.boundary(element.start, gap)?.nest(self.indent()));
                    }
                    documents.push(self.tree(element)?);
                }
                Ok(Document::concat(documents).group())
            }
            DataEquation => match elements.iter().position(|tree| tree.kind == EQUAL) {
                Some(separator) => {
                    let (head, tail) = elements.split_at(separator);
                    let mut documents = Vec::new();
                    for element in tail {
                        let gap = if matches!(element.kind, EQUAL | PIPE) {
                            Gap::Soft
                        } else {
                            self.fixed_gap(element.start)
                        };
                        documents.push(self.boundary(element.start, gap)?);
                        let document = self.tree(element)?;
                        documents.push(if element.kind == DataConstructor {
                            document.align()
                        } else {
                            document
                        });
                    }
                    let tail = Document::concat(documents).nest(self.indent());
                    Ok(self.spaced(head)?.append(tail).group())
                }
                None => self.spaced(&elements),
            },
            TypeForall => {
                let [head @ .., body] = elements.as_slice() else {
                    unreachable!("invariant violated: validated forall has a body");
                };
                let body = self.boundary(body.start, Gap::Soft)?.append(self.tree(body)?);
                Ok(self.spaced(head)?.append(body).own_line_group())
            }
            ExportList | TypeItemsList => self.delimited(tree, false),
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
            | ImportList
            | InstanceConstraints
            | ClassConstraints => self.delimited(tree, true),
            ExpressionIfThenElse => self.conditional(&elements),
            ExpressionCaseOf => self.case_expression(&elements),
            ExpressionLambda => self.lambda(&elements),
            ExpressionLetIn => self.let_expression(&elements),
            ExpressionAdo => self.ado_expression(&elements),
            ValueEquation
            | LetBindingEquation
            | LetBindingPattern
            | InstanceEquationStatement
            | CaseBranch => {
                if let Some(last) = elements.pop_if(|tree| tree.kind == Unconditional) {
                    elements.extend(last.elements());
                }
                match elements.iter().position(|tree| matches!(tree.kind, EQUAL | RIGHT_ARROW)) {
                    Some(separator) => {
                        self.equation(&elements[..=separator], &elements[separator + 1..])
                    }
                    None => self.spaced(&elements),
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
            | PatternGuarded
            | TypeSynonymEquation
            | NewtypeEquation => {
                if elements.first().is_some_and(|tree| tree.kind == LEFT_PARENTHESIS) {
                    return self.delimited(tree, true);
                }
                let separator = elements.iter().position(|tree| {
                    matches!(
                        tree.kind,
                        DOUBLE_COLON | COLON | EQUAL | LEFT_ARROW | RIGHT_ARROW | PERIOD
                    )
                });
                match separator {
                    Some(separator) => {
                        self.equation(&elements[..=separator], &elements[separator + 1..])
                    }
                    None => self.spaced(&elements),
                }
            }
            WhereExpression => self.where_expression(&[], &elements),
            InstanceChain => self.sequence(&elements, |_| Gap::Hard(1)),
            ClassDeclaration => {
                let parenthesized = elements.get(1).is_some_and(|tree| {
                    tree.kind == ClassConstraints
                        && tree.children.first().is_some_and(|tree| tree.kind == LEFT_PARENTHESIS)
                });
                self.declaration(&elements, if parenthesized { 2 } else { 1 })
            }
            InstanceDeclaration | DeriveDeclaration => {
                if let [else_keyword, rest @ ..] = elements.as_slice()
                    && let [first, ..] = rest
                    && else_keyword.kind == ELSE
                    && tree.children.iter().any(|tree| tree.kind == LAYOUT_SEPARATOR)
                {
                    let rest = self.boundary(first.start, Gap::Hard(1))?.append(self.spaced(rest)?);
                    return Ok(self.tree(else_keyword)?.append(rest));
                }
                let head = elements.iter().position(|tree| tree.kind == InstanceHead);
                let constrained = head.is_some_and(|head| {
                    elements[..head].iter().any(|tree| tree.kind == InstanceConstraints)
                });
                match head {
                    Some(head) if constrained => self.declaration(&elements, head),
                    _ => self.spaced(&elements),
                }
            }
            ModuleHeader | ImportStatement => self.application(&elements, 2),
            _ => self.spaced(&elements),
        }
    }
}

fn is_block(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        DoStatements | CaseBranches | LetBindingStatements | ClassStatements | InstanceStatements
    )
}
