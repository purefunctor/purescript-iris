use std::sync::Arc;

use files::{FileId, ForeignSourceKind};

use crate::ModuleDiagnostic;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    file_id: FileId,
    name: Arc<str>,
    source: Arc<str>,
    dependencies: Arc<[FileId]>,
    diagnostics: Arc<[ModuleDiagnostic]>,
    foreign_kind: Option<ForeignSourceKind>,
    stylex_theme_source: Option<Arc<str>>,
    requires_runtime: bool,
}

impl Module {
    pub(crate) fn new(
        file_id: FileId,
        name: String,
        source: String,
        dependencies: Vec<FileId>,
        diagnostics: Vec<ModuleDiagnostic>,
        foreign_kind: Option<ForeignSourceKind>,
        stylex_theme_source: Option<String>,
        requires_runtime: bool,
    ) -> Module {
        Module {
            file_id,
            name: name.into(),
            source: source.into(),
            dependencies: dependencies.into(),
            diagnostics: diagnostics.into(),
            foreign_kind,
            stylex_theme_source: stylex_theme_source.map(Arc::from),
            requires_runtime,
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

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn dependencies(&self) -> &[FileId] {
        &self.dependencies
    }

    pub fn diagnostics(&self) -> &[ModuleDiagnostic] {
        &self.diagnostics
    }

    pub fn foreign_kind(&self) -> Option<ForeignSourceKind> {
        self.foreign_kind
    }

    /// The module's StyleX theme definitions, written to [`stylex_theme_module_filename`].
    pub fn stylex_theme_source(&self) -> Option<&str> {
        self.stylex_theme_source.as_deref()
    }

    pub fn requires_runtime(&self) -> bool {
        self.requires_runtime
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

pub fn foreign_module_filename(module_name: &str, kind: ForeignSourceKind) -> String {
    format!("{module_name}/foreign.{}", kind.extension())
}
