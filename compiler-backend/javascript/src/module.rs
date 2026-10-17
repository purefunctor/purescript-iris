use std::sync::Arc;

use files::{FileId, ForeignSourceKind};

use crate::ModuleDiagnostic;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    file_id: FileId,
    name: Arc<str>,
    dependencies: Arc<[FileId]>,
    diagnostics: Arc<[ModuleDiagnostic]>,
    requires_runtime: bool,
    files: ModuleFiles,
}

impl Module {
    pub(crate) fn new(
        file_id: FileId,
        name: String,
        dependencies: Vec<FileId>,
        diagnostics: Vec<ModuleDiagnostic>,
        requires_runtime: bool,
        files: ModuleFiles,
    ) -> Module {
        Module {
            file_id,
            name: name.into(),
            dependencies: dependencies.into(),
            diagnostics: diagnostics.into(),
            requires_runtime,
            files,
        }
    }

    pub fn file_id(&self) -> FileId {
        self.file_id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn filename(&self) -> String {
        module_filename(&self.name)
    }

    pub fn dependencies(&self) -> &[FileId] {
        &self.dependencies
    }

    pub fn diagnostics(&self) -> &[ModuleDiagnostic] {
        &self.diagnostics
    }

    pub fn requires_runtime(&self) -> bool {
        self.requires_runtime
    }

    pub fn files(&self) -> &ModuleFiles {
        &self.files
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleFiles {
    index_source: Arc<str>,
    stylex_theme_source: Option<Arc<str>>,
    foreign_kind: Option<ForeignSourceKind>,
}

impl ModuleFiles {
    pub(crate) fn new(
        index_source: String,
        stylex_theme_source: Option<String>,
        foreign_kind: Option<ForeignSourceKind>,
    ) -> ModuleFiles {
        ModuleFiles {
            index_source: index_source.into(),
            stylex_theme_source: stylex_theme_source.map(Arc::from),
            foreign_kind,
        }
    }

    pub fn index_source(&self) -> &str {
        &self.index_source
    }

    /// The module's StyleX theme definitions, written to [`stylex_theme_module_filename`].
    pub fn stylex_theme_source(&self) -> Option<&str> {
        self.stylex_theme_source.as_deref()
    }

    pub fn foreign_kind(&self) -> Option<ForeignSourceKind> {
        self.foreign_kind
    }
}

pub fn runtime_filename() -> &'static str {
    "runtime.js"
}

pub fn runtime_source() -> &'static str {
    include_str!("runtime.js")
}

pub fn module_filename(module_name: &str) -> String {
    format!("{module_name}/index.js")
}

/// StyleX accepts `defineVars`, `defineConsts`, and `defineMarker` only in files named
/// `*.stylex.js`, which its default module resolution recognizes without configuration.
pub fn stylex_theme_module_filename(module_name: &str) -> String {
    format!("{module_name}/{STYLEX_THEME_FILENAME}")
}

pub(crate) const STYLEX_THEME_FILENAME: &str = "index.stylex.js";

pub fn foreign_filename(kind: ForeignSourceKind) -> String {
    format!("foreign.{}", kind.extension())
}

pub fn foreign_module_filename(module_name: &str, kind: ForeignSourceKind) -> String {
    format!("{module_name}/{}", foreign_filename(kind))
}
