use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{fs, io, process};

use files::FileId;
use rayon::prelude::*;
use serde::Serialize;
use thiserror::Error;
use url::Url;

use super::compile::ProjectCompilation;

const METADATA_VERSION: u32 = 1;
const METADATA_FILENAME: &str = "metadata.json";
const PACKAGE_FILENAME: &str = "package.json";
const PACKAGE_SOURCE: &str = "{\"type\":\"module\"}\n";

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

pub(crate) struct GeneratedModule {
    pub(crate) javascript: Arc<javascript::Module>,
    pub(crate) source_map: javascript::SourceMap,
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
        modules: &[GeneratedModule],
    ) -> Result<(), OutputError> {
        if modules.iter().any(|module| module.javascript.requires_runtime()) {
            let runtime = self.directory.join(javascript::runtime_filename());
            write_if_changed(&runtime, javascript::runtime_source().as_bytes())?;
        }
        let package = self.directory.join(PACKAGE_FILENAME);
        write_if_changed(&package, PACKAGE_SOURCE.as_bytes())?;
        let module_writes = modules.par_iter().map(|module| self.write_module(compilation, module));
        let module_writes = module_writes.collect::<Vec<_>>();
        module_writes.into_iter().collect()
    }

    fn write_module(
        &self,
        compilation: &ProjectCompilation,
        generated: &GeneratedModule,
    ) -> Result<(), OutputError> {
        let GeneratedModule { javascript: module, source_map } = generated;
        let name = module.name();
        let files = module.files();
        let source = source_path(compilation, module.file_id());
        let index = self.directory.join(module.filename());
        let comment = javascript::source_mapping_url_comment();
        let index_source = format!("{}{comment}", files.index_source());
        write_if_changed(&index, index_source.as_bytes())?;
        let source_map_path = self.directory.join(javascript::source_map_module_filename(name));
        let source_map = source_map.to_json(&source_map_source_url(&source_map_path, &source));
        write_if_changed(&source_map_path, source_map.as_bytes())?;
        if let Some(source) = files.stylex_theme_source() {
            let path = self.directory.join(javascript::stylex_theme_module_filename(name));
            write_if_changed(&path, source.as_bytes())?;
        }
        if let Some(kind) = files.foreign_kind() {
            let path = self.directory.join(javascript::foreign_module_filename(name, kind));
            let foreign = compilation
                .source_foreign_content(module.file_id())
                .expect("invariant violated: generated module requires missing foreign content");
            write_if_changed(&path, foreign.as_bytes())?;
        }
        let path = index.with_file_name(METADATA_FILENAME);
        let metadata = self.module_metadata(compilation, module, source);
        let mut content = serde_json::to_vec_pretty(&metadata)
            .map_err(|source| OutputError::Serialize { path: PathBuf::clone(&path), source })?;
        content.push(b'\n');
        replace_if_changed(&path, &content)
    }

    fn module_metadata<'a>(
        &'a self,
        compilation: &'a ProjectCompilation,
        module: &javascript::Module,
        source: PathBuf,
    ) -> ModuleMetadata<'a> {
        let file_id = module.file_id();
        let package = compilation.source_metadata(file_id).and_then(Option::as_ref);
        let package = package.expect("invariant violated: a generated module has no package");
        ModuleMetadata {
            version: METADATA_VERSION,
            iris: &self.iris,
            package: &package.name,
            editable: package.editable,
            source,
            foreign: module.files().foreign_kind().map(javascript::foreign_filename),
        }
    }
}

fn source_path(compilation: &ProjectCompilation, file_id: FileId) -> PathBuf {
    let source = compilation.source_path(file_id).and_then(|locator| {
        let url = Url::parse(&locator).ok()?;
        url.to_file_path().ok()
    });
    source.expect("invariant violated: a generated module has no source path")
}

fn source_map_source_url(source_map: &Path, source: &Path) -> String {
    let source_url = Url::from_file_path(source)
        .expect("invariant violated: a generated module source path is not absolute");
    let same_volume = source_map.components().next() == source.components().next();
    let relative = Url::from_file_path(source_map)
        .ok()
        .filter(|_| same_volume)
        .and_then(|source_map_url| source_map_url.make_relative(&source_url));
    relative.unwrap_or_else(|| source_url.to_string())
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

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn source_map_sources_on_another_volume_are_absolute() {
        let source_map = Path::new(r"C:\project\output\Main\index.js.map");
        let source = Path::new(r"D:\project\src\Main.purs");
        let source_url = source_map_source_url(source_map, source);
        assert_eq!(source_url, "file:///D:/project/src/Main.purs");
    }
}
