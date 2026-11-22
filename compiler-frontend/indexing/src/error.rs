use building_types::Severity;

use crate::source::*;
use crate::{TermItemId, TypeItemId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    DataSignature(DataSignatureId),
    DataEquation(DataEquationId),
    NewtypeSignature(NewtypeSignatureId),
    NewtypeEquation(NewtypeEquationId),
    SynonymSignature(TypeSignatureId),
    SynonymEquation(TypeEquationId),
    ClassSignature(ClassSignatureId),
    ClassDeclaration(ClassDeclarationId),
    ValueSignature(ValueSignatureId),
    ValueEquation(ValueEquationId),
    Term(TermItemId),
    Type(TypeItemId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExistingKind {
    Term(TermItemId),
    Type(TypeItemId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidExportHint {
    AliasedModule { import: ImportId },
    ImportedItem { import: ImportId, item: ImportItemId },
}

#[derive(Debug, PartialEq, Eq)]
pub enum IndexingError {
    DuplicateItem { kind: ItemKind, existing: ExistingKind },
    MismatchedItem { kind: ItemKind, existing: ExistingKind },
    OrphanRole { id: TypeRoleId },
    DuplicateRole { id: TypeRoleId, existing: TypeRoleId },
    UnsupportedRole { id: TypeRoleId },
    InvalidExport { id: ExportItemId, hint: Option<InvalidExportHint> },
    DuplicateExport { id: ExportItemId, existing: ExportItemId },
    DuplicateImport { duplicate: ImportItemId, existing: ImportItemId },
}

impl IndexingError {
    pub fn severity(&self) -> Severity {
        match self {
            IndexingError::DuplicateImport { .. } | IndexingError::DuplicateExport { .. } => {
                Severity::Warning
            }
            IndexingError::DuplicateItem { .. }
            | IndexingError::MismatchedItem { .. }
            | IndexingError::OrphanRole { .. }
            | IndexingError::DuplicateRole { .. }
            | IndexingError::UnsupportedRole { .. }
            | IndexingError::InvalidExport { .. } => Severity::Error,
        }
    }
}
