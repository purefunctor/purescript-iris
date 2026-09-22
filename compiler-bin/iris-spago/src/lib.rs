//! Spago manifest parsing and package metadata for Iris.
//!
//! This crate owns the `spago.yaml` schema shared by `iris-build` and `iris-lsp`. It parses
//! manifests into package metadata and produces the source globs that file traversal expands.
//! Filesystem traversal itself lives with its consumers, notably `iris-build`.
//!
//! It also owns the Spago command invocation shared by `iris-build` and `iris-package`. Spago
//! selects registry package sets and fetches dependencies; source discovery uses package manifests
//! and fetched sources after Spago records the exact resolution.

pub mod command;
pub mod manifest;
pub mod registry;
pub mod sources;

pub use command::{SpagoCommand, SpagoError};

pub use manifest::{
    Dependency, ExecutionConfig, ExtraPackage, GitPackage, LegacyPackage, LocalPackage, Manifest,
    ManifestError, Package, SetAddress, TestConfig, Workspace, parse_manifest, read_manifest,
};
pub use registry::{RegistryManifest, parse_registry_manifest};
pub use sources::{
    PURS_GLOB, SRC_DIRECTORY, TEST_DIRECTORY, package_source_directories, source_glob,
};

/// PureScript release whose package ecosystem Iris supports.
pub const COMPILER_VERSION: &str = "0.15.15";

/// Name of the Spago manifest file discovered in workspace directories.
pub const MANIFEST_FILE: &str = "spago.yaml";
