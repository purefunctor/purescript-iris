use std::io::Write;
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
        ("help_format", &["format", "--help"]),
        ("help_watch", &["watch", "--help"]),
        ("help_watch_query", &["watch", "query", "--help"]),
        ("help_skills", &["skills", "--help"]),
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

#[test]
fn format_reads_stdin_without_project_configuration() {
    let workspace = TestWorkspace::empty();
    let mut child = workspace.spawn(&["format", "-"]);
    child.stdin.take().unwrap().write_all("module Main where\r\nvalue=\"😀\"".as_bytes()).unwrap();
    let output = child.wait_with_output().unwrap();
    support::assert_success(&output);
    assert_eq!(output.stdout, "module Main where\n\nvalue = \"😀\"\n".as_bytes());
    assert!(output.stderr.is_empty());
    for (source, status) in [("module Main where", 1), ("module Main where\n", 0)] {
        let mut child = workspace.spawn(&["format", "--check"]);
        child.stdin.take().unwrap().write_all(source.as_bytes()).unwrap();
        let output = child.wait_with_output().unwrap();
        assert_eq!(output.status.code(), Some(status));
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }
    workspace.assert_spago_calls("", &[]);
}

#[test]
fn format_preview_check_and_write_have_distinct_filesystem_effects() {
    let workspace = TestWorkspace::empty();
    let source = "module Main where\nx=1\n";
    let formatted = "module Main where\n\nx = 1\n";
    workspace.write("Main.purs", source);
    workspace.write("Clean.purs", "module Clean where\n");

    let preview = workspace.command(&["format", "Main.purs"]);
    support::assert_success(&preview);
    assert_eq!(preview.stdout, formatted.as_bytes());
    assert_eq!(workspace.read("Main.purs"), source);

    let check = workspace.command(&["format", "--check", "Main.purs", "Clean.purs"]);
    assert_eq!(check.status.code(), Some(1));
    assert_eq!(check.stdout, b"Main.purs\n");
    assert!(check.stderr.is_empty());
    assert_eq!(workspace.read("Main.purs"), source);

    let write = workspace.command(&["format", "--write", "Main.purs", "./Main.purs", "Clean.purs"]);
    support::assert_success(&write);
    assert!(write.stdout.is_empty());
    assert_eq!(workspace.read("Main.purs"), formatted);
    assert_eq!(workspace.read("Clean.purs"), "module Clean where\n");
    support::assert_success(&workspace.command(&["format", "--check", "Main.purs", "Clean.purs"]));
}

#[test]
fn format_rejects_invalid_modes_and_leaves_all_inputs_untouched_on_syntax_errors() {
    let workspace = TestWorkspace::empty();
    let source = "module Main where\nx=1\n";
    let invalid = "module Invalid where\nnewtype Broken = Broken\n";
    workspace.write("Main.purs", source);
    workspace.write("Invalid.purs", invalid);
    let cases: &[(&str, &[&str])] = &[
        ("format_conflicting_modes", &["format", "--check", "--write", "Main.purs"]),
        ("format_write_stdin", &["format", "--write"]),
        ("format_mixed_stdin", &["format", "--check", "-", "Main.purs"]),
        ("format_multiple_previews", &["format", "Main.purs", "Invalid.purs"]),
        ("format_invalid_source", &["format", "--write", "Main.purs", "Invalid.purs"]),
    ];
    for (name, arguments) in cases {
        let output = workspace.command(arguments);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        snapshot_output(name, &output);
        assert_eq!(workspace.read("Main.purs"), source);
        assert_eq!(workspace.read("Invalid.purs"), invalid);
    }
}

#[cfg(unix)]
#[test]
fn format_preserves_permissions_and_rejects_symlink_writes() {
    use std::fs;
    use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};

    let workspace = TestWorkspace::empty();
    let path = workspace.path().join("Main.purs");
    workspace.write("Main.purs", "module Main where\nx=1\n");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    support::assert_success(&workspace.command(&["format", "--write", "Main.purs"]));
    assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o640);
    let inode = fs::metadata(&path).unwrap().ino();
    support::assert_success(&workspace.command(&["format", "--write", "Main.purs"]));
    assert_eq!(fs::metadata(&path).unwrap().ino(), inode, "unchanged file was replaced");

    symlink("Main.purs", workspace.path().join("Link.purs")).unwrap();
    let output = workspace.command(&["format", "--write", "Link.purs"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(fs::symlink_metadata(workspace.path().join("Link.purs")).unwrap().is_symlink());
    assert_eq!(workspace.read("Main.purs"), "module Main where\n\nx = 1\n");
    snapshot_output("format_symlink", &output);
    for paths in [["Main.purs", "Link.purs"], ["Link.purs", "Main.purs"]] {
        workspace.write("Main.purs", "module Main where\nx=2\n");
        let output = workspace.command(&["format", "--write", paths[0], paths[1]]);
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(workspace.read("Main.purs"), "module Main where\nx=2\n");
    }
}
