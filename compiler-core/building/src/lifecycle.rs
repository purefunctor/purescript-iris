use std::sync::Arc;

use files::{FileId, Files, ForeignFileCandidates, ForeignFileId, ForeignFiles, ForeignSourceKind};
use rayon::prelude::*;
use rustc_hash::FxHashMap;
use smol_str::SmolStr;

use crate::QueryEngine;

mod change;
mod event;
mod foreign;
mod source;

#[cfg(test)]
mod tests;

pub use change::*;
pub use event::*;

#[derive(Debug)]
pub struct FileLifecycle<Version, Metadata> {
    units: FxHashMap<SourceUnitKey, SourceUnit<Version, Metadata>>,
    source_owners: FxHashMap<Arc<str>, SourceUnitKey>,
    foreign_owners: FxHashMap<Arc<str>, SourceUnitKey>,
    source_units: FxHashMap<FileId, SourceUnitKey>,
    source_files: Files,
    foreign_files: ForeignFiles,
}

impl<Version, Metadata> Default for FileLifecycle<Version, Metadata> {
    fn default() -> FileLifecycle<Version, Metadata> {
        FileLifecycle {
            units: FxHashMap::default(),
            source_owners: FxHashMap::default(),
            foreign_owners: FxHashMap::default(),
            source_units: FxHashMap::default(),
            source_files: Files::default(),
            foreign_files: ForeignFiles::default(),
        }
    }
}

#[derive(Debug)]
struct SourceUnit<Version, Metadata> {
    source: Member<SourceDocument<Version, Metadata>>,
    foreign: ForeignMembers<Version>,
}

impl<Version, Metadata> Default for SourceUnit<Version, Metadata> {
    fn default() -> SourceUnit<Version, Metadata> {
        SourceUnit { source: Member::Missing, foreign: ForeignMembers::default() }
    }
}

#[derive(Debug)]
struct ForeignMembers<Version> {
    javascript: Member<ForeignDocument<Version>>,
    jsx: Member<ForeignDocument<Version>>,
}

impl<Version> Default for ForeignMembers<Version> {
    fn default() -> ForeignMembers<Version> {
        ForeignMembers { javascript: Member::Missing, jsx: Member::Missing }
    }
}

impl<Version> ForeignMembers<Version> {
    fn get(&self, kind: ForeignSourceKind) -> &Member<ForeignDocument<Version>> {
        match kind {
            ForeignSourceKind::JavaScript => &self.javascript,
            ForeignSourceKind::Jsx => &self.jsx,
        }
    }

    fn get_mut(&mut self, kind: ForeignSourceKind) -> &mut Member<ForeignDocument<Version>> {
        match kind {
            ForeignSourceKind::JavaScript => &mut self.javascript,
            ForeignSourceKind::Jsx => &mut self.jsx,
        }
    }
}

#[derive(Debug, Default)]
enum Member<Document> {
    #[default]
    Missing,
    Present(Document),
}

#[derive(Debug)]
struct SourceDocument<Version, Metadata> {
    id: FileId,
    metadata: Metadata,
    content: EffectiveContent<Version>,
}

#[derive(Debug)]
struct ForeignDocument<Version> {
    id: ForeignFileId,
    content: EffectiveContent<Version>,
}

#[derive(Debug)]
enum EffectiveContent<Version> {
    Open { text: Arc<str>, version: Version },
    Disk { text: Arc<str> },
    Retained { text: Arc<str>, failure: ReloadFailure },
}

impl<Version> EffectiveContent<Version> {
    fn authority(&self) -> ContentAuthority {
        match self {
            EffectiveContent::Open { .. } => ContentAuthority::Open,
            EffectiveContent::Disk { .. } => ContentAuthority::Disk,
            EffectiveContent::Retained { .. } => ContentAuthority::Retained,
        }
    }

    fn text(&self) -> &Arc<str> {
        match self {
            EffectiveContent::Open { text, .. }
            | EffectiveContent::Disk { text }
            | EffectiveContent::Retained { text, .. } => text,
        }
    }

    fn reload_failure(&self) -> Option<&ReloadFailure> {
        let EffectiveContent::Retained { failure, .. } = self else {
            return None;
        };
        Some(failure)
    }
}

