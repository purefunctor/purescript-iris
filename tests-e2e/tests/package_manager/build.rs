use std::process::Output;

use super::support::{TestWorkspace, assert_success};

fn snapshot_output(name: &str, output: &Output) {
    let status = output.status.code().map_or_else(|| "signal".to_owned(), |code| code.to_string());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    insta::with_settings!({omit_expression => true}, {
        insta::assert_snapshot!(
            name,
            format!("status: {status}\n--- stdout\n{stdout}--- stderr\n{stderr}")
        );
    });
}

#[test]
fn builds_the_selected_workspace_package() {
    let workspace = TestWorkspace::empty();
    workspace.write("spago.yaml", "workspace: {}\n");
    workspace.write(
        "packages/application/spago.yaml",
        "package:\n  name: application\n  dependencies: []\n",
    );
    workspace.write("packages/application/src/Main.purs", "module Main where\n\nvalue = 42\n");
    workspace
        .write("packages/library/spago.yaml", "package:\n  name: library\n  dependencies: []\n");
    workspace.write("packages/library/src/Library.purs", "module Library where\n");

    let output = workspace.command_in("packages/application/src", &["build", "--quiet"]);

    assert_success(&output);
    assert!(workspace.path().join("output/Main/index.js").is_file());
    assert!(!workspace.path().join("output/Library/index.js").exists());
    workspace.assert_spago_calls(
        "packages/application/src",
        &[&["fetch", "-p", "application"], &["sources", "--json", "-p", "application"]],
    );
}

#[test]
fn builds_the_whole_workspace_from_a_root_package_subdirectory() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "spago.yaml",
        "workspace: {}\npackage:\n  name: application\n  dependencies: [library]\n",
    );
    workspace.write(
        "src/Application.purs",
        "module Application where\n\nimport Library (library)\n\napplication = library\n",
    );
    workspace
        .write("packages/library/spago.yaml", "package:\n  name: library\n  dependencies: []\n");
    workspace.write("packages/library/src/Library.purs", "module Library where\n\nlibrary = 42\n");

    let output = workspace.command_in("src", &["build", "--quiet"]);

    assert_success(&output);
    assert!(workspace.path().join("output/Application/index.js").is_file());
    assert!(workspace.path().join("output/Library/index.js").is_file());
    workspace.assert_spago_calls("src", &[&["fetch"], &["sources", "--json"]]);
}

#[test]
fn excludes_gitignored_packages_from_selection() {
    let workspace = TestWorkspace::empty();
    workspace
        .write("spago.yaml", "workspace: {}\npackage:\n  name: application\n  dependencies: []\n");
    workspace.write(".gitignore", "ignored/\n");
    workspace.write("ignored/spago.yaml", "package:\n  name: ignored\n  dependencies: []\n");

    let output = workspace.command(&["build", "--quiet", "--package", "ignored"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "workspace package 'ignored' was not found; available packages: application\n"
    );
    workspace.assert_spago_calls("", &[]);
}

#[test]
fn excludes_nested_workspaces_from_selection() {
    let workspace = TestWorkspace::empty();
    workspace
        .write("spago.yaml", "workspace: {}\npackage:\n  name: application\n  dependencies: []\n");
    workspace.write(
        "nested/spago.yaml",
        "workspace: {}\npackage:\n  name: nested\n  dependencies: []\n",
    );
    workspace
        .write("nested/packages/child/spago.yaml", "package:\n  name: child\n  dependencies: []\n");

    let output = workspace.command(&["build", "--quiet", "--package", "child"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "workspace package 'child' was not found; available packages: application\n"
    );
    workspace.assert_spago_calls("", &[]);
}

#[test]
fn suppresses_diagnostics_without_changing_failure_status() {
    let workspace = TestWorkspace::empty();
    workspace
        .write("spago.yaml", "workspace: {}\npackage:\n  name: application\n  dependencies: []\n");
    workspace.write("src/Main.purs", "module Main where\n\nbroken = missing\n");

    let output = workspace.command(&["build", "--quiet", "--no-diagnostics"]);

    snapshot_output("suppressed_error_diagnostics", &output);
}
