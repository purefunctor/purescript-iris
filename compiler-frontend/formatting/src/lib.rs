use std::borrow::Cow;
use std::fmt;
use std::ops::Range;

use itertools::Itertools;
use lexing::Lexed;
use syntax::ast::AstNode;
use syntax::{SyntaxElement, SyntaxKind, SyntaxNode, WalkEvent, cst};

use SyntaxKind::*;

mod printer;

/// Layout and spelling preferences.
#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub line_width: usize,
    pub indent_width: usize,
    /// Emit Unicode built-in spellings, preserving operator names and record labels.
    pub unicode: bool,
}

impl Default for Config {
    fn default() -> Config {
        Config { line_width: 80, indent_width: 2, unicode: false }
    }
}

impl Config {
    fn token_text<'source>(
        &self,
        lexed: &'source Lexed<'_>,
        context: &[SyntaxKind],
        index: usize,
    ) -> &'source str {
        if self.unicode {
            match (lexed.kind(index), context.first()) {
                (DOUBLE_COLON, _) => return "∷",
                (LEFT_ARROW, _) => return "←",
                (RIGHT_ARROW, _) => return "→",
                (RIGHT_THICK_ARROW, _) => return "⇒",
                (LEFT_THICK_ARROW, Some(ClassConstraints)) => return "⇐",
                (FORALL, Some(TypeForall)) => return "∀",
                _ => {}
            }
        }
        lexed.text(index)
    }

    pub fn validate(&self) -> Result<(), FormatError> {
        if self.line_width == 0 {
            return Err(FormatError::InvalidConfig("line width must be positive"));
        }
        if self.indent_width == 0 || self.indent_width > u16::MAX as usize {
            return Err(FormatError::InvalidConfig(
                "indent width must be between 1 and 65535 spaces",
            ));
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum FormatError {
    InvalidConfig(&'static str),
    InvalidSource(String),
    ChangedSyntax,
}

impl fmt::Display for FormatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FormatError::InvalidConfig(message) => {
                write!(formatter, "invalid formatting options: {message}")
            }
            FormatError::InvalidSource(message) => write!(formatter, "cannot format: {message}"),
            FormatError::ChangedSyntax => {
                write!(formatter, "formatting would change syntax; source was not modified")
            }
        }
    }
}

impl std::error::Error for FormatError {}

/// Formats a complete module, sorting imports while preserving lexical atoms
/// and the offside-rule structure.
pub fn format_with_config(source: &str, config: &Config) -> Result<String, FormatError> {
    config.validate()?;
    let lexed = lexing::lex(source);
    let layout = lexing::layout(&lexed);
    let (parsed, errors) = parsing::parse(&lexed, &layout);
    if let Some(error) = errors.first() {
        return Err(FormatError::InvalidSource(format!(
            "{}:{}: {}",
            error.position.line, error.position.column, error.message
        )));
    }
    validate_syntax(&parsed.syntax_node(), &lexed)?;
    if let Some((source, order)) = sort_imports(&parsed.syntax_node(), &lexed)? {
        let candidate = lexing::lex(&source);
        let candidate_layout = lexing::layout(&candidate);
        let tokens_changed = candidate.len() != lexed.len()
            || order.iter().enumerate().any(|(index, &original)| {
                candidate.error(index).is_some()
                    || candidate.kind(index) != lexed.kind(original)
                    || candidate.qualifier(index) != lexed.qualifier(original)
                    || candidate.text(index) != lexed.text(original)
            });
        let mut positions = vec![0; order.len()];
        for (index, &original) in order.iter().enumerate() {
            positions[original] = index;
        }
        if tokens_changed
            || !layout
                .iter()
                .filter(|kind| kind.is_layout_token())
                .eq(candidate_layout.iter().filter(|kind| kind.is_layout_token()))
            || collect_comments(&lexed, Some(&positions))? != collect_comments(&candidate, None)?
        {
            return Err(FormatError::ChangedSyntax);
        }
        let (parsed, errors) = parsing::parse(&candidate, &candidate_layout);
        if !errors.is_empty() || validate_syntax(&parsed.syntax_node(), &candidate).is_err() {
            return Err(FormatError::ChangedSyntax);
        }
        return render_validated(&parsed.syntax_node(), &candidate, &candidate_layout, config);
    }
    render_validated(&parsed.syntax_node(), &lexed, &layout, config)
}

