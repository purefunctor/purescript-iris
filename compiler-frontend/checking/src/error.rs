//! Implements the errors emitted by the type checker.

use std::sync::Arc;

use smol_str::SmolStr;

use crate::core::{SmolStrId, TypeId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCrumb {
    TermDeclaration(indexing::TermItemId),
    TypeDeclaration(indexing::TypeItemId),
    InstanceDeclaration(indexing::InstanceItemId),
    DeriveDeclaration(indexing::DeriveItemId),
    ConstructorArgument(lowering::TypeId),

    InferringKind(lowering::TypeId),
    CheckingKind(lowering::TypeId),

    InferringBinder(lowering::BinderId),
    CheckingBinder(lowering::BinderId),

    InferringExpression(lowering::ExpressionId),
    CheckingExpression(lowering::ExpressionId),

    InferringDoBind(lowering::DoStatementId),
    InferringDoDiscard(lowering::DoStatementId),
    CheckingDoLet(lowering::DoStatementId),

    InferringAdoMap(lowering::DoStatementId),
    InferringAdoApply(lowering::DoStatementId),
    CheckingAdoLet(lowering::DoStatementId),

    CheckingLetName(lowering::LetBindingNameGroupId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectOrigin {
    pub effect: TypeId,
    pub crumbs: Arc<[ErrorCrumb]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    AmbiguousConstraint {
        constraint: TypeId,
    },
    CannotDeriveClass {
        class_file: files::FileId,
        class_id: indexing::TypeItemId,
    },
    CannotDeriveForType {
        type_id: TypeId,
    },
    CannotGeneraliseRecursiveFunction {
        type_id: TypeId,
    },
    ContravariantOccurrence {
        type_id: TypeId,
    },
    CovariantOccurrence {
        type_id: TypeId,
    },
    CannotUnify {
        t1: TypeId,
        t2: TypeId,
    },
    DeriveInvalidArity {
        class_file: files::FileId,
        class_id: indexing::TypeItemId,
        expected: usize,
        actual: usize,
    },
    DeriveNotSupportedYet {
        class_file: files::FileId,
        class_id: indexing::TypeItemId,
    },
    DeriveMissingFunctor,
    EmptyAdoBlock,
    EmptyDoBlock,
    EscapedSkolem {
        skolem: TypeId,
        type_id: TypeId,
    },
    TermHole {
        source_term: lowering::ExpressionId,
    },
    TypeHole {
        source_type: lowering::TypeId,
    },
    InvalidFinalBind,
    InvalidFinalLet,
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
    InvalidTypeApplication {
        function_type: TypeId,
        function_kind: TypeId,
        argument_type: TypeId,
    },
    ExpectedNewtype {
        type_id: TypeId,
    },
    InvalidNewtypeDeriveSkolemArguments,
    NonLocalNewtype {
        type_id: TypeId,
    },
    NoInstanceFound {
        given: Arc<[TypeId]>,
        constraint: TypeId,
    },
    OverlappingInstances {
        constraint: TypeId,
        instances: Arc<[TypeId]>,
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
    TooManyBinders {
        signature: Option<lowering::TypeId>,
        expected: u32,
        actual: u32,
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
    CoercibleConstructorNotInScope {
        file_id: files::FileId,
        item_id: indexing::TypeItemId,
    },
    CustomWarning {
        message_id: SmolStrId,
    },
    RedundantPatterns {
        patterns: Arc<[SmolStr]>,
    },
    MissingPatterns {
        patterns: Arc<[SmolStr]>,
    },
    CustomFailure {
        message_id: SmolStrId,
    },
    PropertyIsMissing {
        labels: Arc<[SmolStr]>,
    },
    AdditionalProperty {
        labels: Arc<[SmolStr]>,
    },
    MissingEffects {
        missing: Arc<[TypeId]>,
        allowed: TypeId,
        origins: Arc<[EffectOrigin]>,
        declaration: Arc<[ErrorCrumb]>,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub struct CheckingError {
    pub kind: ErrorKind,
    pub crumbs: Arc<[ErrorCrumb]>,
}
