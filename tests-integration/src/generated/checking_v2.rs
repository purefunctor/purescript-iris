use std::fmt::Write;

use building::QueryEngine;
use checking_v2::error::ErrorCrumb;
use checking_v2::pretty::{self, Printer};
use checking_v2::tree::{
    Abstraction, BinderId, BinderKind, CaseBranch, CheckedTree, Evidence, EvidenceBinderId,
    ExpressionId, ExpressionKind, Guard, Guarded, Literal, LocalBinding, Parameter, RecordUpdate,
    TermReference, ValueDeclaration,
};
use checking_v2::{CheckedModule, InstanceOrigin};
use files::FileId;
use indexing::{TermItemId, TypeItemId};
use itertools::Itertools;
use line_index::LineIndex;
use smol_str::{SmolStr, format_smolstr};

struct Report<'a> {
    engine: &'a QueryEngine,
    file_id: FileId,
    checked: &'a CheckedModule,
    printer: Printer<'a>,
}

fn type_name(engine: &QueryEngine, file_id: FileId, type_id: TypeItemId) -> SmolStr {
    let Ok(indexed) = engine.indexed(file_id) else {
        return SmolStr::new_static("<unknown>");
    };
    let name = indexed.items[type_id].name.as_ref();
    name.map(SmolStr::clone).unwrap_or_else(|| SmolStr::new_static("<anonymous>"))
}

fn term_name(engine: &QueryEngine, file_id: FileId, term_id: TermItemId) -> SmolStr {
    let Ok(indexed) = engine.indexed(file_id) else {
        return SmolStr::new_static("<unknown>");
    };
    let name = indexed.items[term_id].name.as_ref();
    name.map(SmolStr::clone).unwrap_or_else(|| SmolStr::new_static("<anonymous>"))
}

fn origin_name(engine: &QueryEngine, file_id: FileId, origin: InstanceOrigin) -> SmolStr {
    let Ok(indexed) = engine.indexed(file_id) else {
        return SmolStr::new_static("<unknown>");
    };
    let name = match origin {
        InstanceOrigin::Instance(id) => indexed.items[id].name.as_ref(),
        InstanceOrigin::Derive(id) => indexed.items[id].name.as_ref(),
    };
    let index = match origin {
        InstanceOrigin::Instance(id) => id.into_raw().into_u32(),
        InstanceOrigin::Derive(id) => id.into_raw().into_u32(),
    };
    name.map(SmolStr::clone).unwrap_or_else(|| format_smolstr!("<instance {index}>"))
}

