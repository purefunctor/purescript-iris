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

const ATTESTATION: &str = r"(?m)^(?:(?:Verifying GitHub release attestation|warning: GitHub release attestation verification skipped .*|warning: A GitHub CLI with attestation support is not installed; .*|warning: Install or update gh .*)\n)+";

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
#[cfg_attr(
    all(target_os = "linux", target_arch = "aarch64"),
    ignore = "Iris 0.1.5 has no Linux ARM64 release archive"
)]
fn replaces_the_running_executable_with_a_release() {
    let workspace = TestWorkspace::empty();
    let executable = install_iris(&workspace);
    let current = version(&executable);

    let output = update_to_previous_release(&executable);
    let mut settings = insta::Settings::clone_current();
    settings.add_filter(&regex::escape(&current), "[CURRENT]");
    settings.add_filter(r"(?m)^(Downloading iris \S+ for )\S+$", "${1}[TARGET]");
    settings.add_filter(ATTESTATION, "[ATTESTATION]\n");
    if cfg!(windows) {
        settings.add_filter(r"iris\.exe\b", "iris");
    }
    settings.bind(|| {
        insta::assert_snapshot!(workspace.report(&output), @"
        status: 0
        --- stdout
        Replaced Iris [CURRENT] with Iris 0.1.5 at [WORKSPACE]/bin/iris.
        Restart `iris watch` and editor language servers to use Iris 0.1.5.
        --- stderr
        Downloading iris v0.1.5 for [TARGET]
        [ATTESTATION]
        warning: Iris 0.1.5 cannot update itself; run the installer to update it.
        ");
    });
    assert_eq!(version(&executable), PREVIOUS_RELEASE);

    // Windows cannot delete the running executable, so it remains beside the new one.
    let files = fs::read_dir(executable.parent().unwrap()).unwrap();
    let files = files.map(|entry| entry.unwrap().file_name().into_string().unwrap()).sorted();
    let expected: &[&str] = if cfg!(windows) { &["iris.exe", "iris.exe.old"] } else { &["iris"] };
    assert_eq!(files.collect_vec(), expected);
}

#[cfg(unix)]
#[test]
#[cfg_attr(
    all(target_os = "linux", target_arch = "aarch64"),
    ignore = "Iris 0.1.5 has no Linux ARM64 release archive"
)]
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

    insta::assert_snapshot!(workspace.report(&output), @"
    status: 1
    --- stdout
    --- stderr
    cannot write to [WORKSPACE]/bin; update Iris with the permissions it was installed with
    ");
    assert_eq!(version(&executable), current);
}

#[test]
fn reports_the_latest_canary_without_installing_it() {
    let workspace = TestWorkspace::empty();
    let executable = install_iris(&workspace);
    let current = version(&executable);

    let output = run(&executable, &["self", "update", "--check", "--canary"]);
    let canary = (r"\d+\.\d+\.\d+-dev\.[0-9a-f]{12} is available", "[CANARY] is available");
    let current_version = regex::escape(&current);
    insta::with_settings!({filters => vec![canary, (current_version.as_str(), "[CURRENT]")]}, {
        insta::assert_snapshot!(workspace.report(&output), @"
        status: 100
        --- stdout
        Iris [CANARY] is available; this is Iris [CURRENT].
        --- stderr
        ");
    });
    assert_eq!(version(&executable), current);
}
