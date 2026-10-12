//! Replacing the running executable.
//!
//! The new executable is staged beside the current one, so that it can run from a directory that
//! allows execution, unlike a `noexec` temporary directory, and so that replacing the current
//! executable is a rename within one file system.

use std::env::consts::EXE_SUFFIX;
use std::env::{self};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

use tempfile::{NamedTempFile, TempPath};
use thiserror::Error;

use crate::release::Release;

#[derive(Debug, Error)]
pub enum InstallError {
    #[error("failed to locate the running Iris: {source}")]
    CurrentExecutable {
        #[source]
        source: io::Error,
    },
    #[error("cannot write to {}; update Iris with the permissions it was installed with", .directory.display())]
    PermissionDenied { directory: PathBuf },
    #[error("failed to stage the new Iris in {}: {source}", .directory.display())]
    Stage {
        directory: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to run the new Iris: {source}")]
    Run {
        #[source]
        source: io::Error,
    },
    #[error("the new Iris failed to start ({status}): {stderr}")]
    Start { status: ExitStatus, stderr: String },
    #[error("expected the new Iris to report `{expected}`, but it reported `{actual}`")]
    VersionMismatch { expected: String, actual: String },
    #[error("failed to replace {}: {source}", .path.display())]
    Replace {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// Windows denies deleting a file that a process runs, or that is read-only or protected.
    #[cfg(windows)]
    #[error("cannot remove {} from an earlier update; stop the Iris processes using it, or remove it yourself, then retry", .path.display())]
    PreviousExecutable { path: PathBuf },
}

/// The running executable, with symbolic links resolved, so that a link on `PATH` keeps pointing
/// at the replaced file.
pub fn current_executable() -> Result<PathBuf, InstallError> {
    let current_executable_error = |source| InstallError::CurrentExecutable { source };
    let executable = env::current_exe().map_err(current_executable_error)?;
    dunce::canonicalize(executable).map_err(current_executable_error)
}

/// Fails before any download when the executable's directory cannot receive a new executable.
/// The probe is deleted at once, so that interrupting the download leaves nothing behind.
pub fn check_writable(executable: &Path) -> Result<(), InstallError> {
    staging_file(executable)?;
    Ok(())
}

/// A new executable beside the one it replaces, deleted unless it is installed.
pub struct Staged {
    path: TempPath,
}

impl Staged {
    pub fn create(executable: &Path, contents: &[u8]) -> Result<Staged, InstallError> {
        let directory = parent_directory(executable);
        let mut file = staging_file(executable)?;
        file.write_all(contents).map_err(|source| stage_error(directory, source))?;
        #[cfg(unix)]
        {
            use std::fs::Permissions;
            use std::os::unix::fs::PermissionsExt;
            let permissions = Permissions::from_mode(0o755);
            let permissions = file.as_file().set_permissions(permissions);
            permissions.map_err(|source| stage_error(directory, source))?;
        }
        // Closing the file first, since Linux refuses to run a file that is open for writing.
        Ok(Staged { path: file.into_temp_path() })
    }

    /// Runs the staged executable, so that a release which fails to start on this system, or which
    /// is not the requested release, never replaces a working one.
    pub fn check_version(&self, release: &Release) -> Result<(), InstallError> {
        let output = Command::new(&self.path).arg("--version").output();
        let output = output.map_err(|source| InstallError::Run { source })?;
        if !output.status.success() {
            // The dynamic loader explains failures such as a missing glibc version on stderr.
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(InstallError::Start { status: output.status, stderr });
        }
        let actual = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let expected = release.version_output();
        if actual == expected {
            Ok(())
        } else {
            Err(InstallError::VersionMismatch { expected, actual })
        }
    }

    /// Whether the staged executable can update itself, which releases before
    /// `iris self update` cannot.
    pub fn has_self_update(&self) -> bool {
        let status = Command::new(&self.path)
            .args(["self", "update", "--help"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        status.is_ok_and(|status| status.success())
    }

    /// Renames the staged executable over `executable`, rather than writing into it: processes
    /// such as `iris lsp` keep running the old file, and macOS terminates processes whose signed
    /// executable is modified in place.
    #[cfg(not(windows))]
    pub fn install(self, executable: &Path) -> Result<(), InstallError> {
        self.path.persist(executable).map_err(|error| replace_error(executable, error.error))
    }

    /// Windows does not allow replacing a running executable, but it allows renaming one, so the
    /// running executable moves aside to a fixed name. It cannot be deleted while it runs, so the
    /// next update deletes it.
    #[cfg(windows)]
    pub fn install(self, executable: &Path) -> Result<(), InstallError> {
        use std::fs;

        let previous = executable.with_extension("exe.old");
        match fs::remove_file(&previous) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                return Err(InstallError::PreviousExecutable { path: previous });
            }
            Err(error) => return Err(replace_error(&previous, error)),
        }
        fs::rename(executable, &previous).map_err(|error| replace_error(executable, error))?;
        if let Err(error) = self.path.persist(executable) {
            let _ = fs::rename(&previous, executable);
            return Err(replace_error(executable, error.error));
        }
        Ok(())
    }
}

fn parent_directory(executable: &Path) -> &Path {
    executable.parent().expect("invariant violated: executable has a parent")
}

fn staging_file(executable: &Path) -> Result<NamedTempFile, InstallError> {
    let directory = parent_directory(executable);
    let file = tempfile::Builder::new()
        .prefix(".iris-self-update.")
        .suffix(EXE_SUFFIX)
        .tempfile_in(directory);
    file.map_err(|source| stage_error(directory, source))
}

fn stage_error(directory: &Path, source: io::Error) -> InstallError {
    match source.kind() {
        io::ErrorKind::PermissionDenied => {
            InstallError::PermissionDenied { directory: directory.into() }
        }
        _ => InstallError::Stage { directory: directory.into(), source },
    }
}

fn replace_error(path: &Path, source: io::Error) -> InstallError {
    match source.kind() {
        io::ErrorKind::PermissionDenied => {
            InstallError::PermissionDenied { directory: parent_directory(path).into() }
        }
        _ => InstallError::Replace { path: path.into(), source },
    }
}