fn render_validated(
    root: &SyntaxNode,
    lexed: &Lexed<'_>,
    layout: &[SyntaxKind],
    config: &Config,
) -> Result<String, FormatError> {
    let comments = collect_comments(lexed, None)?;
    let contexts = token_contexts(root, lexed);
    let output = printer::render(root, lexed, &contexts, config)?;
    let candidate = lexing::lex(&output);
    let token_changed = |(index, context): (usize, &Vec<SyntaxKind>)| {
        candidate.error(index).is_some()
            || candidate.kind(index) != lexed.kind(index)
            || candidate.qualifier(index) != lexed.qualifier(index)
            || candidate.text(index) != config.token_text(lexed, context, index)
    };
    let tokens_changed = lexing::layout(&candidate) != layout
        || candidate.len() != lexed.len()
        || contexts.iter().enumerate().any(token_changed);
    if tokens_changed || collect_comments(&candidate, None)? != comments {
        return Err(FormatError::ChangedSyntax);
    }
    Ok(output)
}

struct ImportFragment<'source> {
    open_prelude: bool,
    module_name: &'source str,
    tokens: Range<usize>,
}

// The printer's trivia boundaries depend on source order. Reparse reordered
// import fragments rather than giving it noncontiguous CST children.
fn sort_imports(
    root: &SyntaxNode,
    lexed: &Lexed<'_>,
) -> Result<Option<(String, Vec<usize>)>, FormatError> {
    let Some(module) = cst::Module::cast(root.clone()) else {
        unreachable!("invariant violated: parsed source must have a module root");
    };
    let Some(imports) = module.imports() else {
        return Ok(None);
    };
    let imports = imports.children().map(|import| import_fragment(&import, lexed));
    let mut imports = imports.collect::<Vec<_>>();

    let (Some(first), Some(last)) = (imports.first(), imports.last()) else {
        return Ok(None);
    };
    let (first_token, after_last_token) = (first.tokens.start, last.tokens.end);
    let prefix_end = fragment_boundary(lexed, first_token)?;
    let suffix_start = fragment_boundary(lexed, after_last_token)?;
    let indentation = " ".repeat(lexed.position(first_token).column as usize - 1);

    imports.sort_by(|left, right| {
        let open_prelude_first = left.open_prelude.cmp(&right.open_prelude).reverse();
        open_prelude_first
            .then_with(|| left.module_name.split('.').cmp(right.module_name.split('.')))
    });

    let mut source = lexed.source[..prefix_end].to_owned();
    let mut order = (0..first_token).collect::<Vec<_>>();
    let mut previous_open_prelude = None;
    for ImportFragment { open_prelude, tokens, .. } in imports {
        let separator = if previous_open_prelude == Some(open_prelude) { "\n" } else { "\n\n" };
        source.push_str(separator);
        source.push_str(&indentation);

        let start = fragment_boundary(lexed, tokens.start)?;
        let end = fragment_boundary(lexed, tokens.end)?;
        source.push_str(lexed.source[start..end].trim_start());
        order.extend(tokens);
        previous_open_prelude = Some(open_prelude);
    }

    source.push_str(&lexed.source[suffix_start..]);
    order.extend(after_last_token..lexed.len());
    Ok((source != lexed.source).then_some((source, order)))
}

fn import_fragment<'source>(
    import: &cst::ImportStatement,
    lexed: &Lexed<'source>,
) -> ImportFragment<'source> {
    let range = import.syntax().text_range();
    let start = lexed.first_text_start_from(u32::from(range.start()));
    let end = lexed.first_text_start_from(u32::from(range.end()));

    let Some(module_name) = import.module_name() else {
        unreachable!("invariant violated: parsed import must have a module name");
    };
    let name_token =
        lexed.first_text_start_from(u32::from(module_name.syntax().text_range().start()));
    let info = lexed.info(name_token);
    let module_name = &lexed.source[info.annotation as usize..info.token as usize];
    let open_prelude = module_name == "Prelude"
        && import.import_alias().is_none()
        && import.import_list().is_none();

    ImportFragment { open_prelude, module_name, tokens: start..end }
}

