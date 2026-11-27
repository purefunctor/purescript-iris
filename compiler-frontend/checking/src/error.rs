//! Implements the errors emitted by the type checker.

use std::sync::Arc;

use building_types::Severity;
use smol_str::SmolStr;

use crate::core::{IntegerId, SmolStrId, TypeId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCrumb {
    TermDeclaration(indexing::TermItemId),
    TypeDeclaration(indexing::TypeItemId),
    InstanceDeclaration(indexing::InstanceItemId),
    DeriveDeclaration(indexing::DeriveItemId),
    ConstructorArgument(lowering::TypeId),
    CapabilityBundle(TypeId),

    InferringKind(lowering::TypeId),
    CheckingKind(lowering::TypeId),

    InferringBinder(lowering::BinderId),
    CheckingBinder(lowering::BinderId),

    InferringExpression(lowering::ExpressionId),
    CheckingExpression(lowering::ExpressionId),
    RecordPun(lowering::RecordPunId),

    InferringDoBind(lowering::DoStatementId),
    InferringDoDiscard(lowering::DoStatementId),
    CheckingDoLet(lowering::DoStatementId),

    InferringAdoMap(lowering::DoStatementId),
    InferringAdoApply(lowering::DoStatementId),
    CheckingAdoLet(lowering::DoStatementId),

    CheckingLetName(lowering::LetBindingNameGroupId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    Constraint(ConstraintErrorKind),
    Capability(CapabilityErrorKind),
    CapabilityBundle(CapabilityBundleErrorKind),
    Instance(InstanceErrorKind),
    Derive(DeriveErrorKind),
    Unification(UnificationErrorKind),
    Type(TypeErrorKind),
    Term(TermErrorKind),
    Hole(HoleErrorKind),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstraintErrorKind {
    AmbiguousConstraint { constraint: TypeId },
    NoInstanceFound { given: Arc<[TypeId]>, constraint: TypeId },
    OverlappingInstances { constraint: TypeId, instances: Arc<[TypeId]> },
    CoercibleConstructorNotInScope { file_id: files::FileId, item_id: indexing::TypeItemId },
    CustomWarning { message_id: SmolStrId },
    CustomFailure { message_id: SmolStrId },
    ReflectedIntegerOutOfRange { integer: IntegerId },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityErrorKind {
    AmbiguousCapability { service: TypeId, services: TypeId, roles: Arc<[SmolStr]> },
    CapabilityNotDeclared { service: TypeId, services: TypeId },
    UnresolvedCapability { service: TypeId, services: TypeId },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityBundleErrorKind {
    InvalidCapabilityBundleMember { member: TypeId },
    InvalidCapabilityBundlePosition { bundle: TypeId },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstanceErrorKind {
    InstanceHeadMismatch {
        class_file: files::FileId,
        class_item: indexing::TypeItemId,
        expected: usize,
        actual: usize,
    },
    InstanceHeadLabeledRow {
        class_file: files::FileId,
        class_item: indexing::TypeItemId,
        position: usize,
        type_id: TypeId,
    },
    InstanceMemberTypeMismatch {
        expected: TypeId,
        actual: TypeId,
    },
    MissingInstanceMembers {
        members: Arc<[SmolStr]>,
    },
    OrphanInstance {
        constraint: TypeId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeriveErrorKind {
    CannotDeriveClass {
        class_file: files::FileId,
        class_id: indexing::TypeItemId,
    },
    CannotDeriveForType {
        type_id: TypeId,
    },
    ContravariantOccurrence {
        type_id: TypeId,
    },
    CovariantOccurrence {
        type_id: TypeId,
    },
    DeriveInvalidArity {
        class_file: files::FileId,
        class_id: indexing::TypeItemId,
        expected: usize,
        actual: usize,
    },
    DeriveMissingClass {
        module_name: &'static str,
        class_name: &'static str,
    },
    ExpectedNewtype {
        type_id: TypeId,
    },
    InvalidNewtypeDeriveSkolemArguments,
    NonLocalNewtype {
        type_id: TypeId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnificationErrorKind {
    CannotUnify {
        t1: TypeId,
        t2: TypeId,
        /// The types of the failed check, which contain `t1` and `t2`.
        outer: (TypeId, TypeId),
    },
    EscapedSkolem {
        skolem: TypeId,
        type_id: TypeId,
    },
    PropertyIsMissing {
        labels: Arc<[SmolStr]>,
    },
    AdditionalProperty {
        labels: Arc<[SmolStr]>,
    },
    CannotGeneraliseRecursiveFunction {
        type_id: TypeId,
    },
    UnreportedUnknownType {
        type_id: TypeId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeErrorKind {
    InvalidTypeApplication {
        function_type: TypeId,
        function_kind: TypeId,
        argument_type: TypeId,
    },
    NoVisibleTypeVariable {
        function_type: TypeId,
    },
    PartialSynonymApplication {
        id: lowering::TypeId,
    },
    RecursiveSynonymExpansion {
        file_id: files::FileId,
        type_id: indexing::TypeItemId,
    },
    TypeSignatureVariableMismatch {
        id: lowering::TypeId,
        expected: u32,
        actual: u32,
    },
    InvalidRoleDeclaration {
        index: usize,
        declared: crate::core::Role,
        inferred: crate::core::Role,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TermErrorKind {
    EmptyAdoBlock,
    EmptyDoBlock,
    InvalidFinalBind,
    InvalidFinalLet,
    TooManyBinders { signature: Option<lowering::TypeId>, expected: u32, actual: u32 },
    RedundantPatterns { patterns: Arc<[SmolStr]> },
    MissingPatterns { patterns: Arc<[SmolStr]> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HoleErrorKind {
    TermHole { source_term: lowering::ExpressionId },
    TypeHole { source_type: lowering::TypeId },
}

impl From<ConstraintErrorKind> for ErrorKind {
    fn from(kind: ConstraintErrorKind) -> ErrorKind {
        ErrorKind::Constraint(kind)
    }
}

impl From<CapabilityErrorKind> for ErrorKind {
    fn from(kind: CapabilityErrorKind) -> ErrorKind {
        ErrorKind::Capability(kind)
    }
}

impl From<CapabilityBundleErrorKind> for ErrorKind {
    fn from(kind: CapabilityBundleErrorKind) -> ErrorKind {
        ErrorKind::CapabilityBundle(kind)
    }
}

impl From<InstanceErrorKind> for ErrorKind {
    fn from(kind: InstanceErrorKind) -> ErrorKind {
        ErrorKind::Instance(kind)
    }
}

impl From<DeriveErrorKind> for ErrorKind {
    fn from(kind: DeriveErrorKind) -> ErrorKind {
        ErrorKind::Derive(kind)
    }
}

impl From<UnificationErrorKind> for ErrorKind {
    fn from(kind: UnificationErrorKind) -> ErrorKind {
        ErrorKind::Unification(kind)
    }
}

impl From<TypeErrorKind> for ErrorKind {
    fn from(kind: TypeErrorKind) -> ErrorKind {
        ErrorKind::Type(kind)
    }
}

impl From<TermErrorKind> for ErrorKind {
    fn from(kind: TermErrorKind) -> ErrorKind {
        ErrorKind::Term(kind)
    }
}

impl From<HoleErrorKind> for ErrorKind {
    fn from(kind: HoleErrorKind) -> ErrorKind {
        ErrorKind::Hole(kind)
    }
}

impl ErrorKind {
    pub fn severity(&self) -> Severity {
        match self {
            ErrorKind::Constraint(ConstraintErrorKind::CustomWarning { .. })
            | ErrorKind::Instance(InstanceErrorKind::MissingInstanceMembers { .. })
            | ErrorKind::Term(
                TermErrorKind::InvalidFinalBind
                | TermErrorKind::RedundantPatterns { .. }
                | TermErrorKind::MissingPatterns { .. },
            ) => Severity::Warning,
            _ => Severity::Error,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct CheckingError {
    pub kind: ErrorKind,
    pub crumbs: Arc<[ErrorCrumb]>,
}

impl CheckingError {
    pub fn severity(&self) -> Severity {
        self.kind.severity()
    }
}
