use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{fs, io};

use rayon::prelude::*;
use thiserror::Error;

use super::compilation::CompilationState;
use super::manifest::Manifest;

const MANIFEST_FILENAME: &str = "manifest.json";

#[derive(Debug, Error)]
pub enum OutputError {
    #[error("failed to write {}: {source}", path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to serialize the build manifest: {0}")]
    Serialize(#[source] serde_json::Error),
    #[error("{error}; failed to remove the build manifest {} as well: {removal}", path.display())]
    StaleManifest { error: Box<OutputError>, path: PathBuf, removal: io::Error },
}

pub(crate) struct BuildOutput {
    directory: PathBuf,
}

impl BuildOutput {
    pub(crate) fn new(directory: PathBuf) -> BuildOutput {
        BuildOutput { directory }
    }

    pub(crate) fn directory(&self) -> &Path {
        &self.directory
    }

    pub(crate) fn write(
        &self,
        compilation: &CompilationState,
        modules: &[Arc<javascript::Module>],
        manifest: &Manifest,
    ) -> Result<(), OutputError> {
        let mut content = serde_json::to_vec_pretty(manifest).map_err(OutputError::Serialize)?;
        content.push(b'\n');
        let written = write_modules(compilation, modules, &self.directory)
            .and_then(|changed| self.write_manifest(&content, changed));
        written.map_err(|error| self.remove_manifest(error))
    }

    fn write_manifest(&self, content: &[u8], files_changed: bool) -> Result<(), OutputError> {
        let path = self.directory.join(MANIFEST_FILENAME);
        if !files_changed && fs::read(&path).is_ok_and(|previous| previous == content) {
            return Ok(());
        }
        let failure = |source| OutputError::Write { path: PathBuf::clone(&path), source };
        fs::create_dir_all(&self.directory).map_err(failure)?;
        let mut builder = tempfile::Builder::new();
        builder.prefix(".manifest-").suffix(".json");
        #[cfg(unix)]
        builder.permissions(std::os::unix::fs::PermissionsExt::from_mode(0o666));
        let mut temporary = builder.tempfile_in(&self.directory).map_err(failure)?;
        temporary.write_all(content).map_err(failure)?;
        temporary.persist(&path).map_err(|error| failure(error.error))?;
        Ok(())
    }

    fn remove_manifest(&self, error: OutputError) -> OutputError {
        let path = self.directory.join(MANIFEST_FILENAME);
        match fs::remove_file(&path) {
            Ok(()) => error,
            Err(removal) if removal.kind() == io::ErrorKind::NotFound => error,
            Err(removal) => OutputError::StaleManifest { error: Box::new(error), path, removal },
        }
    }
}

fn write_modules(
    compilation: &CompilationState,
    modules: &[Arc<javascript::Module>],
    output: &Path,
) -> Result<bool, OutputError> {
    let mut changed = false;
    if modules.iter().any(|module| module.requires_runtime()) {
        let runtime = output.join(javascript::runtime_filename());
        changed |= write_if_changed(&runtime, javascript::runtime_source().as_bytes())?;
    }
    let module_writes = modules.par_iter().map(|module| write_module(compilation, module, output));
    let module_writes = module_writes.collect::<Vec<_>>();
    for module_changed in module_writes {
        changed |= module_changed?;
    }
    Ok(changed)
}

fn write_module(
    compilation: &CompilationState,
    module: &javascript::Module,
    output: &Path,
) -> Result<bool, OutputError> {
    let name = module.name();
    let index = output.join(module.filename());
    let mut changed = write_if_changed(&index, module.source().as_bytes())?;
    if let Some(source) = module.stylex_theme_source() {
        let path = output.join(javascript::stylex_theme_module_filename(name));
        changed |= write_if_changed(&path, source.as_bytes())?;
    }
    if let Some(kind) = module.foreign_kind() {
        let path = output.join(javascript::foreign_module_filename(name, kind));
        let foreign = compilation
            .source_foreign_content(module.file_id())
            .expect("invariant violated: generated module requires missing foreign content");
        changed |= write_if_changed(&path, foreign.as_bytes())?;
    }
    Ok(changed)
}

fn write_if_changed(path: &Path, content: &[u8]) -> Result<bool, OutputError> {
    let failure = |source| OutputError::Write { path: path.to_path_buf(), source };
    match fs::read(path) {
        Ok(previous) if previous == content => return Ok(false),
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let parent = path.parent().expect("invariant violated: an output file has no parent");
            fs::create_dir_all(parent).map_err(failure)?;
        }
        Err(error) => return Err(failure(error)),
    }
    fs::write(path, content).map_err(failure)?;
    Ok(true)
}
