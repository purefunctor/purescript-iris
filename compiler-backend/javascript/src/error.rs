use building_types::Severity;
use files::FileId;
use functional::tree::GlobalId;
use thiserror::Error;

pub type ModuleResult<T> = Result<T, ModuleError>;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ModuleDiagnostic {
    #[error("This declaration is cyclic with other declarations")]
    InitializerCycle { declarations: Vec<GlobalId> },
}

impl ModuleDiagnostic {
    pub fn severity(&self) -> Severity {
        match self {
            ModuleDiagnostic::InitializerCycle { .. } => Severity::Error,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ModuleError {
    #[error(transparent)]
    Functional(#[from] functional::ModuleError),
    #[error("cannot convert functional module {file_id:?} to JavaScript: {state}")]
    Unsupported { file_id: FileId, state: UnsupportedState },
}

impl ModuleError {
    pub fn severity(&self) -> Option<Severity> {
        match self {
            ModuleError::Functional(error) => error.severity(),
            ModuleError::Unsupported { .. } => Some(Severity::Error),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UnsupportedState {
    #[error("number literal {value:?} is not a finite JavaScript number")]
    InvalidNumber { value: String },
    #[error("integer literal {value} is outside the signed 32-bit range")]
    InvalidInteger { value: i64 },
    #[error("local global {name:?} has no JavaScript declaration")]
    MissingGlobal { name: String },
    #[error("local value {name:?} has no JavaScript binding")]
    MissingLocal { name: String },
}
