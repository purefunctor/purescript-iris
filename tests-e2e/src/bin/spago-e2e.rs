use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::process::{self, Command};
use std::time::Duration;
use std::{env, thread};

/// Records the invocation, optionally cooperates with the deterministic test
/// gate, and then delegates to the real pinned Spago.
///
/// The gate is controlled entirely through the environment so that production
/// code carries no test hooks:
///
/// - `IRIS_E2E_SPAGO_PID`: writes this process's id for retirement checks.
/// - `IRIS_E2E_SPAGO_STARTED`: writes a marker once the invocation begins.
/// - `IRIS_E2E_SPAGO_RELEASE`: blocks until this file exists.
/// - `IRIS_E2E_SPAGO_FAIL`: writes the value to stderr and exits non-zero.
fn main() {
    let arguments = env::args_os().skip(1).collect::<Vec<_>>();
    let log_path = env::var_os("IRIS_E2E_SPAGO_LOG").expect("missing Spago log path");
    let current_directory = env::current_dir().expect("failed to read Spago working directory");
    let mut log = OpenOptions::new().create(true).append(true).open(log_path).unwrap();
    let mut record = current_directory.display().to_string();
    for argument in &arguments {
        record.push('\t');
        record.push_str(&argument.to_string_lossy());
    }
    record.push('\n');
    log.write_all(record.as_bytes()).unwrap();

    if let Some(path) = env::var_os("IRIS_E2E_SPAGO_PID") {
        std::fs::write(path, process::id().to_string()).unwrap();
    }
    if let Some(path) = env::var_os("IRIS_E2E_SPAGO_STARTED") {
        std::fs::write(path, "started\n").unwrap();
    }
    if let Some(path) = env::var_os("IRIS_E2E_SPAGO_RELEASE") {
        let path = PathBuf::from(path);
        while !path.exists() {
            thread::sleep(Duration::from_millis(5));
        }
    }
    if let Some(message) = env::var_os("IRIS_E2E_SPAGO_FAIL") {
        eprint!("{}", message.to_string_lossy());
        process::exit(1);
    }

    let executable = env::var_os("IRIS_E2E_SPAGO").expect("missing real Spago executable");
    let status = Command::new(executable).args(arguments).status().unwrap();
    process::exit(status.code().unwrap_or(1));
}
