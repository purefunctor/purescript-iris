use std::env::consts::EXE_SUFFIX;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use itertools::Itertools;

#[path = "support.rs"]
mod support;

use support::{TestWorkspace, assert_success};

/// A published stable release that predates `iris self update`, so installing it also reports that
/// the installed release cannot update itself.
const PREVIOUS_RELEASE: &str = "0.1.5";

/// Copies the Iris under test into the workspace, so that updating it leaves the build untouched.
fn install_iris(workspace: &TestWorkspace) -> PathBuf {
    let directory = workspace.path().join("bin");
    fs::create_dir_all(&directory).unwrap();
    let executable = directory.join(format!("iris{EXE_SUFFIX}"));
    fs::copy(env!("CARGO_BIN_EXE_iris-e2e"), &executable).unwrap();
    dunce::canonicalize(executable).unwrap()
}

fn run(executable: &Path, arguments: &[&str]) -> Output {
    Command::new(executable).args(arguments).env("NO_COLOR", "1").output().unwrap()
}

fn version(executable: &Path) -> String {
    let output = run(executable, &["--version"]);
    assert_success(&output);
    let output = String::from_utf8(output.stdout).unwrap();
    output.trim_end().strip_prefix("iris ").unwrap().to_owned()
}

fn update_to_previous_release(executable: &Path) -> Output {
    run(executable, &["self", "update", &format!("v{PREVIOUS_RELEASE}")])
}

#[test]
fn replaces_the_running_executable_with_a_release() {
    let workspace = TestWorkspace::empty();
    let executable = install_iris(&workspace);
    let current = version(&executable);

    let output = update_to_previous_release(&executable);
    assert_success(&output);
    let replaced = format!(
        "Replaced Iris {current} with Iris {PREVIOUS_RELEASE} at {}.",
        executable.display()
    );
    let restart = format!(
        "Restart `iris watch` and editor language servers to use Iris {}.",
        PREVIOUS_RELEASE
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.lines().collect_vec(), [replaced, restart]);
    let warning = format!(
        "warning: Iris {PREVIOUS_RELEASE} cannot update itself; run the installer to update it."
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(stderr.lines().last(), Some(warning.as_str()));
    assert_eq!(version(&executable), PREVIOUS_RELEASE);

    // Windows cannot delete the running executable, so it remains beside the new one.
    let files = fs::read_dir(executable.parent().unwrap()).unwrap();
    let files = files.map(|entry| entry.unwrap().file_name().into_string().unwrap()).sorted();
    let expected: &[&str] = if cfg!(windows) { &["iris.exe", "iris.exe.old"] } else { &["iris"] };
    assert_eq!(files.collect_vec(), expected);
}

#[cfg(unix)]
#[test]
fn replaces_the_file_that_a_symbolic_link_points_to() {
    let workspace = TestWorkspace::empty();
    let executable = install_iris(&workspace);
    let link = workspace.path().join("iris");
    std::os::unix::fs::symlink(&executable, &link).unwrap();

    assert_success(&update_to_previous_release(&link));
    assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    assert_eq!(version(&executable), PREVIOUS_RELEASE);
}

#[cfg(unix)]
#[test]
fn reports_an_installation_directory_it_cannot_write_to() {
    use std::os::unix::fs::PermissionsExt;

    let workspace = TestWorkspace::empty();
    let executable = install_iris(&workspace);
    let current = version(&executable);
    let directory = executable.parent().unwrap();
    fs::set_permissions(directory, fs::Permissions::from_mode(0o555)).unwrap();
    let output = update_to_previous_release(&executable);
    fs::set_permissions(directory, fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(output.status.code(), Some(1));
    let error = format!(
        "cannot write to {}; update Iris with the permissions it was installed with\n",
        directory.display()
    );
    assert_eq!(String::from_utf8_lossy(&output.stderr), error);
    assert_eq!(version(&executable), current);
}

#[test]
fn reports_the_latest_canary_without_installing_it() {
    let workspace = TestWorkspace::empty();
    let executable = install_iris(&workspace);
    let current = version(&executable);

    let output = run(&executable, &["self", "update", "--check", "--canary"]);
    assert_eq!(output.status.code(), Some(100), "{output:?}");
    let pattern = format!(
        r"^Iris \d+\.\d+\.\d+-dev\.[0-9a-f]{{12}} is available; this is Iris {}\.\n$",
        regex::escape(&current)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(regex::Regex::new(&pattern).unwrap().is_match(&stdout), "{stdout}");
    assert_eq!(version(&executable), current);
}
