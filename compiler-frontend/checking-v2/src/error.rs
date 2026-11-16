use smol_str::SmolStr;

use crate::types::TypeId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCrumb {
    TermDeclaration(indexing::TermItemId),
    TypeDeclaration(indexing::TypeItemId),
    InstanceDeclaration(indexing::InstanceItemId),
    DeriveDeclaration(indexing::DeriveItemId),
    Type(lowering::TypeId),
    Expression(lowering::ExpressionId),
    Binder(lowering::BinderId),
    LetBinding(lowering::LetBindingNameGroupId),
    DoStatement(lowering::DoStatementId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    TypesDoNotUnify { expected: TypeId, actual: TypeId },
    KindsDoNotUnify { expected: TypeId, actual: TypeId },
    InfiniteType { unification: TypeId, solution: TypeId },
    InfiniteKind { unification: TypeId, solution: TypeId },
    EscapedSkolem { rigid: TypeId, solution: TypeId },
    PropertyIsMissing { label: SmolStr },
    AdditionalProperty { label: SmolStr },
    DuplicateLabel { label: SmolStr },
    PartiallyAppliedSynonym,
    CannotApplyType { function_kind: TypeId },
    CannotApplyExpressionOfTypeOnType { function_type: TypeId },
    ExpectedFunction { function_type: TypeId },
    IncorrectConstructorArity { expected: usize, actual: usize },
    OverlappingArgNames { name: SmolStr },
    ArgListLengthsDiffer { expected: usize, actual: usize },
    NoInstanceFound { constraint: TypeId },
    OverlappingInstances { constraint: TypeId, instances: usize },
    AmbiguousTypeVariables { constraint: TypeId },
    PossiblyInfiniteInstance { constraint: TypeId },
    PossiblyInfiniteCoercibleInstance { constraint: TypeId },
    UserDefinedError { message: TypeId },
    UserDefinedWarning { message: TypeId },
    HoleInferredType { name: SmolStr, inferred: TypeId },
    WildcardInferredType { inferred: TypeId },
    CycleInDeclaration { name: SmolStr },
    RoleMismatch { declared: crate::types::Role, inferred: crate::types::Role },
    RoleDeclarationArityMismatch { expected: usize, actual: usize },
    OrphanInstance,
    InvalidInstanceHead { argument: TypeId },
    CannotDerive { class: SmolStr },
    MissingClassMember { name: SmolStr },
    ExtraneousClassMember { name: SmolStr },
    CannotGeneralizeRecursiveFunction { constraint: TypeId },
    QuantificationCheckFailureInType { elaborated: TypeId },
    UnsupportedFeature { feature: &'static str },
    InvalidSyntax,
}

impl ErrorKind {
    pub fn map_types(self, mut transform: impl FnMut(TypeId) -> TypeId) -> ErrorKind {
        match self {
            ErrorKind::TypesDoNotUnify { expected, actual } => ErrorKind::TypesDoNotUnify {
                expected: transform(expected),
                actual: transform(actual),
            },
            ErrorKind::KindsDoNotUnify { expected, actual } => ErrorKind::KindsDoNotUnify {
                expected: transform(expected),
                actual: transform(actual),
            },
            ErrorKind::InfiniteType { unification, solution } => ErrorKind::InfiniteType {
                unification: transform(unification),
                solution: transform(solution),
            },
            ErrorKind::InfiniteKind { unification, solution } => ErrorKind::InfiniteKind {
                unification: transform(unification),
                solution: transform(solution),
            },
            ErrorKind::EscapedSkolem { rigid, solution } => {
                ErrorKind::EscapedSkolem { rigid: transform(rigid), solution: transform(solution) }
            }
            ErrorKind::UserDefinedError { message } => {
                ErrorKind::UserDefinedError { message: transform(message) }
            }
            ErrorKind::UserDefinedWarning { message } => {
                ErrorKind::UserDefinedWarning { message: transform(message) }
            }
            ErrorKind::CannotApplyType { function_kind } => {
                ErrorKind::CannotApplyType { function_kind: transform(function_kind) }
            }
            ErrorKind::CannotApplyExpressionOfTypeOnType { function_type } => {
                ErrorKind::CannotApplyExpressionOfTypeOnType {
                    function_type: transform(function_type),
                }
            }
            ErrorKind::ExpectedFunction { function_type } => {
                ErrorKind::ExpectedFunction { function_type: transform(function_type) }
            }
            ErrorKind::NoInstanceFound { constraint } => {
                ErrorKind::NoInstanceFound { constraint: transform(constraint) }
            }
            ErrorKind::OverlappingInstances { constraint, instances } => {
                ErrorKind::OverlappingInstances { constraint: transform(constraint), instances }
            }
            ErrorKind::AmbiguousTypeVariables { constraint } => {
                ErrorKind::AmbiguousTypeVariables { constraint: transform(constraint) }
            }
            ErrorKind::PossiblyInfiniteInstance { constraint } => {
                ErrorKind::PossiblyInfiniteInstance { constraint: transform(constraint) }
            }
            ErrorKind::PossiblyInfiniteCoercibleInstance { constraint } => {
                ErrorKind::PossiblyInfiniteCoercibleInstance { constraint: transform(constraint) }
            }
            ErrorKind::HoleInferredType { name, inferred } => {
                ErrorKind::HoleInferredType { name, inferred: transform(inferred) }
            }
            ErrorKind::WildcardInferredType { inferred } => {
                ErrorKind::WildcardInferredType { inferred: transform(inferred) }
            }
            ErrorKind::InvalidInstanceHead { argument } => {
                ErrorKind::InvalidInstanceHead { argument: transform(argument) }
            }
            ErrorKind::CannotGeneralizeRecursiveFunction { constraint } => {
                ErrorKind::CannotGeneralizeRecursiveFunction { constraint: transform(constraint) }
            }
            ErrorKind::QuantificationCheckFailureInType { elaborated } => {
                ErrorKind::QuantificationCheckFailureInType { elaborated: transform(elaborated) }
            }
            other => other,
        }
    }

    pub fn is_warning(&self) -> bool {
        matches!(
            self,
            ErrorKind::UserDefinedWarning { .. }
                | ErrorKind::HoleInferredType { .. }
                | ErrorKind::WildcardInferredType { .. }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckingError {
    pub kind: ErrorKind,
    pub crumbs: Vec<ErrorCrumb>,
}