impl<Version, Metadata> FileLifecycle<Version, Metadata>
where
    Version: Clone + Ord,
{
    pub fn apply(
        &mut self,
        engine: &QueryEngine,
        event: LifecycleEvent<Version, Metadata>,
    ) -> LifecycleChange {
        self.apply_event(engine, event, &mut ModuleRegistration::Immediate)
    }

    /// Applies events in order, parsing the sources they introduce in parallel.
    ///
    /// The resulting state is the same as applying each event individually.
    pub fn apply_all(
        &mut self,
        engine: &QueryEngine,
        events: impl IntoIterator<Item = LifecycleEvent<Version, Metadata>>,
    ) -> LifecycleChange {
        let mut pending = vec![];
        let mut change = LifecycleChange::default();
        for event in events {
            // Updating or removing a source must observe earlier registrations,
            // including ownership displaced by a duplicate module name.
            if let LifecycleEvent::Source { unit, .. } = &event
                && self.source_id(unit.source()).is_some()
            {
                self.register_pending_modules(engine, std::mem::take(&mut pending));
            }
            let registration = &mut ModuleRegistration::Deferred(&mut pending);
            change.combine(self.apply_event(engine, event, registration));
        }
        self.register_pending_modules(engine, pending);
        change
    }

    fn apply_event(
        &mut self,
        engine: &QueryEngine,
        event: LifecycleEvent<Version, Metadata>,
        registration: &mut ModuleRegistration<'_>,
    ) -> LifecycleChange {
        let unit = match &event {
            LifecycleEvent::Source { unit, .. } | LifecycleEvent::Foreign { unit, .. } => unit,
        };
        if let Some(warning) = self.locator_conflict(unit) {
            let mut change = LifecycleChange::default();
            change.warnings.push(warning);
            return change;
        }
        match event {
            LifecycleEvent::Source { unit, event } => {
                self.apply_source(engine, unit, event, registration)
            }
            LifecycleEvent::Foreign { unit, kind, event } => {
                self.apply_foreign(engine, unit, kind, event)
            }
        }
    }

    pub fn is_open(&self, document: &DocumentKey) -> bool {
        let (unit, kind) = match document {
            DocumentKey::Source(unit) => (unit, DocumentKind::Source),
            DocumentKey::Foreign(unit, kind) => (unit, DocumentKind::Foreign(*kind)),
        };
        let Some(source_unit) = self.units.get(unit) else {
            return false;
        };
        match kind {
            DocumentKind::Source => match &source_unit.source {
                Member::Missing => false,
                Member::Present(document) => document.content.authority() == ContentAuthority::Open,
            },
            DocumentKind::Foreign(_) => match document {
                DocumentKey::Foreign(_, kind) => match source_unit.foreign.get(*kind) {
                    Member::Missing => false,
                    Member::Present(document) => {
                        document.content.authority() == ContentAuthority::Open
                    }
                },
                DocumentKey::Source(_) => unreachable!(),
            },
        }
    }

    pub fn source_id(&self, locator: &str) -> Option<FileId> {
        self.source_files.id(locator)
    }

    pub fn contains_source(&self, file_id: FileId) -> bool {
        self.source_files.contains(file_id)
    }

    pub fn source_path(&self, file_id: FileId) -> Option<Arc<str>> {
        self.source_files.contains(file_id).then(|| self.source_files.path(file_id))
    }

    pub fn source_ids(&self) -> impl Iterator<Item = FileId> + '_ {
        self.source_files.iter_id()
    }

    pub fn source_metadata(&self, file_id: FileId) -> Option<&Metadata> {
        let unit = self.source_units.get(&file_id)?;
        let source_unit = self.units.get(unit)?;
        let Member::Present(source) = &source_unit.source else {
            return None;
        };
        Some(&source.metadata)
    }

    pub fn source_version(&self, file_id: FileId) -> Option<Version> {
        let unit = self.source_units.get(&file_id)?;
        let source_unit = self.units.get(unit)?;
        let Member::Present(source) = &source_unit.source else {
            return None;
        };
        let EffectiveContent::Open { version, .. } = &source.content else {
            return None;
        };
        Some(Version::clone(version))
    }

    pub fn source_authority(&self, unit: &SourceUnitKey) -> Option<ContentAuthority> {
        let source_unit = self.units.get(unit)?;
        let Member::Present(source) = &source_unit.source else {
            return None;
        };
        Some(source.content.authority())
    }

    pub fn source_reload_failure(&self, unit: &SourceUnitKey) -> Option<&ReloadFailure> {
        let source_unit = self.units.get(unit)?;
        let Member::Present(source) = &source_unit.source else {
            return None;
        };
        source.content.reload_failure()
    }

    pub fn foreign_authority(&self, unit: &SourceUnitKey) -> Option<ContentAuthority> {
        let source_unit = self.units.get(unit)?;
        let Member::Present(foreign) = source_unit.foreign.get(ForeignSourceKind::JavaScript)
        else {
            return None;
        };
        Some(foreign.content.authority())
    }

    pub fn foreign_reload_failure(&self, unit: &SourceUnitKey) -> Option<&ReloadFailure> {
        let source_unit = self.units.get(unit)?;
        let Member::Present(foreign) = source_unit.foreign.get(ForeignSourceKind::JavaScript)
        else {
            return None;
        };
        foreign.content.reload_failure()
    }

    pub fn foreign_id(&self, locator: &str) -> Option<ForeignFileId> {
        self.foreign_files.id(locator)
    }

    fn locator_conflict(&self, unit: &SourceUnitKey) -> Option<LifecycleWarning> {
        if let Some(owner) = self.source_owners.get(unit.source())
            && owner != unit
        {
            return Some(LifecycleWarning::LocatorAlreadyOwned {
                locator: Arc::clone(&unit.source),
                owner: SourceUnitKey::clone(owner),
                requested: SourceUnitKey::clone(unit),
            });
        }
        for kind in ForeignSourceKind::ALL {
            let locator = unit.foreign_for(kind);
            if let Some(owner) = self.foreign_owners.get(locator)
                && owner != unit
            {
                return Some(LifecycleWarning::LocatorAlreadyOwned {
                    locator: Arc::from(locator),
                    owner: SourceUnitKey::clone(owner),
                    requested: SourceUnitKey::clone(unit),
                });
            }
        }
        None
    }

    fn register_pending_modules(&self, engine: &QueryEngine, pending: Vec<PendingModule>) {
        pending.par_iter().for_each(|module| {
            let _ = engine.snapshot().parsed(module.id);
        });
        for module in pending {
            update_module_name(engine, module.id, module.previous_name);
        }
    }

    fn store_unit(&mut self, unit: SourceUnitKey, source_unit: SourceUnit<Version, Metadata>) {
        if source_unit.is_missing() {
            let source_owner = self.source_owners.remove(unit.source());
            debug_assert!(source_owner.is_none_or(|owner| owner == unit));
            for kind in ForeignSourceKind::ALL {
                let foreign_owner = self.foreign_owners.remove(unit.foreign_for(kind));
                debug_assert!(foreign_owner.is_none_or(|owner| owner == unit));
            }
            return;
        }

        let previous_source_owner =
            self.source_owners.insert(Arc::clone(&unit.source), SourceUnitKey::clone(&unit));
        debug_assert!(previous_source_owner.is_none_or(|owner| owner == unit));
        for kind in ForeignSourceKind::ALL {
            let locator = Arc::from(unit.foreign_for(kind));
            let previous_foreign_owner =
                self.foreign_owners.insert(locator, SourceUnitKey::clone(&unit));
            debug_assert!(previous_foreign_owner.is_none_or(|owner| owner == unit));
        }
        self.units.insert(unit, source_unit);
    }
}

