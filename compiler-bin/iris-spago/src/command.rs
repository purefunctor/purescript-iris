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
    #[error("Spago {command} failed with status {status}")]
    Failed { command: String, status: String },
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
        let mut arguments = vec!["fetch".to_owned()];
        add_selection(&mut arguments, selected);
        let output = self.execute(&arguments)?;
        if show_output {
            forward_output(&output)?;
        }
        ensure_success("fetch", &output)
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

    fn execute(&self, arguments: &[String]) -> Result<Output, SpagoError> {
        Command::new(&self.executable)
            .args(arguments)
            .current_dir(&self.current_directory)
            .env("PATH", &self.path)
            .output()
            .map_err(SpagoError::Execute)
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
    Err(SpagoError::Failed { command: command.to_owned(), status: output.status.to_string() })
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
