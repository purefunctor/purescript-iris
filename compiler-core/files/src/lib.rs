use core::{fmt, str};
use std::collections::BTreeMap;
use std::sync::Arc;

use rayon::iter::{IntoParallelIterator, ParallelIterator};
use rustc_hash::FxHashMap;

#[derive(Debug, Default)]
pub struct Files {
    paths: FxHashMap<Arc<str>, FileId>,
    files: BTreeMap<FileId, FileData>,
    next_id: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileId {
    index: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SourceDialect {
    #[default]
    PureScript,
    Iris,
}

impl SourceDialect {
    pub fn from_path(path: &str) -> SourceDialect {
        if path.ends_with(".iris") { SourceDialect::Iris } else { SourceDialect::PureScript }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ForeignSourceKind {
    JavaScript,
    Jsx,
}

impl ForeignSourceKind {
    pub const ALL: [ForeignSourceKind; 2] = [ForeignSourceKind::JavaScript, ForeignSourceKind::Jsx];

    pub const fn extension(self) -> &'static str {
        match self {
            ForeignSourceKind::JavaScript => "js",
            ForeignSourceKind::Jsx => "jsx",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ForeignFileCandidates {
    javascript: Option<ForeignFileId>,
    jsx: Option<ForeignFileId>,
}

impl ForeignFileCandidates {
    pub fn get(self, kind: ForeignSourceKind) -> Option<ForeignFileId> {
        match kind {
            ForeignSourceKind::JavaScript => self.javascript,
            ForeignSourceKind::Jsx => self.jsx,
        }
    }

    pub fn insert(&mut self, id: ForeignFileId) {
        match id.kind() {
            ForeignSourceKind::JavaScript => self.javascript = Some(id),
            ForeignSourceKind::Jsx => self.jsx = Some(id),
        }
    }

    pub fn remove(&mut self, id: ForeignFileId) -> bool {
        let candidate = match id.kind() {
            ForeignSourceKind::JavaScript => &mut self.javascript,
            ForeignSourceKind::Jsx => &mut self.jsx,
        };
        if *candidate != Some(id) {
            return false;
        }
        *candidate = None;
        true
    }

    pub fn unique(self) -> Option<ForeignFileId> {
        match (self.javascript, self.jsx) {
            (Some(id), None) | (None, Some(id)) => Some(id),
            (None, None) | (Some(_), Some(_)) => None,
        }
    }

    pub fn count(self) -> usize {
        usize::from(self.javascript.is_some()) + usize::from(self.jsx.is_some())
    }
}

impl fmt::Debug for FileId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Idx::<File>({})", self.index)
    }
}

impl FileId {
    pub const fn new(index: u32) -> FileId {
        FileId { index }
    }

    pub const fn into_raw(self) -> u32 {
        self.index
    }
}

#[derive(Debug)]
struct FileData {
    path: Arc<str>,
    content: Arc<str>,
}

#[derive(Debug, Default)]
pub struct ForeignFiles {
    paths: FxHashMap<Arc<str>, ForeignFileId>,
    files: FxHashMap<ForeignFileId, ForeignFileData>,
    next_id: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ForeignFileId {
    index: u32,
    kind: ForeignSourceKind,
}

impl ForeignFileId {
    pub const fn kind(self) -> ForeignSourceKind {
        self.kind
    }
}

#[derive(Debug)]
struct ForeignFileData {
    path: Arc<str>,
    content: Arc<str>,
}

impl Files {
    pub fn insert(&mut self, path: impl Into<Arc<str>>, content: impl Into<Arc<str>>) -> FileId {
        let path = path.into();
        let content = content.into();
        if let Some(file_id) = self.paths.get(path.as_ref()).copied() {
            let file = self.file_mut(file_id);
            file.content = content;
            return file_id;
        }

        let file_id = FileId { index: self.next_id };
        self.next_id = self.next_id.checked_add(1).expect("invariant violated: too many files");
        let file = FileData { path: Arc::clone(&path), content };
        self.paths.insert(path, file_id);
        self.files.insert(file_id, file);
        file_id
    }

    pub fn id(&self, path: &str) -> Option<FileId> {
        self.paths.get(path).copied()
    }

    pub fn contains(&self, file_id: FileId) -> bool {
        self.files.contains_key(&file_id)
    }

    pub fn path(&self, file_id: FileId) -> Arc<str> {
        let file = self.file(file_id);
        Arc::clone(&file.path)
    }

    pub fn content(&self, file_id: FileId) -> Arc<str> {
        let file = self.file(file_id);
        Arc::clone(&file.content)
    }

    pub fn dialect(&self, file_id: FileId) -> SourceDialect {
        SourceDialect::from_path(&self.file(file_id).path)
    }

    pub fn remove(&mut self, path: &str) -> Option<FileId> {
        let file_id = self.paths.remove(path)?;
        self.files.remove(&file_id).expect("invariant violated: expected valid FileId");
        Some(file_id)
    }

    pub fn iter_id(&self) -> impl Iterator<Item = FileId> + '_ {
        self.files.keys().copied()
    }

    pub fn par_iter_id(&self) -> impl ParallelIterator<Item = FileId> {
        let file_ids = self.files.keys().copied();
        let file_ids = file_ids.collect::<Vec<_>>();
        file_ids.into_par_iter()
    }

    fn file(&self, file_id: FileId) -> &FileData {
        self.files.get(&file_id).expect("invariant violated: expected valid FileId")
    }

    fn file_mut(&mut self, file_id: FileId) -> &mut FileData {
        self.files.get_mut(&file_id).expect("invariant violated: expected valid FileId")
    }
}

impl ForeignFiles {
    pub fn insert(
        &mut self,
        kind: ForeignSourceKind,
        path: impl Into<Arc<str>>,
        content: impl Into<Arc<str>>,
    ) -> ForeignFileId {
        let path = path.into();
        let content = content.into();
        if let Some(foreign_file_id) = self.paths.get(path.as_ref()).copied() {
            assert_eq!(foreign_file_id.kind(), kind, "foreign source kind changed for locator");
            let file = self.file_mut(foreign_file_id);
            file.content = content;
            return foreign_file_id;
        }

        let foreign_file_id = ForeignFileId { index: self.next_id, kind };
        self.next_id =
            self.next_id.checked_add(1).expect("invariant violated: too many foreign files");
        let file = ForeignFileData { path: Arc::clone(&path), content };
        self.paths.insert(path, foreign_file_id);
        self.files.insert(foreign_file_id, file);
        foreign_file_id
    }

    pub fn id(&self, path: &str) -> Option<ForeignFileId> {
        self.paths.get(path).copied()
    }

    pub fn path(&self, foreign_file_id: ForeignFileId) -> Arc<str> {
        let file = self.file(foreign_file_id);
        Arc::clone(&file.path)
    }

    pub fn content(&self, foreign_file_id: ForeignFileId) -> Arc<str> {
        let file = self.file(foreign_file_id);
        Arc::clone(&file.content)
    }

    pub fn set_content(&mut self, foreign_file_id: ForeignFileId, content: impl Into<Arc<str>>) {
        let file = self.file_mut(foreign_file_id);
        file.content = content.into();
    }

    pub fn remove(&mut self, path: &str) -> Option<ForeignFileId> {
        let foreign_file_id = self.paths.remove(path)?;
        self.files
            .remove(&foreign_file_id)
            .expect("invariant violated: expected valid ForeignFileId");
        Some(foreign_file_id)
    }

    fn file(&self, foreign_file_id: ForeignFileId) -> &ForeignFileData {
        self.files.get(&foreign_file_id).expect("invariant violated: expected valid ForeignFileId")
    }

    fn file_mut(&mut self, foreign_file_id: ForeignFileId) -> &mut ForeignFileData {
        self.files
            .get_mut(&foreign_file_id)
            .expect("invariant violated: expected valid ForeignFileId")
    }
}

#[cfg(test)]
mod tests {
    use super::{Files, ForeignFiles, ForeignSourceKind};

    #[test]
    fn test_basic() {
        let mut files = Files::default();

        let k = "src/Main.purs";
        let v = "module Main where\n\n";

        let id = files.insert(k, v);

        assert_eq!(files.id(k), Some(id));
        assert_eq!(files.path(id).as_ref(), k);
        assert_eq!(files.content(id).as_ref(), v);
    }

    #[test]
    fn test_remove_files() {
        let mut files = Files::default();
        let content = "module Main where\n";
        let removed_id = files.insert("src/Main.purs", content);
        let retained_id = files.insert("src/Retained.purs", "module Retained where\n");

        assert_eq!(files.remove("src/Main.purs"), Some(removed_id));
        assert_eq!(files.id("src/Main.purs"), None);
        assert_eq!(files.path(retained_id).as_ref(), "src/Retained.purs");

        let replacement_id = files.insert("src/Main.purs", content);
        assert_ne!(replacement_id, removed_id);

        let retained_file_count = files.files.len();
        let mut previous_id = replacement_id;
        for number in 0..32 {
            let path = format!("src/Temporary-{number}.purs");
            let temporary_id = files.insert(path.as_str(), content);
            assert!(temporary_id > previous_id);
            assert_eq!(files.remove(&path), Some(temporary_id));
            previous_id = temporary_id;
        }
        assert_eq!(files.files.len(), retained_file_count);
    }

    #[test]
    fn test_foreign_files() {
        let mut files = ForeignFiles::default();

        let path = "src/Main.js";
        let content = "export const life = 42;\n";
        let id = files.insert(ForeignSourceKind::JavaScript, path, content);

        assert_eq!(files.id(path), Some(id));
        assert_eq!(id.kind(), ForeignSourceKind::JavaScript);
        assert_eq!(files.path(id).as_ref(), path);
        assert_eq!(files.content(id).as_ref(), content);

        let retained_path = "src/Retained.js";
        let retained_id = files.insert(
            ForeignSourceKind::JavaScript,
            retained_path,
            "export const retained = 1;",
        );

        assert_eq!(files.remove(path), Some(id));
        assert_eq!(files.id(path), None);
        assert!(!files.files.contains_key(&id));
        assert_eq!(files.path(retained_id).as_ref(), retained_path);

        let replacement_id = files.insert(ForeignSourceKind::JavaScript, path, content);
        assert_ne!(replacement_id, id);
        assert_eq!(files.remove(path), Some(replacement_id));

        let retained_file_count = files.files.len();
        let mut previous_id = replacement_id;
        for number in 0..32 {
            let path = format!("src/Temporary-{number}.js");
            let temporary_id = files.insert(ForeignSourceKind::JavaScript, path.as_str(), content);
            assert!(temporary_id > previous_id);
            assert_eq!(files.remove(&path), Some(temporary_id));
            previous_id = temporary_id;
        }
        assert_eq!(files.files.len(), retained_file_count);
    }
}