pub fn report(engine: &QueryEngine, id: FileId, name: &str) -> String {
    let mut out = String::default();
    writeln!(out, "module {name}").unwrap();

    let checked = match engine.checked_v2(id) {
        Ok(checked) => checked,
        Err(error) => {
            writeln!(out, "\nquery failed: {error:?}").unwrap();
            return out;
        }
    };

    let names = |file_id: FileId, type_id: TypeItemId| type_name(engine, file_id, type_id);
    let record = engine.module_file("Prim").and_then(|prim| {
        let resolved = engine.resolved(prim).ok()?;
        resolved.exports.lookup_type("Record")
    });
    let printer = Printer::new(engine.checking_v2_interners(), &names, record);
    let report = Report { engine, file_id: id, checked: &checked, printer };

    let indexed = engine.indexed(id).unwrap();

    writeln!(out, "\nTypes").unwrap();
    for (type_id, item) in indexed.items.iter_types() {
        let Some(name) = &item.name else { continue };
        let Some(&kind) = checked.type_kinds.get(&type_id) else { continue };
        writeln!(out, "  {name} :: {}", report.printer.render(kind)).unwrap();
    }

    writeln!(out, "\nRoles").unwrap();
    for (type_id, item) in indexed.items.iter_types() {
        let Some(name) = &item.name else { continue };
        let Some(roles) = checked.roles.get(&type_id) else { continue };
        if roles.is_empty() {
            continue;
        }
        let roles = roles.iter().map(|&role| pretty::role(role)).join(" ");
        writeln!(out, "  {name} {roles}").unwrap();
    }

    writeln!(out, "\nTerms").unwrap();
    for (term_id, item) in indexed.items.iter_terms() {
        let Some(name) = &item.name else { continue };
        let Some(&type_id) = checked.term_types.get(&term_id) else { continue };
        writeln!(out, "  {name} :: {}", report.printer.render(type_id)).unwrap();
    }

    if !checked.instances.is_empty() {
        writeln!(out, "\nInstances").unwrap();
        for instance in &checked.instances {
            let (class_file, class_id) = instance.class;
            let class = type_name(engine, class_file, class_id);
            let arguments =
                instance.arguments.iter().map(|&argument| report.printer.render_argument(argument));
            let constraints =
                instance.constraints.iter().map(|&constraint| report.printer.render(constraint));
            let constraints = constraints.collect_vec();
            let context = if constraints.is_empty() {
                String::new()
            } else {
                format!("{} => ", constraints.join(", "))
            };
            let name = origin_name(engine, id, instance.origin);
            let chain = match instance.chain {
                Some((_, position)) if position > 0 => format!(" (chain position {position})"),
                _ => String::new(),
            };
            writeln!(out, "  {name} :: {context}{class} {}{chain}", arguments.format(" ")).unwrap();
        }
    }

    writeln!(out, "\nErrors").unwrap();
    let content = engine.content(id).unwrap();
    let line_index = LineIndex::new(&content);
    for error in &checked.errors {
        let location = report.location(id, &content, &line_index, &error.crumbs);
        let message = report.printer.render_error(&error.kind);
        writeln!(out, "  {location}{message}").unwrap();
    }

    writeln!(out, "\nTree").unwrap();
    for (term_id, item) in indexed.items.iter_terms() {
        let Some(name) = &item.name else { continue };
        let Some(value) = checked.tree.values.get(&term_id) else { continue };
        report.value(&mut out, 1, name, value);
    }
    for (origin, declaration) in checked.tree.instances.iter().sorted_by_key(|(origin, _)| **origin)
    {
        let name = origin_name(engine, id, *origin);
        let evidence = declaration.evidence.iter().map(|&(constraint, binder)| {
            format!("{} : {}", evidence_binder(binder), report.printer.render(constraint))
        });
        writeln!(out, "  instance {name} [{}]", evidence.format(", ")).unwrap();
        for (index, &slot) in declaration.superclasses.iter().enumerate() {
            let evidence = report.evidence(checked.tree.evidence(slot));
            writeln!(out, "    superclass {index} = {evidence}").unwrap();
        }
        for member in declaration.members.iter() {
            let (file_id, item) = member.item;
            let name = term_name(engine, file_id, item);
            report.value(&mut out, 2, &name, &member.value);
        }
    }

    out
}

