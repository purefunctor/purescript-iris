//! Direct JavaScript code generation from functional trees.
//!
//! Generated modules target ES2022 and require Node.js 22 or newer. In particular, source names that
//! are not JavaScript identifiers use string-literal module export names.

mod convert;
mod error;
mod module;
mod program_text;
mod source_map;
mod tree;
mod writer;

use std::sync::Arc;

pub use convert::convert_module;
pub use error::{ModuleDiagnostic, ModuleError, ModuleResult, UnsupportedState};
pub use module::{
    Module, ModuleFiles, foreign_filename, foreign_module_filename, module_filename,
    runtime_filename, runtime_source, source_map_module_filename, source_mapping_url_comment,
    stylex_theme_module_filename,
};
pub use source_map::{Position, SourceMap, SourceMapping, source_map};

use building_types::QueryResult;
use files::{FileId, ForeignFileId};

pub trait ExternalQueries: functional::ExternalQueries {
    fn foreign_file(&self, file_id: FileId) -> QueryResult<Option<ForeignFileId>>;
}

pub trait ModuleQueries: ExternalQueries {
    fn javascript(&self, file_id: FileId) -> QueryResult<ModuleResult<Arc<Module>>>;
}
