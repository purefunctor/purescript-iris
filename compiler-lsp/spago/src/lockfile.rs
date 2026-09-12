use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use globset::Glob;
use itertools::Itertools;
use rustc_hash::FxHashMap;
use serde::Deserialize;
use smol_str::SmolStr;
use walkdir::WalkDir;

#[derive(Debug, Deserialize)]
pub struct Lockfile {
    pub workspace: Workspace,
    pub packages: Packages,
}

#[derive(Debug, Deserialize)]
pub struct Workspace {
    pub packages: FxHashMap<SmolStr, WorkspacePackage>,

    #[serde(default)]
    pub extra_packages: FxHashMap<SmolStr, ExtraPackage>,
}

#[derive(Debug, Deserialize)]
pub struct WorkspacePackage {
    pub path: PathBuf,
    #[serde(default)]
    pub core: PackageDependencies,
    #[serde(default)]
    pub test: PackageDependencies,
}

#[derive(Debug, Default, Deserialize)]
pub struct PackageDependencies {
    #[serde(default)]
    dependencies: Vec<Dependency>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Dependency {
    Named(SmolStr),
    Versioned(BTreeMap<SmolStr, serde_json::Value>),
}

#[derive(Debug, Deserialize)]
pub struct ExtraPackage {
    pub subdir: Option<PathBuf>,

    /// Present in Spago lockfile schema for some extra-packages (e.g. local/path-based).
    /// Not currently used by `Lockfile::sources()`.
    pub path: Option<PathBuf>,
}

pub type Packages = FxHashMap<SmolStr, PackageEntry>;

pub type PackagesBySource = BTreeMap<SmolStr, PackageSources>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageSources {
    pub reference: PackageReference,
    pub roots: Vec<PathBuf>,
    pub sources: Vec<PathBuf>,
    pub dependencies: BTreeSet<SmolStr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageReference {
    Workspace,
    Git { url: Option<SmolStr>, rev: SmolStr, version: SmolStr, subdir: Option<PathBuf> },
    Local,
    Registry { version: SmolStr },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PackageEntry {
    Git {
        #[serde(default)]
        url: Option<SmolStr>,
        rev: SmolStr,
        #[serde(default)]
        subdir: Option<PathBuf>,
        #[serde(default)]
        dependencies: Vec<SmolStr>,
    },
    Local {
        path: SmolStr,
        #[serde(default)]
        dependencies: Vec<SmolStr>,
    },
    Registry {
        version: SmolStr,
        #[serde(default)]
        dependencies: Vec<SmolStr>,
    },
}

impl Lockfile {
    pub fn sources(&self) -> impl Iterator<Item = PathBuf> {
        self.sources_by_package().into_values().flat_map(|package| package.sources)
    }

    pub fn sources_by_package(&self) -> PackagesBySource {
        let mut packages = PackagesBySource::new();

        for (name, package) in &self.workspace.packages {
            let roots = vec![PathBuf::clone(&package.path)];
            let sources = vec![package.path.join("src"), package.path.join("test")];
            let dependencies = package
                .core
                .dependencies
                .iter()
                .chain(&package.test.dependencies)
                .flat_map(Dependency::names)
                .cloned()
                .collect();
            packages.insert(
                SmolStr::clone(name),
                PackageSources {
                    reference: PackageReference::Workspace,
                    roots,
                    sources,
                    dependencies,
                },
            );
        }

        let base = Path::new(".spago").join("p");

        for (name, package) in &self.packages {
            let mut roots = vec![];
            let mut sources = vec![];
            let reference = match package {
                PackageEntry::Git { url, rev, subdir, .. } => {
                    let root = base.join(name).join(rev);
                    roots.push(PathBuf::clone(&root));
                    sources.push(root.join("src"));
                    sources.push(root.join("test"));

                    let subdir = subdir.as_ref().or_else(|| {
                        self.workspace
                            .extra_packages
                            .get(name)
                            .and_then(|extra_package| extra_package.subdir.as_ref())
                    });

                    let subdir = subdir.filter(|subdir| is_safe_subdir(subdir));

                    if let Some(subdir) = subdir {
                        let root = base.join(name).join(rev).join(subdir);
                        roots.push(PathBuf::clone(&root));
                        sources.push(root.join("src"));
                        sources.push(root.join("test"));
                    }

                    PackageReference::Git {
                        url: url.clone(),
                        rev: SmolStr::clone(rev),
                        version: SmolStr::clone(rev),
                        subdir: subdir.cloned(),
                    }
                }
                PackageEntry::Local { path, .. } => {
                    let root = Path::new(path).to_path_buf();
                    roots.push(PathBuf::clone(&root));
                    sources.push(root.join("src"));
                    sources.push(root.join("test"));
                    PackageReference::Local
                }
                PackageEntry::Registry { version, .. } => {
                    let name_version = format!("{name}-{version}");
                    let root = base.join(&name_version);
                    roots.push(PathBuf::clone(&root));
                    sources.push(root.join("src"));
                    sources.push(root.join("test"));
                    PackageReference::Registry { version: SmolStr::clone(version) }
                }
            };

            let dependencies = match package {
                PackageEntry::Git { dependencies, .. }
                | PackageEntry::Local { dependencies, .. }
                | PackageEntry::Registry { dependencies, .. } => {
                    dependencies.iter().cloned().collect()
                }
            };

            packages.insert(
                SmolStr::clone(name),
                PackageSources { reference, roots, sources, dependencies },
            );
        }

        packages
    }

    pub fn walk(&self, root: impl AsRef<Path>) -> impl Iterator<Item = PathBuf> {
        self.sources().filter_map(with_root(root)).flat_map(find_purs_files)
    }

    pub fn walk_by_package(&self, root: impl AsRef<Path>) -> PackagesBySource {
        let root = root.as_ref();

        let packages = self.sources_by_package().into_iter().map(|(name, package)| {
            let sources = package
                .sources
                .into_iter()
                .filter_map(with_root(root))
                .flat_map(find_purs_files)
                .collect_vec();
            (name, PackageSources { sources, ..package })
        });

        packages.collect()
    }
}

impl Dependency {
    fn names(&self) -> Vec<&SmolStr> {
        match self {
            Dependency::Named(name) => vec![name],
            Dependency::Versioned(packages) => packages.keys().collect_vec(),
        }
    }
}

fn with_root(root: impl AsRef<Path>) -> impl Fn(PathBuf) -> Option<PathBuf> {
    move |source| root.as_ref().join(source).canonicalize().ok()
}

fn is_safe_subdir(subdir: &Path) -> bool {
    if subdir.as_os_str().is_empty() {
        return false;
    }

    // Treat lockfile paths as untrusted input: only allow "normal" relative paths.
    !subdir.components().any(|component| match component {
        Component::Prefix(_) | Component::RootDir | Component::ParentDir | Component::CurDir => {
            true
        }
        Component::Normal(_) => false,
    })
}

fn find_purs_files(root: PathBuf) -> impl Iterator<Item = PathBuf> {
    let matcher = Glob::new("**/*.purs").unwrap().compile_matcher();
    WalkDir::new(root).into_iter().filter_map(move |entry| {
        let entry = entry.ok()?;
        let path = entry.path();
        if matcher.is_match(path) { Some(path.to_path_buf()) } else { None }
    })
}
