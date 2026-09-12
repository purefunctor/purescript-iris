use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;
use std::{fs, io};

use building::{DiskObservation, LifecycleChange, QueryError, ReloadFailure, SourceUnitKey};
use console::Style;
use files::ForeignSourceKind;
use itertools::Itertools;
use notify::event::{CreateKind, ModifyKind, RemoveKind};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use thiserror::Error;
use url::Url;

use crate::cli::ColorChoice;
use crate::compilation::CompilationState;
use crate::compile::{self, BuildConfig, BuildOutcome, CompileError, Resilience};
use crate::walk;

const DEBOUNCE_DURATION: Duration = Duration::from_millis(100);
const MODULE_DISPLAY_LIMIT: usize = 3;

pub struct WatchConfig {
    pub current_directory: PathBuf,
    pub output: PathBuf,
    pub inputs: Vec<PathBuf>,
    pub quiet: bool,
    pub color: ColorChoice,
}

#[derive(Debug, Error)]
pub enum WatchError {
    #[error("failed to convert path to a file URL: {0}")]
    InvalidPath(PathBuf),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Notify(#[from] notify::Error),
    #[error(transparent)]
    Query(#[from] QueryError),
    #[error(transparent)]
    Walk(#[from] walk::Error),
}

pub fn watch(config: WatchConfig) -> Result<(), WatchError> {
    let (mut workspace, initial_change) = WatchWorkspace::new(
        config.current_directory.clone(),
        config.inputs.clone(),
        config.output.clone(),
    )?;
    let (sender, receiver) = mpsc::channel();
    let mut watcher = RecommendedWatcher::new(sender, notify::Config::default())?;
    for root in &workspace.watch_roots {
        watcher.watch(root, RecursiveMode::Recursive)?;
    }
    report_lifecycle_warnings(&initial_change.lifecycle);
    if !config.quiet {
        report_changed_modules(&initial_change.modules, compile::use_color(config.color));
    }
    rebuild_or_report(&mut workspace, &config);

    loop {
        let first = receiver.recv().map_err(|error| {
            io::Error::new(io::ErrorKind::BrokenPipe, format!("watch channel closed: {error}"))
        })?;
        let mut events = vec![first?];
        loop {
            match receiver.recv_timeout(DEBOUNCE_DURATION) {
                Ok(event) => events.push(event?),
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(
                        io::Error::new(io::ErrorKind::BrokenPipe, "watch channel closed").into()
                    );
                }
            }
        }

        let change = workspace.synchronize_events(events)?;
        report_lifecycle_warnings(&change.lifecycle);
        if !change.modules.is_empty() {
            if !config.quiet {
                report_changed_modules(&change.modules, compile::use_color(config.color));
            }
            rebuild_or_report(&mut workspace, &config);
        }
    }
}

fn rebuild_or_report(workspace: &mut WatchWorkspace, config: &WatchConfig) {
    if let Err(error) = rebuild(workspace, config) {
        eprintln!("Compilation failed: {error}");
        tracing::error!(?error, "Watch compilation failed");
    }
}

fn rebuild(workspace: &mut WatchWorkspace, config: &WatchConfig) -> Result<(), CompileError> {
    if workspace.compilation.input_source_ids().is_empty() {
        if let Err(error) = workspace.reconcile_outputs(BTreeSet::new()) {
            eprintln!("Failed to remove stale output: {error}");
        }
        if !config.quiet {
            println!("No input files found; waiting for changes.");
        }
        return Ok(());
    }

    let build_config = BuildConfig {
        output: &config.output,
        current_directory: &workspace.current_directory,
        color: compile::use_color(config.color),
        progress: false,
        diagnostics: true,
        resilience: Resilience::Strict,
    };
    match compile::build(&workspace.compilation, &build_config)? {
        BuildOutcome::Succeeded(outputs) => {
            if let Err(error) = workspace.reconcile_outputs(outputs) {
                eprintln!("Compilation succeeded, but stale output could not be removed: {error}");
            }
        }
        BuildOutcome::Diagnostics => {}
    }
    Ok(())
}

fn report_lifecycle_warnings(change: &LifecycleChange) {
    for warning in change.warnings() {
        tracing::warn!("{warning}");
        eprintln!("Watch warning: {warning}");
    }
}

fn report_changed_modules(module_names: &BTreeSet<String>, color: bool) {
    if module_names.is_empty() {
        return;
    }

    let timestamp = jiff::Zoned::now().strftime("%H:%M:%S");
    let timestamp =
        Style::new().cyan().dim().force_styling(color).apply_to(format!("[{timestamp}]"));
    let count = module_names.len();
    let noun = if count == 1 { "file" } else { "files" };
    let mut displayed = module_names.iter().take(MODULE_DISPLAY_LIMIT).map(String::as_str);
    let displayed = displayed.join(", ");
    let omitted = count.saturating_sub(MODULE_DISPLAY_LIMIT);

    if omitted == 0 {
        println!("{timestamp} {count} {noun} changed: {displayed}");
    } else {
        println!("{timestamp} {count} {noun} changed: {displayed}, … +{omitted} more");
    }
}

#[derive(Default)]
struct WorkspaceChange {
    lifecycle: LifecycleChange,
    modules: BTreeSet<String>,
}

impl WorkspaceChange {
    fn combine(&mut self, other: WorkspaceChange) {
        self.lifecycle.combine(other.lifecycle);
        self.modules.extend(other.modules);
    }
}

struct WatchWorkspace {
    current_directory: PathBuf,
    inputs: Vec<PathBuf>,
    output: PathBuf,
    watch_roots: BTreeSet<PathBuf>,
    source_globs: globset::GlobSet,
    source_paths: BTreeSet<PathBuf>,
    generated_outputs: BTreeSet<PathBuf>,
    compilation: CompilationState,
}

impl WatchWorkspace {
    fn new(
        current_directory: PathBuf,
        inputs: Vec<PathBuf>,
        output: PathBuf,
    ) -> Result<(WatchWorkspace, WorkspaceChange), WatchError> {
        let walked = walk::walk(&current_directory, &inputs)?;
        let watch_roots = walked.roots.iter().map(|root| persistent_watch_root(root)).collect();
        let source_paths = walked
            .files
            .into_iter()
            .filter(|path| !path.starts_with(&output))
            .collect::<BTreeSet<_>>();
        let mut compilation = CompilationState::new();
        let mut initial_change = WorkspaceChange::default();
        for path in &source_paths {
            initial_change.combine(observe_source_unit(&mut compilation, path)?);
        }

        let workspace = WatchWorkspace {
            current_directory,
            inputs,
            output,
            watch_roots,
            source_globs: walked.globs,
            source_paths,
            generated_outputs: BTreeSet::new(),
            compilation,
        };
        Ok((workspace, initial_change))
    }

    fn synchronize_events(
        &mut self,
        events: impl IntoIterator<Item = Event>,
    ) -> Result<WorkspaceChange, WatchError> {
        let mut rescan = false;
        let mut paths = BTreeSet::new();
        for event in events {
            if !event.paths.is_empty()
                && event.paths.iter().all(|path| path.starts_with(&self.output))
            {
                continue;
            }
            if event.need_rescan() {
                rescan = true;
            }
            if matches!(event.kind, EventKind::Access(_)) {
                continue;
            }
            if matches!(
                event.kind,
                EventKind::Create(CreateKind::Folder)
                    | EventKind::Modify(ModifyKind::Name(_))
                    | EventKind::Remove(RemoveKind::Folder)
                    | EventKind::Any
                    | EventKind::Other
            ) {
                rescan = true;
            }
            paths.extend(event.paths.into_iter().filter(|path| !path.starts_with(&self.output)));
        }

        if rescan { self.rescan() } else { self.synchronize_paths(paths) }
    }

    fn synchronize_paths(
        &mut self,
        paths: impl IntoIterator<Item = PathBuf>,
    ) -> Result<WorkspaceChange, WatchError> {
        let mut source_paths = BTreeSet::new();
        let mut foreign_paths = BTreeSet::new();
        for path in paths {
            if path.starts_with(&self.output) {
                continue;
            }
            match path.extension().and_then(|extension| extension.to_str()) {
                Some("purs") => {
                    if self.source_paths.contains(&path)
                        || !self.source_globs.matches(&path).is_empty()
                    {
                        source_paths.insert(path);
                    }
                }
                Some("js" | "jsx") => {
                    let source_path = path.with_extension("purs");
                    if self.source_paths.contains(&source_path) {
                        foreign_paths.insert(source_path);
                    }
                }
                _ => {}
            }
        }

        let mut change = WorkspaceChange::default();
        for path in source_paths {
            change.combine(observe_source_unit(&mut self.compilation, &path)?);
            if path.exists() {
                self.source_paths.insert(path);
            } else {
                self.source_paths.remove(&path);
            }
        }
        for source_path in foreign_paths {
            change.combine(observe_foreign(&mut self.compilation, &source_path)?);
        }
        Ok(change)
    }

    fn rescan(&mut self) -> Result<WorkspaceChange, WatchError> {
        let walked = walk::walk(&self.current_directory, &self.inputs)?;
        let current_paths = walked
            .files
            .into_iter()
            .filter(|path| !path.starts_with(&self.output))
            .collect::<BTreeSet<_>>();
        let affected_paths = self.source_paths.union(&current_paths).cloned();
        let affected_paths = affected_paths.collect::<Vec<_>>();

        let mut change = WorkspaceChange::default();
        for path in affected_paths {
            change.combine(observe_source_unit(&mut self.compilation, &path)?);
        }
        self.source_globs = walked.globs;
        self.source_paths = current_paths;
        Ok(change)
    }

    fn reconcile_outputs(&mut self, current: BTreeSet<PathBuf>) -> io::Result<()> {
        for stale in self.generated_outputs.difference(&current) {
            match fs::remove_file(stale) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        self.generated_outputs = current;
        Ok(())
    }
}

fn observe_source_unit(
    compilation: &mut CompilationState,
    source_path: &Path,
) -> Result<WorkspaceChange, WatchError> {
    let unit = source_unit(source_path)?;
    let previous_source = compilation.source_content(unit.source())?;
    let previous_foreign =
        ForeignSourceKind::ALL.map(|kind| compilation.foreign_content(unit.foreign_for(kind)));
    let previous_name = compilation.module_name(unit.source())?;

    let source = observe_disk(source_path);
    let mut lifecycle = compilation.observe_source(SourceUnitKey::clone(&unit), source);
    for kind in ForeignSourceKind::ALL {
        let foreign = observe_disk(&source_path.with_extension(kind.extension()));
        lifecycle.combine(compilation.observe_foreign(SourceUnitKey::clone(&unit), kind, foreign));
    }

    let current_source = compilation.source_content(unit.source())?;
    let current_foreign =
        ForeignSourceKind::ALL.map(|kind| compilation.foreign_content(unit.foreign_for(kind)));
    let mut modules = BTreeSet::new();
    if previous_source != current_source || previous_foreign != current_foreign {
        let name = compilation.module_name(unit.source())?.or(previous_name);
        modules.insert(name.unwrap_or_else(|| fallback_module_name(source_path)));
    }
    Ok(WorkspaceChange { lifecycle, modules })
}

fn observe_foreign(
    compilation: &mut CompilationState,
    source_path: &Path,
) -> Result<WorkspaceChange, WatchError> {
    let unit = source_unit(source_path)?;
    let previous_foreign =
        ForeignSourceKind::ALL.map(|kind| compilation.foreign_content(unit.foreign_for(kind)));
    let mut lifecycle = LifecycleChange::default();
    for kind in ForeignSourceKind::ALL {
        let foreign = observe_disk(&source_path.with_extension(kind.extension()));
        lifecycle.combine(compilation.observe_foreign(SourceUnitKey::clone(&unit), kind, foreign));
    }
    let current_foreign =
        ForeignSourceKind::ALL.map(|kind| compilation.foreign_content(unit.foreign_for(kind)));
    let mut modules = BTreeSet::new();
    if previous_foreign != current_foreign {
        let name = compilation.module_name(unit.source())?;
        modules.insert(name.unwrap_or_else(|| fallback_module_name(source_path)));
    }
    Ok(WorkspaceChange { lifecycle, modules })
}

fn source_unit(source_path: &Path) -> Result<SourceUnitKey, WatchError> {
    let source_url = Url::from_file_path(source_path)
        .map_err(|()| WatchError::InvalidPath(source_path.to_path_buf()))?;
    let foreign_path = source_path.with_extension("js");
    let foreign_url = Url::from_file_path(&foreign_path)
        .map_err(|()| WatchError::InvalidPath(foreign_path.clone()))?;
    Ok(SourceUnitKey::new(source_url.as_str(), foreign_url.as_str()))
}

fn observe_disk(path: &Path) -> DiskObservation {
    match fs::read_to_string(path) {
        Ok(content) => DiskObservation::Found(content.into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => DiskObservation::NotFound,
        Err(error) => DiskObservation::Failed(ReloadFailure::new(error.kind(), error.to_string())),
    }
}

fn fallback_module_name(source_path: &Path) -> String {
    source_path
        .file_stem()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| source_path.display().to_string())
}

fn persistent_watch_root(root: &Path) -> PathBuf {
    // The parent remains watched so deleting the input root does not prevent observing its recreation.
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
    use tempfile::TempDir;

    fn workspace() -> (TempDir, WatchWorkspace) {
        let temporary = TempDir::new().unwrap();
        let root = temporary.path();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/Main.purs"), "module Main where\n\nvalue = 1\n").unwrap();
        let (workspace, _) = WatchWorkspace::new(
            root.to_path_buf(),
            vec![PathBuf::from("src/**/*.purs")],
            root.join("output"),
        )
        .unwrap();
        (temporary, workspace)
    }

    #[test]
    fn synchronizes_source_additions_modifications_and_deletions() {
        let (temporary, mut workspace) = workspace();
        let main = temporary.path().join("src/Main.purs");
        let original = workspace.compilation.input_source_ids()[0];

        fs::write(&main, "module Main where\n\nvalue = 2\n").unwrap();
        let change = workspace.synchronize_paths([main.clone()]).unwrap();
        assert_eq!(change.modules, BTreeSet::from([String::from("Main")]));
        assert_eq!(workspace.compilation.input_source_ids(), vec![original]);
        assert_eq!(
            workspace.compilation.snapshot().content(original).unwrap().as_ref(),
            "module Main where\n\nvalue = 2\n"
        );

        let added = temporary.path().join("src/Added.purs");
        fs::write(&added, "module Added where\n").unwrap();
        let change = workspace.synchronize_paths([added.clone()]).unwrap();
        assert_eq!(change.modules, BTreeSet::from([String::from("Added")]));
        assert_eq!(workspace.compilation.input_source_ids().len(), 2);

        fs::remove_file(&main).unwrap();
        let change = workspace.synchronize_paths([main]).unwrap();
        assert_eq!(change.modules, BTreeSet::from([String::from("Main")]));
        assert_eq!(workspace.compilation.input_source_ids().len(), 1);
    }

    #[test]
    fn synchronizes_foreign_changes_for_tracked_sources() {
        let (temporary, mut workspace) = workspace();
        let source_id = workspace.compilation.input_source_ids()[0];
        let foreign = temporary.path().join("src/Main.jsx");
        fs::write(&foreign, "export const value = 1;\n").unwrap();

        let change = workspace.synchronize_paths([foreign]).unwrap();

        assert_eq!(change.lifecycle.changed_sources().collect::<Vec<_>>(), vec![source_id]);
        assert_eq!(change.modules, BTreeSet::from([String::from("Main")]));
        assert_eq!(
            workspace.compilation.snapshot().foreign_file(source_id).map(|file| file.kind()),
            Some(ForeignSourceKind::Jsx)
        );

        let javascript = temporary.path().join("src/Main.js");
        fs::write(&javascript, "export const value = 2;\n").unwrap();
        workspace.synchronize_paths([javascript]).unwrap();
        assert_eq!(workspace.compilation.snapshot().foreign_files(source_id).count(), 2);
    }

    #[test]
    fn ignores_events_when_disk_content_is_unchanged() {
        let (temporary, mut workspace) = workspace();
        let main = temporary.path().join("src/Main.purs");

        let change = workspace.synchronize_paths([main]).unwrap();

        assert!(change.modules.is_empty());
    }

    #[test]
    fn full_rescan_reloads_existing_sources_and_removes_missing_sources() {
        let (temporary, mut workspace) = workspace();
        let main = temporary.path().join("src/Main.purs");
        let original = workspace.compilation.input_source_ids()[0];
        fs::write(&main, "module Main where\n\nvalue = 2\n").unwrap();

        workspace.rescan().unwrap();
        assert_eq!(
            workspace.compilation.snapshot().content(original).unwrap().as_ref(),
            "module Main where\n\nvalue = 2\n"
        );

        fs::remove_file(main).unwrap();
        workspace.rescan().unwrap();
        assert!(workspace.compilation.input_source_ids().is_empty());
    }

    #[test]
    fn excludes_output_from_source_membership() {
        let temporary = TempDir::new().unwrap();
        let root = temporary.path();
        fs::create_dir_all(root.join("output")).unwrap();
        fs::write(root.join("output/Generated.purs"), "module Generated where\n").unwrap();

        let (workspace, _) = WatchWorkspace::new(
            root.to_path_buf(),
            vec![PathBuf::from("**/*.purs")],
            root.join("output"),
        )
        .unwrap();

        assert!(workspace.compilation.input_source_ids().is_empty());
    }

    #[test]
    fn removes_outputs_missing_from_the_latest_successful_build() {
        let (temporary, mut workspace) = workspace();
        let stale = temporary.path().join("output/Stale/index.js");
        let retained = temporary.path().join("output/Main/index.js");
        fs::create_dir_all(stale.parent().unwrap()).unwrap();
        fs::create_dir_all(retained.parent().unwrap()).unwrap();
        fs::write(&stale, "stale").unwrap();
        fs::write(&retained, "retained").unwrap();
        workspace.generated_outputs = BTreeSet::from([stale.clone(), retained.clone()]);

        workspace.reconcile_outputs(BTreeSet::from([retained.clone()])).unwrap();

        assert!(!stale.exists());
        assert!(retained.exists());
        assert_eq!(workspace.generated_outputs, BTreeSet::from([retained]));
    }

    #[test]
    fn rebuilds_after_a_recoverable_output_error() {
        let (temporary, mut workspace) = workspace();
        let output = temporary.path().join("output");
        let config = WatchConfig {
            current_directory: temporary.path().to_path_buf(),
            output: output.clone(),
            inputs: vec![PathBuf::from("src/**/*.purs")],
            quiet: true,
            color: ColorChoice::Never,
        };
        fs::write(&output, "not a directory").unwrap();

        rebuild_or_report(&mut workspace, &config);

        fs::remove_file(&output).unwrap();
        rebuild_or_report(&mut workspace, &config);
        assert!(output.join("Main/index.js").is_file());
    }

    #[test]
    fn watches_the_parent_of_an_existing_input_root() {
        let temporary = TempDir::new().unwrap();
        let input_root = temporary.path().join("src");
        fs::create_dir(&input_root).unwrap();

        assert_eq!(persistent_watch_root(&input_root), temporary.path());
    }
}
