use files::FileId;
use indexing::TypeItemId;
use smol_str::SmolStr;

use crate::Interners;
use crate::error::ErrorKind;
use crate::types::{Role, Type, TypeId, UnificationId};

pub struct Printer<'a> {
    interners: &'a Interners,
    names: &'a dyn Fn(FileId, TypeItemId) -> SmolStr,
    record: Option<(FileId, TypeItemId)>,
}

const PRECEDENCE_TOP: u8 = 0;
const PRECEDENCE_FUNCTION_ARGUMENT: u8 = 1;
const PRECEDENCE_APPLICATION_FUNCTION: u8 = 2;
const PRECEDENCE_APPLICATION_ARGUMENT: u8 = 3;

impl<'a> Printer<'a> {
    pub fn new(
        interners: &'a Interners,
        names: &'a dyn Fn(FileId, TypeItemId) -> SmolStr,
        record: Option<(FileId, TypeItemId)>,
    ) -> Printer<'a> {
        Printer { interners, names, record }
    }

    pub fn render(&self, id: TypeId) -> String {
        let mut output = String::new();
        self.render_into(id, PRECEDENCE_TOP, &mut output);
        output
    }

    pub fn render_argument(&self, id: TypeId) -> String {
        let mut output = String::new();
        self.render_into(id, PRECEDENCE_APPLICATION_ARGUMENT, &mut output);
        output
    }

    pub fn render_error(&self, kind: &ErrorKind) -> String {
        let render = |id: TypeId| self.render(id);
        match kind {
            ErrorKind::TypesDoNotUnify { expected, actual } => {
                format!("TypesDoNotUnify: {} ~ {}", render(*expected), render(*actual))
            }
            ErrorKind::KindsDoNotUnify { expected, actual } => {
                format!("KindsDoNotUnify: {} ~ {}", render(*expected), render(*actual))
            }
            ErrorKind::InfiniteType { unification, solution } => {
                format!("InfiniteType: {} ~ {}", render(*unification), render(*solution))
            }
            ErrorKind::InfiniteKind { unification, solution } => {
                format!("InfiniteKind: {} ~ {}", render(*unification), render(*solution))
            }
            ErrorKind::EscapedSkolem { rigid, solution } => {
                format!("EscapedSkolem: {} in {}", render(*rigid), render(*solution))
            }
            ErrorKind::PropertyIsMissing { label } => format!("PropertyIsMissing: {label}"),
            ErrorKind::AdditionalProperty { label } => format!("AdditionalProperty: {label}"),
            ErrorKind::DuplicateLabel { label } => format!("DuplicateLabel: {label}"),
            ErrorKind::PartiallyAppliedSynonym => "PartiallyAppliedSynonym".to_string(),
            ErrorKind::CannotApplyType { function_kind } => {
                format!("CannotApplyType: {}", render(*function_kind))
            }
            ErrorKind::CannotApplyExpressionOfTypeOnType { function_type } => {
                format!("CannotApplyExpressionOfTypeOnType: {}", render(*function_type))
            }
            ErrorKind::ExpectedFunction { function_type } => {
                format!("ExpectedFunction: {}", render(*function_type))
            }
            ErrorKind::IncorrectConstructorArity { expected, actual } => {
                format!("IncorrectConstructorArity: expected {expected}, actual {actual}")
            }
            ErrorKind::OverlappingArgNames { name } => format!("OverlappingArgNames: {name}"),
            ErrorKind::ArgListLengthsDiffer { expected, actual } => {
                format!("ArgListLengthsDiffer: expected {expected}, actual {actual}")
            }
            ErrorKind::NoInstanceFound { constraint } => {
                format!("NoInstanceFound: {}", render(*constraint))
            }
            ErrorKind::OverlappingInstances { constraint, instances } => {
                format!("OverlappingInstances: {} ({instances} instances)", render(*constraint))
            }
            ErrorKind::AmbiguousTypeVariables { constraint } => {
                format!("AmbiguousTypeVariables: {}", render(*constraint))
            }
            ErrorKind::PossiblyInfiniteInstance { constraint } => {
                format!("PossiblyInfiniteInstance: {}", render(*constraint))
            }
            ErrorKind::PossiblyInfiniteCoercibleInstance { constraint } => {
                format!("PossiblyInfiniteCoercibleInstance: {}", render(*constraint))
            }
            ErrorKind::UserDefinedError { message } => {
                format!("UserDefinedError: {}", render(*message))
            }
            ErrorKind::UserDefinedWarning { message } => {
                format!("UserDefinedWarning: {}", render(*message))
            }
            ErrorKind::HoleInferredType { name, inferred } => {
                format!("HoleInferredType: ?{name} :: {}", render(*inferred))
            }
            ErrorKind::WildcardInferredType { inferred } => {
                format!("WildcardInferredType: {}", render(*inferred))
            }
            ErrorKind::CycleInDeclaration { name } => format!("CycleInDeclaration: {name}"),
            ErrorKind::RoleMismatch { declared, inferred } => {
                format!("RoleMismatch: declared {}, inferred {}", role(*declared), role(*inferred))
            }
            ErrorKind::RoleDeclarationArityMismatch { expected, actual } => {
                format!("RoleDeclarationArityMismatch: expected {expected}, actual {actual}")
            }
            ErrorKind::OrphanInstance => "OrphanInstance".to_string(),
            ErrorKind::InvalidInstanceHead { argument } => {
                format!("InvalidInstanceHead: {}", render(*argument))
            }
            ErrorKind::CannotDerive { class } => format!("CannotDerive: {class}"),
            ErrorKind::MissingClassMember { name } => format!("MissingClassMember: {name}"),
            ErrorKind::ExtraneousClassMember { name } => format!("ExtraneousClassMember: {name}"),
            ErrorKind::CannotGeneralizeRecursiveFunction { constraint } => {
                format!("CannotGeneralizeRecursiveFunction: {}", render(*constraint))
            }
            ErrorKind::QuantificationCheckFailure { kind } => {
                format!("QuantificationCheckFailure: {}", render(*kind))
            }
            ErrorKind::UnsupportedFeature { feature } => format!("UnsupportedFeature: {feature}"),
            ErrorKind::InvalidSyntax => "InvalidSyntax".to_string(),
        }
    }

    fn render_into(&self, id: TypeId, precedence: u8, output: &mut String) {
        let interners = self.interners;
        let parenthesise = |output: &mut String, needed: bool, body: &dyn Fn(&mut String)| {
            if needed {
                output.push('(');
            }
            body(output);
            if needed {
                output.push(')');
            }
        };

        match interners.lookup_type(id) {
            Type::Application(function, argument) => {
                if let Type::Constructor(file_id, type_id) = interners.lookup_type(function)
                    && self.record == Some((file_id, type_id))
                    && let Type::Row(_) = interners.lookup_type(argument)
                {
                    self.render_row(argument, '{', '}', output);
                    return;
                }
                parenthesise(output, precedence >= PRECEDENCE_APPLICATION_ARGUMENT, &|output| {
                    self.render_into(function, PRECEDENCE_APPLICATION_FUNCTION, output);
                    output.push(' ');
                    self.render_into(argument, PRECEDENCE_APPLICATION_ARGUMENT, output);
                });
            }
            Type::KindApplication(function, argument) => {
                parenthesise(output, precedence >= PRECEDENCE_APPLICATION_ARGUMENT, &|output| {
                    self.render_into(function, PRECEDENCE_APPLICATION_FUNCTION, output);
                    output.push_str(" @");
                    self.render_into(argument, PRECEDENCE_APPLICATION_ARGUMENT, output);
                });
            }
            Type::Function(argument, result) => {
                parenthesise(output, precedence > PRECEDENCE_TOP, &|output| {
                    self.render_into(argument, PRECEDENCE_FUNCTION_ARGUMENT, output);
                    output.push_str(" -> ");
                    self.render_into(result, PRECEDENCE_TOP, output);
                });
            }
            Type::Forall(..) => {
                parenthesise(output, precedence > PRECEDENCE_TOP, &|output| {
                    output.push_str("forall");
                    let mut current = id;
                    while let Type::Forall(binder_id, inner) = interners.lookup_type(current) {
                        let binder = interners.lookup_forall_binder(binder_id);
                        output.push_str(" (");
                        if binder.visible {
                            output.push('@');
                        }
                        output.push_str(interners.lookup_text(binder.text));
                        output.push_str(" :: ");
                        self.render_into(binder.kind, PRECEDENCE_TOP, output);
                        output.push(')');
                        current = inner;
                    }
                    output.push_str(". ");
                    self.render_into(current, PRECEDENCE_TOP, output);
                });
            }
            Type::Constrained(constraint, constrained) => {
                parenthesise(output, precedence > PRECEDENCE_TOP, &|output| {
                    self.render_into(constraint, PRECEDENCE_FUNCTION_ARGUMENT, output);
                    output.push_str(" => ");
                    self.render_into(constrained, PRECEDENCE_TOP, output);
                });
            }
            Type::Kinded(inner, kind) => {
                output.push('(');
                self.render_into(inner, PRECEDENCE_TOP, output);
                output.push_str(" :: ");
                self.render_into(kind, PRECEDENCE_TOP, output);
                output.push(')');
            }
            Type::Constructor(file_id, type_id) => {
                output.push_str(&(self.names)(file_id, type_id));
            }
            Type::Synonym(application_id) => {
                let application = interners.lookup_synonym_application(application_id);
                self.render_into(application.expansion, precedence, output);
            }
            Type::Integer(value) => output.push_str(&value.to_string()),
            Type::String(text) => {
                output.push('"');
                output.push_str(interners.lookup_text(text));
                output.push('"');
            }
            Type::Row(_) => self.render_row(id, '(', ')', output),
            Type::Variable(binder_id) => {
                let binder = interners.lookup_forall_binder(binder_id);
                output.push_str(interners.lookup_text(binder.text));
            }
            Type::Rigid(rigid_id) => {
                let rigid = interners.lookup_rigid(rigid_id);
                output.push_str(interners.lookup_text(rigid.text));
                output.push('#');
                output.push_str(&rigid.name.unique.to_string());
            }
            Type::Unification(UnificationId(index)) => {
                output.push('?');
                output.push_str(&index.to_string());
            }
            Type::Unknown => output.push_str("???"),
        }
    }

    fn render_row(&self, id: TypeId, open: char, close: char, output: &mut String) {
        let Type::Row(row_id) = self.interners.lookup_type(id) else {
            self.render_into(id, PRECEDENCE_TOP, output);
            return;
        };
        let row = self.interners.lookup_row_type(row_id);
        output.push(open);
        if !row.fields.is_empty() {
            output.push(' ');
        }
        for (index, field) in row.fields.iter().enumerate() {
            if index > 0 {
                output.push_str(", ");
            }
            output.push_str(&field.label);
            output.push_str(" :: ");
            self.render_into(field.id, PRECEDENCE_TOP, output);
        }
        if let Some(tail) = row.tail {
            output.push_str(" | ");
            self.render_into(tail, PRECEDENCE_TOP, output);
        }
        if !row.fields.is_empty() || row.tail.is_some() {
            output.push(' ');
        }
        output.push(close);
    }
}

pub fn role(role: Role) -> &'static str {
    match role {
        Role::Phantom => "phantom",
        Role::Representational => "representational",
        Role::Nominal => "nominal",
    }
}
