//! Source globs and package metadata derived from manifests.
//!
//! Spago hard-codes `src/**/*.purs` for library sources and adds `test/**/*.purs` when test
//! builds are enabled. These helpers produce the corresponding relative globs so traversal in
//! `iris-build` expands exactly the directories Spago would read. Nothing here touches the
//! filesystem.

use std::path::{Path, PathBuf};

use smol_str::SmolStr;

use super::manifest::{ExtraPackage, Package};

/// Library source directory within a package.
pub const SRC_DIRECTORY: &str = "src";

/// Test source directory within a package.
pub const TEST_DIRECTORY: &str = "test";

/// File pattern matched within every source directory.
pub const PURS_GLOB: &str = "**/*.purs";

/// Joins the PureScript glob onto a source directory: `src` becomes `src/**/*.purs`.
pub fn source_glob(directory: &Path) -> PathBuf {
    directory.join(PURS_GLOB)
}

/// Library and test source directories of a package: `src` and `test`.
pub fn package_source_directories() -> [PathBuf; 2] {
    [PathBuf::from(SRC_DIRECTORY), PathBuf::from(TEST_DIRECTORY)]
}

impl Package {
    /// Names of the package library dependencies.
    pub fn core_dependency_names(&self) -> impl Iterator<Item = &SmolStr> + '_ {
        self.dependencies.iter().map(|dependency| &dependency.name)
    }

    /// Names of the package test dependencies.
    pub fn test_dependency_names(&self) -> impl Iterator<Item = &SmolStr> + '_ {
        self.test
            .iter()
            .flat_map(|test| test.dependencies.iter())
            .map(|dependency| &dependency.name)
    }

    /// Names of every dependency, mirroring the flattened lockfile entries Iris reads today.
    pub fn all_dependency_names(&self) -> impl Iterator<Item = &SmolStr> + '_ {
        self.core_dependency_names().chain(self.test_dependency_names())
    }
}

impl ExtraPackage {
    /// Package subdirectory inside a fetched checkout, if any.
    pub fn subdirectory(&self) -> Option<&Path> {
        match self {
            ExtraPackage::Git(package) => package.subdir.as_deref(),
            ExtraPackage::Registry(_) | ExtraPackage::Local(_) | ExtraPackage::Legacy(_) => None,
        }
    }

    /// Names of the extra-package dependencies declared in the workspace manifest.
    pub fn dependency_names(&self) -> Vec<&SmolStr> {
        match self {
            ExtraPackage::Git(package) => {
                package.dependencies.iter().flatten().map(|dependency| &dependency.name).collect()
            }
            ExtraPackage::Legacy(package) => {
                package.dependencies.iter().map(|dependency| &dependency.name).collect()
            }
            ExtraPackage::Registry(_) | ExtraPackage::Local(_) => Vec::new(),
        }
    }

    /// Source directories contributed by an extra package.
    ///
    /// Every dependency only exposes library sources: plain `src`, or
    /// `<subdir>/src` for Git checkouts with a subdirectory.
    pub fn dependency_source_directories(&self) -> Vec<PathBuf> {
        match self {
            ExtraPackage::Git(package) => {
                let src = PathBuf::from(SRC_DIRECTORY);
                vec![package.subdir.as_deref().map_or(src.clone(), |subdir| subdir.join(src))]
            }
            ExtraPackage::Registry(_) | ExtraPackage::Local(_) | ExtraPackage::Legacy(_) => {
                vec![PathBuf::from(SRC_DIRECTORY)]
            }
        }
    }
}
