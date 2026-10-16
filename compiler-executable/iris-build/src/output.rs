use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{fs, io, process};

use rayon::prelude::*;
use serde::Serialize;
use thiserror::Error;
use url::Url;

use super::compile::ProjectCompilation;

const METADATA_VERSION: u32 = 1;
const METADATA_FILENAME: &str = "metadata.json";

#[derive(Debug, Error)]
pub enum OutputError {
    #[error("failed to write {}: {source}", path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to serialize {}: {source}", path.display())]
    Serialize {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
}

#[derive(Serialize)]
struct ModuleMetadata<'a> {
    version: u32,
    iris: &'a str,
    package: &'a str,
    editable: bool,
    source: PathBuf,
    foreign: Option<String>,
}

pub(crate) struct BuildOutput {
    directory: PathBuf,
    iris: String,
}

impl BuildOutput {
    pub(crate) fn new(directory: PathBuf, iris: String) -> BuildOutput {
        BuildOutput { directory, iris }
    }

    pub(crate) fn directory(&self) -> &Path {
        &self.directory
    }

    pub(crate) fn write(
        &self,
        compilation: &ProjectCompilation,
        modules: &[Arc<javascript::Module>],
    ) -> Result<(), OutputError> {
        if modules.iter().any(|module| module.requires_runtime()) {
            let runtime = self.directory.join(javascript::runtime_filename());
            write_if_changed(&runtime, javascript::runtime_source().as_bytes())?;
        }
        let module_writes = modules.par_iter().map(|module| self.write_module(compilation, module));
        let module_writes = module_writes.collect::<Vec<_>>();
        module_writes.into_iter().collect()
    }

    fn write_module(
        &self,
        compilation: &ProjectCompilation,
        module: &javascript::Module,
    ) -> Result<(), OutputError> {
        let name = module.name();
        let index = self.directory.join(module.filename());
        write_if_changed(&index, module.source().as_bytes())?;
        if let Some(source) = module.stylex_theme_source() {
            let path = self.directory.join(javascript::stylex_theme_module_filename(name));
            write_if_changed(&path, source.as_bytes())?;
        }
        if let Some(kind) = module.foreign_kind() {
            let path = self.directory.join(javascript::foreign_module_filename(name, kind));
            let foreign = compilation
                .source_foreign_content(module.file_id())
                .expect("invariant violated: generated module requires missing foreign content");
            write_if_changed(&path, foreign.as_bytes())?;
        }
        let path = index.with_file_name(METADATA_FILENAME);
        let metadata = self.module_metadata(compilation, module);
        let mut content = serde_json::to_vec_pretty(&metadata)
            .map_err(|source| OutputError::Serialize { path: PathBuf::clone(&path), source })?;
        content.push(b'\n');
        replace_if_changed(&path, &content)
    }

    fn module_metadata<'a>(
        &'a self,
        compilation: &'a ProjectCompilation,
        module: &javascript::Module,
    ) -> ModuleMetadata<'a> {
        let file_id = module.file_id();
        let package = compilation.source_metadata(file_id).and_then(Option::as_ref);
        let package = package.expect("invariant violated: a generated module has no package");
        let source = compilation.source_path(file_id).and_then(|locator| {
            let url = Url::parse(&locator).ok()?;
            url.to_file_path().ok()
        });
        let source = source.expect("invariant violated: a generated module has no source path");
        ModuleMetadata {
            version: METADATA_VERSION,
            iris: &self.iris,
            package: &package.name,
            editable: package.editable,
            source,
            foreign: module.foreign_kind().map(javascript::foreign_filename),
        }
    }
}

fn write_if_changed(path: &Path, content: &[u8]) -> Result<(), OutputError> {
    write_changed_with(path, content, |path, content| fs::write(path, content))
}

fn replace_if_changed(path: &Path, content: &[u8]) -> Result<(), OutputError> {
    write_changed_with(path, content, replace)
}

fn write_changed_with(
    path: &Path,
    content: &[u8],
    write: impl FnOnce(&Path, &[u8]) -> io::Result<()>,
) -> Result<(), OutputError> {
    let failure = |source| OutputError::Write { path: path.to_path_buf(), source };
    match fs::read(path) {
        Ok(previous) if previous == content => return Ok(()),
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let parent = path.parent().expect("invariant violated: an output file has no parent");
            fs::create_dir_all(parent).map_err(failure)?;
        }
        Err(error) => return Err(failure(error)),
    }
    write(path, content).map_err(failure)
}

fn replace(path: &Path, content: &[u8]) -> io::Result<()> {
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(format!(".{}.tmp", process::id()));
    let temporary = PathBuf::from(temporary);
    let replaced = fs::write(&temporary, content).and_then(|()| fs::rename(&temporary, path));
    if replaced.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    replaced
}
