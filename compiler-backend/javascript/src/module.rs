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
    requires_runtime: bool,
    requires_effect_module: bool,
}

impl Module {
    pub(crate) fn new(
        file_id: FileId,
        name: String,
        source: String,
        dependencies: Vec<FileId>,
        diagnostics: Vec<ModuleDiagnostic>,
        foreign_kind: Option<ForeignSourceKind>,
        requires_runtime: bool,
        requires_effect_module: bool,
    ) -> Module {
        Module {
            file_id,
            name: name.into(),
            source: source.into(),
            dependencies: dependencies.into(),
            diagnostics: diagnostics.into(),
            foreign_kind,
            requires_runtime,
            requires_effect_module,
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

    pub fn requires_runtime(&self) -> bool {
        self.requires_runtime
    }

    pub fn requires_effect_module(&self) -> bool {
        self.requires_effect_module
    }
}

pub fn runtime_filename() -> &'static str {
    "runtime.js"
}

pub fn runtime_source() -> &'static str {
    include_str!("runtime.js")
}

pub fn effect_filename() -> &'static str {
    "effect.js"
}

pub fn effect_source() -> &'static str {
    include_str!("effect.js")
}

pub fn module_filename(module_name: &str) -> String {
    format!("{module_name}/index.js")
}

pub fn foreign_module_filename(module_name: &str, kind: ForeignSourceKind) -> String {
    format!("{module_name}/foreign.{}", kind.extension())
}
