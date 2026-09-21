//! Owned functional trees used for backend optimization and code generation.

pub mod convert;
pub mod error;
pub mod native;
pub mod optimize;
pub mod pretty;
pub mod stylex;
pub mod tree;

pub use convert::convert_module;
pub use error::{ModuleError, ModuleResult, UnsupportedState};

use std::sync::Arc;

use building_types::QueryResult;
use files::FileId;

pub trait ExternalQueries {
    fn functional(&self, file_id: FileId) -> QueryResult<ModuleResult<Arc<tree::Module>>>;
}
