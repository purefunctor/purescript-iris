//! Direct JavaScript code generation from functional trees.
//!
//! Generated modules target ES2022 and require Node.js 16 or newer. In particular, source names that
//! are not JavaScript identifiers use string-literal module export names.

mod convert;
mod error;
mod module;
mod tree;
mod writer;

use std::sync::Arc;

pub use convert::convert_module;
pub use error::{ModuleDiagnostic, ModuleError, ModuleResult, UnsupportedState};
pub use module::{
    Module, effect_filename, effect_source, foreign_module_filename, module_filename,
    runtime_filename, runtime_source,
};

use building_types::QueryResult;
use files::{FileId, ForeignFileId};

pub trait ExternalQueries: functional::ExternalQueries {
    fn foreign_file(&self, file_id: FileId) -> QueryResult<Option<ForeignFileId>>;
}

pub trait ModuleQueries: ExternalQueries {
    fn javascript(&self, file_id: FileId) -> QueryResult<ModuleResult<Arc<Module>>>;
}
