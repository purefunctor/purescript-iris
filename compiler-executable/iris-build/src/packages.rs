//! Package source discovery from manifests and fetched checkouts.
//!
//! After `spago fetch` populates `.spago`, the lockfile records each external
//! package's location and dependencies. Discovery walks the dependency closure
//! from the selected workspace packages and synthesizes the `src` and `test`
//! globs Spago itself reads. Workspace package dependencies come from their
//! manifests; external package dependencies come from Spago's resolution.

use std::collections::{BTreeMap, VecDeque};
use std::path::{Component, Path, PathBuf};
use std::{fs, io};

use itertools::Itertools;
use serde::Deserialize;
use smol_str::SmolStr;
use thiserror::Error;
use unicode_general_category::{GeneralCategory, get_general_category};

use super::workspace::Workspace;

#[derive(Debug, Error)]
pub enum PackagesError {
    #[error("failed to read Spago resolution {path}: {source}")]
    ReadResolution {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse Spago resolution {path}: {source}")]
    ParseResolution {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error(
        "Spago resolution contains no entry for package '{name}'; run `spago fetch` to update spago.lock"
    )]
    MissingPackageResolution { name: SmolStr },
    #[error("no fetched sources for package '{name}'; run `spago fetch` to download dependencies")]
    UnfetchedPackage { name: SmolStr },
    #[error("local package '{name}' has no directory at {path}")]
    MissingLocalPackage { name: SmolStr, path: PathBuf },
    #[error("git package '{name}' has unsafe subdirectory {subdirectory}")]
    UnsafeGitSubdirectory { name: SmolStr, subdirectory: PathBuf },
    #[error("git package '{name}' subdirectory {subdirectory} resolves outside its checkout")]
    EscapedGitSubdirectory { name: SmolStr, subdirectory: PathBuf },
    #[error("source file {path} is claimed by packages '{first}' and '{second}'")]
    ConflictingSource { path: PathBuf, first: SmolStr, second: SmolStr },
    #[error("failed to canonicalize package path {path}: {source}")]
    CanonicalizePath {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    Walk(#[from] super::walk::Error),
}

/// A single package with its discovered sources.
#[derive(Debug)]
pub struct DiscoveredPackage {
    /// The package name as declared in manifests.
    pub name: SmolStr,
    /// The PureScript files owned by this package.
    pub files: Vec<PathBuf>,
    /// The names of this package's dependencies.
    pub dependencies: Vec<SmolStr>,
    /// Whether the sources are editable project files rather than fetched checkouts.
    pub editable: bool,
    /// The package locations relative to the workspace root.
    pub roots: Vec<PathBuf>,
}

/// Every package in the dependency closure with the globs that find them.
#[derive(Debug)]
pub struct DiscoveredPackages {
    /// Source globs relative to the workspace root.
    pub source_globs: Vec<PathBuf>,
    /// The discovered packages in dependency-closure order.
    pub packages: Vec<DiscoveredPackage>,
}

#[derive(Debug, Deserialize)]
struct Resolution {
    packages: BTreeMap<SmolStr, ResolvedDependency>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum ResolvedDependency {
    Git { rev: SmolStr, subdir: Option<PathBuf>, dependencies: Vec<SmolStr> },
    Local { path: PathBuf, dependencies: Vec<SmolStr> },
    Registry { version: SmolStr, dependencies: Vec<SmolStr> },
}

/// Discovers the dependency closure of the selected workspace packages.
///
/// Without a selection, the closure covers every workspace package. Every
/// returned file is assigned to exactly one package; overlapping or
/// unassignable sources are reported instead of silently misattributed.
///
/// Discovery requires the dependencies recorded by `spago fetch` to be
/// present; missing resolution or fetched sources are reported as errors so
/// that consumers never analyze a partially installed project.
pub fn discover_packages(workspace: &Workspace) -> Result<DiscoveredPackages, PackagesError> {
    let resolution = read_resolution(&workspace.root)?;

    let mut discovered = BTreeMap::new();
    let mut queue = VecDeque::new();
    if let Some(selected) = workspace.selected.as_deref() {
        queue.push_back((SmolStr::new(selected), true));
    } else {
        queue.extend(workspace.packages.keys().map(|name| (SmolStr::new(name), true)));
    }
    while let Some((name, include_test_dependencies)) = queue.pop_front() {
        if discovered.contains_key(&name) {
            continue;
        }
        let resolved =
            resolve_package(workspace, resolution.as_ref(), &name, include_test_dependencies)?;
        queue.extend(resolved.dependencies.iter().cloned().map(|name| (name, false)));
        discovered.insert(SmolStr::clone(&name), resolved);
    }

    let mut source_globs = vec![];
    let mut owners: BTreeMap<PathBuf, SmolStr> = BTreeMap::new();
    let mut files: BTreeMap<SmolStr, Vec<PathBuf>> = BTreeMap::new();
    for resolved in discovered.values() {
        let mut package_globs = vec![];
        for directory in &resolved.source_directories {
            let relative = directory.strip_prefix(&workspace.root).unwrap_or(directory);
            package_globs.push(relative.join(iris_spago::PURS_GLOB));
        }
        source_globs.extend(package_globs.iter().cloned());
        let walked =
            super::walk::walk_filtered(&workspace.root, package_globs, Vec::<PathBuf>::new())?;
        for file in walked.files {
            if let Some(owner) =
                owners.insert(PathBuf::clone(&file), SmolStr::clone(&resolved.name))
                && owner != resolved.name
            {
                return Err(PackagesError::ConflictingSource {
                    path: file,
                    first: owner,
                    second: SmolStr::clone(&resolved.name),
                });
            }
            files.entry(SmolStr::clone(&resolved.name)).or_default().push(file);
        }
    }

    let packages = discovered
        .into_values()
        .map(|resolved| DiscoveredPackage {
            files: files.remove(&resolved.name).unwrap_or_default(),
            dependencies: resolved.dependencies,
            editable: resolved.editable,
            roots: vec![resolved.relative],
            name: resolved.name,
        })
        .collect_vec();
    Ok(DiscoveredPackages { source_globs, packages })
}

struct ResolvedPackage {
    name: SmolStr,
    relative: PathBuf,
    source_directories: Vec<PathBuf>,
    dependencies: Vec<SmolStr>,
    editable: bool,
}

fn resolve_package(
    workspace: &Workspace,
    resolution: Option<&Resolution>,
    name: &SmolStr,
    include_test_dependencies: bool,
) -> Result<ResolvedPackage, PackagesError> {
    if let Some(package) = workspace.packages.get(name.as_str()) {
        let mut source_directories = vec![package.root.join(iris_spago::SRC_DIRECTORY)];
        let mut dependencies = package.manifest.core_dependency_names().cloned().collect_vec();
        if package.has_tests {
            source_directories.push(package.root.join(iris_spago::TEST_DIRECTORY));
        }
        if package.has_tests && include_test_dependencies {
            dependencies.extend(package.manifest.test_dependency_names().cloned());
        }
        return Ok(ResolvedPackage {
            relative: relative_location(&workspace.root, &package.root),
            source_directories,
            dependencies,
            editable: true,
            name: SmolStr::clone(name),
        });
    }
    let locked = resolution
        .and_then(|resolution| resolution.packages.get(name))
        .ok_or_else(|| PackagesError::MissingPackageResolution { name: SmolStr::clone(name) })?;
    match locked {
        ResolvedDependency::Git { rev, subdir, dependencies } => {
            let checkout = git_checkout_location(workspace, name, rev)?;
            let location = git_package_location(name, &checkout, subdir.as_deref())?;
            let source_directories = vec![location.join(iris_spago::SRC_DIRECTORY)];
            Ok(ResolvedPackage {
                relative: relative_location(&workspace.root, &location),
                source_directories,
                dependencies: Vec::clone(dependencies),
                editable: false,
                name: SmolStr::clone(name),
            })
        }
        ResolvedDependency::Local { path, dependencies } => {
            let location = workspace.root.join(path);
            if !location.is_dir() {
                return Err(PackagesError::MissingLocalPackage {
                    name: SmolStr::clone(name),
                    path: location,
                });
            }
            let source_directories = vec![location.join(iris_spago::SRC_DIRECTORY)];
            Ok(ResolvedPackage {
                relative: relative_location(&workspace.root, &location),
                source_directories,
                dependencies: Vec::clone(dependencies),
                editable: true,
                name: SmolStr::clone(name),
            })
        }
        ResolvedDependency::Registry { version, dependencies } => {
            resolve_registry_package(workspace, name, version, dependencies)
        }
    }
}

fn resolve_registry_package(
    workspace: &Workspace,
    name: &SmolStr,
    version: &SmolStr,
    dependencies: &[SmolStr],
) -> Result<ResolvedPackage, PackagesError> {
    let packages_directory = workspace.root.join(".spago").join("p");
    let location = packages_directory.join(format!("{name}-{version}"));
    if !location.is_dir() {
        return Err(PackagesError::UnfetchedPackage { name: SmolStr::clone(name) });
    }

    let source_directories = vec![location.join(iris_spago::SRC_DIRECTORY)];
    Ok(ResolvedPackage {
        relative: relative_location(&workspace.root, &location),
        source_directories,
        dependencies: dependencies.to_vec(),
        editable: false,
        name: SmolStr::clone(name),
    })
}

fn git_checkout_location(
    workspace: &Workspace,
    name: &SmolStr,
    reference: &SmolStr,
) -> Result<PathBuf, PackagesError> {
    let package_directory = workspace.root.join(".spago").join("p").join(name.as_str());
    if !package_directory.is_dir() {
        return Err(PackagesError::UnfetchedPackage { name: SmolStr::clone(name) });
    }
    let declared = package_directory.join(escape_path_component(reference));
    if declared.is_dir() {
        return Ok(declared);
    }
    Err(PackagesError::UnfetchedPackage { name: SmolStr::clone(name) })
}

fn read_resolution(root: &Path) -> Result<Option<Resolution>, PackagesError> {
    let path = root.join("spago.lock");
    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(PackagesError::ReadResolution { path, source }),
    };
    let resolution = serde_json::from_str(&contents)
        .map_err(|source| PackagesError::ParseResolution { path, source })?;
    Ok(Some(resolution))
}

fn git_package_location(
    name: &SmolStr,
    checkout: &Path,
    subdirectory: Option<&Path>,
) -> Result<PathBuf, PackagesError> {
    let Some(subdirectory) = subdirectory else {
        return Ok(checkout.to_path_buf());
    };
    if !is_safe_subdirectory(subdirectory) {
        return Err(PackagesError::UnsafeGitSubdirectory {
            name: SmolStr::clone(name),
            subdirectory: subdirectory.to_path_buf(),
        });
    }

    let location = checkout.join(subdirectory);
    let canonical_checkout = canonicalize_path(checkout)?;
    let canonical_location = canonicalize_path(&location)?;
    if !canonical_location.starts_with(&canonical_checkout) {
        return Err(PackagesError::EscapedGitSubdirectory {
            name: SmolStr::clone(name),
            subdirectory: subdirectory.to_path_buf(),
        });
    }
    Ok(location)
}

fn is_safe_subdirectory(subdirectory: &Path) -> bool {
    !subdirectory.as_os_str().is_empty()
        && subdirectory.components().all(|component| matches!(component, Component::Normal(_)))
}

fn escape_path_component(value: &str) -> String {
    let mut escaped = String::new();
    for character in value.chars() {
        let category = get_general_category(character);
        if matches!(category, GeneralCategory::LowercaseLetter)
            || character.is_ascii_digit()
            || matches!(character, '.' | ',' | '-' | '+')
        {
            escaped.push(character);
        } else if character == '_' {
            escaped.push_str("_-");
        } else if matches!(
            category,
            GeneralCategory::TitlecaseLetter | GeneralCategory::UppercaseLetter
        ) {
            escaped.push('_');
            let [lowercase, _] = unicode_case_mapping::to_lowercase(character);
            let lowercase = char::from_u32(lowercase)
                .filter(|lowercase| *lowercase != '\0')
                .unwrap_or(character);
            escaped.push(lowercase);
        } else {
            match character {
                '/' => escaped.push_str("%s"),
                '\\' => escaped.push_str("%b"),
                ':' => escaped.push_str("%c"),
                '@' => escaped.push_str("%a"),
                '~' => escaped.push_str("%t"),
                '*' => escaped.push_str("%r"),
                '?' => escaped.push_str("%q"),
                '"' => escaped.push_str("%d"),
                '<' => escaped.push_str("%l"),
                '>' => escaped.push_str("%g"),
                '|' => escaped.push_str("%p"),
                ' ' => escaped.push_str("%w"),
                '%' => escaped.push_str("%%"),
                _ => escaped.push_str(&format!("%{:x}", u32::from(character))),
            }
        }
    }
    escaped
}

fn relative_location(root: &Path, location: &Path) -> PathBuf {
    location.strip_prefix(root).map(Path::to_path_buf).unwrap_or_else(|_| location.to_path_buf())
}

fn canonicalize_path(path: &Path) -> Result<PathBuf, PackagesError> {
    dunce::canonicalize(path)
        .map_err(|source| PackagesError::CanonicalizePath { path: path.to_path_buf(), source })
}
