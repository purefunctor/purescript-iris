//! Compiler query state used by build execution and long-lived consumers.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::{fs, io};

use building::{
    DiskObservation, FileLifecycle, ForeignEvent, LifecycleChange, LifecycleEvent, QueryEngine,
    QueryError, SourceEvent, SourceUnitKey,
};
use files::{FileId, ForeignSourceKind};
use prim_constants::MODULE_MAP;
use tempfile::TempDir;
use url::Url;

pub struct MaterializedPrim {
    directory: TempDir,
}

impl MaterializedPrim {
    pub fn new() -> io::Result<MaterializedPrim> {
        let directory = TempDir::new()?;
        for (name, content) in MODULE_MAP {
            fs::write(directory.path().join(format!("{name}.purs")), content)?;
        }
        Ok(MaterializedPrim { directory })
    }
}

pub struct CompilationState<Version = (), Metadata = ()> {
    engine: QueryEngine,
    files: FileLifecycle<Version, Metadata>,
    sources: BTreeSet<FileId>,
    prim: MaterializedPrim,
}

pub struct CompilationParts<Version, Metadata> {
    pub engine: QueryEngine,
    pub files: FileLifecycle<Version, Metadata>,
    pub prim: MaterializedPrim,
}

impl<Version: Clone + Ord, Metadata: Clone> CompilationState<Version, Metadata> {
    pub fn new(
        prim: MaterializedPrim,
        prim_metadata: Metadata,
    ) -> CompilationState<Version, Metadata> {
        let engine = QueryEngine::default();
        let mut files = FileLifecycle::default();

        for (name, content) in MODULE_MAP {
            let path = prim.directory.path().join(format!("{name}.purs"));
            let source = Url::from_file_path(&path)
                .expect("invariant violated: failed to create Prim module file URL");
            let foreign_path = path.with_extension("js");
            let foreign = Url::from_file_path(&foreign_path)
                .expect("invariant violated: failed to create Prim foreign file URL");
            let event = LifecycleEvent::Source {
                unit: SourceUnitKey::new(source.as_str(), foreign.as_str()),
                event: SourceEvent::DiskObserved {
                    disk: DiskObservation::Found(Arc::from(*content)),
                    metadata: Metadata::clone(&prim_metadata),
                },
            };

            let change = files.apply(&engine, event);
            let id = change
                .changed_sources()
                .next()
                .expect("invariant violated: Prim source lifecycle did not insert a source");
            engine.set_module_file(name, id);
        }

        CompilationState { engine, files, sources: BTreeSet::new(), prim }
    }

    pub fn observe_source(
        &mut self,
        unit: SourceUnitKey,
        disk: DiskObservation,
        metadata: Metadata,
    ) -> LifecycleChange {
        let event =
            LifecycleEvent::Source { unit, event: SourceEvent::DiskObserved { disk, metadata } };
        let change = self.files.apply(&self.engine, event);
        self.sources.extend(change.changed_sources());
        for removed in change.removed_sources() {
            self.sources.remove(&removed.file_id);
        }
        change
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

    pub(crate) fn query_engine(&self) -> &QueryEngine {
        &self.engine
    }

    pub fn source_ids(&self) -> impl Iterator<Item = FileId> + '_ {
        self.sources.iter().copied()
    }

    pub fn into_parts(self) -> CompilationParts<Version, Metadata> {
        CompilationParts { engine: self.engine, files: self.files, prim: self.prim }
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
    fn prim_modules_are_materialized() {
        let prim = MaterializedPrim::new().unwrap();
        let compilation = CompilationState::<(), ()>::new(prim, ());
        let prim = compilation.snapshot().module_file("Prim").unwrap();
        let source = compilation.source_path(prim).unwrap();
        let path = Url::parse(&source).unwrap().to_file_path().unwrap();

        assert!(path.is_file());
        assert_eq!(fs::read_to_string(path).unwrap(), MODULE_MAP[0].1);
    }
}
