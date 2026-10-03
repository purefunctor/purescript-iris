use pretty::{Arena, DocAllocator, DocBuilder};
use syntax::{SyntaxElement, SyntaxKind, SyntaxNode};

use crate::{FormatError, trivia};

type Doc<'a> = DocBuilder<'a, Arena<'a>>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Separator {
    None,
    Space,
    Line,
    LineOrEmpty,
    Hard,
    Blank,
}

struct Piece<'a> {
    document: Doc<'a>,
    start: usize,
    end: usize,
    kind: SyntaxKind,
    first: SyntaxKind,
    first_end: usize,
}

pub(crate) fn render(root: &SyntaxNode, source: &str) -> Result<String, FormatError> {
    let arena = Arena::new();
    let printer = Printer { arena: &arena, source };
    let module = printer.node(root)?.ok_or(FormatError::IncompleteSyntax(root.kind()))?;
    let (leading, _) = printer.gap(0, module.start, Separator::None)?;
    let (trailing, _) = printer.gap(module.end, source.len(), Separator::Hard)?;
    let document = leading.append(module.document).append(trailing);
    let mut output = String::new();
    document.render_fmt(100, &mut output).expect("writing to a String cannot fail");
    Ok(output)
}

struct Printer<'a> {
    arena: &'a Arena<'a>,
    source: &'a str,
}

