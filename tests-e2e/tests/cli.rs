use std::process::Output;

#[path = "support.rs"]
mod support;

use support::TestWorkspace;

fn snapshot_output(name: &str, workspace: &TestWorkspace, output: &Output) {
    let version = (r"(?m)^iris \d+\.\d+\.\d+(?:-dev\.[0-9a-f]{7,64})?$", "iris [version]");
    insta::with_settings!({omit_expression => true, filters => vec![version]}, {
        insta::assert_snapshot!(name, workspace.report(output));
    });
}

#[test]
fn prints_help_for_every_command_path() {
    let workspace = TestWorkspace::empty();
    let paths: &[(&str, &[&str])] = &[
        ("help_root", &["--help"]),
        ("help_new", &["new", "--help"]),
        ("help_add", &["add", "--help"]),
        ("help_build", &["build", "--help"]),
        ("help_watch", &["watch", "--help"]),
        ("help_watch_query", &["watch", "query", "--help"]),
        ("help_skills", &["skills", "--help"]),
        ("help_format", &["format", "--help"]),
        ("help_lsp", &["lsp", "--help"]),
        ("help_run", &["run", "--help"]),
        ("help_test", &["test", "--help"]),
        ("help_self", &["self", "--help"]),
        ("help_self_update", &["self", "update", "--help"]),
    ];

    for (name, arguments) in paths {
        snapshot_output(name, &workspace, &workspace.command(arguments));
    }
}

#[test]
fn prints_version_to_stdout() {
    let workspace = TestWorkspace::empty();
    snapshot_output("version", &workspace, &workspace.command(&["--version"]));
}

#[test]
fn run_and_test_require_separator_before_trailing_arguments() {
    let workspace = TestWorkspace::empty();
    for (name, arguments) in [
        ("run_requires_separator", &["run", "argument"][..]),
        ("test_requires_separator", &["test", "argument"][..]),
    ] {
        snapshot_output(name, &workspace, &workspace.command(arguments));
    }
}

#[test]
fn add_requires_dependencies() {
    let workspace = TestWorkspace::empty();
    snapshot_output("add_requires_dependencies", &workspace, &workspace.command(&["add"]));
}