// One annotation can hold both the previous import's trailing comments and
// the next import's leading comments; they must move with different imports.
fn fragment_boundary(lexed: &Lexed<'_>, index: usize) -> Result<usize, FormatError> {
    let annotation = lexed.annotation(index).unwrap_or_default();
    let (comments, _) = trivia(annotation)?;
    let trailing = comments.iter().take_while(|comment| !comment.whitespace.contains('\n'));
    let end = trailing.last().map_or(0, |comment| comment.end);
    Ok(lexed.info(index).annotation as usize - annotation.len() + end)
}

fn validate_syntax(root: &SyntaxNode, lexed: &Lexed<'_>) -> Result<(), FormatError> {
    // Recovery can omit required operands without reporting a parser error.
    // Check grammatical slots, not whether containers happen to be empty: empty
    // arrays, records, rows and ado bodies are legitimate syntax.
    for event in root.preorder() {
        let WalkEvent::Enter(node) = event else { continue };
        let has_child = |predicate: fn(SyntaxKind) -> bool| {
            node.children().any(|child| predicate(child.kind()))
        };
        let has_kind = |kind| node.children().any(|child| child.kind() == kind);
        let has_token = |kind| node.children_with_tokens().any(|child| child.kind() == kind);
        let has_kind_containing = |kind, predicate: fn(SyntaxKind) -> bool| {
            node.children().any(|child| {
                child.kind() == kind && child.children().any(|child| predicate(child.kind()))
            })
        };
        let type_count =
            || node.children().filter(|child| cst::Type::can_cast(child.kind())).count();
        let complete = match node.kind() {
            Qualifier => {
                let qualifies_name = || {
                    node.parent()
                        .is_some_and(|parent| matches!(parent.kind(), QualifiedName | ModuleName))
                };
                let qualifies_block_keyword = || {
                    node.next_sibling_or_token()
                        .is_some_and(|element| matches!(element.kind(), DO | ADO))
                };
                qualifies_name() || qualifies_block_keyword()
            }
            Unconditional | LetBindingPattern => has_kind(WhereExpression),
            PatternGuarded => has_kind(WhereExpression) && has_child(cst::PatternGuard::can_cast),
            WhereExpression
            | ExpressionIf
            | ExpressionThen
            | ExpressionElse
            | ExpressionParenthesized
            | ExpressionNegate
            | ExpressionApplicationChain
            | ExpressionTermArgument
            | ExpressionTick
            | ExpressionLetIn
            | ExpressionAdo
            | ExpressionInfixChain
            | ExpressionInfixPair
            | ExpressionOperatorPair
            | RecordUpdateLeaf
            | DoStatementDiscard
            | CaseTrunk
            | PatternGuardExpression => has_child(cst::Expression::can_cast),
            ExpressionOperatorChain => {
                let elements = node
                    .children_with_tokens()
                    .filter(|element| !matches!(element.kind(), Annotation | Qualifier));
                let elements = elements.collect::<Vec<_>>();
                let is_expression =
                    |element: &SyntaxElement| cst::Expression::can_cast(element.kind());
                let starts_with_operand = || elements.first().is_some_and(is_expression);
                let ends_with_operand_or_pair = || {
                    elements.last().is_some_and(|element| {
                        is_expression(element) || element.kind() == ExpressionOperatorPair
                    })
                };
                let every_operator_has_operand = || {
                    elements.iter().tuple_windows().all(|(previous, next)| {
                        previous.kind() != QualifiedName || is_expression(next)
                    })
                };
                starts_with_operand() && ends_with_operand_or_pair() && every_operator_has_operand()
            }
            ExpressionLambda => has_kind(FunctionBinders) && has_child(cst::Expression::can_cast),
            DoStatementBind | PatternGuardBinder => {
                has_child(cst::Binder::can_cast) && has_child(cst::Expression::can_cast)
            }
            ExpressionDo => has_kind_containing(DoStatements, cst::DoStatement::can_cast),
            CaseBranches => has_kind(CaseBranch),
            LetBindingStatements => has_child(cst::LetBinding::can_cast),
            RecordField => {
                has_child(cst::Expression::can_cast)
                    || has_child(cst::Binder::can_cast)
                    || has_child(cst::Type::can_cast)
            }
            RecordUpdates => has_child(cst::RecordUpdate::can_cast),
            BinderTyped => has_child(cst::Binder::can_cast) && has_child(cst::Type::can_cast),
            BinderNamed | BinderParenthesized | BinderOperatorChain | BinderOperatorPair
            | CaseBranchBinders => has_child(cst::Binder::can_cast),
            ExpressionTyped => {
                has_child(cst::Expression::can_cast) && has_child(cst::Type::can_cast)
            }
            TypeArrow | TypeConstrained | TypeKinded => type_count() == 2,
            TypeForall => has_kind(TypeVariableBinding) && has_child(cst::Type::can_cast),
            TypeVariableBinding => {
                let kinded = || has_token(DOUBLE_COLON);
                let parenthesized = || has_token(LEFT_PARENTHESIS);
                has_token(LOWER)
                    && (!parenthesized() || kinded())
                    && (!kinded() || has_child(cst::Type::can_cast))
            }
            TypeInteger => has_token(INTEGER),
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
            | ExpressionTypeArgument
            | TypeSynonymEquation
            | TypeRowItem
            | TypeRowTail
            | TypeParenthesized
            | TypeOperatorChain
            | TypeOperatorPair
            | ClassConstraints
            | InstanceConstraints => has_child(cst::Type::can_cast),
            NewtypeEquation => has_kind_containing(DataConstructor, cst::Type::can_cast),
            ClassFunctionalDependencies => has_child(cst::FunctionalDependency::can_cast),
            FunctionalDependencyDetermined => has_kind(TypeVariable),
            FunctionalDependencyDetermines => node
                .children_with_tokens()
                .skip_while(|element| element.kind() != RIGHT_ARROW)
                .any(|element| element.kind() == TypeVariable),
            TypeRoleDeclaration => has_kind(TypeRole),
            _ => true,
        };
        if !complete {
            let offset = u32::from(node.text_range().start());
            let position = lexed.position(lexed.first_text_start_from(offset));
            return Err(FormatError::InvalidSource(format!(
                "{}:{}: incomplete syntax in {:?}",
                position.line,
                position.column,
                node.kind()
            )));
        }
    }
    Ok(())
}

