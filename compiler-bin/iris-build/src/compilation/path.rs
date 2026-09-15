use std::io;
use std::path::{Path, PathBuf};

use building::SourceUnitKey;
use path_absolutize::Absolutize;
use url::Url;

/// A lexical document identity. Filesystem aliases deliberately remain distinct.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct DocumentPath {
    path: PathBuf,
}

impl DocumentPath {
    pub fn new(path: &Path) -> io::Result<DocumentPath> {
        if !path.is_absolute() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "expected an absolute path"));
        }
        let path = dunce::simplified(path).absolutize()?.into_owned();

        #[cfg(windows)]
        let path = {
            use std::path::{Component, Prefix};

            let mut components = path.components();
            let Some(Component::Prefix(prefix)) = components.next() else {
                return Ok(DocumentPath { path });
            };
            let Prefix::Disk(drive) = prefix.kind() else {
                return Ok(DocumentPath { path });
            };

            let mut normalized = PathBuf::from(format!("{}:", drive.to_ascii_uppercase() as char));
            normalized.extend(components);
            normalized
        };

        Ok(DocumentPath { path })
    }

    pub fn from_uri(uri: &Url) -> io::Result<DocumentPath> {
        let path = uri.to_file_path().map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "expected a local file URI")
        })?;
        DocumentPath::new(&path)
    }

    pub fn as_path(&self) -> &Path {
        &self.path
    }

    pub fn uri(&self) -> io::Result<Url> {
        Url::from_file_path(&self.path).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "path cannot be represented as a file URI")
        })
    }

    pub fn source_unit(&self) -> io::Result<SourceUnitKey> {
        let source = DocumentPath::new(&self.path.with_extension("purs"))?.uri()?;
        let javascript = DocumentPath::new(&self.path.with_extension("js"))?.uri()?;
        let jsx = DocumentPath::new(&self.path.with_extension("jsx"))?.uri()?;
        Ok(SourceUnitKey::with_foreign_sources(source.as_str(), javascript.as_str(), jsx.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_spelling_does_not_define_identity() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("Main.purs");
        let expected = DocumentPath::new(&path).unwrap();
        let uri = expected.uri().unwrap();
        let encoded = Url::parse(&uri.as_str().replace("Main", "%4dain")).unwrap();
        assert_eq!(DocumentPath::from_uri(&encoded).unwrap(), expected);
        let dotted = directory.path().join("missing/.././Main.purs");
        assert_eq!(DocumentPath::new(&dotted).unwrap(), expected);
        assert_eq!(DocumentPath::from_uri(&expected.uri().unwrap()).unwrap(), expected);
    }

    #[cfg(windows)]
    #[test]
    fn drive_spelling_does_not_define_identity() {
        let lower =
            DocumentPath::from_uri(&Url::parse("file:///c:/src/Main.purs").unwrap()).unwrap();
        let upper =
            DocumentPath::from_uri(&Url::parse("file:///C:/src/Main.purs").unwrap()).unwrap();
        assert_eq!(lower, upper);
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_remain_distinct() {
        let directory = tempfile::tempdir().unwrap();
        let real = directory.path().join("real");
        let alias = directory.path().join("alias");
        std::fs::create_dir(&real).unwrap();
        std::os::unix::fs::symlink(&real, &alias).unwrap();
        let real = DocumentPath::new(&real.join("Main.purs")).unwrap();
        let alias = DocumentPath::new(&alias.join("Main.purs")).unwrap();
        assert_ne!(real, alias);
        assert_ne!(real.uri().unwrap(), alias.uri().unwrap());
    }
}
