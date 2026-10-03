pub mod module_name_map;
pub use module_name_map::*;

use std::sync::Arc;

use files::{FileId, ForeignFileId};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QueryKey {
    Content(FileId),
    Dialect(FileId),
    Foreign(FileId),
    ForeignContent(ForeignFileId),
    ForeignModule(ForeignFileId),
    ForeignValidation(FileId),
    Module(ModuleNameId),
    Parsed(FileId),
    Stabilized(FileId),
    Indexed(FileId),
    Lowered(FileId),
    Grouped(FileId),
    Resolved(FileId),
    Exported(FileId),
    Bracketed(FileId),
    Sectioned(FileId),
    CheckedCore,
    Checked(FileId),
    Documented(FileId),
    Functional(FileId),
    JavaScript(FileId),
}

#[derive(Error, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QueryError {
    #[error("Cancelled")]
    Cancelled,
    #[error("Cycle detected")]
    Cycle { stack: Arc<[QueryKey]> },
    #[error("Missing content for {file_id:?}")]
    MissingContent { file_id: FileId },
}

pub type QueryResult<T> = Result<T, QueryError>;

pub trait QueryProxy {
    type Parsed;
    type Stabilized;
    type Indexed;
    type Lowered;
    type Grouped;
    type Resolved;
    type Exported;
    type Bracketed;
    type Sectioned;
    type Checked;
    type Documented;

    fn content(&self, id: FileId) -> QueryResult<Arc<str>>;

    fn parsed(&self, id: FileId) -> QueryResult<Self::Parsed>;

    fn stabilized(&self, id: FileId) -> QueryResult<Self::Stabilized>;

    fn indexed(&self, id: FileId) -> QueryResult<Self::Indexed>;

    fn lowered(&self, id: FileId) -> QueryResult<Self::Lowered>;

    fn grouped(&self, id: FileId) -> QueryResult<Self::Grouped>;

    fn resolved(&self, id: FileId) -> QueryResult<Self::Resolved>;

    fn exported(&self, id: FileId) -> QueryResult<Self::Exported>;

    fn bracketed(&self, id: FileId) -> QueryResult<Self::Bracketed>;

    fn sectioned(&self, id: FileId) -> QueryResult<Self::Sectioned>;

    fn checked(&self, id: FileId) -> QueryResult<Self::Checked>;

    fn documented(&self, id: FileId) -> QueryResult<Self::Documented>;

    fn prim_id(&self) -> FileId;

    fn module_file(&self, name: &str) -> Option<FileId>;
}
