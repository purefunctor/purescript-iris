//! The output directory, changed only through [`update`].
//!
//! A manifest is a build's record of the files it owns in the output directory. An update writes
//! the files of the next record, removes the files only the previous record names, and replaces
//! the manifest last, so a reader that sees a new manifest sees the files it describes.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{fs, io};

use itertools::Itertools;
use rayon::prelude::*;

use super::compilation::CompilationState;
use super::compile::CompileError;
use super::manifest::{BuildManifest, MANIFEST_FILENAME, ProjectDescription};

/// A file a build writes into the output directory.
pub(crate) struct OutputFile {
    /// The path relative to the output directory, with forward slashes.
    pub path: String,
    pub content: Arc<str>,
}

/// The files a build owns in the output directory, and the manifest that records them.
pub(crate) struct BuildOutput {
    pub manifest: BuildManifest,
    pub files: Vec<OutputFile>,
}

impl BuildOutput {
    pub(crate) fn new(
        project: &ProjectDescription,
        compilation: &CompilationState,
        modules: &[Arc<javascript::Module>],
    ) -> Result<BuildOutput, CompileError> {
        let manifest = BuildManifest::new(project, compilation, modules)?;
        let mut files = Vec::new();
        if manifest.runtime.is_some() {
            files.push(OutputFile {
                path: javascript::runtime_filename().to_owned(),
                content: Arc::from(javascript::runtime_source()),
            });
        }
        for module in modules {
            let name = module.name();
            files.push(OutputFile { path: module.filename(), content: Arc::from(module.source()) });
            if let Some(source) = module.stylex_theme_source() {
                let path = javascript::stylex_theme_module_filename(name);
                files.push(OutputFile { path, content: Arc::from(source) });
            }
            if let Some(kind) = module.foreign_kind() {
                let path = javascript::foreign_module_filename(name, kind);
                let content = compilation.source_foreign_content(module.file_id()).expect(
                    "invariant violated: a generated module requires missing foreign content",
                );
                files.push(OutputFile { path, content });
            }
        }
        Ok(BuildOutput { manifest, files })
    }
}

/// Replaces the files `previous` recorded in `output` with those of `next`.
///
/// The manifest is replaced exactly when the output changed, through a temporary file and a
/// rename. Without `next`, the files `previous` names and the manifest are removed. If a step
/// fails, the files this update created are removed and the files it replaced get their previous
/// content back, and the manifest is left as it was.
pub(crate) fn update(
    output: &Path,
    previous: Option<&BuildManifest>,
    next: Option<&BuildOutput>,
) -> io::Result<()> {
    let mut written = Vec::new();
    let result = apply(output, previous, next, &mut written);
    if result.is_err() {
        for file in &written {
            file.restore();
        }
    }
    result
}

struct WrittenFile {
    path: PathBuf,
    /// The content the file had before, or `None` when this update created it.
    previous: Option<Vec<u8>>,
    changed: bool,
}

impl WrittenFile {
    /// Puts the file back the way it was before the update. Nothing more can be done about a
    /// failure here; the next successful update writes the file again.
    fn restore(&self) {
        if !self.changed {
            return;
        }
        let _ = match &self.previous {
            Some(previous) => fs::write(&self.path, previous),
            None => fs::remove_file(&self.path),
        };
    }
}

