use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::process::{self, Command};
use std::time::Duration;
use std::{env, thread};

const DESCENDANT_ARGUMENT: &str = "--iris-e2e-descendant";

/// Records the invocation, optionally cooperates with the deterministic test
/// gate, and then delegates to the real pinned Spago.
///
/// The gate is controlled entirely through the environment so that production
/// code carries no test hooks:
///
/// - `IRIS_E2E_SPAGO_PID`: writes this process's id for retirement checks.
/// - `IRIS_E2E_SPAGO_DESCENDANT_PID`: spawns a pipe-inheriting descendant and writes its id.
/// - `IRIS_E2E_SPAGO_DESCENDANT_RELEASE`: keeps that descendant alive until this file exists.
/// - `IRIS_E2E_SPAGO_EXIT_AFTER_DESCENDANT`: exits once that descendant is running.
/// - `IRIS_E2E_SPAGO_STARTED`: writes a marker once the invocation begins.
/// - `IRIS_E2E_SPAGO_RELEASE`: blocks until this file exists.
/// - `IRIS_E2E_SPAGO_GATE_DIRECTORY`: creates numbered started, pid, and release files.
/// - `IRIS_E2E_SPAGO_FAIL`: writes the value to stderr and exits non-zero.
fn main() {
    let arguments = env::args_os().skip(1).collect::<Vec<_>>();
    if arguments.first().is_some_and(|argument| argument == DESCENDANT_ARGUMENT) {
        run_descendant();
        return;
    }

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
    if env::var_os("IRIS_E2E_SPAGO_DESCENDANT_PID").is_some() {
        Command::new(env::current_exe().unwrap()).arg(DESCENDANT_ARGUMENT).spawn().unwrap();
    }
    if env::var_os("IRIS_E2E_SPAGO_EXIT_AFTER_DESCENDANT").is_some() {
        let descendant = env::var_os("IRIS_E2E_SPAGO_DESCENDANT_PID")
            .expect("exiting Spago shim requires a descendant pid path");
        let descendant = PathBuf::from(descendant);
        while !descendant.exists() {
            thread::sleep(Duration::from_millis(5));
        }
        process::exit(0);
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
    if let Some(directory) = env::var_os("IRIS_E2E_SPAGO_GATE_DIRECTORY") {
        wait_for_attempt_release(PathBuf::from(directory));
    }
    if let Some(message) = env::var_os("IRIS_E2E_SPAGO_FAIL") {
        eprint!("{}", message.to_string_lossy());
        process::exit(1);
    }

    let executable = env::var_os("IRIS_E2E_SPAGO").expect("missing real Spago executable");
    let status = Command::new(executable).args(arguments).status().unwrap();
    process::exit(status.code().unwrap_or(1));
}

fn wait_for_attempt_release(directory: PathBuf) {
    std::fs::create_dir_all(&directory).unwrap();
    let mut attempt = 1;
    loop {
        let path = directory.join(format!("{attempt}.started"));
        match OpenOptions::new().write(true).create_new(true).open(path) {
            Ok(mut file) => {
                writeln!(file, "{}", process::id()).unwrap();
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => attempt += 1,
            Err(error) => panic!("failed to create Spago attempt marker: {error}"),
        }
    }
    let release = directory.join(format!("{attempt}.release"));
    while !release.exists() {
        thread::sleep(Duration::from_millis(5));
    }
}

fn run_descendant() {
    let pid =
        env::var_os("IRIS_E2E_SPAGO_DESCENDANT_PID").expect("missing Spago descendant pid path");
    let release = env::var_os("IRIS_E2E_SPAGO_DESCENDANT_RELEASE")
        .expect("missing Spago descendant release path");
    std::fs::write(pid, process::id().to_string()).unwrap();
    let release = PathBuf::from(release);
    while !release.exists() {
        thread::sleep(Duration::from_millis(5));
    }
}
