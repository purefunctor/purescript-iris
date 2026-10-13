use files::FileId;

/// The source files registered under a module name, ordered by [`FileId`].
///
/// A module name normally belongs to a single file. Each source that declares
/// the name keeps its registration so that the conflict remains observable and
/// resolves once all but one of the declarations are renamed or removed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModuleFiles {
    files: Vec<FileId>,
}

impl ModuleFiles {
    pub fn insert(&mut self, file_id: FileId) {
        if let Err(index) = self.files.binary_search(&file_id) {
            self.files.insert(index, file_id);
        }
    }

    pub fn remove(&mut self, file_id: FileId) -> bool {
        let Ok(index) = self.files.binary_search(&file_id) else {
            return false;
        };
        self.files.remove(index);
        true
    }

    /// The file that imports of the module name resolve to.
    ///
    /// While a duplicate is reported, importers resolve against the lowest
    /// [`FileId`] so that the choice does not depend on registration order.
    pub fn primary(&self) -> Option<FileId> {
        self.files.first().copied()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn is_duplicated(&self) -> bool {
        self.files.len() > 1
    }
}
