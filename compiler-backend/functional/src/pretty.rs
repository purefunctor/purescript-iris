//! Stable text rendering for functional-tree snapshots.

use pretty::{Arena, DocAllocator, DocBuilder};

use crate::stylex::{StyleXCallTarget, StyleXConditionalCase, StyleXExpression};
use crate::tree::{
    BinaryOperator, Declaration, DeclarationKind, EffectExpression, ExpressionId, ExpressionKind,
    Field, GlobalId, Guard, IndirectModuleExports, Literal, Module, Parameter, PatternId,
    PatternKind, RecordUpdate, ReflectableEvidence, ReflectableOrdering, SynthesizedEvidence,
    UnaryOperator,
};

type Doc<'a> = DocBuilder<'a, Arena<'a>, ()>;

const DEFAULT_WIDTH: usize = 100;

pub fn render(module: &Module) -> String {
    let arena = Arena::new();
    let printer = Printer { arena: &arena, module };
    let indirect = module.surface.indirect.iter().map(|exports| printer.indirect_exports(exports));
    let declarations =
        module.declarations.iter().map(|declaration| printer.declaration(declaration));
    let documents = indirect.chain(declarations);
    let declaration_separator = arena.hardline().append(arena.hardline());
    let document = arena.intersperse(documents, declaration_separator);
    let mut output = String::new();
    document
        .render_fmt(DEFAULT_WIDTH, &mut output)
        .expect("critical failure: failed to render normalized functional tree");
    output
}

struct Printer<'a, 'm> {
    arena: &'a Arena<'a>,
    module: &'m Module,
}