impl Report<'_> {
    fn location(
        &self,
        id: FileId,
        content: &str,
        line_index: &LineIndex,
        crumbs: &[ErrorCrumb],
    ) -> String {
        let declaration = crumbs.iter().find_map(|crumb| match crumb {
            ErrorCrumb::TermDeclaration(item) => Some(term_name(self.engine, id, *item)),
            ErrorCrumb::TypeDeclaration(item) => Some(type_name(self.engine, id, *item)),
            ErrorCrumb::InstanceDeclaration(item) => {
                Some(origin_name(self.engine, id, InstanceOrigin::Instance(*item)))
            }
            ErrorCrumb::DeriveDeclaration(item) => {
                Some(origin_name(self.engine, id, InstanceOrigin::Derive(*item)))
            }
            _ => None,
        });
        let declaration = declaration.map(|name| format!("[{name}] ")).unwrap_or_default();

        let Ok(stabilized) = self.engine.stabilized(id) else {
            return declaration;
        };
        let pointer = crumbs.iter().rev().find_map(|crumb| match crumb {
            ErrorCrumb::Expression(expression) => stabilized.syntax_ptr(*expression),
            ErrorCrumb::Binder(binder) => stabilized.syntax_ptr(*binder),
            ErrorCrumb::Type(type_id) => stabilized.syntax_ptr(*type_id),
            ErrorCrumb::DoStatement(statement) => stabilized.syntax_ptr(*statement),
            _ => None,
        });
        let Some(pointer) = pointer else {
            return declaration;
        };
        let range = pointer.text_range();
        let start = line_index.line_col(range.start());
        let text = content.get(usize::from(range.start())..usize::from(range.end()));
        let text = text.unwrap_or_default().trim();
        let text = text.lines().next().unwrap_or_default();
        let text = if text.chars().count() > 40 {
            format!("{}…", text.chars().take(40).collect::<String>())
        } else {
            text.to_string()
        };
        format!("{declaration}{}:{} `{text}` ", start.line + 1, start.col + 1)
    }

    fn value(&self, out: &mut String, depth: usize, name: &str, value: &ValueDeclaration) {
        let indent = "  ".repeat(depth);
        writeln!(out, "{indent}{name} :: {}", self.printer.render(value.type_id)).unwrap();
        let type_binders = value.type_binders.iter().map(|&binder| self.printer.render(binder));
        let evidence = value.evidence.iter().map(|&(constraint, binder)| {
            format!("{} : {}", evidence_binder(binder), self.printer.render(constraint))
        });
        writeln!(
            out,
            "{indent}  abstract [{}] [{}]",
            type_binders.format(", "),
            evidence.format(", ")
        )
        .unwrap();
        let abstracts = value.parameters.iter().any(|parameter| match parameter {
            Parameter::Abstract(_) => true,
            Parameter::Argument(_) => false,
        });
        if abstracts {
            let parameters = value.parameters.iter().map(|parameter| self.parameter(parameter));
            writeln!(out, "{indent}  parameters [{}]", parameters.format(", ")).unwrap();
        }
        for equation in value.equations.iter() {
            let binders = equation.binders.iter().map(|&binder| self.binder(binder)).join(" ");
            let guarded = self.guarded(&equation.guarded);
            writeln!(out, "{indent}  {name} {binders} {guarded}").unwrap();
        }
    }

    fn parameter(&self, parameter: &Parameter) -> String {
        match parameter {
            Parameter::Argument(type_id) => self.printer.render(*type_id),
            Parameter::Abstract(Abstraction { type_binders, evidence }) => {
                let type_binders = type_binders.iter().map(|&binder| self.printer.render(binder));
                let evidence = evidence.iter().map(|&(constraint, binder)| {
                    format!("{} : {}", evidence_binder(binder), self.printer.render(constraint))
                });
                format!("Λ[{}] [{}]", type_binders.format(", "), evidence.format(", "))
            }
        }
    }

    fn tree(&self) -> &CheckedTree {
        &self.checked.tree
    }

    fn guarded(&self, guarded: &Guarded) -> String {
        match guarded {
            Guarded::Unconditional(expression) => format!("= {}", self.expression(*expression)),
            Guarded::Conditionals(conditionals) => {
                let conditionals = conditionals.iter().map(|conditional| {
                    let guards = conditional.guards.iter().map(|guard| self.guard(guard));
                    format!(
                        "| {} = {}",
                        guards.format(", "),
                        self.expression(conditional.expression)
                    )
                });
                conditionals.format(" ").to_string()
            }
        }
    }

    fn guard(&self, guard: &Guard) -> String {
        match guard {
            Guard::Boolean(expression) => self.expression(*expression),
            Guard::Pattern(binder, expression) => {
                format!("{} <- {}", self.binder(*binder), self.expression(*expression))
            }
            Guard::Let(bindings) => format!("let {{ {} }}", self.local_bindings(bindings)),
        }
    }

    fn local_bindings(&self, bindings: &[LocalBinding]) -> String {
        let bindings = bindings.iter().map(|binding| match binding {
            LocalBinding::Value(value) => {
                let equations = value.value.equations.iter().map(|equation| {
                    let binders = equation.binders.iter().map(|&binder| self.binder(binder));
                    format!("{} {}", binders.format(" "), self.guarded(&equation.guarded))
                });
                let type_binders =
                    value.value.type_binders.iter().map(|&binder| self.printer.render(binder));
                let evidence =
                    value.value.evidence.iter().map(|&(_, binder)| evidence_binder(binder));
                format!(
                    "{} :: {} [{}] [{}] {}",
                    value.name,
                    self.printer.render(value.value.type_id),
                    type_binders.format(", "),
                    evidence.format(", "),
                    equations.format("; "),
                )
            }
            LocalBinding::Pattern { binder, expression } => {
                format!("{} = {}", self.binder(*binder), self.expression(*expression))
            }
        });
        bindings.format("; ").to_string()
    }

    fn reference(&self, reference: TermReference) -> String {
        match reference {
            TermReference::Global(file_id, item) | TermReference::Constructor(file_id, item) => {
                term_name(self.engine, file_id, item).to_string()
            }
            TermReference::Binder(binder) => match &self.tree().binders[binder].kind {
                BinderKind::Variable(name) | BinderKind::Named(name, _) => name.to_string(),
                _ => "<binder>".to_string(),
            },
            TermReference::Let(group) => {
                let Ok(lowered) = self.engine.lowered(self.file_id) else {
                    return "<let>".to_string();
                };
                let name = lowered.tree.get_let_binding_group(group).name.as_ref();
                name.map_or_else(|| "<let>".to_string(), SmolStr::to_string)
            }
        }
    }

    fn expression(&self, id: ExpressionId) -> String {
        let expression = &self.tree().expressions[id];
        match &expression.kind {
            ExpressionKind::Reference(reference) => self.reference(*reference),
            ExpressionKind::Instantiate { expression, type_arguments, evidence } => {
                let type_arguments =
                    type_arguments.iter().map(|&argument| self.printer.render(argument));
                let evidence =
                    evidence.iter().map(|&slot| self.evidence(self.tree().evidence(slot)));
                format!(
                    "({} @[{}] {{{}}})",
                    self.expression(*expression),
                    type_arguments.format(", "),
                    evidence.format(", ")
                )
            }
            ExpressionKind::Abstract { type_binders, evidence, expression } => {
                let type_binders = type_binders.iter().map(|&binder| self.printer.render(binder));
                let evidence = evidence.iter().map(|&(constraint, binder)| {
                    format!("{} : {}", evidence_binder(binder), self.printer.render(constraint))
                });
                format!(
                    "(Λ[{}] [{}]. {})",
                    type_binders.format(", "),
                    evidence.format(", "),
                    self.expression(*expression)
                )
            }
            ExpressionKind::Application { function, argument } => {
                format!("({} {})", self.expression(*function), self.expression(*argument))
            }
            ExpressionKind::Lambda { binder, body } => {
                format!("(\\{} -> {})", self.binder(*binder), self.expression(*body))
            }
            ExpressionKind::Case { scrutinees, branches } => {
                let scrutinees = scrutinees.iter().map(|&scrutinee| self.expression(scrutinee));
                let branches = branches.iter().map(|CaseBranch { binders, guarded }| {
                    let binders = binders.iter().map(|&binder| self.binder(binder));
                    let guarded = self.guarded(guarded).replacen("= ", "-> ", 1);
                    format!("{} {guarded}", binders.format(", "))
                });
                format!("(case {} of {{ {} }})", scrutinees.format(", "), branches.format("; "))
            }
            ExpressionKind::Let { bindings, body } => {
                format!(
                    "(let {{ {} }} in {})",
                    self.local_bindings(bindings),
                    self.expression(*body)
                )
            }
            ExpressionKind::IfThenElse { condition, then, else_ } => format!(
                "(if {} then {} else {})",
                self.expression(*condition),
                self.expression(*then),
                self.expression(*else_)
            ),
            ExpressionKind::Literal(literal) => render_literal(literal),
            ExpressionKind::Array(elements) => {
                format!(
                    "[{}]",
                    elements.iter().map(|&element| self.expression(element)).format(", ")
                )
            }
            ExpressionKind::Record(fields) => {
                let fields = fields
                    .iter()
                    .map(|(label, value)| format!("{label}: {}", self.expression(*value)));
                format!("{{ {} }}", fields.format(", "))
            }
            ExpressionKind::RecordAccess { record, label } => {
                format!("{}.{label}", self.expression(*record))
            }
            ExpressionKind::RecordUpdate { record, updates } => {
                format!("({} {{ {} }})", self.expression(*record), self.record_updates(updates))
            }
            ExpressionKind::Hole(name) => format!("?{name}"),
            ExpressionKind::Error => "<error>".to_string(),
        }
    }

    fn record_updates(&self, updates: &[RecordUpdate]) -> String {
        let updates = updates.iter().map(|update| match update {
            RecordUpdate::Leaf { label, expression } => {
                format!("{label} = {}", self.expression(*expression))
            }
            RecordUpdate::Branch { label, updates } => {
                format!("{label} {{ {} }}", self.record_updates(updates))
            }
        });
        updates.format(", ").to_string()
    }

    fn binder(&self, id: BinderId) -> String {
        let binder = &self.tree().binders[id];
        let type_id = self.printer.render(binder.type_id);
        let kind = match &binder.kind {
            BinderKind::Variable(name) => name.to_string(),
            BinderKind::Wildcard => "_".to_string(),
            BinderKind::Named(name, inner) => format!("{name}@{}", self.binder(*inner)),
            BinderKind::Literal(literal) => render_literal(literal),
            BinderKind::Constructor { file_id, item, arguments } => {
                let name = term_name(self.engine, *file_id, *item);
                let arguments = arguments.iter().map(|&argument| self.binder(argument));
                format!("{name} {}", arguments.format(" ")).trim_end().to_string()
            }
            BinderKind::Array(elements) => {
                format!("[{}]", elements.iter().map(|&element| self.binder(element)).format(", "))
            }
            BinderKind::Record(fields) => {
                let fields = fields
                    .iter()
                    .map(|(label, binder)| format!("{label}: {}", self.binder(*binder)));
                format!("{{ {} }}", fields.format(", "))
            }
            BinderKind::Error => "<error>".to_string(),
        };
        format!("({kind} :: {type_id})")
    }

    fn evidence(&self, evidence: &Evidence) -> String {
        match evidence {
            Evidence::Unsolved => "<unsolved>".to_string(),
            Evidence::Binder(binder) => evidence_binder(*binder),
            Evidence::Instance { file_id, origin, type_arguments, arguments } => {
                let name = origin_name(self.engine, *file_id, *origin);
                let type_arguments =
                    type_arguments.iter().map(|&argument| self.printer.render(argument));
                let arguments =
                    arguments.iter().map(|&slot| self.evidence(self.tree().evidence(slot)));
                format!("{name}@[{}]({})", type_arguments.format(", "), arguments.format(", "))
            }
            Evidence::Superclass { from, class, index } => {
                let (file_id, type_id) = *class;
                let class = type_name(self.engine, file_id, type_id);
                format!("{}.{class}#{index}", self.evidence(from))
            }
            Evidence::Builtin { class, value } => {
                let (file_id, type_id) = *class;
                let class = type_name(self.engine, file_id, type_id);
                format!("builtin {class} {value:?}")
            }
        }
    }
}

fn evidence_binder(EvidenceBinderId(index): EvidenceBinderId) -> String {
    format!("d{index}")
}

fn render_literal(literal: &Literal) -> String {
    match literal {
        Literal::Integer(value) => value.to_string(),
        Literal::Number(value) => value.to_string(),
        Literal::String(value) => format!("{value:?}"),
        Literal::Char(value) => format!("{value:?}"),
        Literal::Boolean(value) => value.to_string(),
    }
}
