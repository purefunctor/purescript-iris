//! The versioned build manifest written beside generated modules.
//!
//! Tools such as bundlers read the manifest to learn which package owns each generated module,
//! where its source is, and which modules it imports, without asking a running watcher. See
//! `documentation/build-manifest.md` for the format and the promises made to readers.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use itertools::Itertools;
use path_absolutize::Absolutize;
use serde::{Serialize, Serializer};
use smol_str::SmolStr;
use url::Url;

use super::compilation::{CompilationState, SourceOwner};
use super::compile::CompileError;
use super::packages::DiscoveredPackage;

/// The schema version, increased whenever a change could break an existing reader.
pub const MANIFEST_VERSION: u32 = 1;

pub const MANIFEST_FILENAME: &str = "iris-manifest.json";

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BuildManifest {
    pub version: u32,
    /// The version of the Iris that wrote the manifest.
    pub iris: String,
    #[serde(serialize_with = "serialize_path")]
    pub workspace_root: PathBuf,
    /// The runtime module relative to the output directory, when a generated module needs it.
    pub runtime: Option<String>,
    pub packages: BTreeMap<SmolStr, ManifestPackage>,
    pub modules: BTreeMap<SmolStr, ManifestModule>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ManifestPackage {
    /// Whether the sources are project files rather than a fetched checkout.
    pub editable: bool,
    #[serde(serialize_with = "serialize_path")]
    pub root: PathBuf,
    pub dependencies: Vec<SmolStr>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ManifestModule {
    pub package: SmolStr,
    #[serde(serialize_with = "serialize_path")]
    pub source: PathBuf,
    /// The generated module relative to the output directory.
    pub javascript: String,
    /// The copied foreign module relative to the output directory.
    pub foreign: Option<String>,
    /// The generated StyleX theme module relative to the output directory.
    pub stylex_theme: Option<String>,
    /// The modules the generated module imports.
    pub dependencies: Vec<SmolStr>,
}

/// What a manifest records about the project itself, which is the same for every build of it.
#[derive(Clone, Debug)]
pub struct ProjectDescription {
    pub iris: String,
    pub workspace_root: PathBuf,
    pub packages: BTreeMap<SmolStr, ManifestPackage>,
}

impl ProjectDescription {
    pub fn new(
        iris: String,
        workspace_root: &Path,
        packages: &[DiscoveredPackage],
    ) -> ProjectDescription {
        let packages = packages.iter().map(|package| {
            let root = workspace_root
                .join(&package.root)
                .absolutize()
                .expect("invariant violated: an absolute path needs no working directory")
                .into_owned();
            let dependencies = package.dependencies.iter().cloned().sorted().dedup();
            let manifest_package = ManifestPackage {
                editable: package.editable,
                root,
                dependencies: dependencies.collect_vec(),
            };
            (SmolStr::clone(&package.name), manifest_package)
        });
        ProjectDescription {
            iris,
            workspace_root: workspace_root.to_path_buf(),
            packages: packages.collect(),
        }
    }
}

impl BuildManifest {
    /// The manifest of a build that generated `modules`.
    pub(crate) fn new(
        project: &ProjectDescription,
        compilation: &CompilationState,
        modules: &[Arc<javascript::Module>],
    ) -> Result<BuildManifest, CompileError> {
        let manifest_modules = modules.iter().map(|module| {
            let name = module.name();
            let package = match compilation.source_metadata(module.file_id()) {
                Some(SourceOwner::Package(package)) => SmolStr::clone(package),
                Some(SourceOwner::Builtin) | None => {
                    unreachable!("invariant violated: a generated module has no package")
                }
            };
            let dependencies = module
                .dependencies()
                .iter()
                .map(|dependency| SmolStr::clone(&dependency.module_name));
            let manifest_module = ManifestModule {
                package,
                source: source_path(compilation, module)?,
                javascript: module.filename(),
                foreign: module
                    .foreign_kind()
                    .map(|kind| javascript::foreign_module_filename(name, kind)),
                stylex_theme: module
                    .stylex_theme_source()
                    .map(|_| javascript::stylex_theme_module_filename(name)),
                dependencies: dependencies.sorted().dedup().collect_vec(),
            };
            Ok((SmolStr::new(name), manifest_module))
        });
        let modules_by_name = manifest_modules.collect::<Result<BTreeMap<_, _>, CompileError>>()?;
        let requires_runtime = modules.iter().any(|module| module.requires_runtime());
        Ok(BuildManifest {
            version: MANIFEST_VERSION,
            iris: String::clone(&project.iris),
            workspace_root: PathBuf::clone(&project.workspace_root),
            runtime: requires_runtime.then(|| javascript::runtime_filename().to_owned()),
            packages: BTreeMap::clone(&project.packages),
            modules: modules_by_name,
        })
    }

    /// Every generated file the manifest names, relative to the output directory.
    pub fn generated_files(&self) -> BTreeSet<&str> {
        let module_files = self.modules.values().flat_map(|module| {
            [Some(&module.javascript), module.foreign.as_ref(), module.stylex_theme.as_ref()]
        });
        let files = self.runtime.iter().chain(module_files.flatten());
        files.map(String::as_str).collect()
    }

    pub fn to_json(&self) -> String {
        let mut json = serde_json::to_string_pretty(self)
            .expect("invariant violated: the build manifest is not serializable");
        json.push('\n');
        json
    }
}

fn source_path(
    compilation: &CompilationState,
    module: &javascript::Module,
) -> Result<PathBuf, CompileError> {
    let locator = compilation
        .source_path(module.file_id())
        .expect("invariant violated: a generated module has no lifecycle path");
    let path = Url::parse(&locator).ok().and_then(|url| url.to_file_path().ok());
    path.ok_or_else(|| CompileError::InvalidPath(PathBuf::from(&*locator)))
}

/// Paths use forward slashes on every platform, which JavaScript tools accept on Windows too.
fn serialize_path<S: Serializer>(path: &Path, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&path.to_string_lossy().replace('\\', "/"))
}
