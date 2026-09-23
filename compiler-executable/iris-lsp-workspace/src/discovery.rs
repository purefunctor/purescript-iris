//! Workspace discovery and package source roots.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use iris_build::compile::{InitialBuildConfig, PackageExecution, build_initial};
use iris_build::events::BuildEventSink;
use iris_build::plan::PackageInput;
use itertools::Itertools;
use path_absolutize::Absolutize;
use smol_str::SmolStr;

use crate::preparation::PreparationError;
use crate::state::{PreparedWorkspace, SourceMetadata, SourceRoot};

struct DiscoveredWorkspace {
    root: PathBuf,
    source_globs: Vec<PathBuf>,
    packages: Vec<PackageInput>,
    metadata: BTreeMap<PathBuf, SourceMetadata>,
    source_roots: Vec<SourceRoot>,
}

fn discover_workspace(
    workspace: &iris_build::Workspace,
    client_root: &Path,
) -> Result<DiscoveredWorkspace, PreparationError> {
    let discovered = iris_build::discover_packages(workspace)?;

    let packages = discovered.packages.iter().map(|package| PackageInput {
        name: SmolStr::clone(&package.name),
        source_identities: Vec::clone(&package.files),
        dependencies: Vec::clone(&package.dependencies),
    });

    let packages = packages.collect_vec();

    let metadata = discovered.packages.iter().flat_map(|package| {
        let metadata = SourceMetadata::Package { editable: package.editable };
        package
            .files
            .iter()
            .map(move |file| (PathBuf::clone(file), SourceMetadata::clone(&metadata)))
    });

    let metadata = metadata.collect::<BTreeMap<_, _>>();

    let source_roots = discovered
        .packages
        .iter()
        .map(|package| package_source_roots(&workspace.root, client_root, package));
    let source_root_groups =
        source_roots.process_results(|source_roots| source_roots.collect_vec())?;
    let mut source_roots = source_root_groups.into_iter().flatten().collect_vec();
    source_roots
        .sort_by_key(|source_root| std::cmp::Reverse(source_root.path.components().count()));

    Ok(DiscoveredWorkspace {
        root: PathBuf::clone(&workspace.root),
        source_globs: discovered.source_globs,
        packages,
        metadata,
        source_roots,
    })
}

/// Builds the initial compilation for a discovered workspace.
///
/// This is the blocking half of preparation: it maps discovered packages to the `iris-build`
/// inputs and runs the initial build. It must run on a blocking thread.
pub(crate) fn build_prepared_workspace(
    workspace: iris_build::Workspace,
    client_root: PathBuf,
    events: &dyn BuildEventSink,
) -> Result<PreparedWorkspace, PreparationError> {
    let discovered = discover_workspace(&workspace, &client_root)?;
    let DiscoveredWorkspace { root, source_globs, packages, metadata, source_roots } = discovered;

    let initial = build_initial::<i32, SourceMetadata, _>(InitialBuildConfig {
        root: &root,
        source_globs: &source_globs,
        excluded: &[],
        packages,
        prim_metadata: SourceMetadata::Builtin,
        source_metadata: |path: &Path| {
            metadata
                .get(path)
                .cloned()
                .expect("invariant violated: discovered source has no LSP metadata")
        },
        execution: PackageExecution::Parallel,
        events,
    })?;

    tracing::info!("Loaded {} files.", metadata.len());
    Ok(PreparedWorkspace { compilation: initial.into_compilation(), source_roots })
}

/// The directories whose files belong to `package`, including canonical spellings and aliases
/// under the editor's root, so that a document opened through a symlink gets package metadata.
pub(crate) fn package_source_roots(
    workspace_root: &Path,
    client_root: &Path,
    package: &iris_build::DiscoveredPackage,
) -> io::Result<Vec<SourceRoot>> {
    let metadata = SourceMetadata::Package { editable: package.editable };
    let canonical_client_root = dunce::canonicalize(client_root).ok();

    let mut roots = vec![];
    for root in &package.roots {
        let root = workspace_root.join(root).absolutize()?.to_path_buf();
        roots.push(SourceRoot {
            path: PathBuf::clone(&root),
            metadata: SourceMetadata::clone(&metadata),
        });

        let canonical = dunce::canonicalize(&root).ok();
        if let Some(canonical) = &canonical
            && *canonical != root
        {
            roots.push(SourceRoot {
                path: PathBuf::clone(canonical),
                metadata: SourceMetadata::clone(&metadata),
            });
        }

        if let Some(client_root_canonical) = &canonical_client_root
            && let Some(canonical) = &canonical
            && let Ok(relative) = canonical.strip_prefix(client_root_canonical)
        {
            let alias = client_root.join(relative).absolutize()?.to_path_buf();
            if roots.iter().all(|root| root.path != alias) {
                roots.push(SourceRoot { path: alias, metadata: SourceMetadata::clone(&metadata) });
            }
        }
    }

    Ok(roots)
}