fn token_contexts(root: &SyntaxNode, lexed: &Lexed<'_>) -> Vec<Vec<SyntaxKind>> {
    let mut contexts = vec![Vec::new(); lexed.len()];
    for token in root.tokens() {
        if token.kind().is_layout_token() || token.kind() == END_OF_FILE {
            continue;
        }
        let ancestors = token.parent_ancestors().map(|node| node.kind()).collect::<Vec<_>>();
        if ancestors.contains(&Annotation) || ancestors.contains(&Qualifier) {
            continue;
        }
        let start = u32::from(token.text_range().start());
        if let Some(index) = lexed.find_text_start(start) {
            contexts[index] = ancestors;
        }
    }
    contexts
}

#[derive(Debug, PartialEq, Eq)]
struct Comment<'source> {
    whitespace: &'source str,
    text: Cow<'source, str>,
    line: bool,
    end: usize,
}

impl Comment<'_> {
    fn forces_line(&self) -> bool {
        self.line || self.whitespace.contains('\n') || self.text.contains('\n')
    }
}

fn trivia(annotation: &str) -> Result<(Vec<Comment<'_>>, &str), FormatError> {
    let mut comments = Vec::new();
    let mut remaining = annotation;
    while !remaining.is_empty() {
        let whitespace_length = remaining.len() - remaining.trim_start().len();
        let whitespace = &remaining[..whitespace_length];
        remaining = &remaining[whitespace_length..];
        if remaining.is_empty() {
            return Ok((comments, whitespace));
        }
        let line = remaining.starts_with("--");
        let length = if line {
            remaining.find('\n').unwrap_or(remaining.len())
        } else if remaining.starts_with("{-") {
            let mut depth = 1;
            let mut index = 2;
            while index < remaining.len() && depth > 0 {
                if remaining[index..].starts_with("{-") {
                    depth += 1;
                    index += 2;
                } else if remaining[index..].starts_with("-}") {
                    depth -= 1;
                    index += 2;
                } else {
                    index += remaining[index..].chars().next().unwrap().len_utf8();
                }
            }
            if depth != 0 {
                return Err(FormatError::InvalidSource("unterminated block comment".into()));
            }
            index
        } else {
            return Err(FormatError::InvalidSource("unrecognized source annotation".into()));
        };
        let text = &remaining[..length];
        let text = if line { text.strip_suffix('\r').unwrap_or(text) } else { text };
        let text = if text.contains("\r\n") {
            Cow::Owned(text.replace("\r\n", "\n"))
        } else {
            Cow::Borrowed(text)
        };
        let end = annotation.len() - remaining.len() + length;
        comments.push(Comment { whitespace, text, line, end });
        remaining = &remaining[length..];
    }
    Ok((comments, ""))
}

