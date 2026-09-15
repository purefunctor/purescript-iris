use std::path::{Path, PathBuf};

use lsp_types::Url;
use path_absolutize::Absolutize;

use super::error::LspError;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct DocumentPath {
    path: PathBuf,
}

impl DocumentPath {
    pub(super) fn from_uri(uri: &Url) -> Result<DocumentPath, LspError> {
        let path = uri.to_file_path().map_err(|_| LspError::InvalidFileUri(Url::clone(uri)))?;
        if !path.is_absolute() {
            return Err(LspError::InvalidFileUri(Url::clone(uri)));
        }
        DocumentPath::new(&path)
    }

    pub(super) fn new(path: &Path) -> Result<DocumentPath, LspError> {
        if !path.is_absolute() {
            return Err(LspError::PathParseFail(path.to_path_buf()));
        }
        let path = path.absolutize()?.into_owned();

        #[cfg(windows)]
        let path = {
            use std::path::{Component, Prefix};

            let mut components = path.components();
            let mut normalized = PathBuf::new();
            for component in &mut components {
                let Component::Prefix(prefix) = component else {
                    normalized.push(component.as_os_str());
                    continue;
                };

                let (Prefix::Disk(drive) | Prefix::VerbatimDisk(drive)) = prefix.kind() else {
                    normalized.push(component.as_os_str());
                    continue;
                };

                normalized.push(format!("{}:", char::from(drive.to_ascii_uppercase())));
            }
            normalized
        };

        Ok(DocumentPath { path })
    }

    pub(super) fn uri(&self) -> Result<Url, LspError> {
        Url::from_file_path(&self.path)
            .map_err(|_| LspError::PathParseFail(PathBuf::clone(&self.path)))
    }

    pub(super) fn with_extension(&self, extension: &str) -> DocumentPath {
        DocumentPath { path: self.path.with_extension(extension) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_spelling_does_not_change_identity() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("Main File.purs");
        let ordinary = Url::from_file_path(&path).unwrap();
        let encoded = ordinary.as_str().replace("Main", "%4dain");
        let decorated = Url::parse(&format!("{encoded}?view=1#selection")).unwrap();

        let expected = DocumentPath::new(&path).unwrap();
        assert_eq!(DocumentPath::from_uri(&ordinary).unwrap(), expected);
        assert_eq!(DocumentPath::from_uri(&decorated).unwrap(), expected);
        assert_eq!(expected.uri().unwrap(), ordinary);
    }

    #[test]
    fn lexical_components_do_not_require_existing_files() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("missing/.././Main.purs");
        let expected = DocumentPath::new(&directory.path().join("Main.purs")).unwrap();

        assert_eq!(DocumentPath::new(&path).unwrap(), expected);
        let uri = Url::from_file_path(&path).unwrap();
        assert_eq!(DocumentPath::from_uri(&uri).unwrap(), expected);
    }

    #[cfg(windows)]
    #[test]
    fn drive_spelling_does_not_change_identity() {
        let expected = DocumentPath::new(Path::new(r"C:\workspace\Main.purs")).unwrap();
        let uri = Url::parse("file:///c:/workspace/Main.purs").unwrap();

        assert_eq!(DocumentPath::from_uri(&uri).unwrap(), expected);
        assert_eq!(expected.uri().unwrap().as_str(), "file:///C:/workspace/Main.purs");
    }

    #[cfg(unix)]
    #[test]
    fn filesystem_aliases_remain_distinct_documents() {
        let directory = tempfile::tempdir().unwrap();
        let real = directory.path().join("real");
        let link = directory.path().join("link");
        std::fs::create_dir(&real).unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        std::fs::write(real.join("Main.purs"), "").unwrap();

        let real = DocumentPath::new(&real.join("Main.purs")).unwrap();
        let link = DocumentPath::new(&link.join("Main.purs")).unwrap();
        assert_eq!(
            std::fs::canonicalize(&real.path).unwrap(),
            std::fs::canonicalize(&link.path).unwrap()
        );
        assert_ne!(real, link);
        assert_ne!(real.uri().unwrap(), link.uri().unwrap());
    }
}
