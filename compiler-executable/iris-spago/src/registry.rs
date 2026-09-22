//! Manifests for fetched registry packages.
//!
//! Registry packages arrive under `.spago` without a `spago.yaml`. Instead,
//! each checkout carries a `purs.json` file that records the package name,
//! the resolved version, and dependency version ranges. Only the dependency
//! names matter for source discovery.

use std::collections::BTreeMap;

use serde::Deserialize;
use smol_str::SmolStr;

/// A parsed `purs.json` manifest from a fetched registry package.
#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct RegistryManifest {
    pub name: SmolStr,
    pub version: SmolStr,
    pub dependencies: BTreeMap<SmolStr, SmolStr>,
}

impl RegistryManifest {
    /// Iterates over the names of the transitive dependencies.
    pub fn dependency_names(&self) -> impl Iterator<Item = &SmolStr> {
        self.dependencies.keys()
    }
}

/// Parses the contents of a `purs.json` manifest.
pub fn parse_registry_manifest(contents: &str) -> Result<RegistryManifest, serde_json::Error> {
    serde_json::from_str(contents)
}
