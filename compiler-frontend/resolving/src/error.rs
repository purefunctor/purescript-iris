use files::FileId;
use indexing::{
    ImportId, ImportItemId, InstanceSourceItemId, OrderedTermItemId, TermItemId, TypeItemId,
};
use smol_str::SmolStr;

use crate::ExportSource;

/// The kind of errors produced during name resolution.
#[derive(Debug, PartialEq, Eq)]
pub enum ResolvingError {
    /// Another source file declares the same module name.
    DuplicateModule {
        name: SmolStr,
    },
    InstanceNameConflict {
        name: SmolStr,
        instance: InstanceSourceItemId,
        existing: OrderedTermItemId,
    },
    TermExportConflict {
        name: SmolStr,
        existing: (FileId, TermItemId, ExportSource),
        duplicate: (FileId, TermItemId, ExportSource),
    },
    TypeExportConflict {
        name: SmolStr,
        existing: (FileId, TypeItemId, ExportSource),
        duplicate: (FileId, TypeItemId, ExportSource),
    },

    ExistingTerm {
        existing: (FileId, TermItemId),
        duplicate: (FileId, TermItemId),
    },
    ExistingType {
        existing: (FileId, TypeItemId),
        duplicate: (FileId, TypeItemId),
    },

    InvalidImportStatement {
        id: ImportId,
    },

    InvalidImportItem {
        id: ImportItemId,
    },
}