fn collect_comments<'source>(
    lexed: &'source Lexed<'_>,
    positions: Option<&[usize]>,
) -> Result<Vec<(usize, Cow<'source, str>, bool)>, FormatError> {
    let mut result = Vec::new();
    for (index, annotation) in lexed.annotations().enumerate() {
        let (comments, _) = trivia(annotation.unwrap_or_default())?;
        for comment in comments {
            let standalone = index == 0 || comment.whitespace.contains('\n');
            let anchor = if standalone { index } else { index - 1 };
            let anchor = positions.map_or(anchor, |positions| positions[anchor]);
            result.push((anchor, comment.text, standalone));
        }
    }
    result.sort_by_key(|(anchor, _, standalone)| (*anchor, *standalone));
    Ok(result)
}

#[derive(Clone, Copy)]
enum Gap {
    Tight,
    Space,
    BrokenSpace,
    Soft,
    SoftEmpty,
    Hard(usize),
}

fn spacing(lexed: &Lexed<'_>, contexts: &[Vec<SyntaxKind>], index: usize) -> Gap {
    let previous = lexed.kind(index - 1);
    let current = lexed.kind(index);
    let context = &contexts[index];
    let previous_context = &contexts[index - 1];
    let hugs_delimiter = || {
        matches!(
            (previous, current),
            (_, COMMA | RIGHT_PARENTHESIS | RIGHT_SQUARE)
                | (LEFT_PARENTHESIS | LEFT_SQUARE | BACKSLASH, _)
                | (LEFT_CURLY, RIGHT_CURLY)
        )
    };
    let record_label_colon = || current == COLON && context.contains(&RecordField);
    let period_boundary =
        || current == PERIOD || (previous == PERIOD && !previous_context.contains(&TypeForall));
    let at_boundary = || (current == AT && context.contains(&BinderNamed)) || previous == AT;
    let negative_sign = || {
        previous == MINUS
            && previous_context.first().is_some_and(|kind| {
                matches!(kind, ExpressionNegate | BinderInteger | BinderNumber | TypeInteger)
            })
    };
    let type_member_list = || {
        matches!((previous, current), (UPPER, LEFT_PARENTHESIS | DOUBLE_PERIOD_OPERATOR_NAME))
            && (context.contains(&ImportType) || context.contains(&ExportType))
    };
    let infix_tick = || {
        (current == TICK && previous_context.contains(&ExpressionTick))
            || (previous == TICK && context.contains(&ExpressionTick))
    };
    if hugs_delimiter()
        || record_label_colon()
        || period_boundary()
        || at_boundary()
        || negative_sign()
        || type_member_list()
        || infix_tick()
    {
        // A syntactically tight boundary must not merge into a different lexical atom.
        let spelling =
            |index| format!("{}{}", lexed.qualifier(index).unwrap_or_default(), lexed.text(index));
        let joined = spelling(index - 1) + &spelling(index);
        let probe = lexing::lex(&joined);
        let same_atom = |probe_index, source_index| {
            probe.kind(probe_index) == lexed.kind(source_index)
                && probe.qualifier(probe_index) == lexed.qualifier(source_index)
                && probe.text(probe_index) == lexed.text(source_index)
        };
        if probe.len() == 3 && same_atom(0, index - 1) && same_atom(1, index) {
            return Gap::Tight;
        }
    }
    Gap::Space
}
