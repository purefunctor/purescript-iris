use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::path::{Path, PathBuf};
use std::{fs, io};

use files::ForeignSourceKind;
use itertools::Itertools;
use path_absolutize::Absolutize;
use rayon::prelude::*;
use serde::Serialize;
use smol_str::SmolStr;
use syntax::cst;
use thiserror::Error;

use super::ownership::SourceOwnership;
use super::packages::{DiscoveredPackages, PackagesError};
use super::walk;

const MANIFEST_VERSION: u32 = 1;

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("failed to normalize package root: {0}")]
    PackageRoot(io::Error),
    #[error("invalid package source glob: {0}")]
    SourceGlob(#[from] globset::Error),
    #[error(transparent)]
    Walk(#[from] walk::Error),
    #[error("failed to read {}: {source}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    Ownership(#[from] PackagesError),
    #[error("module {module} is declared by both {} and {}", first.display(), second.display())]
    DuplicateModule { module: String, first: PathBuf, second: PathBuf },
}

#[derive(Debug, Serialize)]
pub(crate) struct Manifest {
    version: u32,
    iris: String,
    workspace_root: PathBuf,
    packages: BTreeMap<SmolStr, ManifestPackage>,
    modules: LayoutModules,
}

#[derive(Clone, Debug, Serialize)]
struct ManifestPackage {
    editable: bool,
    root: PathBuf,
    dependencies: Vec<SmolStr>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct ManifestModule {
    package: SmolStr,
    source: PathBuf,
    javascript: String,
    foreign: Option<String>,
}

impl ManifestModule {
    pub(crate) fn package(&self) -> &SmolStr {
        &self.package
    }

    pub(crate) fn source(&self) -> &Path {
        &self.source
    }
}

pub(crate) type LayoutModules = BTreeMap<String, ManifestModule>;

pub(crate) struct WorkspaceLayout {
    root: PathBuf,
    source_globs: Vec<PathBuf>,
    packages: BTreeMap<SmolStr, ManifestPackage>,
    ownership: SourceOwnership,
}

impl WorkspaceLayout {
    pub(crate) fn new(
        root: &Path,
        discovered: &DiscoveredPackages,
    ) -> Result<WorkspaceLayout, ManifestError> {
        let packages = discovered.packages.iter().map(|package| {
            let [package_root] = package.roots.as_slice() else {
                unreachable!("invariant violated: a discovered package has exactly one root")
            };
            let package_root = root.join(package_root).absolutize()?.into_owned();
            let dependencies = package.dependencies.iter().cloned().sorted().dedup().collect_vec();
            let entry =
                ManifestPackage { editable: package.editable, root: package_root, dependencies };
            Ok((SmolStr::clone(&package.name), entry))
        });
        let packages = packages.collect::<io::Result<_>>().map_err(ManifestError::PackageRoot)?;
        let ownership = SourceOwnership::new(root, &discovered.packages)?;
        Ok(WorkspaceLayout {
            root: root.to_path_buf(),
            source_globs: discovered.source_globs.clone(),
            packages,
            ownership,
        })
    }

    pub(crate) fn source_globs(&self) -> &[PathBuf] {
        &self.source_globs
    }

    pub(crate) fn ownership(&self) -> &SourceOwnership {
        &self.ownership
    }

    pub(crate) fn modules(&self, output: &Path) -> Result<LayoutModules, ManifestError> {
        let walked = walk::walk_filtered(&self.root, &self.source_globs, [output])?;
        let entries = walked.files.par_iter().map(|path| self.module(path));
        let entries = entries.collect::<Result<Vec<_>, _>>()?;
        let mut modules = LayoutModules::new();
        for (name, module) in entries.into_iter().flatten() {
            match modules.entry(name) {
                Entry::Vacant(entry) => {
                    entry.insert(module);
                }
                Entry::Occupied(entry) => {
                    return Err(ManifestError::DuplicateModule {
                        module: entry.key().clone(),
                        first: entry.get().source.clone(),
                        second: module.source,
                    });
                }
            }
        }
        Ok(modules)
    }

    pub(crate) fn manifest(&self, iris: &str, modules: LayoutModules) -> Manifest {
        Manifest {
            version: MANIFEST_VERSION,
            iris: iris.to_owned(),
            workspace_root: self.root.clone(),
            packages: self.packages.clone(),
            modules,
        }
    }

    fn module(&self, path: &Path) -> Result<Option<(String, ManifestModule)>, ManifestError> {
        let package = self
            .ownership
            .owner(path)?
            .expect("invariant violated: a walked source belongs to no package");
        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(ManifestError::Read { path: path.to_path_buf(), source }),
        };
        let lexed = lexing::lex(&content);
        let tokens = lexing::layout(&lexed);
        let (parsed, _) = parsing::parse(&lexed, &tokens);
        let Some(name) = parsed.module_name(&content) else {
            return Ok(None);
        };
        let declares_foreign = parsed.cst().statements().is_some_and(|statements| {
            let mut declarations = statements.children();
            declarations.any(|declaration| {
                matches!(declaration, cst::Declaration::ForeignImportValueDeclaration(_))
            })
        });
        let foreign = declares_foreign
            .then(|| javascript::foreign_module_filename(&name, foreign_kind(path)));
        let module = ManifestModule {
            package: SmolStr::clone(package),
            source: path.to_path_buf(),
            javascript: javascript::module_filename(&name),
            foreign,
        };
        Ok(Some((name.to_string(), module)))
    }
}

fn foreign_kind(path: &Path) -> ForeignSourceKind {
    let existing = ForeignSourceKind::ALL.into_iter();
    let existing = existing.filter(|kind| path.with_extension(kind.extension()).is_file());
    existing.exactly_one().unwrap_or(ForeignSourceKind::JavaScript)
}
