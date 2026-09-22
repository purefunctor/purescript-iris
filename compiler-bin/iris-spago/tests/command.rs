use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::{env, fs};

use iris_spago::{SpagoCommand, SpagoError};

struct EnvironmentVariable {
    name: &'static str,
    previous: Option<OsString>,
}

impl EnvironmentVariable {
    fn set(name: &'static str, value: &Path) -> EnvironmentVariable {
        let previous = env::var_os(name);
        // This integration-test binary has one test and does not spawn threads.
        unsafe { env::set_var(name, value) };
        EnvironmentVariable { name, previous }
    }
}

impl Drop for EnvironmentVariable {
    fn drop(&mut self) {
        if let Some(previous) = &self.previous {
            unsafe { env::set_var(self.name, previous) };
        } else {
            unsafe { env::remove_var(self.name) };
        }
    }
}

#[test]
fn executes_spago_package_commands_through_the_compiler_shim() {
    let temporary = tempfile::tempdir().unwrap();
    let executable = write_executable(temporary.path());
    let log = temporary.path().join("calls");
    let _executable = EnvironmentVariable::set("IRIS_SPAGO", &executable);
    let _log = EnvironmentVariable::set("IRIS_SPAGO_TEST_LOG", &log);
    let command = SpagoCommand::new(temporary.path()).unwrap();

    assert_eq!(command.latest_package_set("0.15.15").unwrap(), "80.4.0");
    command.fetch(Some("application"), true).unwrap();
    command.add("application", &["console".to_owned(), "effect".to_owned()], true).unwrap();
    assert!(matches!(command.fetch(None, true), Err(SpagoError::Failed { .. })));
    let calls = fs::read_to_string(log).unwrap().replace("\r\n", "\n");
    assert_eq!(
        calls,
        r#"registry package-sets --latest --json --quiet
fetch -p application
fetch -p application --test-deps console effect
"#
    );
}

#[cfg(unix)]
fn write_executable(directory: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let executable = directory.join("spago");
    fs::write(
        &executable,
        r#"#!/bin/sh
command -v purs >/dev/null || exit 8
case "$*" in
  "registry package-sets --latest --json --quiet")
    printf '%s\n' "$*" >> "$IRIS_SPAGO_TEST_LOG"
    printf '%s\n' '[{"version":"99.0.0","compiler":"0.15.16"},{"version":"80.4.0","compiler":"0.15.15"}]'
    exit 0
    ;;
  "fetch -p application"|"fetch -p application --test-deps console effect")
    printf '%s\n' "$*" >> "$IRIS_SPAGO_TEST_LOG"
    exit 0
    ;;
esac
exit 7
"#,
    )
    .unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    executable
}

#[cfg(windows)]
fn write_executable(directory: &Path) -> PathBuf {
    let executable = directory.join("spago.cmd");
    fs::write(
        &executable,
        r#"@echo off
where purs >nul 2>nul || exit /b 8
if "%*"=="registry package-sets --latest --json --quiet" goto package_sets
if "%*"=="fetch -p application" goto success
if "%*"=="fetch -p application --test-deps console effect" goto success
exit /b 7
:package_sets
echo %*>>"%IRIS_SPAGO_TEST_LOG%"
echo [{"version":"99.0.0","compiler":"0.15.16"},{"version":"80.4.0","compiler":"0.15.15"}]
exit /b 0
:success
echo %*>>"%IRIS_SPAGO_TEST_LOG%"
exit /b 0
"#,
    )
    .unwrap();
    executable
}