impl<'a> Printer<'a, '_> {
    fn indirect_exports(&self, exports: &IndirectModuleExports) -> Doc<'a> {
        let dependency = self
            .module
            .dependencies
            .iter()
            .find(|dependency| dependency.file_id == exports.file_id)
            .expect("invariant violated: indirect exports have no module dependency");
        let globals =
            exports.globals.iter().map(|global| self.arena.text(global.item_name.to_string()));
        let globals = self.arena.intersperse(globals, ", ");
        self.arena
            .text(format!("@export {} {{ ", dependency.module_name))
            .append(globals)
            .append(" }")
    }

    fn declaration(&self, declaration: &Declaration) -> Doc<'a> {
        let name = &declaration.global.item_name;
        let export = if declaration.exported { "@export " } else { "" };
        match declaration.kind {
            DeclarationKind::Foreign => self.arena.text(format!("{export}foreign {name}")),
            DeclarationKind::Constructor { arity } => {
                self.arena.text(format!("{export}constructor {name}/{arity}"))
            }
            DeclarationKind::Value(expression) => {
                let prefix = match declaration.global.id {
                    GlobalId::Term(..) | GlobalId::Generated(..) => "",
                    GlobalId::Instance(..) => "instance ",
                };
                let recursion = declaration
                    .recursive_group
                    .map(|group| format!("recursive[{}] ", group.0))
                    .unwrap_or_default();
                let declaration = self.arena.text(format!("{export}{prefix}{recursion}{name} ="));
                let expression = self.expression(expression);
                declaration.append(self.arena.space()).append(expression)
            }
        }
    }

    fn expression(&self, expression_id: ExpressionId) -> Doc<'a> {
        self.expression_at(expression_id, ExpressionPrecedence::Abstraction)
    }

    fn expression_at(
        &self,
        expression_id: ExpressionId,
        required_precedence: ExpressionPrecedence,
    ) -> Doc<'a> {
        let expression = &self.module.storage[expression_id];
        let precedence = match expression.kind {
            ExpressionKind::Error
            | ExpressionKind::Abstraction { .. }
            | ExpressionKind::UncurriedAbstraction { .. }
            | ExpressionKind::IfThenElse { .. }
            | ExpressionKind::Case { .. }
            | ExpressionKind::Guarded { .. }
            | ExpressionKind::Let { .. }
            | ExpressionKind::LetPattern { .. } => ExpressionPrecedence::Abstraction,
            ExpressionKind::Unary { .. }
            | ExpressionKind::Binary { .. }
            | ExpressionKind::Application { .. }
            | ExpressionKind::UncurriedApplication { .. }
            | ExpressionKind::StyleX(_)
            | ExpressionKind::Effect { .. }
            | ExpressionKind::SynthesizedEvidence { .. } => ExpressionPrecedence::Application,
            ExpressionKind::RecordUpdate { .. } => ExpressionPrecedence::RecordUpdate,
            ExpressionKind::Literal { .. }
            | ExpressionKind::Array { .. }
            | ExpressionKind::Record { .. }
            | ExpressionKind::Project { .. }
            | ExpressionKind::Constructor { .. }
            | ExpressionKind::Global { .. }
            | ExpressionKind::Native { .. }
            | ExpressionKind::Local { .. }
            | ExpressionKind::TrivialEvidence => ExpressionPrecedence::Atom,
        };
        let document = self.expression_unparenthesized(expression_id);
        if precedence < required_precedence {
            self.arena.text("(").append(document).append(")")
        } else {
            document
        }
    }

    fn expression_unparenthesized(&self, expression_id: ExpressionId) -> Doc<'a> {
        let expression = &self.module.storage[expression_id];
        match &expression.kind {
            ExpressionKind::Error => self.arena.text("<error>"),
            ExpressionKind::Literal { literal } => self.literal(literal),
            ExpressionKind::Array { elements } => {
                let elements = elements.iter().map(|element| self.expression(*element));
                self.delimited("[", elements, "]")
            }
            ExpressionKind::Record { fields } => {
                let fields = fields.iter().map(|field| {
                    let label = self.field(&field.field);
                    let expression = self.expression(field.expression);
                    let expression = self.arena.line().append(expression).nest(2);
                    self.arena.text(format!("{label}:")).append(expression).group()
                });
                self.braced(fields)
            }
            ExpressionKind::RecordUpdate { record, updates } => {
                let record = self.expression_at(*record, ExpressionPrecedence::Atom);
                let updates = updates.iter().map(|update| self.record_update(update));
                let updates = self.braced(updates);
                record.append(self.arena.space()).append(updates)
            }
            ExpressionKind::Project { record, field } => {
                let record = self.expression_at(*record, ExpressionPrecedence::Atom);
                let field = self.field(field);
                record.append(self.arena.text(format!(".{field}")))
            }
            ExpressionKind::Unary { operator, value } => {
                let operator = match operator {
                    UnaryOperator::BooleanNot => "boolean.not",
                    UnaryOperator::IntegerNegate => "integer.negate",
                    UnaryOperator::NumberNegate => "number.negate",
                };
                let value = self.expression_at(*value, ExpressionPrecedence::Atom);
                self.arena.text(operator).append(" ").append(value)
            }
            ExpressionKind::Binary { operator, left, right } => {
                let operator = match operator {
                    BinaryOperator::IntegerAdd => "integer.add",
                    BinaryOperator::IntegerSubtract => "integer.subtract",
                    BinaryOperator::IntegerMultiply => "integer.multiply",
                };
                let left = self.expression_at(*left, ExpressionPrecedence::Atom);
                let right = self.expression_at(*right, ExpressionPrecedence::Atom);
                self.arena.text(operator).append(" ").append(left).append(" ").append(right)
            }
            ExpressionKind::Constructor { global } | ExpressionKind::Global { global } => {
                self.arena.text(global.item_name.to_string())
            }
            ExpressionKind::Native { operation } => {
                self.arena.text(format!("<native {}>", operation.javascript_name()))
            }
            ExpressionKind::Local { parameter } => self.parameter(parameter),
            ExpressionKind::Abstraction { parameters, body } => {
                let parameters = if parameters.is_empty() {
                    self.arena.text("()")
                } else {
                    let parameters = parameters
                        .iter()
                        .map(|pattern| self.pattern_at(*pattern, PatternPrecedence::Atom));
                    self.arena.intersperse(parameters, self.arena.space())
                };
                let abstraction = self.arena.text("\\").append(parameters).append(" ->");
                let body_document = self.expression(*body);
                if self.expression_requires_body_break(*body) {
                    let body = self.arena.hardline().append(body_document).nest(2);
                    abstraction.append(body)
                } else {
                    let body = self.arena.line().append(body_document).nest(2);
                    abstraction.append(body).group()
                }
            }
            ExpressionKind::UncurriedAbstraction { parameters, body } => {
                let parameters = parameters
                    .iter()
                    .map(|pattern| self.pattern_at(*pattern, PatternPrecedence::Atom));
                let parameters = self.arena.intersperse(parameters, self.arena.text(", "));
                let abstraction = self.arena.text("uncurried \\").append(parameters).append(" ->");
                let body_document = self.expression(*body);
                if self.expression_requires_body_break(*body) {
                    let body = self.arena.hardline().append(body_document).nest(2);
                    abstraction.append(body)
                } else {
                    let body = self.arena.line().append(body_document).nest(2);
                    abstraction.append(body).group()
                }
            }
            ExpressionKind::Application { function, arguments, .. } => {
                let function = self.expression_at(*function, ExpressionPrecedence::Application);
                if arguments.is_empty() {
                    function.append("()")
                } else {
                    let arguments = arguments
                        .iter()
                        .map(|argument| self.expression_at(*argument, ExpressionPrecedence::Atom));
                    let arguments = self.arena.intersperse(arguments, self.arena.line());
                    let arguments = self.arena.line().append(arguments).nest(2);
                    function.append(arguments).group()
                }
            }
            ExpressionKind::UncurriedApplication { function, arguments, .. } => {
                let function = self.expression_at(*function, ExpressionPrecedence::Application);
                let arguments = arguments
                    .iter()
                    .map(|argument| self.expression_at(*argument, ExpressionPrecedence::Atom));
                let arguments = self.delimited("(", arguments, ")");
                self.arena.text("uncurried.call ").append(function).append(arguments)
            }
            ExpressionKind::StyleX(stylex) => self.stylex_expression(stylex),
            ExpressionKind::IfThenElse { condition, then, else_ } => {
                let condition = self.expression(*condition);
                let then = self.expression(*then);
                let then = self.arena.line().append("then ").append(then).nest(2);
                let else_ = self.expression(*else_);
                let else_ = self.arena.line().append("else ").append(else_).nest(2);
                self.arena.text("if ").append(condition).append(then).append(else_).group()
            }
            ExpressionKind::Case { scrutinees, alternatives } => {
                let scrutinees = scrutinees.iter().map(|scrutinee| self.expression(*scrutinee));
                let scrutinees = self.arena.intersperse(scrutinees, self.arena.text(", "));
                let alternatives = alternatives.iter().map(|alternative| {
                    let patterns =
                        alternative.patterns.iter().map(|pattern| self.pattern(*pattern));
                    let patterns = self.arena.intersperse(patterns, self.arena.text(", "));
                    let expression = self.expression(alternative.expression);
                    let expression = self.arena.hardline().append(expression).nest(2);
                    patterns.append(" ->").append(expression)
                });
                let alternatives = self.arena.intersperse(alternatives, self.arena.hardline());
                let alternatives = self.arena.hardline().append(alternatives).nest(2);
                self.arena.text("case ").append(scrutinees).append(" of").append(alternatives)
            }
            ExpressionKind::Guarded { alternatives } => {
                let alternatives = alternatives.iter().map(|alternative| {
                    let guards = alternative.guards.iter().map(|guard| self.guard(guard));
                    let guards = self.arena.intersperse(guards, self.arena.text(", "));
                    let expression = self.expression(alternative.expression);
                    let expression = self.arena.line().append(expression).nest(2);
                    self.arena.text("| ").append(guards).append(" ->").append(expression).group()
                });
                self.arena.intersperse(alternatives, self.arena.hardline())
            }
            ExpressionKind::Let { recursive, bindings, body } => {
                let bindings = bindings.iter().map(|binding| {
                    let recursive = if *recursive { "rec " } else { "" };
                    let parameter = self.parameter(&binding.parameter);
                    let expression = self.expression(binding.expression);
                    self.arena.text(recursive).append(parameter).append(" = ").append(expression)
                });
                let bindings = self.arena.intersperse(bindings, self.arena.hardline());
                let bindings = self.arena.hardline().append(bindings).nest(2);
                let body = self.expression(*body);
                self.arena
                    .text("let")
                    .append(bindings)
                    .append(self.arena.hardline())
                    .append("in ")
                    .append(body)
            }
            ExpressionKind::LetPattern { pattern, value, body } => {
                let pattern = self.pattern(*pattern);
                let value = self.expression(*value);
                let binding = pattern.append(" = ").append(value);
                let binding = self.arena.hardline().append(binding).nest(2);
                let body = self.expression(*body);
                self.arena
                    .text("let")
                    .append(binding)
                    .append(self.arena.hardline())
                    .append("in ")
                    .append(body)
            }
            ExpressionKind::Effect { effect } => self.effect(effect),
            ExpressionKind::SynthesizedEvidence { evidence } => self.synthesized_evidence(evidence),
            ExpressionKind::TrivialEvidence => self.arena.text("<trivial evidence>"),
        }
    }

    fn stylex_expression(&self, stylex: &StyleXExpression) -> Doc<'a> {
        match stylex {
            StyleXExpression::Call { target, arguments } => {
                let namespace = match target {
                    StyleXCallTarget::Root(_) => "stylex.",
                    StyleXCallTarget::Types(_) => "stylex.types.",
                };
                let arguments = arguments
                    .iter()
                    .map(|argument| self.expression_at(*argument, ExpressionPrecedence::Atom));
                let function = self.arena.text(format!("{namespace}{}", target.name()));
                self.arena
                    .intersperse(std::iter::once(function).chain(arguments), self.arena.space())
            }
            StyleXExpression::Conditional { condition, style } => {
                let condition = self.expression_at(*condition, ExpressionPrecedence::Atom);
                let style = self.expression_at(*style, ExpressionPrecedence::Atom);
                self.arena
                    .text("stylex.conditional ")
                    .append(condition)
                    .append(self.arena.space())
                    .append(style)
            }
            StyleXExpression::ConditionalCase(case) => self.stylex_conditional_case(case),
            StyleXExpression::ConditionalValue { default, cases } => {
                let default = self.expression_at(*default, ExpressionPrecedence::Atom);
                let cases = cases.iter().map(|case| self.stylex_conditional_case(case));
                self.arena
                    .text("stylex.conditionalValue ")
                    .append(default)
                    .append(self.arena.space())
                    .append(self.delimited("[", cases, "]"))
            }
        }
    }

    fn stylex_conditional_case(&self, case: &StyleXConditionalCase) -> Doc<'a> {
        let selector = self.expression_at(case.selector, ExpressionPrecedence::Atom);
        let marker =
            case.marker.map(|marker| self.expression_at(marker, ExpressionPrecedence::Atom));
        let value = self.expression_at(case.value, ExpressionPrecedence::Atom);
        let arguments = std::iter::once(selector).chain(marker).chain(std::iter::once(value));
        self.arena
            .text(format!("stylex.when.{}", case.relation.name()))
            .append(self.delimited("(", arguments, ")"))
    }

    fn pattern(&self, pattern_id: PatternId) -> Doc<'a> {
        self.pattern_at(pattern_id, PatternPrecedence::Application)
    }

    fn pattern_at(&self, pattern_id: PatternId, required_precedence: PatternPrecedence) -> Doc<'a> {
        let pattern = &self.module.storage[pattern_id];
        let precedence = match &pattern.kind {
            PatternKind::Constructor { arguments, .. } if !arguments.is_empty() => {
                PatternPrecedence::Application
            }
            _ => PatternPrecedence::Atom,
        };
        let document = match &pattern.kind {
            PatternKind::Variable(parameter) => self.parameter(parameter),
            PatternKind::Named { parameter, pattern } => {
                let parameter = self.parameter(parameter);
                let pattern = self.pattern(*pattern);
                parameter.append("@(").append(pattern).append(")")
            }
            PatternKind::Wildcard => self.arena.text("_"),
            PatternKind::Literal(literal) => self.literal(literal),
            PatternKind::Array(elements) => {
                let elements = elements.iter().map(|element| self.pattern(*element));
                self.delimited("[", elements, "]")
            }
            PatternKind::Record(fields) => {
                let fields = fields.iter().map(|field| {
                    let label = self.field(&field.field);
                    let pattern = self.pattern(field.pattern);
                    self.arena.text(format!("{label}: ")).append(pattern)
                });
                self.braced(fields)
            }
            PatternKind::Constructor { global, arguments } => {
                let arguments = arguments
                    .iter()
                    .map(|argument| self.pattern_at(*argument, PatternPrecedence::Atom));
                let arguments = arguments.map(|argument| self.arena.space().append(argument));
                let arguments = self.arena.concat(arguments);
                self.arena.text(global.item_name.to_string()).append(arguments)
            }
        };
        if precedence < required_precedence {
            self.arena.text("(").append(document).append(")")
        } else {
            document
        }
    }

    fn guard(&self, guard: &Guard) -> Doc<'a> {
        match guard {
            Guard::Boolean(expression) => self.expression(*expression),
            Guard::Pattern { expression, pattern } => {
                let pattern = self.pattern(*pattern);
                let expression = self.expression(*expression);
                pattern.append(" <- ").append(expression)
            }
        }
    }

    fn record_update(&self, update: &RecordUpdate) -> Doc<'a> {
        match update {
            RecordUpdate::Leaf { field, expression } => {
                let field = self.field(field);
                let expression = self.expression(*expression);
                let expression = self.arena.line().append(expression).nest(2);
                self.arena.text(format!("{field} =")).append(expression).group()
            }
            RecordUpdate::Branch { field, updates } => {
                let field = self.field(field);
                let updates = updates.iter().map(|update| self.record_update(update));
                let updates = self.braced(updates);
                self.arena.text(field).append(self.arena.space()).append(updates)
            }
        }
    }

    fn effect(&self, effect: &EffectExpression) -> Doc<'a> {
        match effect {
            EffectExpression::Pure(expression) => {
                let expression = self.expression_at(*expression, ExpressionPrecedence::Atom);
                self.arena.text("effect.pure ").append(expression)
            }
            EffectExpression::Bind { action, parameter, body } => {
                let action = self.expression_at(*action, ExpressionPrecedence::Atom);
                let parameter = self.parameter(parameter);
                let body = self.expression(*body);
                let continuation = self
                    .arena
                    .line()
                    .append("\\")
                    .append(parameter)
                    .append(" -> ")
                    .append(body)
                    .nest(2);
                self.arena.text("effect.bind ").append(action).append(continuation).group()
            }
            EffectExpression::Map { function, action } => {
                let function = self.expression_at(*function, ExpressionPrecedence::Atom);
                let action = self.expression_at(*action, ExpressionPrecedence::Atom);
                let arguments = self
                    .arena
                    .line()
                    .append(function)
                    .append(self.arena.line())
                    .append(action)
                    .nest(2);
                self.arena.text("effect.map").append(arguments).group()
            }
            EffectExpression::Apply { function_action, argument_action } => {
                let function_action =
                    self.expression_at(*function_action, ExpressionPrecedence::Atom);
                let argument_action =
                    self.expression_at(*argument_action, ExpressionPrecedence::Atom);
                let arguments = self
                    .arena
                    .line()
                    .append(function_action)
                    .append(self.arena.line())
                    .append(argument_action)
                    .nest(2);
                self.arena.text("effect.apply").append(arguments).group()
            }
        }
    }

    fn synthesized_evidence(&self, evidence: &SynthesizedEvidence) -> Doc<'a> {
        let rendered = match evidence {
            SynthesizedEvidence::IsSymbol(symbol) => format!("symbol {symbol:?}"),
            SynthesizedEvidence::Reflectable(ReflectableEvidence::Integer(value)) => {
                format!("reflectable {value}")
            }
            SynthesizedEvidence::Reflectable(ReflectableEvidence::String(value)) => {
                format!("reflectable {value:?}")
            }
            SynthesizedEvidence::Reflectable(ReflectableEvidence::Boolean(value)) => {
                format!("reflectable {value}")
            }
            SynthesizedEvidence::Reflectable(ReflectableEvidence::Ordering(ordering)) => {
                match ordering {
                    ReflectableOrdering::Less => "reflectable LT".into(),
                    ReflectableOrdering::Equal => "reflectable EQ".into(),
                    ReflectableOrdering::Greater => "reflectable GT".into(),
                }
            }
            SynthesizedEvidence::AbortIdentity(identity) => {
                format!("abort identity {identity:?}")
            }
        };
        self.arena.text(rendered)
    }

    fn literal(&self, literal: &Literal) -> Doc<'a> {
        let rendered = match literal {
            Literal::String(value) => format!("{value:?}"),
            Literal::Char(value) => format!("{value:?}"),
            Literal::Boolean(value) => value.to_string(),
            Literal::Integer(value) => value.to_string(),
            Literal::Number(value) => value.to_string(),
        };
        self.arena.text(rendered)
    }

    fn parameter(&self, parameter: &Parameter) -> Doc<'a> {
        self.arena.text(format!("{}%{}", parameter.name, parameter.id.0))
    }

    fn field(&self, field: &Field) -> String {
        let name = field.name.as_str();
        let is_valid_identifier = !name.is_empty()
            && name.chars().enumerate().all(|(position, character)| {
                if position == 0 {
                    character.is_ascii_lowercase() || character == '_'
                } else {
                    character.is_ascii_alphanumeric() || character == '_' || character == '\''
                }
            });
        if is_valid_identifier { name.to_string() } else { format!("{name:?}") }
    }

    fn expression_requires_body_break(&self, expression_id: ExpressionId) -> bool {
        matches!(
            self.module.storage[expression_id].kind,
            ExpressionKind::IfThenElse { .. }
                | ExpressionKind::Case { .. }
                | ExpressionKind::Guarded { .. }
                | ExpressionKind::Let { .. }
                | ExpressionKind::LetPattern { .. }
        )
    }

    fn delimited<I>(&self, open: &'static str, documents: I, close: &'static str) -> Doc<'a>
    where
        I: IntoIterator<Item = Doc<'a>>,
    {
        let separator = self.arena.text(",").append(self.arena.line());
        let documents = self.arena.intersperse(documents, separator);
        let documents = self.arena.softline_().append(documents).nest(2);
        let closing_line = self.arena.softline_();
        self.arena.text(open).append(documents).append(closing_line).append(close).group()
    }

    fn braced<I>(&self, documents: I) -> Doc<'a>
    where
        I: IntoIterator<Item = Doc<'a>>,
    {
        let separator = self.arena.text(",").append(self.arena.line());
        let documents = self.arena.intersperse(documents, separator);
        let documents = self.arena.line().append(documents).nest(2);
        let closing_line = self.arena.line();
        self.arena.text("{").append(documents).append(closing_line).append("}").group()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ExpressionPrecedence {
    Abstraction,
    RecordUpdate,
    Application,
    Atom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum PatternPrecedence {
    Application,
    Atom,
}
