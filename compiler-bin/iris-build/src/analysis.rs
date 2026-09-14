//! Editor compilation preparation without semantic query warming or transport state.
//!
//! A prepared compilation has discovered disk inputs and authoritative overlays installed.
//! It has not been checked: source diagnostics do not prevent readiness. Consumers may reconcile
//! newer overlays through the compilation lifecycle before issuing their first query.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use std::{io, thread};

use building::{ForeignEvent, LifecycleEvent, SourceEvent, SourceUnitKey};
use command_group::{CommandGroup, GroupChild};
pub use configuration::SourceDiscovery;
use files::ForeignSourceKind;
use path_absolutize::Absolutize;
use thiserror::Error;
use url::Url;

use crate::compilation::{CompilationState, MaterializedPrim};
use crate::compile::{self, CompileError};
use crate::events::{BuildEvent, BuildEventSink, BuildOutcome};
use crate::plan::{BuildPlan, BuildPlanError, PackageInput, SelectedSource};
use crate::walk;

#[derive(Clone, Debug)]
pub struct AnalysisConfig {
    pub root: PathBuf,
    pub sources: SourceDiscovery,
}

#[derive(Clone, Debug)]
pub struct AnalysisOverlay {
    pub uri: Url,
    pub text: Arc<str>,
    pub version: i32,
}

#[derive(Clone, Debug, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> CancellationToken {
        CancellationToken::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    fn check(&self) -> Result<(), AnalysisError> {
        if self.is_cancelled() { Err(AnalysisError::Cancelled) } else { Ok(()) }
    }
}

