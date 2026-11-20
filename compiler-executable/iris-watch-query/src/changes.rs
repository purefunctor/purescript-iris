use std::path::{Path, PathBuf};
use std::{fs, io};

use iris_analysis::refactor::{SourceEdit, apply_edits};
use similar::TextDiff;
use thiserror::Error;

use crate::{RefactorAnswer, RefactorEdit, sha256_hex};

#[derive(Debug, Error)]
pub enum RefactorChangeError {
    #[error("could not read {}: {source}", .path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("refusing to modify {path}: file changed on disk since the watcher snapshot")]
    Changed { path: String },
    #[error("failed to apply edits to {path}: {message}")]
    Edits { path: String, message: String },
    #[error("could not inspect {}: {source}", .path.display())]
    Inspect {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("refusing to replace symlink {}", .path.display())]
    Symlink { path: PathBuf },
    #[error("could not write {}: {source}", .path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

pub struct RefactorChanges {
    files: Vec<ChangedFile>,
}

struct ChangedFile {
    disk_path: PathBuf,
    path: String,
    original: String,
    modified: String,
}

impl RefactorChanges {
    pub fn prepare(
        root: &Path,
        answer: &RefactorAnswer,
    ) -> Result<RefactorChanges, RefactorChangeError> {
        let mut files = Vec::with_capacity(answer.files.len());
        for file in &answer.files {
            let disk_path = root.join(&file.path);
            let original = fs::read_to_string(&disk_path).map_err(|source| {
                let path = PathBuf::clone(&disk_path);
                RefactorChangeError::Read { path, source }
            })?;
            if sha256_hex(&original) != file.sha256 {
                let path = String::clone(&file.path);
                return Err(RefactorChangeError::Changed { path });
            }
            let modified = apply_file_edits(&original, &file.edits).map_err(|message| {
                let path = String::clone(&file.path);
                RefactorChangeError::Edits { path, message }
            })?;
            let path = String::clone(&file.path);
            files.push(ChangedFile { disk_path, path, original, modified });
        }
        Ok(RefactorChanges { files })
    }

    pub fn patch(&self) -> String {
        let patches = self
            .files
            .iter()
            .map(|file| render_unified_diff(&file.path, &file.original, &file.modified));
        patches.collect()
    }

    pub fn write(&self) -> Result<(), RefactorChangeError> {
        for file in &self.files {
            let metadata = fs::symlink_metadata(&file.disk_path).map_err(|source| {
                let path = PathBuf::clone(&file.disk_path);
                RefactorChangeError::Inspect { path, source }
            })?;
            if metadata.file_type().is_symlink() {
                let path = PathBuf::clone(&file.disk_path);
                return Err(RefactorChangeError::Symlink { path });
            }
        }
        for file in &self.files {
            fs::write(&file.disk_path, &file.modified).map_err(|source| {
                let path = PathBuf::clone(&file.disk_path);
                RefactorChangeError::Write { path, source }
            })?;
        }
        Ok(())
    }
}

fn apply_file_edits(source: &str, edits: &[RefactorEdit]) -> Result<String, String> {
    let source_edits = edits.iter().map(|edit| SourceEdit {
        start_byte: edit.start_byte,
        end_byte: edit.end_byte,
        new_text: String::clone(&edit.new_text),
    });
    let source_edits = source_edits.collect::<Vec<_>>();
    apply_edits(source, &source_edits).map_err(|error| error.to_string())
}

fn render_unified_diff(path: &str, original: &str, modified: &str) -> String {
    let diff = TextDiff::from_lines(original, modified);
    let mut unified = diff.unified_diff();
    unified.context_radius(3);
    unified.header(&format!("a/{path}"), &format!("b/{path}"));
    unified.to_string()
}
