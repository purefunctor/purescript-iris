use std::process::Output;

#[path = "support.rs"]
mod support;

use support::TestWorkspace;

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
fn prints_help_for_every_command_path() {
    let workspace = TestWorkspace::empty();
    let paths: &[(&str, &[&str])] = &[
        ("help_root", &["--help"]),
        ("help_new", &["new", "--help"]),
        ("help_add", &["add", "--help"]),
        ("help_build", &["build", "--help"]),
        ("help_watch", &["watch", "--help"]),
        ("help_lsp", &["lsp", "--help"]),
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
    assert!(!output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    snapshot_output("version", &output);
}

#[test]
fn rejects_unpromoted_commands() {
    let workspace = TestWorkspace::empty();
    for command in ["compile", "run", "test", "docs"] {
        let output = workspace.command(&[command]);
        assert_eq!(output.status.code(), Some(2), "{command} was accepted");
        assert!(output.stdout.is_empty(), "{command} wrote stdout");
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

#[test]
fn rejects_invalid_lsp_configuration_before_starting() {
    let workspace = TestWorkspace::empty();
    let cases: &[(&str, &[&str])] = &[
        ("config_invalid", &["lsp", "--config", "{"]),
        ("config_conflict", &["lsp", "--config", "{}", "--config-file", "missing.json"]),
        ("config_file_missing", &["lsp", "--config-file", "missing.json"]),
    ];
    for (name, arguments) in cases {
        let output = workspace.command(arguments);
        assert_eq!(output.status.code(), Some(2), "{name} was accepted");
        assert!(output.stdout.is_empty(), "{name} wrote stdout");
        insta::with_settings!({filters => vec![
            (r"No such file or directory \(os error 2\)|The system cannot find the file specified\. \(os error 2\)", "[FILE NOT FOUND]"),
        ]}, {
            snapshot_output(name, &output);
        });
    }
}