#[derive(Debug, Error)]
pub enum AnalysisError {
    #[error("analysis preparation cancelled")]
    Cancelled,
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Compile(#[from] CompileError),
    #[error(transparent)]
    Plan(#[from] BuildPlanError),
    #[error(transparent)]
    Spago(#[from] spago::LockfileGlobSetError),
    #[error("source discovery command exited with {status}: {stderr}")]
    CommandFailed { status: ExitStatus, stderr: String },
    #[error("source discovery output is not UTF-8: {0}")]
    CommandOutput(#[from] std::string::FromUtf8Error),
    #[error("failed to convert path to a file URL: {}", .0.display())]
    InvalidPath(PathBuf),
}

#[derive(Clone, Debug)]
pub struct AnalysisSourceRoot {
    pub path: PathBuf,
    pub editable: bool,
}

#[derive(Clone, Debug)]
pub struct AnalysisSelection {
    /// Disk selection only; opening a buffer does not add it to this set.
    pub selected_sources: BTreeSet<Arc<str>>,
    /// Most specific package roots precede their ancestors, including canonical aliases.
    pub source_roots: Vec<AnalysisSourceRoot>,
    root: PathBuf,
    metadata: BTreeMap<PathBuf, bool>,
}

impl AnalysisSelection {
    /// Editable metadata for a supported file document, including buffer-only sources.
    /// Like the editor's open-document policy, files outside known roots participate read-only.
    /// Foreign documents inherit the metadata of their associated PureScript source.
    pub fn metadata(&self, uri: &Url) -> Option<bool> {
        let path = uri.to_file_path().ok()?;
        if ![".purs", ".js", ".jsx"].iter().any(|extension| uri.path().ends_with(extension)) {
            return None;
        }
        let source = if uri.path().ends_with(".purs") { path } else { path.with_extension("purs") };
        if let Some(editable) = self.metadata.get(&source) {
            return Some(*editable);
        }
        let package = self.source_roots.iter().find(|root| source.starts_with(&root.path));
        Some(package.map_or_else(|| source.starts_with(&self.root), |root| root.editable))
    }
}

pub struct PreparedAnalysis {
    pub compilation: CompilationState<i32, bool>,
    pub selection: AnalysisSelection,
}

pub fn prepare(
    config: &AnalysisConfig,
    overlays: &[AnalysisOverlay],
    prim: Arc<MaterializedPrim>,
    cancellation: &CancellationToken,
    events: &impl BuildEventSink,
) -> Result<PreparedAnalysis, AnalysisError> {
    cancellation.check()?;
    let started = Instant::now();
    events.send(BuildEvent::Preparing);
    let root = config.root.absolutize()?.into_owned();
    let (selection, packages) = discover(&root, &config.sources, cancellation)?;
    cancellation.check()?;
    let selected = selection.metadata.keys().map(|path| {
        let identity = dunce::canonicalize(path)?;
        Ok(SelectedSource { path: path.clone(), identity })
    });
    let selected = selected.collect::<Result<Vec<_>, io::Error>>()?;
    let packages = packages.into_iter().map(|package| {
        let identities = package.source_identities.iter().map(dunce::canonicalize);
        let source_identities = identities.collect::<Result<Vec<_>, _>>()?;
        Ok(PackageInput { source_identities, ..package })
    });
    let packages = packages.collect::<Result<Vec<_>, io::Error>>()?;
    let plan = BuildPlan::new(selected, packages)?;
    events.send(BuildEvent::PlanReady { package_count: plan.package_count() });

    let mut compilation = CompilationState::new(prim, false);
    let source_paths = plan.packages().flat_map(|package| package.source_paths.iter().cloned());
    let mut source_paths = source_paths.collect::<BTreeSet<_>>();
    for overlay in overlays {
        cancellation.check()?;
        let Some(metadata) = selection.metadata(&overlay.uri) else {
            continue;
        };
        let is_source = overlay.uri.path().ends_with(".purs");
        let source_uri =
            if is_source { overlay.uri.clone() } else { sibling_uri(&overlay.uri, "purs") };
        let source = source_uri.to_file_path().expect("supported document has a file path");
        let javascript_uri = sibling_uri(&source_uri, "js");
        let jsx_uri = sibling_uri(&source_uri, "jsx");
        let unit = SourceUnitKey::with_foreign_sources(
            source_uri.as_str(),
            javascript_uri.as_str(),
            jsx_uri.as_str(),
        );
        let text = Arc::clone(&overlay.text);
        let version = overlay.version;
        let event = if is_source {
            source_paths.insert(source);
            LifecycleEvent::Source { unit, event: SourceEvent::Opened { text, version, metadata } }
        } else {
            let kind = if overlay.uri.path().ends_with(".js") {
                ForeignSourceKind::JavaScript
            } else {
                ForeignSourceKind::Jsx
            };
            LifecycleEvent::Foreign { unit, kind, event: ForeignEvent::Opened { text, version } }
        };
        compilation.apply(event);
    }
    for path in source_paths {
        cancellation.check()?;
        let uri = file_uri(&path)?;
        let metadata = selection.metadata(&uri).expect("source selection must have metadata");
        compile::load_source(&mut compilation, &path, metadata)?;
    }
    cancellation.check()?;
    let outcome = if compilation.source_ids().next().is_some() {
        BuildOutcome::Succeeded
    } else {
        BuildOutcome::NoInputs
    };
    events.send(BuildEvent::Finished { duration: started.elapsed(), outcome });
    Ok(PreparedAnalysis { compilation, selection })
}

fn discover(
    root: &Path,
    sources: &SourceDiscovery,
    cancellation: &CancellationToken,
) -> Result<(AnalysisSelection, Vec<PackageInput>), AnalysisError> {
    let mut metadata = BTreeMap::new();
    let mut source_roots = vec![];
    let mut packages = vec![];
    match sources {
        SourceDiscovery::Spago {} => {
            for (name, package) in spago::source_files_by_package(root)? {
                cancellation.check()?;
                let editable = matches!(
                    package.reference,
                    spago::PackageReference::Workspace | spago::PackageReference::Local
                );
                for path in &package.sources {
                    metadata.insert(path.clone(), editable);
                }
                for path in package.roots {
                    let path = root.join(path).absolutize()?.into_owned();
                    source_roots.push(AnalysisSourceRoot { path: path.clone(), editable });
                    if let Ok(canonical) = dunce::canonicalize(&path)
                        && canonical != path
                    {
                        source_roots.push(AnalysisSourceRoot { path: canonical, editable });
                    }
                }
                packages.push(PackageInput {
                    name,
                    source_identities: package.sources,
                    dependencies: package.dependencies.into_iter().collect(),
                });
            }
        }
        SourceDiscovery::Command { program, arguments } => {
            let output = source_command(root, program, arguments, cancellation)?;
            let walked = walk::walk_filtered(root, output.lines(), std::iter::empty::<&Path>())
                .map_err(CompileError::from)?;
            let files = walked
                .files
                .into_iter()
                .filter(|path| path.extension().is_some_and(|extension| extension == "purs"));
            let files = files.collect::<Vec<_>>();
            for path in &files {
                metadata.insert(path.clone(), path.starts_with(root));
            }
            source_roots.push(AnalysisSourceRoot { path: root.to_path_buf(), editable: true });
            packages.push(PackageInput {
                name: "unmanaged".into(),
                source_identities: files,
                dependencies: vec![],
            });
        }
    }
    source_roots.sort_by_key(|root| std::cmp::Reverse(root.path.components().count()));
    let locators = metadata.keys().map(|path| file_uri(path).map(|uri| Arc::from(uri.as_str())));
    let selected_sources = locators.collect::<Result<_, _>>()?;
    let selection =
        AnalysisSelection { selected_sources, source_roots, root: root.to_path_buf(), metadata };
    Ok((selection, packages))
}

fn file_uri(path: &Path) -> Result<Url, AnalysisError> {
    Url::from_file_path(path).map_err(|_| AnalysisError::InvalidPath(path.to_path_buf()))
}

fn sibling_uri(uri: &Url, extension: &str) -> Url {
    let path = uri.path();
    let file_name_start = path.rfind('/').map_or(0, |index| index + 1);
    let extension_start = path[file_name_start..]
        .rfind('.')
        .filter(|index| *index > 0)
        .map_or(path.len(), |index| file_name_start + index);
    let mut sibling = uri.clone();
    sibling.set_path(&format!("{}.{extension}", &path[..extension_start]));
    sibling
}

struct SourceCommand(GroupChild);

impl Drop for SourceCommand {
    fn drop(&mut self) {
        // The leader may exit while descendants still run. Keep group ownership until cleanup.
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn source_command(
    root: &Path,
    program: &str,
    arguments: &[String],
    cancellation: &CancellationToken,
) -> Result<String, AnalysisError> {
    cancellation.check()?;
    let mut stdout = tempfile::tempfile()?;
    let mut stderr = tempfile::tempfile()?;
    let child = Command::new(program)
        .args(arguments)
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(stdout.try_clone()?)
        .stderr(stderr.try_clone()?)
        .group_spawn()?;
    let mut child = SourceCommand(child);
    let status = loop {
        cancellation.check()?;
        if let Some(status) = child.0.try_wait()? {
            break status;
        }
        thread::sleep(Duration::from_millis(10));
    };
    drop(child);
    cancellation.check()?;
    if !status.success() {
        stderr.seek(SeekFrom::Start(0))?;
        let mut output = vec![];
        stderr.read_to_end(&mut output)?;
        return Err(AnalysisError::CommandFailed {
            status,
            stderr: String::from_utf8_lossy(&output).into_owned(),
        });
    }
    stdout.seek(SeekFrom::Start(0))?;
    let mut output = vec![];
    stdout.read_to_end(&mut output)?;
    Ok(String::from_utf8(output)?)
}
