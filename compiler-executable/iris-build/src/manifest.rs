//! The versioned build manifest written beside generated modules.
//!
//! Tools such as bundlers read the manifest to learn which package owns each generated module
//! without asking a running watcher. See `documentation/build-manifest.md` for the format.

use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::{fs, io};

use files::FileId;
use itertools::Itertools;
use serde::Serialize;
use smol_str::SmolStr;
use url::Url;

use super::compilation::CompilationState;
use super::compile::CompileError;

/// The schema version, increased whenever a change could break an existing reader.
pub const MANIFEST_VERSION: u32 = 1;

pub const MANIFEST_FILENAME: &str = "iris-manifest.json";

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BuildManifest {
    pub version: u32,
    /// The version of Iris that wrote the manifest.
    pub iris: SmolStr,
    /// The absolute path of the Spago workspace root.
    pub workspace_root: String,
    /// The runtime module relative to the output directory, if a generated module requires it.
    pub runtime: Option<String>,
    pub packages: BTreeMap<SmolStr, ManifestPackage>,
    pub modules: BTreeMap<String, ManifestModule>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ManifestPackage {
    /// Whether the package is a workspace or local package rather than a fetched dependency.
    pub editable: bool,
    /// The absolute package directory.
    pub root: String,
    pub dependencies: Vec<SmolStr>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ManifestModule {
    pub package: SmolStr,
    /// The absolute path of the PureScript source.
    pub source: String,
    /// The generated module relative to the output directory.
    pub javascript: String,
    /// The copied foreign module relative to the output directory.
    pub foreign: Option<String>,
    /// The generated StyleX theme module relative to the output directory.
    pub stylex_theme: Option<String>,
    /// The modules that the generated module imports.
    pub dependencies: Vec<String>,
}

/// What the manifest records about a discovered package, which build planning does not need.
#[derive(Clone, Debug)]
pub(crate) struct PackageMetadata {
    pub name: SmolStr,
    pub editable: bool,
    /// The absolute package directory.
    pub root: PathBuf,
    pub dependencies: Vec<SmolStr>,
}

/// The facts a build knows besides its generated modules.
pub(crate) struct ManifestInputs<'a> {
    pub version: &'a SmolStr,
    pub packages: &'a [PackageMetadata],
    /// The package that owns each source path.
    pub source_packages: &'a BTreeMap<PathBuf, SmolStr>,
}

impl BuildManifest {
    pub(crate) fn new(
        root: &Path,
        inputs: &ManifestInputs<'_>,
        compilation: &CompilationState,
        modules: &[Arc<javascript::Module>],
    ) -> Result<BuildManifest, CompileError> {
        let packages = inputs.packages.iter().map(|package| {
            let root = absolute_path(&package.root)?;
            let dependencies = package.dependencies.iter().cloned().sorted().dedup().collect_vec();
            let editable = package.editable;
            Ok((SmolStr::clone(&package.name), ManifestPackage { editable, root, dependencies }))
        });
        let packages = packages.collect::<Result<BTreeMap<_, _>, CompileError>>()?;

        let module_names = modules.iter().map(|module| (module.file_id(), module.name()));
        let module_names = module_names.collect::<HashMap<_, _>>();
        let manifest_modules = modules.iter().map(|module| {
            let name = module.name();
            let source = source_path(compilation, module.file_id())?;
            let package = inputs
                .source_packages
                .get(&source)
                .expect("invariant violated: a generated module's source has no package");
            let dependencies = module.dependencies().iter().map(|&dependency| {
                if let Some(name) = module_names.get(&dependency) {
                    return Ok(name.to_string());
                }
                // A resilient build writes a module even when a dependency failed to generate.
                let locator = compilation
                    .source_path(dependency)
                    .expect("invariant violated: an imported module has no lifecycle path");
                let name = compilation.module_name(&locator)?;
                Ok(name.expect("invariant violated: an imported module has no name"))
            });
            let dependencies = dependencies.collect::<Result<Vec<_>, CompileError>>()?;
            let dependencies = dependencies.into_iter().sorted().dedup().collect_vec();
            let foreign =
                module.foreign_kind().map(|kind| javascript::foreign_module_filename(name, kind));
            let stylex_theme = module
                .stylex_theme_source()
                .map(|_| javascript::stylex_theme_module_filename(name));
            let manifest_module = ManifestModule {
                package: SmolStr::clone(package),
                source: absolute_path(&source)?,
                javascript: module.filename(),
                foreign,
                stylex_theme,
                dependencies,
            };
            Ok((name.to_owned(), manifest_module))
        });
        let modules_by_name = manifest_modules.collect::<Result<BTreeMap<_, _>, CompileError>>()?;

        let requires_runtime = modules.iter().any(|module| module.requires_runtime());
        let runtime = requires_runtime.then(|| javascript::runtime_filename().to_owned());
        Ok(BuildManifest {
            version: MANIFEST_VERSION,
            iris: SmolStr::clone(inputs.version),
            workspace_root: absolute_path(root)?,
            runtime,
            packages,
            modules: modules_by_name,
        })
    }

    /// Replaces the manifest in `output` unless it is unchanged, so a reader never observes a
    /// partially written file. Returns the manifest's path.
    pub(crate) fn write(&self, output: &Path) -> io::Result<PathBuf> {
        let path = output.join(MANIFEST_FILENAME);
        let mut content = serde_json::to_string_pretty(self)
            .expect("invariant violated: the build manifest is not serializable");
        content.push('\n');
        match fs::read(&path) {
            Ok(previous) if previous == content.as_bytes() => return Ok(path),
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        fs::create_dir_all(output)?;
        let mut temporary = tempfile::NamedTempFile::new_in(output)?;
        temporary.write_all(content.as_bytes())?;
        temporary.persist(&path).map_err(|error| error.error)?;
        Ok(path)
    }
}

fn source_path(compilation: &CompilationState, file_id: FileId) -> Result<PathBuf, CompileError> {
    let locator = compilation
        .source_path(file_id)
        .expect("invariant violated: a generated module has no lifecycle path");
    let path = Url::parse(&locator).ok().and_then(|url| url.to_file_path().ok());
    path.ok_or_else(|| CompileError::InvalidPath(PathBuf::from(&*locator)))
}

/// The absolute `path` with forward slashes on every platform, which JavaScript tools accept on
/// Windows too. The path must be normalized.
fn absolute_path(path: &Path) -> Result<String, CompileError> {
    let failure = || CompileError::ManifestPath(path.to_path_buf());
    let mut prefix = String::new();
    let mut segments = Vec::new();
    for component in path.components() {
        match component {
            Component::Prefix(component) => {
                let component = component.as_os_str().to_str().ok_or_else(failure)?;
                prefix.push_str(&component.replace('\\', "/"));
            }
            Component::RootDir => {}
            Component::Normal(segment) => segments.push(segment.to_str().ok_or_else(failure)?),
            Component::CurDir | Component::ParentDir => return Err(failure()),
        }
    }
    if !path.has_root() {
        return Err(failure());
    }
    Ok(format!("{prefix}/{}", segments.join("/")))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn absolute_paths_use_forward_slashes() {
        let absolute = |path| absolute_path(Path::new(path)).ok();
        assert_eq!(absolute(r"C:\workspace\app").as_deref(), Some("C:/workspace/app"));
        assert_eq!(absolute(r"\\server\share\app").as_deref(), Some("//server/share/app"));
    }
}
