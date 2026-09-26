use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{fs, io};

use ignore::WalkBuilder;
use itertools::Itertools;
use serde::Deserialize;
use thiserror::Error;

const MANIFEST: &str = "spago.yaml";

#[derive(Debug, Error)]
pub enum WorkspaceError {
    #[error("no Spago workspace found from {0}")]
    MissingWorkspace(PathBuf),
    #[error("workspace package '{requested}' was not found; available packages: {available}")]
    UnknownPackage { requested: String, available: String },
    #[error("workspace contains no packages")]
    NoPackages,
    #[error("workspace package selection is ambiguous; use --package <NAME>")]
    AmbiguousPackage,
    #[error("duplicate workspace package name '{name}' in {first} and {second}")]
    DuplicatePackage { name: String, first: PathBuf, second: PathBuf },
    #[error("failed to read {path}: {source}")]
    ReadManifest {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to canonicalize working directory {path}: {source}")]
    CanonicalizeDirectory {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse {path}: {source}")]
    ParseManifest {
        path: PathBuf,
        #[source]
        source: yaml_serde::Error,
    },
    #[error(transparent)]
    Walk(#[from] ignore::Error),
}

#[derive(Deserialize)]
struct Manifest {
    workspace: Option<yaml_serde::Value>,
    package: Option<PackageManifest>,
}

#[derive(Deserialize)]
struct PackageManifest {
    name: String,
}

pub struct Workspace {
    selected: Option<String>,
}

impl Workspace {
    pub fn find_ancestor(current_directory: &Path) -> Result<Option<PathBuf>, WorkspaceError> {
        for directory in current_directory.ancestors().skip(1) {
            let path = directory.join(MANIFEST);
            if path.is_file() && read_manifest(&path)?.workspace.is_some() {
                return Ok(Some(directory.to_path_buf()));
            }
        }
        Ok(None)
    }

    pub fn discover(
        current_directory: &Path,
        requested_package: Option<&str>,
    ) -> Result<Workspace, WorkspaceError> {
        let current_directory = current_directory.canonicalize().map_err(|source| {
            WorkspaceError::CanonicalizeDirectory { path: current_directory.to_path_buf(), source }
        })?;
        let (root, inferred_package) = find_root(&current_directory)?;
        let packages = discover_packages(&root)?;
        if packages.is_empty() {
            return Err(WorkspaceError::NoPackages);
        }

        let selected = if let Some(requested) = requested_package {
            if !packages.contains_key(requested) {
                let available = packages.keys().cloned().collect_vec().join(", ");
                return Err(WorkspaceError::UnknownPackage {
                    requested: requested.to_owned(),
                    available,
                });
            }
            Some(requested.to_owned())
        } else if let Some(inferred) = inferred_package.filter(|name| packages.contains_key(name)) {
            Some(inferred)
        } else if packages.len() == 1 {
            packages.keys().next().cloned()
        } else {
            None
        };

        Ok(Workspace { selected })
    }

    pub fn require_selected(&self) -> Result<&str, WorkspaceError> {
        self.selected.as_deref().ok_or(WorkspaceError::AmbiguousPackage)
    }
}

fn find_root(current_directory: &Path) -> Result<(PathBuf, Option<String>), WorkspaceError> {
    let mut inferred_package = None;
    for directory in current_directory.ancestors() {
        let path = directory.join(MANIFEST);
        if !path.is_file() {
            continue;
        }
        let manifest = read_manifest(&path)?;
        if manifest.workspace.is_some() {
            return Ok((directory.to_path_buf(), inferred_package));
        }
        if inferred_package.is_none()
            && let Some(package) = manifest.package
        {
            inferred_package = Some(package.name);
        }
    }
    Err(WorkspaceError::MissingWorkspace(current_directory.to_path_buf()))
}

fn discover_packages(root: &Path) -> Result<BTreeMap<String, PathBuf>, WorkspaceError> {
    let mut builder = WalkBuilder::new(root);
    builder.hidden(false);
    builder.ignore(false);
    builder.require_git(false);
    builder.git_global(false);
    builder.git_exclude(false);
    builder.parents(false);
    let filter_root = root.to_path_buf();
    builder.filter_entry(move |entry| !excluded_directory(entry.path(), &filter_root));
    let manifests = builder.build().filter_map(manifest_path);
    let mut manifests = manifests.process_results(|entries| entries.collect_vec())?;
    manifests.sort_by_key(|path| path.components().count());

    let mut nested_workspaces = Vec::new();
    let mut packages = BTreeMap::new();
    for path in manifests {
        let package_root =
            path.parent().expect("invariant violated: spago.yaml path has no parent");
        if package_root != root
            && nested_workspaces.iter().any(|nested: &PathBuf| package_root.starts_with(nested))
        {
            continue;
        }

        let manifest = read_manifest(&path)?;
        if package_root != root && manifest.workspace.is_some() {
            nested_workspaces.push(package_root.to_path_buf());
            continue;
        }
        let Some(package) = manifest.package else {
            continue;
        };
        let name = package.name;
        if let Some(first) = packages.insert(String::clone(&name), package_root.to_path_buf()) {
            return Err(WorkspaceError::DuplicatePackage {
                name,
                first,
                second: package_root.to_path_buf(),
            });
        }
    }
    Ok(packages)
}

fn manifest_path(
    entry: Result<ignore::DirEntry, ignore::Error>,
) -> Option<Result<PathBuf, ignore::Error>> {
    let entry = match entry {
        Ok(entry) => entry,
        Err(error) => return Some(Err(error)),
    };
    if !entry.file_type().is_some_and(|file_type| file_type.is_file()) {
        return None;
    }
    if entry.file_name() != MANIFEST {
        return None;
    }
    Some(Ok(entry.into_path()))
}

fn excluded_directory(path: &Path, root: &Path) -> bool {
    if path == root {
        return false;
    }
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some(".git" | ".spago" | "node_modules")
    )
}

fn read_manifest(path: &Path) -> Result<Manifest, WorkspaceError> {
    let source = fs::read_to_string(path)
        .map_err(|source| WorkspaceError::ReadManifest { path: path.to_path_buf(), source })?;
    yaml_serde::from_str(&source)
        .map_err(|source| WorkspaceError::ParseManifest { path: path.to_path_buf(), source })
}