fn apply(
    output: &Path,
    previous: Option<&BuildManifest>,
    next: Option<&BuildOutput>,
    written: &mut Vec<WrittenFile>,
) -> io::Result<()> {
    if next.is_some() {
        fs::create_dir_all(output)?;
    }
    let files = next.map(|next| next.files.as_slice()).unwrap_or_default();
    let writes = files.par_iter().map(|file| write_file(output, file));
    let writes = writes.collect::<Vec<_>>();
    let mut failure = None;
    for write in writes {
        match write {
            Ok(file) => written.push(file),
            Err(error) if failure.is_none() => failure = Some(error),
            Err(_) => {}
        }
    }
    if let Some(error) = failure {
        return Err(error);
    }
    let mut changed = written.iter().any(|file| file.changed);

    let next_files = next.map(|next| next.manifest.generated_files()).unwrap_or_default();
    let stale = previous.map(|previous| previous.generated_files()).unwrap_or_default();
    let stale = stale.difference(&next_files).map(|path| output.join(path));
    for path in stale.collect_vec() {
        match fs::remove_file(&path) {
            Ok(()) => changed = true,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        remove_empty_module_directory(output, &path);
    }

    let manifest_path = output.join(MANIFEST_FILENAME);
    let Some(next) = next else {
        return match fs::remove_file(&manifest_path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        };
    };
    let json = next.manifest.to_json();
    if changed || !file_has_content(&manifest_path, json.as_bytes())? {
        let temporary = output.join(format!("{MANIFEST_FILENAME}.{}", std::process::id()));
        fs::write(&temporary, json)?;
        if let Err(error) = fs::rename(&temporary, &manifest_path) {
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }
    }
    Ok(())
}

fn write_file(output: &Path, file: &OutputFile) -> io::Result<WrittenFile> {
    let path = output.join(&file.path);
    let content = file.content.as_bytes();
    let previous = match fs::read(&path) {
        Ok(previous) => Some(previous),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    let changed = previous.as_deref() != Some(content);
    let written = WrittenFile { path, previous, changed };
    if changed {
        let parent =
            written.path.parent().expect("invariant violated: an output file has no parent");
        let result = fs::create_dir_all(parent).and_then(|()| fs::write(&written.path, content));
        if let Err(error) = result {
            // A write that fails partway may have changed the file already.
            written.restore();
            return Err(error);
        }
    }
    Ok(written)
}

/// Removes the module directory of a removed file once nothing is left in it, so that a tool
/// listing the output directory does not take it for a module.
fn remove_empty_module_directory(output: &Path, removed: &Path) {
    if let Some(directory) = removed.parent()
        && directory != output
    {
        let _ = fs::remove_dir(directory);
    }
}

fn file_has_content(path: &Path, content: &[u8]) -> io::Result<bool> {
    match fs::read(path) {
        Ok(previous) => Ok(previous == content),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::{Duration, SystemTime};

    use smol_str::SmolStr;

    use super::*;
    use crate::manifest::{MANIFEST_VERSION, ManifestModule};

    fn manifest(modules: &[(&str, Option<&str>)]) -> BuildManifest {
        let modules = modules.iter().map(|(name, foreign)| {
            let module = ManifestModule {
                package: SmolStr::new("application"),
                source: PathBuf::from(format!("/workspace/src/{name}.purs")),
                javascript: format!("{name}/index.js"),
                foreign: foreign.map(str::to_owned),
                stylex_theme: None,
                dependencies: Vec::new(),
            };
            (SmolStr::new(name), module)
        });
        BuildManifest {
            version: MANIFEST_VERSION,
            iris: "test".to_owned(),
            workspace_root: PathBuf::from("/workspace"),
            runtime: None,
            packages: BTreeMap::new(),
            modules: modules.collect(),
        }
    }

    fn file(path: &str, content: &str) -> OutputFile {
        OutputFile { path: path.to_owned(), content: Arc::from(content) }
    }

    fn read(output: &Path, path: &str) -> Option<String> {
        fs::read_to_string(output.join(path)).ok()
    }

    #[test]
    fn a_failed_update_puts_the_output_back() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path();
        let previous = BuildOutput { manifest: manifest(&[("Main", None)]), files: vec![] };
        fs::create_dir_all(output.join("Main")).unwrap();
        fs::write(output.join("Main/index.js"), "old").unwrap();
        // A directory where the foreign module goes makes its write fail.
        fs::create_dir_all(output.join("Library/foreign.js")).unwrap();
        let next = BuildOutput {
            manifest: manifest(&[("Main", None), ("Library", Some("Library/foreign.js"))]),
            files: vec![
                file("Main/index.js", "new"),
                file("Library/index.js", "library"),
                file("Library/foreign.js", "foreign"),
            ],
        };

        let result = update(output, Some(&previous.manifest), Some(&next));

        assert!(result.is_err());
        assert_eq!(read(output, "Main/index.js").as_deref(), Some("old"));
        assert_eq!(read(output, "Library/index.js"), None);
        assert!(!output.join(MANIFEST_FILENAME).exists());
    }

    #[test]
    fn an_unchanged_output_keeps_the_manifest() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path();
        let next = BuildOutput {
            manifest: manifest(&[("Main", None)]),
            files: vec![file("Main/index.js", "main")],
        };
        update(output, None, Some(&next)).unwrap();
        let manifest_path = output.join(MANIFEST_FILENAME);
        let written_earlier = SystemTime::UNIX_EPOCH + Duration::from_secs(1);
        fs::File::open(&manifest_path).unwrap().set_modified(written_earlier).unwrap();

        update(output, Some(&next.manifest), Some(&next)).unwrap();
        assert_eq!(fs::metadata(&manifest_path).unwrap().modified().unwrap(), written_earlier);

        let edited = BuildOutput {
            manifest: manifest(&[("Main", None)]),
            files: vec![file("Main/index.js", "edited")],
        };
        update(output, Some(&next.manifest), Some(&edited)).unwrap();
        assert_ne!(fs::metadata(&manifest_path).unwrap().modified().unwrap(), written_earlier);
    }

    #[test]
    fn files_only_the_previous_record_names_are_removed_with_their_empty_directories() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path();
        let previous = BuildOutput {
            manifest: manifest(&[("Main", None), ("Extra", Some("Extra/foreign.js"))]),
            files: vec![
                file("Main/index.js", "main"),
                file("Extra/index.js", "extra"),
                file("Extra/foreign.js", "foreign"),
            ],
        };
        update(output, None, Some(&previous)).unwrap();
        fs::write(output.join("Extra/notes.txt"), "kept").unwrap();
        let next = BuildOutput { manifest: manifest(&[("Main", None)]), files: vec![] };

        update(output, Some(&previous.manifest), Some(&next)).unwrap();

        assert_eq!(read(output, "Main/index.js").as_deref(), Some("main"));
        assert_eq!(read(output, "Extra/index.js"), None);
        assert_eq!(read(output, "Extra/foreign.js"), None);
        assert!(output.join("Extra").is_dir(), "a directory with other files stays");

        update(output, Some(&next.manifest), None).unwrap();
        assert_eq!(read(output, "Main/index.js"), None);
        assert!(!output.join("Main").exists());
        assert!(!output.join(MANIFEST_FILENAME).exists());
    }
}
