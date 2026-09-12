//! Compiler query state used by build execution.

use std::sync::Arc;

use building::{
    DiskObservation, FileLifecycle, ForeignEvent, LifecycleChange, LifecycleEvent, QueryEngine,
    QueryError, SourceEvent, SourceUnitKey,
};
use files::{FileId, ForeignSourceKind};
use itertools::Itertools;
use prim_constants::MODULE_MAP;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceRole {
    Prim,
    Input,
}

pub struct CompilationState {
    engine: QueryEngine,
    files: FileLifecycle<(), SourceRole>,
}

impl CompilationState {
    pub fn new() -> CompilationState {
        let engine = QueryEngine::default();
        let mut files = FileLifecycle::default();

        for (name, content) in MODULE_MAP {
            let source = format!("prim://localhost/{name}.purs");
            let foreign = format!("prim://localhost/{name}.js");
            let event = LifecycleEvent::Source {
                unit: SourceUnitKey::new(source, foreign),
                event: SourceEvent::DiskObserved {
                    disk: DiskObservation::Found(Arc::from(*content)),
                    metadata: SourceRole::Prim,
                },
            };

            let change = files.apply(&engine, event);
            let id = change
                .changed_sources()
                .next()
                .expect("invariant violated: Prim source lifecycle did not insert a source");
            engine.set_module_file(name, id);
        }

        CompilationState { engine, files }
    }

    pub fn observe_source(
        &mut self,
        unit: SourceUnitKey,
        disk: DiskObservation,
    ) -> LifecycleChange {
        let event = LifecycleEvent::Source {
            unit,
            event: SourceEvent::DiskObserved { disk, metadata: SourceRole::Input },
        };
        self.files.apply(&self.engine, event)
    }

    pub fn observe_foreign(
        &mut self,
        unit: SourceUnitKey,
        kind: ForeignSourceKind,
        disk: DiskObservation,
    ) -> LifecycleChange {
        let event =
            LifecycleEvent::Foreign { unit, kind, event: ForeignEvent::DiskObserved { disk } };
        self.files.apply(&self.engine, event)
    }

    pub fn input_sources(&self) -> Vec<FileId> {
        let sources = self
            .files
            .source_ids()
            .filter(|&file_id| self.files.source_metadata(file_id) == Some(&SourceRole::Input));
        sources.collect_vec()
    }

    pub fn source_content(&self, locator: &str) -> Result<Option<Arc<str>>, QueryError> {
        let Some(file_id) = self.files.source_id(locator) else {
            return Ok(None);
        };
        self.engine.content(file_id).map(Some)
    }

    pub fn foreign_content(&self, locator: &str) -> Option<Arc<str>> {
        let file_id = self.files.foreign_id(locator)?;
        self.engine.foreign_content(file_id)
    }

    pub fn snapshot(&self) -> QueryEngine {
        self.engine.snapshot()
    }

    pub fn source_path(&self, file_id: FileId) -> Option<Arc<str>> {
        self.files.source_path(file_id)
    }

    pub fn source_foreign_content(&self, source_id: FileId) -> Option<Arc<str>> {
        let foreign_id = self.engine.foreign_file(source_id)?;
        let content = self
            .engine
            .foreign_content(foreign_id)
            .expect("invariant violated: associated foreign file has no engine content");
        Some(content)
    }

    pub fn module_name(&self, locator: &str) -> Result<Option<String>, QueryError> {
        let Some(file_id) = self.files.source_id(locator) else {
            return Ok(None);
        };
        let engine = self.engine.snapshot();
        let content = engine.content(file_id)?;
        let (parsed, _) = engine.parsed(file_id)?;
        Ok(parsed.module_name(&content).map(|name| name.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prim_modules_are_not_input_sources() {
        let compilation = CompilationState::new();

        assert!(compilation.input_sources().is_empty());
        assert!(compilation.snapshot().module_file("Prim").is_some());
    }
}
