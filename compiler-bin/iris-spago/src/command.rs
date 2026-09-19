use std::ffi::OsString;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::{env, fs};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SpagoError {
    #[error("failed to prepare the Spago compiler compatibility shim: {0}")]
    Shim(io::Error),
    #[error("failed to execute Spago: {0}")]
    Execute(io::Error),
    #[error("Spago {command} failed with status {status}{stderr}")]
    Failed { command: String, status: String, stderr: String },
}

impl SpagoError {
    /// Builds a failure from an exit status and the stderr it produced.
    ///
    /// Supervised callers that capture output themselves use this so that a
    /// failed fetch reports the same capped stderr tail as the blocking path.
    pub fn failed(command: &str, status: impl ToString, stderr: &[u8]) -> SpagoError {
        SpagoError::Failed {
            command: command.to_owned(),
            status: status.to_string(),
            stderr: failure_tail(stderr),
        }
    }
}

pub struct SpagoCommand {
    current_directory: PathBuf,
    executable: OsString,
    path: OsString,
    _shim: tempfile::TempDir,
}

impl SpagoCommand {
    pub fn new(current_directory: &Path) -> Result<SpagoCommand, SpagoError> {
        let shim = tempfile::tempdir().map_err(SpagoError::Shim)?;
        write_purs_shim(shim.path()).map_err(SpagoError::Shim)?;
        let path = prepend_path(shim.path()).map_err(SpagoError::Shim)?;
        let executable = env::var_os("IRIS_SPAGO").unwrap_or_else(|| "spago".into());
        Ok(SpagoCommand {
            current_directory: current_directory.to_path_buf(),
            executable,
            path,
            _shim: shim,
        })
    }

    pub fn fetch(&self, selected: Option<&str>, show_output: bool) -> Result<(), SpagoError> {
        let output = self.execute_fetch(selected)?;
        if show_output {
            forward_output(&output)?;
        }
        ensure_success("fetch", &output)
    }

    /// Builds the configured `spago fetch` invocation for supervised execution.
    ///
    /// Callers that own the process lifetime can spawn this command and drain
    /// its output themselves. The returned command carries the same executable
    /// resolution, working directory, arguments, and compiler shim as
    /// [`SpagoCommand::fetch`].
    pub fn fetch_command(&self, selected: Option<&str>) -> Command {
        let mut arguments = vec!["fetch".to_owned()];
        add_selection(&mut arguments, selected);
        self.command(&arguments)
    }

    pub fn add(
        &self,
        selected: &str,
        packages: &[String],
        test_dependencies: bool,
    ) -> Result<(), SpagoError> {
        let mut arguments = vec!["fetch".to_owned(), "-p".to_owned(), selected.to_owned()];
        if test_dependencies {
            arguments.push("--test-deps".to_owned());
        }
        arguments.extend(packages.iter().cloned());
        let output = self.execute(&arguments)?;
        forward_output(&output)?;
        ensure_success("fetch", &output)
    }

    fn execute_fetch(&self, selected: Option<&str>) -> Result<Output, SpagoError> {
        self.fetch_command(selected).output().map_err(SpagoError::Execute)
    }

    fn execute(&self, arguments: &[String]) -> Result<Output, SpagoError> {
        self.command(arguments).output().map_err(SpagoError::Execute)
    }

    fn command(&self, arguments: &[String]) -> Command {
        let mut command = Command::new(&self.executable);
        command.args(arguments).current_dir(&self.current_directory).env("PATH", &self.path);
        command
    }
}

fn add_selection(arguments: &mut Vec<String>, selected: Option<&str>) {
    if let Some(selected) = selected {
        arguments.push("-p".to_owned());
        arguments.push(selected.to_owned());
    }
}

fn ensure_success(command: &str, output: &Output) -> Result<(), SpagoError> {
    if output.status.success() {
        return Ok(());
    }
    Err(SpagoError::failed(command, output.status, &output.stderr))
}

/// Formats the trailing stderr of a failed command for inclusion in an error.
///
/// The tail is capped so that a chatty tool cannot produce an unbounded
/// diagnostic. Non-UTF-8 output is replaced rather than rejected.
fn failure_tail(stderr: &[u8]) -> String {
    const MAXIMUM: usize = 2048;
    let stderr = String::from_utf8_lossy(stderr);
    let stderr = stderr.trim();
    if stderr.is_empty() {
        return String::new();
    }
    let start = stderr.len().saturating_sub(MAXIMUM);
    let start = stderr
        .char_indices()
        .map(|(index, _)| index)
        .find(|index| *index >= start)
        .unwrap_or(stderr.len());
    format!("\n{}", &stderr[start..])
}

fn forward_output(output: &Output) -> Result<(), SpagoError> {
    io::stdout().write_all(&output.stdout).map_err(SpagoError::Execute)?;
    io::stderr().write_all(&output.stderr).map_err(SpagoError::Execute)
}

fn prepend_path(directory: &Path) -> io::Result<OsString> {
    let mut paths = vec![directory.to_path_buf()];
    if let Some(current) = env::var_os("PATH") {
        paths.extend(env::split_paths(&current));
    }
    env::join_paths(paths).map_err(io::Error::other)
}

#[cfg(unix)]
fn write_purs_shim(directory: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let path = directory.join("purs");
    fs::write(&path, include_str!("../bundled/purs/purs.sh"))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
}

#[cfg(windows)]
fn write_purs_shim(directory: &Path) -> io::Result<()> {
    fs::write(directory.join("purs.cmd"), include_str!("../bundled/purs/purs.cmd"))
}
