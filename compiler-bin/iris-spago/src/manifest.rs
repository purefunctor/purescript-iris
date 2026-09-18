//! Spago manifest (`spago.yaml`) parsing.
//!
//! The schema follows Spago v1: a manifest holds an optional `package` section and an
//! optional `workspace` section. Sections that Iris never consumes, such as `publish`,
//! `backend`, and `buildOpts`, are intentionally unmodeled; serde ignores them so newer
//! Spago options keep parsing without a crate change.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{fs, io};

use serde::{Deserialize, Deserializer};
use smol_str::SmolStr;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("failed to read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_yml::Error,
    },
}

/// Parses a `spago.yaml` manifest from its text.
pub fn parse_manifest(source: &str) -> Result<Manifest, serde_yml::Error> {
    serde_yml::from_str(source)
}

/// Reads and parses the `spago.yaml` manifest at the given path.
pub fn read_manifest(path: &Path) -> Result<Manifest, ManifestError> {
    let source = fs::read_to_string(path)
        .map_err(|source| ManifestError::Read { path: path.to_path_buf(), source })?;
    serde_yml::from_str(&source)
        .map_err(|source| ManifestError::Parse { path: path.to_path_buf(), source })
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Manifest {
    #[serde(default)]
    pub package: Option<Package>,
    #[serde(default)]
    pub workspace: Option<Workspace>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Package {
    pub name: SmolStr,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default, deserialize_with = "deserialize_dependencies")]
    pub dependencies: Vec<Dependency>,
    #[serde(default)]
    pub run: Option<ExecutionConfig>,
    #[serde(default)]
    pub test: Option<TestConfig>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionConfig {
    pub main: Option<String>,
    #[serde(default)]
    pub exec_args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestConfig {
    pub main: String,
    #[serde(default, deserialize_with = "deserialize_dependencies")]
    pub dependencies: Vec<Dependency>,
    #[serde(default)]
    pub exec_args: Vec<String>,
}

/// A package dependency: either a bare name or a name with a Registry constraint.
///
/// ```yaml
/// dependencies:
///   - prelude
///   - effect: ">=4.0.0 <5.0.0"
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    pub name: SmolStr,
    pub constraint: Option<SmolStr>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum DependencyEntry {
    Name(SmolStr),
    Constraints(BTreeMap<SmolStr, SmolStr>),
}

fn deserialize_dependencies<'de, DeserializerT>(
    deserializer: DeserializerT,
) -> Result<Vec<Dependency>, DeserializerT::Error>
where
    DeserializerT: Deserializer<'de>,
{
    Vec::<DependencyEntry>::deserialize(deserializer).map(flatten_dependencies)
}

fn deserialize_optional_dependencies<'de, DeserializerT>(
    deserializer: DeserializerT,
) -> Result<Option<Vec<Dependency>>, DeserializerT::Error>
where
    DeserializerT: Deserializer<'de>,
{
    Option::<Vec<DependencyEntry>>::deserialize(deserializer)
        .map(|entries| entries.map(flatten_dependencies))
}

fn flatten_dependencies(entries: Vec<DependencyEntry>) -> Vec<Dependency> {
    let mut dependencies = vec![];
    for entry in entries {
        match entry {
            DependencyEntry::Name(name) => dependencies.push(Dependency { name, constraint: None }),
            DependencyEntry::Constraints(constraints) => {
                dependencies.extend(
                    constraints.into_iter().map(|(name, constraint)| Dependency {
                        name,
                        constraint: Some(constraint),
                    }),
                );
            }
        }
    }
    dependencies
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    #[serde(default)]
    pub package_set: Option<SetAddress>,
    #[serde(default)]
    pub extra_packages: BTreeMap<SmolStr, ExtraPackage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum SetAddress {
    Registry {
        registry: SmolStr,
    },
    Remote {
        url: SmolStr,
        #[serde(default)]
        hash: Option<SmolStr>,
    },
    Local {
        path: PathBuf,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum ExtraPackage {
    Registry(SmolStr),
    Git(GitPackage),
    Local(LocalPackage),
    Legacy(LegacyPackage),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct GitPackage {
    pub git: SmolStr,
    #[serde(rename = "ref")]
    pub reference: SmolStr,
    #[serde(default)]
    pub subdir: Option<PathBuf>,
    #[serde(default, deserialize_with = "deserialize_optional_dependencies")]
    pub dependencies: Option<Vec<Dependency>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LocalPackage {
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LegacyPackage {
    pub repo: SmolStr,
    pub version: SmolStr,
    #[serde(default, deserialize_with = "deserialize_dependencies")]
    pub dependencies: Vec<Dependency>,
}
