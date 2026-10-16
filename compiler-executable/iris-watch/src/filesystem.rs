//! Filesystem events, collected into batches before the build actor applies them.

use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};

use itertools::Itertools;
use notify::event::{CreateKind, Flag, ModifyKind, RemoveKind};
use notify::{Event, EventKind, PathsMut, RecommendedWatcher, RecursiveMode, Watcher, WatcherKind};
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

pub(crate) struct SourceWatcher {
    watcher: RecommendedWatcher,
    sources: SourcePaths,
    persistent_roots: BTreeSet<PathBuf>,
    unwatched_roots: BTreeSet<PathBuf>,
}

impl SourceWatcher {
    pub(crate) fn start(
        mut watcher: RecommendedWatcher,
        source_roots: &[PathBuf],
        output: &Path,
    ) -> notify::Result<SourceWatcher> {
        let sources = SourcePaths::new(source_roots, output);
        let persistent_roots = sources.roots.iter().map(|root| persistent_watch_root(root));
        let persistent_roots = persistent_roots.collect::<BTreeSet<_>>();
        let mut paths = watcher.paths_mut();
        if watches_whole_subtrees() {
            for persistent_root in outermost(&persistent_roots) {
                paths.add(&persistent_root, RecursiveMode::Recursive)?;
            }
        } else {
            for persistent_root in &persistent_roots {
                paths.add(persistent_root, RecursiveMode::NonRecursive)?;
            }
            for root in sources.roots.iter().filter(|root| root.is_dir()) {
                paths.add(root, RecursiveMode::Recursive)?;
            }
        }
        paths.commit()?;
        let unwatched_roots = BTreeSet::new();
        Ok(SourceWatcher { watcher, sources, persistent_roots, unwatched_roots })
    }

    pub(crate) fn source_event(&mut self, mut event: Event) -> Option<Event> {
        if event.paths.is_empty() {
            return Some(event);
        }
        event.paths.retain(|path| self.sources.concerns(path));
        if event.paths.is_empty() {
            return None;
        }
        if !may_change_directories(event.kind) {
            return Some(event);
        }
        let roots = event.paths.iter().flat_map(|path| self.sources.roots_at_or_below(path));
        let roots = roots.cloned().collect_vec();
        if roots.is_empty() {
            return Some(event);
        }
        if !watches_whole_subtrees() {
            self.unwatched_roots.extend(roots);
        }
        Some(event.set_flag(Flag::Rescan))
    }

    pub(crate) fn watch_unwatched_roots(&mut self) -> notify::Result<bool> {
        if self.unwatched_roots.is_empty() {
            return Ok(true);
        }
        self.persistent_roots.retain(|persistent_root| persistent_root.is_dir());
        let mut paths = self.watcher.paths_mut();
        let mut still_unwatched = BTreeSet::new();
        let mut failure = None;
        for root in std::mem::take(&mut self.unwatched_roots) {
            match watch_root(paths.as_mut(), &mut self.persistent_roots, &root) {
                Ok(()) => {}
                Err(error) if vanished(&error) || !root.exists() => {
                    still_unwatched.insert(root);
                }
                Err(error) => {
                    failure = Some(error);
                    break;
                }
            }
        }
        paths.commit()?;
        self.unwatched_roots = still_unwatched;
        match failure {
            Some(error) => Err(error),
            None => Ok(self.unwatched_roots.is_empty()),
        }
    }
}

struct SourcePaths {
    output: PathBuf,
    roots: BTreeSet<PathBuf>,
    ancestors: BTreeSet<PathBuf>,
}

impl SourcePaths {
    fn new(source_roots: &[PathBuf], output: &Path) -> SourcePaths {
        let roots = outermost(source_roots);
        let ancestors = roots.iter().flat_map(|root| root.ancestors().skip(1));
        let ancestors = ancestors.map(Path::to_path_buf).collect();
        SourcePaths { output: output.to_path_buf(), roots, ancestors }
    }

    fn concerns(&self, path: &Path) -> bool {
        if path.starts_with(&self.output) {
            return false;
        }
        self.ancestors.contains(path)
            || path.ancestors().any(|ancestor| self.roots.contains(ancestor))
    }

