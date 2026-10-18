use std::env;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::process::{self, Command};

const DESCENDANT_ARGUMENT: &str = "--iris-e2e-descendant";

/// Records the invocation, optionally cooperates with the deterministic test
/// gate, and then delegates to the real pinned Spago.
///
/// The gate is controlled entirely through the environment so that production
/// code carries no test hooks:
///
/// - `IRIS_E2E_SPAGO_CONTROL`: connects to this address, reports the role, and
///   blocks until the test releases it; the connection closes when the process exits.
/// - `IRIS_E2E_SPAGO_DESCENDANT`: spawns a pipe-inheriting descendant that is gated
///   on its own control connection.
/// - `IRIS_E2E_SPAGO_EXIT_AFTER_DESCENDANT`: exits once the descendant is spawned.
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

    let control = connect_to_control("spago");
    if env::var_os("IRIS_E2E_SPAGO_DESCENDANT").is_some() {
        let executable = env::current_exe().unwrap();
        #[expect(
            clippy::zombie_processes,
            reason = "the descendant must outlive the shim so E2E tests can verify process-tree retirement"
        )]
        Command::new(executable).arg(DESCENDANT_ARGUMENT).spawn().unwrap();
    }
    if env::var_os("IRIS_E2E_SPAGO_EXIT_AFTER_DESCENDANT").is_some() {
        process::exit(0);
    }
    if let Some(control) = &control {
        wait_for_release(control);
    }
    if let Some(message) = env::var_os("IRIS_E2E_SPAGO_FAIL") {
        eprint!("{}", message.to_string_lossy());
        process::exit(1);
    }
    if is_package_set_query(&arguments) {
        println!(
            r#"[{{"date":"2026-09-20","version":"99.0.0","compiler":"0.15.16"}},{{"date":"2026-09-12","version":"81.1.0","compiler":"0.15.15"}}]"#
        );
        return;
    }

    let executable = env::var_os("IRIS_E2E_SPAGO").expect("missing real Spago executable");
    let status = Command::new(executable).args(arguments).status().unwrap();
    process::exit(status.code().unwrap_or(1));
}

fn is_package_set_query(arguments: &[std::ffi::OsString]) -> bool {
    arguments.len() == 5
        && arguments[0] == "registry"
        && arguments[1] == "package-sets"
        && arguments[2] == "--latest"
        && arguments[3] == "--json"
        && arguments[4] == "--quiet"
}

fn run_descendant() {
    let control = connect_to_control("descendant").expect("missing Spago control address");
    wait_for_release(&control);
}

fn connect_to_control(role: &str) -> Option<TcpStream> {
    let address = env::var("IRIS_E2E_SPAGO_CONTROL").ok()?;
    let mut control = TcpStream::connect(address).expect("failed to connect to the Spago control");
    writeln!(control, "{role}").unwrap();
    Some(control)
}

fn wait_for_release(control: &TcpStream) {
    let mut instruction = String::new();
    let _ = BufReader::new(control).read_line(&mut instruction);
    if instruction != "release\n" {
        process::exit(1);
    }
}