/// Registering the module name of a source requires parsing it, which bulk
/// application defers so that sources are parsed in parallel rather than one
/// at a time under exclusive access to the engine.
enum ModuleRegistration<'a> {
    Immediate,
    Deferred(&'a mut Vec<PendingModule>),
}

impl ModuleRegistration<'_> {
    fn register(&mut self, engine: &QueryEngine, id: FileId, previous_name: Option<SmolStr>) {
        match self {
            ModuleRegistration::Immediate => update_module_name(engine, id, previous_name),
            ModuleRegistration::Deferred(pending) => {
                pending.push(PendingModule { id, previous_name });
            }
        }
    }
}

struct PendingModule {
    id: FileId,
    previous_name: Option<SmolStr>,
}

fn update_module_name(engine: &QueryEngine, id: FileId, previous_name: Option<SmolStr>) {
    let content = engine
        .content(id)
        .expect("invariant violated: source lifecycle requires exclusive engine mutation");
    let (parsed, _) = engine
        .parsed(id)
        .expect("invariant violated: source lifecycle requires exclusive engine mutation");
    let current_name = parsed.module_name(&content);
    if previous_name != current_name
        && let Some(previous_name) = previous_name
    {
        engine.remove_module_file(&previous_name, id);
    }
    if let Some(current_name) = current_name {
        engine.set_module_file(&current_name, id);
    }
}

impl<Version, Metadata> SourceUnit<Version, Metadata> {
    fn is_missing(&self) -> bool {
        matches!(self.source, Member::Missing)
            && matches!(self.foreign.javascript, Member::Missing)
            && matches!(self.foreign.jsx, Member::Missing)
    }

    fn source_id(&self) -> Option<FileId> {
        let Member::Present(source) = &self.source else {
            return None;
        };
        Some(source.id)
    }

    fn foreign_files(&self) -> ForeignFileCandidates {
        let mut candidates = ForeignFileCandidates::default();
        for kind in ForeignSourceKind::ALL {
            let Member::Present(foreign) = self.foreign.get(kind) else {
                continue;
            };
            candidates.insert(foreign.id);
        }
        candidates
    }
}