    fn roots_at_or_below<'a>(&'a self, path: &'a Path) -> impl Iterator<Item = &'a PathBuf> {
        self.roots.iter().filter(move |root| root.starts_with(path))
    }
}

fn outermost<'a>(paths: impl IntoIterator<Item = &'a PathBuf>) -> BTreeSet<PathBuf> {
    let paths = paths.into_iter().map(PathBuf::as_path).collect::<BTreeSet<_>>();
    let enclosed = |path: &Path| path.ancestors().skip(1).any(|ancestor| paths.contains(ancestor));
    let outermost = paths.iter().filter(|path| !enclosed(path));
    outermost.map(|path| path.to_path_buf()).collect()
}

fn watches_whole_subtrees() -> bool {
    matches!(
        RecommendedWatcher::kind(),
        WatcherKind::Fsevent | WatcherKind::ReadDirectoryChangesWatcher
    )
}

fn watch_root(
    paths: &mut dyn PathsMut,
    persistent_roots: &mut BTreeSet<PathBuf>,
    root: &Path,
) -> notify::Result<()> {
    let persistent_root = persistent_watch_root(root);
    if !persistent_roots.contains(&persistent_root) {
        paths.add(&persistent_root, RecursiveMode::NonRecursive)?;
        persistent_roots.insert(persistent_root);
    }
    if root.is_dir() {
        let _ = paths.remove(root);
        paths.add(root, RecursiveMode::Recursive)?;
    }
    Ok(())
}

fn vanished(error: &notify::Error) -> bool {
    match &error.kind {
        notify::ErrorKind::PathNotFound => true,
        notify::ErrorKind::Io(error) => error.kind() == io::ErrorKind::NotFound,
        _ => false,
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

fn may_change_directories(kind: EventKind) -> bool {
    matches!(
        kind,
        EventKind::Create(_)
            | EventKind::Modify(ModifyKind::Name(_))
            | EventKind::Remove(_)
            | EventKind::Any
            | EventKind::Other
    )
}

/// The closest existing directory above `root`, so that the watch survives `root` being deleted
/// and created again.
fn persistent_watch_root(root: &Path) -> PathBuf {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn source_paths() -> SourcePaths {
        let source_roots = [
            PathBuf::from("/workspace/src"),
            PathBuf::from("/workspace/src/vendored/src"),
            PathBuf::from("/workspace/.spago/p/prelude/src"),
        ];
        SourcePaths::new(&source_roots, Path::new("/workspace/src/generated"))
    }

    #[test]
    fn concerns_source_roots_and_their_ancestors_except_the_output_directory() {
        let sources = source_paths();
        for path in [
            "/workspace",
            "/workspace/src",
            "/workspace/src/Main.purs",
            "/workspace/src/vendored/src/Vendored.purs",
            "/workspace/.spago/p/prelude",
        ] {
            assert!(sources.concerns(Path::new(path)), "{path}");
        }
        for path in [
            "/workspace/src/generated",
            "/workspace/src/generated/Main/index.js",
            "/workspace/spago.yaml",
            "/workspace/srcs/Main.purs",
            "/workspace/.spago/p/prelude/test/Main.purs",
        ] {
            assert!(!sources.concerns(Path::new(path)), "{path}");
        }
    }

    #[test]
    fn finds_the_outermost_source_roots_at_or_below_a_directory() {
        let sources = source_paths();
        let roots = |path: &str| sources.roots_at_or_below(Path::new(path)).cloned().collect_vec();
        assert_eq!(
            roots("/workspace"),
            [PathBuf::from("/workspace/.spago/p/prelude/src"), PathBuf::from("/workspace/src")]
        );
        assert_eq!(roots("/workspace/src"), [PathBuf::from("/workspace/src")]);
        assert_eq!(roots("/workspace/src/vendored"), Vec::<PathBuf>::new());
        assert_eq!(roots("/workspace/.spago/p/prelude/src/Prelude"), Vec::<PathBuf>::new());
    }
}
