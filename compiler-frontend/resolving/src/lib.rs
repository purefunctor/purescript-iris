mod algorithm;
mod error;

pub use error::*;

use building_types::{QueryProxy, QueryResult};
use files::FileId;
use indexing::{ImportId, ImportKind, IndexedModule, TermItemId, TypeItemId};
use rustc_hash::FxHashMap;
use smol_str::SmolStr;
use std::sync::Arc;

pub trait ExternalQueries:
    QueryProxy<Indexed = Arc<IndexedModule>, Resolved = Arc<ResolvedModule>>
{
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ResolvedClassMembers {
    members: FxHashMap<(FileId, TypeItemId, SmolStr), (FileId, TermItemId)>,
}

impl ResolvedClassMembers {
    pub fn insert(
        &mut self,
        class_file: FileId,
        class_id: TypeItemId,
        name: SmolStr,
        member_file: FileId,
        term_id: TermItemId,
    ) {
        self.members.insert((class_file, class_id, name), (member_file, term_id));
    }

    pub fn lookup(
        &self,
        class_file: FileId,
        class_id: TypeItemId,
        name: &str,
    ) -> Option<(FileId, TermItemId)> {
        let key = &(class_file, class_id, SmolStr::new(name));
        self.members.get(key).copied()
    }

    pub fn class_members(
        &self,
        class_file: FileId,
        class_id: TypeItemId,
    ) -> impl Iterator<Item = (&SmolStr, FileId, TermItemId)> + '_ {
        self.members
            .iter()
            .filter(move |((file_id, type_id, _), _)| {
                *file_id == class_file && *type_id == class_id
            })
            .map(|((_, _, name), (file, id))| (name, *file, *id))
    }

    pub fn iter(&self) -> impl Iterator<Item = (TypeItemId, &SmolStr, FileId, TermItemId)> + '_ {
        self.members.iter().map(|((_, class_id, name), (file, id))| (*class_id, name, *file, *id))
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ResolvedModule {
    pub unqualified: ResolvedImportsUnqualified,
    pub qualified: ResolvedImportsQualified,
    pub exports: ResolvedExports,
    pub locals: ResolvedLocals,
    pub class: ResolvedClassMembers,
    pub errors: Vec<ResolvingError>,
}

impl ResolvedModule {
    /// Returns the first explicitly imported item, falling back to the first
    /// implicitly imported item; hidden items are never visible.
    fn lookup_visible<'a, ItemId, LookupFn>(
        imports: impl IntoIterator<Item = &'a ResolvedImport>,
        lookup: LookupFn,
    ) -> Option<(FileId, ItemId)>
    where
        LookupFn: Fn(&ResolvedImport) -> Option<(FileId, ItemId, ImportKind)>,
    {
        let mut implicit = None;
        for import in imports {
            match lookup(import) {
                Some((file_id, item_id, ImportKind::Explicit)) => return Some((file_id, item_id)),
                Some((file_id, item_id, ImportKind::Implicit)) if implicit.is_none() => {
                    implicit = Some((file_id, item_id));
                }
                _ => (),
            }
        }
        implicit
    }

    fn lookup_qualified<ItemId, LookupFn, DefaultFn>(
        &self,
        qualifier: &str,
        lookup: LookupFn,
        default: DefaultFn,
    ) -> Option<(FileId, ItemId)>
    where
        LookupFn: Fn(&ResolvedImport) -> Option<(FileId, ItemId, ImportKind)>,
        DefaultFn: FnOnce() -> Option<(FileId, ItemId)>,
    {
        if let Some(imports) = self.qualified.get(qualifier) {
            ResolvedModule::lookup_visible(imports, lookup)
        } else if qualifier == "Prim" {
            default()
        } else {
            None
        }
    }

    fn lookup_unqualified<ItemId, LookupFn>(&self, lookup: LookupFn) -> Option<(FileId, ItemId)>
    where
        LookupFn: Fn(&ResolvedImport) -> Option<(FileId, ItemId, ImportKind)>,
    {
        ResolvedModule::lookup_visible(self.unqualified.values().flatten(), lookup)
    }

    /// Prim is implicitly imported unless the module imports it unqualified,
    /// in which case [`ResolvedModule::lookup_unqualified`] has already
    /// searched those imports.
    fn lookup_implicit_prim<ItemId, DefaultFn>(
        &self,
        default: DefaultFn,
    ) -> Option<(FileId, ItemId)>
    where
        DefaultFn: FnOnce() -> Option<(FileId, ItemId)>,
    {
        if self.unqualified.contains_key("Prim") { None } else { default() }
    }

    pub fn lookup_term(
        &self,
        prim: &ResolvedModule,
        qualifier: Option<&str>,
        name: &str,
    ) -> Option<(FileId, TermItemId)> {
        if let Some(qualifier) = qualifier {
            let lookup_item = |import: &ResolvedImport| import.lookup_term(name);
            let lookup_prim = || prim.exports.lookup_term(name);
            self.lookup_qualified(qualifier, lookup_item, lookup_prim)
        } else {
            let lookup_item = |import: &ResolvedImport| import.lookup_term(name);
            let lookup_prim = || prim.exports.lookup_term(name);
            None.or_else(|| self.locals.lookup_term(name))
                .or_else(|| self.lookup_unqualified(lookup_item))
                .or_else(|| self.lookup_implicit_prim(lookup_prim))
        }
    }

    pub fn lookup_type(
        &self,
        prim: &ResolvedModule,
        qualifier: Option<&str>,
        name: &str,
    ) -> Option<(FileId, TypeItemId)> {
        if let Some(qualifier) = qualifier {
            let lookup_item = |import: &ResolvedImport| import.lookup_type(name);
            let lookup_prim = || prim.exports.lookup_type(name);
            self.lookup_qualified(qualifier, lookup_item, lookup_prim)
        } else {
            let lookup_item = |import: &ResolvedImport| import.lookup_type(name);
            let lookup_prim = || prim.exports.lookup_type(name);
            None.or_else(|| self.locals.lookup_type(name))
                .or_else(|| self.lookup_unqualified(lookup_item))
                .or_else(|| self.lookup_implicit_prim(lookup_prim))
        }
    }

    pub fn lookup_class(
        &self,
        prim: &ResolvedModule,
        qualifier: Option<&str>,
        name: &str,
    ) -> Option<(FileId, TypeItemId)> {
        if let Some(qualifier) = qualifier {
            let lookup_item = |import: &ResolvedImport| import.lookup_class(name);
            let lookup_prim = || prim.exports.lookup_class(name);
            self.lookup_qualified(qualifier, lookup_item, lookup_prim)
        } else {
            let lookup_item = |import: &ResolvedImport| import.lookup_class(name);
            let lookup_prim = || prim.exports.lookup_class(name);
            None.or_else(|| self.locals.lookup_class(name))
                .or_else(|| self.lookup_unqualified(lookup_item))
                .or_else(|| self.lookup_implicit_prim(lookup_prim))
        }
    }

    pub fn lookup_class_member(
        &self,
        class_file: FileId,
        class_id: TypeItemId,
        name: &str,
    ) -> Option<(FileId, TermItemId)> {
        self.class.lookup(class_file, class_id, name)
    }

    pub fn is_term_in_scope(
        &self,
        prim: &ResolvedModule,
        file_id: FileId,
        item_id: TermItemId,
    ) -> bool {
        if self.locals.contains_term(file_id, item_id) {
            return true;
        }

        for imports in self.unqualified.values() {
            for import in imports {
                if import.contains_term(file_id, item_id) {
                    return true;
                }
            }
        }

        for imports in self.qualified.values() {
            for import in imports {
                if import.contains_term(file_id, item_id) {
                    return true;
                }
            }
        }

        // If an unqualified Prim import exists, use its import list;
        if let Some(prim_imports) = self.unqualified.get("Prim") {
            for prim_import in prim_imports {
                if prim_import.contains_term(file_id, item_id) {
                    return true;
                }
            }
        }

        // if a qualified Prim import exists, use its import list;
        if let Some(prim_imports) = self.qualified.get("Prim") {
            for prim_import in prim_imports {
                if prim_import.contains_term(file_id, item_id) {
                    return true;
                }
            }
        }

        // if there are no Prim imports, use the export list.
        if prim.exports.contains_term(file_id, item_id) {
            return true;
        }

        false
    }
}

