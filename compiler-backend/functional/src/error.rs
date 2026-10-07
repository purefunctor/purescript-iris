use checking::evidence::EvidenceVarId;
use checking::tree as checked_tree;
use files::FileId;
use indexing::TermItemId;
use thiserror::Error;

use crate::tree::GlobalId;

pub type ModuleResult<T> = Result<T, ModuleError>;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ModuleError {
    #[error("cannot convert checked module {file_id:?}: {state}")]
    Unsupported { file_id: FileId, state: UnsupportedState },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UnsupportedState {
    #[error("source module has no module name")]
    MissingModuleName,
    #[error("checked binder {0:?} contains an error")]
    BinderError(checked_tree::BinderId),
    #[error("record update contains an error")]
    RecordUpdateError,
    #[error("pattern let binding {0:?} contains an error")]
    PatternBindingError(lowering::LetBindingId),
    #[error("evidence variable {0:?} is unsolved")]
    UnsolvedEvidence(EvidenceVarId),
    #[error("evidence variable {0:?} is cyclic")]
    CyclicEvidence(EvidenceVarId),
    #[error("checked term declaration {0:?} is missing")]
    MissingTermDeclaration(TermItemId),
    #[error("checked instance declaration is missing")]
    MissingInstanceDeclaration,
    #[error("checked local declaration {0:?} is missing")]
    MissingLocalDeclaration(lowering::LetBindingNameGroupId),
    #[error("checked value declaration has no equations")]
    MissingEquation,
    #[error("runtime export name {name:?} refers to conflicting declarations")]
    ConflictingRuntimeExport { name: String, existing: GlobalId, duplicate: GlobalId },
    #[error("exported operator {term_id:?} has no runtime resolution")]
    MissingRuntimeExportOperatorResolution { term_id: TermItemId },
    #[error("instance prerequisite is not represented by given evidence")]
    InvalidInstancePrerequisite,
    #[error("local identity space is exhausted")]
    LocalIdentityOverflow,
    #[error("generated global identity space is exhausted")]
    GeneratedGlobalIdentityOverflow,
    #[error("{function} must be used as a direct, saturated intrinsic call")]
    InvalidStyleXUse { function: String, declaration: GlobalId },
    #[error("{function} {requirement}")]
    InvalidStyleXContext { function: String, requirement: String, declaration: GlobalId },
    #[error("virtual module declaration {module_name}.{item_name} cannot be used at runtime")]
    VirtualModuleRuntimeReference { module_name: String, item_name: String },
}
