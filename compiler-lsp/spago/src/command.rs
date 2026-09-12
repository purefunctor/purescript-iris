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
    #[error("Spago returned an invalid JSON source list: {0}")]
    InvalidSources(serde_json::Error),
    #[error("Spago returned no source paths")]
    EmptySources,
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

    pub fn source_globs(
        &self,
        selected: Option<&str>,
        show_output: bool,
    ) -> Result<Vec<PathBuf>, SpagoError> {
        let mut arguments = vec!["sources".to_owned(), "--json".to_owned()];
        add_selection(&mut arguments, selected);
        let output = self.execute(&arguments)?;
        if show_output {
            io::stderr().write_all(&output.stderr).map_err(SpagoError::Execute)?;
        }
        ensure_success("sources --json", &output)?;
        let sources: Vec<PathBuf> =
            serde_json::from_slice(&output.stdout).map_err(SpagoError::InvalidSources)?;
        if sources.is_empty() {
            return Err(SpagoError::EmptySources);
        }
        Ok(sources)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepares_spago_command_for_a_project() {
        let temporary = tempfile::tempdir().unwrap();
        let command = SpagoCommand::new(temporary.path()).unwrap();

        assert_eq!(command.current_directory, temporary.path());
        assert_eq!(env::split_paths(&command.path).next().as_deref(), Some(command._shim.path()));
    }

    #[cfg(unix)]
    #[test]
    fn executes_spago_package_commands() {
        let temporary = tempfile::tempdir().unwrap();
        let executable = write_executable(
            &temporary,
            r#"#!/bin/sh
case "$*" in
  "fetch -p application") exit 0 ;;
  "fetch -p application --test-deps console effect") exit 0 ;;
  "sources --json -p application")
    printf '%s\n' '["src/**/*.purs", ".spago/p/prelude/1.0.0/src/**/*.purs"]'
    exit 0
    ;;
esac
exit 9
"#,
        );
        let command = test_command(&temporary, &executable);

        command.fetch(Some("application"), true).unwrap();
        command.add("application", &["console".to_owned(), "effect".to_owned()], true).unwrap();
        let sources = command.source_globs(Some("application"), true).unwrap();

        assert_eq!(
            sources,
            vec![
                PathBuf::from("src/**/*.purs"),
                PathBuf::from(".spago/p/prelude/1.0.0/src/**/*.purs"),
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn reports_spago_failures_and_invalid_source_lists() {
        let temporary = tempfile::tempdir().unwrap();
        let executable = write_executable(
            &temporary,
            r#"#!/bin/sh
case "$*" in
  "sources --json -p invalid") printf '%s\n' 'invalid' ;;
  "sources --json -p empty") printf '%s\n' '[]' ;;
  *) exit 7 ;;
esac
"#,
        );
        let command = test_command(&temporary, &executable);

        assert!(matches!(command.fetch(None, true), Err(SpagoError::Failed { .. })));
        assert!(matches!(
            command.source_globs(Some("invalid"), true),
            Err(SpagoError::InvalidSources(_))
        ));
        assert!(matches!(command.source_globs(Some("empty"), true), Err(SpagoError::EmptySources)));
    }

    #[cfg(unix)]
    fn write_executable(temporary: &tempfile::TempDir, source: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let executable = temporary.path().join("spago");
        fs::write(&executable, source).unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
        executable
    }

    #[cfg(unix)]
    fn test_command(temporary: &tempfile::TempDir, executable: &Path) -> SpagoCommand {
        let shim = tempfile::tempdir().unwrap();
        write_purs_shim(shim.path()).unwrap();
        let path = prepend_path(shim.path()).unwrap();
        SpagoCommand {
            current_directory: temporary.path().to_path_buf(),
            executable: executable.as_os_str().to_owned(),
            path,
            _shim: shim,
        }
    }
}