type ResolvedImportsUnqualified = FxHashMap<SmolStr, Vec<ResolvedImport>>;
type ResolvedImportsQualified = FxHashMap<SmolStr, Vec<ResolvedImport>>;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ResolvedLocals {
    terms: FxHashMap<SmolStr, (FileId, TermItemId)>,
    types: FxHashMap<SmolStr, (FileId, TypeItemId)>,
    classes: FxHashMap<SmolStr, (FileId, TypeItemId)>,
}

impl ResolvedLocals {
    pub fn lookup_term(&self, name: &str) -> Option<(FileId, TermItemId)> {
        self.terms.get(name).copied()
    }

    pub fn lookup_type(&self, name: &str) -> Option<(FileId, TypeItemId)> {
        self.types.get(name).copied()
    }

    pub fn contains_term(&self, file: FileId, term: TermItemId) -> bool {
        self.terms.values().any(|&(f, t)| f == file && t == term)
    }

    pub fn iter_terms(&self) -> impl Iterator<Item = (&SmolStr, FileId, TermItemId)> {
        self.terms.iter().map(|(k, (f, i))| (k, *f, *i))
    }

    pub fn iter_types(&self) -> impl Iterator<Item = (&SmolStr, FileId, TypeItemId)> {
        self.types.iter().map(|(k, (f, i))| (k, *f, *i))
    }

