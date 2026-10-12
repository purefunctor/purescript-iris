//! Reading the executable out of a release archive. Archives also carry documentation and license
//! files; only the executable is extracted, so that nothing else in an archive reaches the disk.

use std::io::{self, Write};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ArchiveError {
    #[error("the archive does not contain {entry}")]
    Missing { entry: String },
    #[error("the archive contains more than one {entry}")]
    Duplicate { entry: String },
    #[error("{entry} in the archive is not a regular file")]
    NotRegularFile { entry: String },
    #[error("failed to read the archive: {source}")]
    Read {
        #[from]
        source: io::Error,
    },
    #[cfg(windows)]
    #[error("failed to read the archive: {source}")]
    Zip {
        #[from]
        source: zip::result::ZipError,
    },
}

/// Copies the archive's one `executable` entry to `destination`.
#[cfg(not(windows))]
pub fn extract_executable(
    archive: impl io::Read,
    executable: &str,
    destination: &mut impl Write,
) -> Result<(), ArchiveError> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(archive));
    let mut found = false;
    for entry in archive.entries()? {
        let mut entry = entry?;
        if entry.path_bytes().as_ref() != executable.as_bytes() {
            continue;
        }
        if found {
            return Err(ArchiveError::Duplicate { entry: executable.to_string() });
        }
        if !entry.header().entry_type().is_file() {
            return Err(ArchiveError::NotRegularFile { entry: executable.to_string() });
        }
        io::copy(&mut entry, destination)?;
        found = true;
    }
    if found { Ok(()) } else { Err(ArchiveError::Missing { entry: executable.to_string() }) }
}

/// Copies the archive's one `executable` entry to `destination`.
#[cfg(windows)]
pub fn extract_executable(
    archive: impl io::Read + io::Seek,
    executable: &str,
    destination: &mut impl Write,
) -> Result<(), ArchiveError> {
    let mut archive = zip::ZipArchive::new(archive)?;
    let mut indices = Vec::new();
    for (index, name) in archive.file_names().enumerate() {
        if name? == executable {
            indices.push(index);
        }
    }
    let index = match indices[..] {
        [index] => index,
        [] => return Err(ArchiveError::Missing { entry: executable.to_string() }),
        _ => return Err(ArchiveError::Duplicate { entry: executable.to_string() }),
    };
    let mut entry = archive.by_index(index)?;
    if !entry.is_file() || entry.is_symlink() {
        return Err(ArchiveError::NotRegularFile { entry: executable.to_string() });
    }
    io::copy(&mut entry, destination)?;
    Ok(())
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;

    const EXECUTABLE: &str = "iris-x86_64-unknown-linux-musl/iris";

    enum Entry {
        File(&'static str, &'static [u8]),
        Symlink(&'static str, &'static str),
    }

    fn archive(entries: &[Entry]) -> Vec<u8> {
        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        let mut builder = tar::Builder::new(encoder);
        for entry in entries {
            let mut header = tar::Header::new_gnu();
            match entry {
                Entry::File(path, content) => {
                    header.set_size(content.len() as u64);
                    header.set_mode(0o755);
                    builder.append_data(&mut header, path, *content).unwrap();
                }
                Entry::Symlink(path, target) => {
                    header.set_entry_type(tar::EntryType::Symlink);
                    header.set_size(0);
                    builder.append_link(&mut header, path, target).unwrap();
                }
            }
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    fn extract(entries: &[Entry]) -> Result<Vec<u8>, ArchiveError> {
        let mut destination = Vec::new();
        extract_executable(&archive(entries)[..], EXECUTABLE, &mut destination)?;
        Ok(destination)
    }

    #[test]
    fn rejects_archives_without_exactly_one_regular_executable() {
        let missing = [Entry::File("iris-x86_64-unknown-linux-gnu/iris", b"executable")];
        assert!(matches!(extract(&missing), Err(ArchiveError::Missing { .. })));

        let duplicate = [Entry::File(EXECUTABLE, b"first"), Entry::File(EXECUTABLE, b"second")];
        assert!(matches!(extract(&duplicate), Err(ArchiveError::Duplicate { .. })));

        let symlink = [Entry::Symlink(EXECUTABLE, "/bin/sh")];
        assert!(matches!(extract(&symlink), Err(ArchiveError::NotRegularFile { .. })));
    }
}