impl<'a> Printer<'a> {
    fn separator(&self, separator: Separator) -> Doc<'a> {
        match separator {
            Separator::None => self.arena.nil(),
            Separator::Space => self.arena.space(),
            Separator::Line => self.arena.line(),
            Separator::LineOrEmpty => self.arena.line_(),
            Separator::Hard => self.arena.hardline(),
            Separator::Blank => self.arena.hardline().append(self.arena.hardline()),
        }
    }

    fn verbatim(&self, text: &'a str) -> Doc<'a> {
        if !text.contains('\n') {
            return self.arena.text(text);
        }
        let mut lines = text.split('\n');
        let first = self.arena.text(lines.next().unwrap());
        let rest = self
            .arena
            .concat(lines.map(|line| self.arena.hardline().append(self.arena.text(line))));
        let arena = self.arena;
        // Literal and comment interiors must not inherit the surrounding code's indentation.
        first.append(arena.nesting(move |indent| rest.clone().nest(-(indent as isize)).into_doc()))
    }

    fn gap(
        &self,
        start: usize,
        end: usize,
        separator: Separator,
    ) -> Result<(Doc<'a>, bool), FormatError> {
        let source = &self.source[start..end];
        let comments = trivia::comments(source)?;
        if comments.is_empty() {
            return Ok((self.separator(separator), false));
        }
        let forces_line = comments.iter().any(|comment| {
            comment.line
                || comment.newline_before
                || comment.newline_after
                || comment.text.contains('\n')
        });
        let mut document = self.arena.nil();
        let mut previous_line = false;
        let mut separated = false;
        for (index, comment) in comments.iter().enumerate() {
            let before = if index == 0 && start == 0 {
                Separator::None
            } else if comment.newline_before || previous_line {
                if separator == Separator::Blank && !separated {
                    separated = true;
                    Separator::Blank
                } else {
                    Separator::Hard
                }
            } else {
                Separator::Space
            };
            document = document.append(self.separator(before)).append(self.verbatim(comment.text));
            previous_line = comment.line;
        }
        let last = comments.last().unwrap();
        let after = if separator == Separator::Blank && !separated {
            Separator::Blank
        } else if previous_line
            || last.newline_after
            || last.text.contains('\n')
            || matches!(separator, Separator::Hard | Separator::Blank)
        {
            Separator::Hard
        } else if matches!(separator, Separator::Line | Separator::LineOrEmpty) {
            separator
        } else {
            Separator::Space
        };
        Ok((document.append(self.separator(after)), forces_line))
    }

    fn join(
        &self,
        left: Piece<'a>,
        right: Piece<'a>,
        separator: Separator,
        indent: isize,
    ) -> Result<Piece<'a>, FormatError> {
        let (boundary, forces_line) = self.gap(left.end, right.start, separator)?;
        // A comment can break an otherwise inline declaration head. Its continuation
        // must stay to the right of the enclosing layout block's statement column.
        let indent = if forces_line
            && indent == 0
            && matches!(separator, Separator::None | Separator::Space)
        {
            2
        } else {
            indent
        };
        Ok(Piece {
            document: left.document.append(boundary.append(right.document).nest(indent)),
            start: left.start,
            end: right.end,
            kind: left.kind,
            first: left.first,
            first_end: left.first_end,
        })
    }

    fn node(&self, node: &SyntaxNode) -> Result<Option<Piece<'a>>, FormatError> {
        use SyntaxKind::*;
        if node.kind() == Annotation {
            return Ok(None);
        }
        let mut children = Vec::new();
        for child in node.children_with_tokens() {
            match child {
                SyntaxElement::Node(child) => {
                    if let Some(piece) = self.node(&child)? {
                        children.push(piece);
                    }
                }
                SyntaxElement::Token(token) => {
                    if token.kind().is_layout_token() || token.kind() == END_OF_FILE {
                        continue;
                    }
                    children.push(Piece {
                        document: self.verbatim(token.text(self.source)),
                        start: token.text_range().start().into(),
                        end: token.text_range().end().into(),
                        kind: token.kind(),
                        first: token.kind(),
                        first_end: token.text_range().end().into(),
                    });
                }
            }
        }
        if children.is_empty() {
            return Ok(None);
        }
        let children = self.delimiters(children)?;
        let mut piece = match node.kind() {
            Module | ModuleImports | ModuleStatements | LetBindingStatements | ClassStatements
            | InstanceStatements | CaseBranches | DoStatements | Conditionals | InstanceChain => {
                self.statements(node.kind(), children)?
            }
            ModuleName
            | QualifiedName
            | LabelName
            | Qualifier
            | TypeItemsAll
            | ExportValue
            | ExportClass
            | ExportType
            | ExportOperator
            | ExportTypeOperator
            | ExportModule
            | ImportValue
            | ImportClass
            | ImportType
            | ImportOperator
            | ImportTypeOperator
            | TypeRole
            | TypeConstructor
            | TypeHole
            | TypeInteger
            | TypeOperatorName
            | TypeString
            | TypeVariable
            | TypeWildcard
            | BinderInteger
            | BinderNumber
            | BinderVariable
            | BinderWildcard
            | BinderString
            | BinderChar
            | BinderTrue
            | BinderFalse
            | ExpressionConstructor
            | ExpressionVariable
            | ExpressionOperatorName
            | ExpressionSection
            | ExpressionHole
            | ExpressionString
            | ExpressionChar
            | ExpressionTrue
            | ExpressionFalse
            | ExpressionInteger
            | ExpressionNumber
            | TermOperator
            | TypeOperator
            | RecordPun
            | RecordAccessLabel
            | ModuleHeader
            | ExportList
            | ImportStatement
            | ImportList
            | ImportAlias
            | TypeItemsList
            | ValueSignature
            | ValueEquation
            | FunctionBinders
            | Unconditional
            | WhereExpression
            | PatternGuarded
            | PatternGuardBinder
            | PatternGuardExpression
            | LetBindingSignature
            | LetBindingEquation
            | LetBindingPattern
            | TypeApplicationChain
            | TypeArrow
            | TypeConstrained
            | TypeForall
            | TypeKinded
            | TypeOperatorChain
            | TypeOperatorPair
            | TypeVariableBinding
            | TypeRecord
            | TypeRow
            | TypeRowItem
            | TypeRowTail
            | TypeParenthesized
            | BinderTyped
            | BinderOperatorChain
            | BinderOperatorPair
            | BinderConstructor
            | BinderNamed
            | BinderArray
            | BinderRecord
            | BinderParenthesized
            | ExpressionTyped
            | ExpressionOperatorChain
            | ExpressionOperatorPair
            | ExpressionInfixChain
            | ExpressionInfixPair
            | ExpressionTick
            | ExpressionNegate
            | ExpressionApplicationChain
            | ExpressionTypeArgument
            | ExpressionTermArgument
            | ExpressionIfThenElse
            | ExpressionIf
            | ExpressionThen
            | ExpressionElse
            | ExpressionLetIn
            | ExpressionLambda
            | ExpressionCaseOf
            | ExpressionDo
            | ExpressionAdo
            | ExpressionArray
            | ExpressionRecord
            | ExpressionParenthesized
            | ExpressionRecordAccess
            | ExpressionRecordUpdate
            | CaseTrunk
            | CaseBranchBinders
            | CaseBranch
            | DoStatementBind
            | DoStatementLet
            | DoStatementDiscard
            | RecordField
            | RecordUpdates
            | RecordUpdateLeaf
            | RecordUpdateBranch
            | InfixDeclaration
            | TypeRoleDeclaration
            | TypeSynonymSignature
            | TypeSynonymEquation
            | ClassSignature
            | ClassDeclaration
            | ClassConstraints
            | ClassHead
            | ClassFunctionalDependencies
            | FunctionalDependencyDetermined
            | FunctionalDependencyDetermines
            | ClassMemberStatement
            | InstanceDeclaration
            | InstanceName
            | InstanceConstraints
            | InstanceHead
            | InstanceSignatureStatement
            | InstanceEquationStatement
            | ForeignImportDataDeclaration
            | ForeignImportValueDeclaration
            | NewtypeSignature
            | NewtypeEquation
            | DataSignature
            | DataEquation
            | DataConstructor
            | DeriveDeclaration => self.sequence(node.kind(), children)?,
            kind => return Err(FormatError::UnsupportedSyntax(kind)),
        };
        piece.kind = node.kind();
        piece.document = piece.document.group();
        Ok(Some(piece))
    }

    fn statements(
        &self,
        kind: SyntaxKind,
        children: Vec<Piece<'a>>,
    ) -> Result<Piece<'a>, FormatError> {
        let mut children = children.into_iter();
        let mut output = children.next().unwrap();
        let mut previous_name = self.declaration_name(&output);
        for child in children {
            let name = self.declaration_name(&child);
            let separator = if kind == SyntaxKind::Module
                || (kind == SyntaxKind::ModuleStatements
                    && (name.is_none() || name != previous_name))
            {
                Separator::Blank
            } else {
                Separator::Hard
            };
            output = self.join(output, child, separator, 0)?;
            previous_name = name;
        }
        Ok(output)
    }

    fn declaration_name(&self, piece: &Piece<'a>) -> Option<&'a str> {
        use SyntaxKind::*;
        if !matches!(piece.kind, ValueSignature | ValueEquation) {
            return None;
        }
        Some(&self.source[piece.start..piece.first_end])
    }

    fn sequence(
        &self,
        kind: SyntaxKind,
        children: Vec<Piece<'a>>,
    ) -> Result<Piece<'a>, FormatError> {
        use SyntaxKind::*;
        let mut children = children.into_iter();
        let mut output = children.next().unwrap();
        let mut previous = output.kind;
        for child in children {
            let current = child.kind;
            let (separator, indent) = if previous == WHERE && kind == WhereExpression {
                (Separator::Hard, 4)
            } else if matches!(
                current,
                LetBindingStatements
                    | ClassStatements
                    | InstanceStatements
                    | CaseBranches
                    | DoStatements
                    | Conditionals
            ) {
                (Separator::Hard, 2)
            } else if current == IN && matches!(kind, ExpressionLetIn | ExpressionAdo) {
                (Separator::Hard, 0)
            } else if current == WHERE && kind == WhereExpression {
                (Separator::Hard, 2)
            } else if current == COMMA
                || current == COLON
                || current == PERIOD
                || previous == Qualifier
                || (current == RecordAccessLabel && kind == ExpressionRecordAccess)
                || (previous == PERIOD && kind == RecordAccessLabel)
                || (matches!(current, TypeItemsAll | TypeItemsList)
                    && matches!(kind, ImportType | ExportType))
                || (previous == BACKSLASH && kind == ExpressionLambda)
                || (matches!(previous, AT)
                    && matches!(kind, BinderNamed | ExpressionTypeArgument | TypeVariableBinding))
                || (current == AT && kind == BinderNamed)
                || (previous == MINUS && matches!(kind, BinderInteger | BinderNumber | TypeInteger))
                || (kind == ExpressionTick && (previous == TICK || current == TICK))
            {
                (Separator::None, 0)
            } else if previous == MINUS && kind == ExpressionNegate {
                (if child.first == MINUS { Separator::Space } else { Separator::None }, 0)
            } else if matches!(current, THEN | ELSE) && kind == ExpressionIfThenElse {
                (Separator::Line, 0)
            } else if matches!(previous, EQUAL | DOUBLE_COLON | LEFT_ARROW | RIGHT_ARROW)
                || matches!(
                    kind,
                    ExpressionApplicationChain
                        | TypeApplicationChain
                        | ExpressionOperatorChain
                        | ExpressionInfixChain
                        | TypeOperatorChain
                        | BinderOperatorChain
                )
                || previous == COMMA
                || (current == PIPE && kind == DataEquation)
            {
                (Separator::Line, 2)
            } else {
                (Separator::Space, 0)
            };
            output = self.join(output, child, separator, indent)?;
            previous = current;
        }
        Ok(output)
    }

    fn delimiters(&self, children: Vec<Piece<'a>>) -> Result<Vec<Piece<'a>>, FormatError> {
        use SyntaxKind::*;
        let mut children = children.into_iter();
        let mut result = Vec::new();
        while let Some(opening) = children.next() {
            let closing_kind = match opening.kind {
                LEFT_PARENTHESIS => RIGHT_PARENTHESIS,
                LEFT_CURLY => RIGHT_CURLY,
                LEFT_SQUARE => RIGHT_SQUARE,
                _ => {
                    result.push(opening);
                    continue;
                }
            };
            let separator = if opening.kind == LEFT_PARENTHESIS {
                Separator::LineOrEmpty
            } else {
                Separator::Line
            };
            let mut inside = Vec::new();
            let closing = loop {
                let child = children.next().ok_or(FormatError::IncompleteSyntax(opening.kind))?;
                if child.kind == closing_kind {
                    break child;
                }
                inside.push(child);
            };
            let mut output = if inside.is_empty() {
                self.join(opening, closing, Separator::None, 0)?
            } else {
                let mut inside = inside.into_iter();
                let mut contents = inside.next().unwrap();
                for child in inside {
                    let current = child.kind;
                    let separator =
                        if current == COMMA { Separator::LineOrEmpty } else { Separator::Space };
                    contents = self.join(contents, child, separator, 0)?;
                }
                let output = self.join(opening, contents, separator, 2)?;
                self.join(output, closing, separator, 0)?
            };
            output.document = output.document.group();
            result.push(output);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verbatim_preserves_line_bytes_and_restores_surrounding_indentation() {
        let arena = Arena::new();
        let printer = Printer { arena: &arena, source: "" };
        let literal = "\"\"\"first\r\n\r\n  λ\r\nlast\"\"\"";
        let document = arena
            .hardline()
            .append(printer.verbatim(literal))
            .append(arena.hardline())
            .append("next")
            .nest(4);
        let mut output = String::new();
        document.render_fmt(20, &mut output).unwrap();
        assert_eq!(output, "\n    \"\"\"first\r\n\r\n  λ\r\nlast\"\"\"\n    next");
    }
}