    pub fn lookup_class(&self, name: &str) -> Option<(FileId, TypeItemId)> {
        self.classes.get(name).copied()
    }

    pub fn iter_classes(&self) -> impl Iterator<Item = (&SmolStr, FileId, TypeItemId)> {
        self.classes.iter().map(|(k, (f, i))| (k, *f, *i))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportSource {
    Local,
    Import(ImportId),
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ExportedModule {
    pub local: Arc<[TermItemId]>,
    pub indirect: Arc<[IndirectExports]>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct IndirectExports {
    pub file_id: FileId,
    pub terms: Arc<[TermItemId]>,
}

pub fn export_module(module: &ResolvedModule) -> ExportedModule {
    let mut local = Vec::new();
    let mut indirect = FxHashMap::<FileId, Vec<TermItemId>>::default();
    for &(file_id, term_id, source) in module.exports.terms.values() {
        match source {
            ExportSource::Local => local.push(term_id),
            ExportSource::Import(_) => indirect.entry(file_id).or_default().push(term_id),
        }
    }

    local.sort_by_key(|term_id| term_id.into_raw().into_u32());
    local.dedup();
    let indirect = indirect.into_iter().map(|(file_id, mut terms)| {
        terms.sort_by_key(|term_id| term_id.into_raw().into_u32());
        terms.dedup();
        IndirectExports { file_id, terms: terms.into() }
    });
    let mut indirect = indirect.collect::<Vec<_>>();
    indirect.sort_by_key(|exports| exports.file_id);

    ExportedModule { local: local.into(), indirect: indirect.into() }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ResolvedExports {
    terms: FxHashMap<SmolStr, (FileId, TermItemId, ExportSource)>,
    types: FxHashMap<SmolStr, (FileId, TypeItemId, ExportSource)>,
    classes: FxHashMap<SmolStr, (FileId, TypeItemId, ExportSource)>,
}

impl ResolvedExports {
    pub fn lookup_term(&self, name: &str) -> Option<(FileId, TermItemId)> {
        self.terms.get(name).copied().map(|(f, i, _)| (f, i))
    }

    pub fn lookup_type(&self, name: &str) -> Option<(FileId, TypeItemId)> {
        self.types.get(name).copied().map(|(f, i, _)| (f, i))
    }

    pub fn contains_term(&self, file: FileId, term: TermItemId) -> bool {
        self.terms.values().any(|&(f, t, _)| f == file && t == term)
    }

    pub fn iter_terms(&self) -> impl Iterator<Item = (&SmolStr, FileId, TermItemId)> {
        self.terms.iter().map(|(k, (f, i, _))| (k, *f, *i))
    }

    pub fn iter_types(&self) -> impl Iterator<Item = (&SmolStr, FileId, TypeItemId)> {
        self.types.iter().map(|(k, (f, i, _))| (k, *f, *i))
    }

    pub fn lookup_class(&self, name: &str) -> Option<(FileId, TypeItemId)> {
        self.classes.get(name).copied().map(|(f, i, _)| (f, i))
    }

    pub fn iter_classes(&self) -> impl Iterator<Item = (&SmolStr, FileId, TypeItemId)> {
        self.classes.iter().map(|(k, (f, i, _))| (k, *f, *i))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ResolvedImport {
    pub id: ImportId,
    pub file: FileId,
    pub kind: ImportKind,
    pub exported: bool,
    terms: FxHashMap<SmolStr, (FileId, TermItemId, ImportKind)>,
    types: FxHashMap<SmolStr, (FileId, TypeItemId, ImportKind)>,
    classes: FxHashMap<SmolStr, (FileId, TypeItemId, ImportKind)>,
}

impl ResolvedImport {
    fn new(id: ImportId, file: FileId, kind: ImportKind, exported: bool) -> ResolvedImport {
        let terms = FxHashMap::default();
        let types = FxHashMap::default();
        let classes = FxHashMap::default();
        ResolvedImport { id, file, kind, exported, terms, types, classes }
    }

    pub fn lookup_term(&self, name: &str) -> Option<(FileId, TermItemId, ImportKind)> {
        self.terms.get(name).copied()
    }

    pub fn lookup_type(&self, name: &str) -> Option<(FileId, TypeItemId, ImportKind)> {
        self.types.get(name).copied()
    }

    pub fn contains_term(&self, file: FileId, term: TermItemId) -> bool {
        self.terms
            .values()
            .any(|&(f, t, kind)| f == file && t == term && !matches!(kind, ImportKind::Hidden))
    }

    pub fn iter_terms(&self) -> impl Iterator<Item = (&SmolStr, FileId, TermItemId, ImportKind)> {
        self.terms.iter().map(|(k, (f, i, d))| (k, *f, *i, *d))
    }

    pub fn iter_types(&self) -> impl Iterator<Item = (&SmolStr, FileId, TypeItemId, ImportKind)> {
        self.types.iter().map(|(k, (f, i, d))| (k, *f, *i, *d))
    }

    pub fn lookup_class(&self, name: &str) -> Option<(FileId, TypeItemId, ImportKind)> {
        self.classes.get(name).copied()
    }

    pub fn iter_classes(&self) -> impl Iterator<Item = (&SmolStr, FileId, TypeItemId, ImportKind)> {
        self.classes.iter().map(|(k, (f, i, d))| (k, *f, *i, *d))
    }
}

pub fn resolve_module(queries: &impl ExternalQueries, file: FileId) -> QueryResult<ResolvedModule> {
    let algorithm::State { unqualified, qualified, exports, locals, class, errors } =
        algorithm::resolve_module(queries, file)?;
    Ok(ResolvedModule { unqualified, qualified, exports, locals, class, errors })
}
