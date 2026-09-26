//! Filesystem events, collected into batches before the build actor applies them.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use notify::event::{CreateKind, ModifyKind, RemoveKind};
use notify::{Event, EventKind};
use tokio::time::Instant;

/// Filesystem events collected since the first one arrived.
pub(crate) struct Batch {
    pub(crate) paths: BTreeSet<PathBuf>,
    pub(crate) rescan: bool,
    /// When the batch is applied: a fixed time after its first event, not after the latest one.
    pub(crate) deadline: Instant,
}

impl Batch {
    pub(crate) fn new(deadline: Instant) -> Batch {
        Batch { paths: BTreeSet::new(), rescan: false, deadline }
    }

    pub(crate) fn add(&mut self, event: Event) {
        debug_assert!(!is_access(&event), "access events are never batched");
        self.rescan |= event.need_rescan() || requires_rescan(event.kind);
        self.paths.extend(event.paths);
    }
}

/// Reading a file reports an access event on some platforms, so a rebuild that only reads files
/// must not start another one.
pub(crate) fn is_access(event: &Event) -> bool {
    matches!(event.kind, EventKind::Access(_))
}

fn requires_rescan(kind: EventKind) -> bool {
    matches!(
        kind,
        EventKind::Create(CreateKind::Folder)
            | EventKind::Modify(ModifyKind::Name(_))
            | EventKind::Remove(RemoveKind::Folder)
            | EventKind::Any
            | EventKind::Other
    )
}

/// The closest existing directory above `root`, so that the watch survives `root` being deleted
/// and created again.
pub(crate) fn persistent_watch_root(root: &Path) -> PathBuf {
    let mut candidate = root.parent().unwrap_or(root);
    while !candidate.exists() {
        let Some(parent) = candidate.parent() else {
            break;
        };
        candidate = parent;
    }
    if candidate.is_file() {
        candidate.parent().unwrap_or(candidate).to_path_buf()
    } else {
        candidate.to_path_buf()
    }
}
