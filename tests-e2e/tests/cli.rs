use std::process::Output;

#[path = "support.rs"]
mod support;

use support::TestWorkspace;

fn snapshot_output(name: &str, output: &Output) {
    let status = output.status.code().map_or_else(|| "signal".to_owned(), |code| code.to_string());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    insta::with_settings!({omit_expression => true, filters => vec![(r"(?m)^iris \d+\.\d+\.\d+(?:-dev\.[0-9a-f]{7,64})?$", "iris [version]")]}, {
        insta::assert_snapshot!(
            name,
            format!("status: {status}\n--- stdout\n{stdout}--- stderr\n{stderr}")
        );
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
        ("help_lsp", &["lsp", "--help"]),
        ("help_run", &["run", "--help"]),
        ("help_test", &["test", "--help"]),
    ];

    for (name, arguments) in paths {
        let output = workspace.command(arguments);
        assert!(output.status.success(), "{name} help failed");
        assert!(!output.stdout.is_empty(), "{name} help did not write stdout");
        assert!(output.stderr.is_empty(), "{name} help wrote stderr");
        snapshot_output(name, &output);
    }
}

#[test]
fn prints_version_to_stdout() {
    let workspace = TestWorkspace::empty();
    let output = workspace.command(&["--version"]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let manifest = include_str!("../../compiler-executable/iris-cli/Cargo.toml");
    let version = manifest
        .lines()
        .find_map(|line| line.strip_prefix("version = \"")?.strip_suffix('"'))
        .expect("iris-cli package version");
    let version = match option_env!("IRIS_BUILD_REVISION") {
        Some(revision) => format!("{version}-dev.{}", revision.to_ascii_lowercase()),
        None => version.to_owned(),
    };
    assert_eq!(output.stdout, format!("iris {version}\n").as_bytes());
    snapshot_output("version", &output);
}

#[test]
fn rejects_unpromoted_commands() {
    let workspace = TestWorkspace::empty();
    for (name, command) in [("rejects_compile", "compile"), ("rejects_docs", "docs")] {
        let output = workspace.command(&[command]);
        assert_eq!(output.status.code(), Some(2), "{command} was accepted");
        assert!(output.stdout.is_empty(), "{command} wrote stdout");
        snapshot_output(name, &output);
    }
}

#[test]
fn rejects_removed_lsp_configuration_options() {
    let workspace = TestWorkspace::empty();
    for (name, arguments) in [
        ("rejects_lsp_config", ["lsp", "--config", "{}"]),
        ("rejects_lsp_config_file", ["lsp", "--config-file", "iris.json"]),
    ] {
        let output = workspace.command(&arguments);
        assert_eq!(output.status.code(), Some(2), "{arguments:?} was accepted");
        assert!(output.stdout.is_empty(), "{arguments:?} wrote stdout");
        snapshot_output(name, &output);
    }
}

#[test]
fn run_and_test_require_separator_before_trailing_arguments() {
    let workspace = TestWorkspace::empty();
    for (name, arguments) in [
        ("run_requires_separator", &["run", "argument"][..]),
        ("test_requires_separator", &["test", "argument"][..]),
    ] {
        let output = workspace.command(arguments);
        assert!(!output.status.success(), "{name} unexpectedly succeeded");
        snapshot_output(name, &output);
    }
}

#[test]
fn add_requires_dependencies() {
    let workspace = TestWorkspace::empty();
    let output = workspace.command(&["add"]);
    assert_eq!(output.status.code(), Some(2), "add without dependencies succeeded");
    assert!(output.stdout.is_empty(), "add error wrote stdout");
    snapshot_output("add_requires_dependencies", &output);
}
