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
        ("help_lsp_short", &["lsp", "-h"]),
        ("help_run", &["run", "--help"]),
        ("help_test", &["test", "--help"]),
        ("help_compile", &["compile", "--help"]),
        ("help_docs", &["docs", "--help"]),
        ("help_docs_typescript", &["docs", "typescript", "--help"]),
    ];

    for (name, arguments) in paths {
        let output = workspace.command(arguments);
        assert!(output.status.success(), "{name} failed");
        assert!(!output.stdout.is_empty(), "{name} did not write stdout");
        assert!(output.stderr.is_empty(), "{name} wrote stderr");
        snapshot_output(name, &output);
    }
}

#[test]
fn root_help_groups_commands_by_role() {
    let workspace = TestWorkspace::empty();
    let output = workspace.command(&["--help"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    let project = stdout.find("Project commands:").unwrap();
    let legacy = stdout.find("Legacy commands:").unwrap();
    let documentation = stdout.find("Documentation commands:").unwrap();
    assert!(project < legacy && legacy < documentation);
    for command in ["new", "add", "build", "watch", "lsp", "run", "test"] {
        assert!(stdout[project..legacy].contains(command));
    }
    assert!(stdout[legacy..documentation].contains("compile"));
    assert!(stdout[documentation..].contains("docs"));
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
fn invalid_option_values_point_to_the_supplied_argument() {
    let workspace = TestWorkspace::empty();
    for (name, arguments) in [
        ("invalid_choice", vec!["build", "--color", "alway"]),
        ("invalid_choice_equals", vec!["lsp", "--lsp-log=verböse"]),
        ("invalid_choice_escaped", vec!["lsp", "--lsp-log", "warn\n\"λ\""]),
        ("unicode_unknown_flag", vec!["--λ"]),
    ] {
        let output = workspace.command(&arguments);
        assert_eq!(output.status.code(), Some(2), "{name}");
        assert!(output.stdout.is_empty(), "{name} wrote stdout");
        snapshot_output(name, &output);
    }
}

#[test]
fn rejects_unknown_flags_and_duplicate_scalar_options() {
    let workspace = TestWorkspace::empty();
    let cases: &[(&str, &[&str])] = &[
        ("unknown_root_flag", &["--unknown"]),
        ("duplicate_lsp_scalar", &["lsp", "--config", "{}", "--config", "null"]),
        ("unknown_build_flag", &["build", "--unknown"]),
        ("duplicate_build_scalar", &["build", "--package", "one", "--package", "two"]),
        ("unknown_compile_flag", &["compile", "--unknown", "Main.purs"]),
        (
            "duplicate_compile_scalar",
            &["compile", "--output", "one", "--output", "two", "Main.purs"],
        ),
        ("unknown_docs_flag", &["docs", "--unknown", "--package", "."]),
        (
            "duplicate_docs_scalar",
            &["docs", "--output", "one", "--output", "two", "--package", "."],
        ),
        ("unknown_typescript_flag", &["docs", "typescript", "--unknown"]),
        (
            "duplicate_typescript_scalar",
            &["docs", "typescript", "--output", "one", "--output", "two"],
        ),
    ];

    for (name, arguments) in cases {
        let output = workspace.command(arguments);
        assert!(!output.status.success(), "{name} unexpectedly succeeded");
        assert!(output.stdout.is_empty(), "{name} wrote stdout");
        assert!(!output.stderr.is_empty(), "{name} did not write stderr");
        snapshot_output(name, &output);
    }
}

#[test]
fn rejects_missing_required_arguments() {
    let workspace = TestWorkspace::empty();
    let cases: &[(&str, &[&str])] = &[
        ("root_requires_subcommand", &[]),
        ("log_file_requires_subcommand", &["--log-file"]),
        ("add_requires_dependencies", &["add"]),
        ("compile_requires_input_or_package", &["compile"]),
        ("compile_package_requires_value", &["compile", "--package"]),
        ("docs_requires_package_or_project", &["docs"]),
        ("docs_package_requires_value", &["docs", "--package"]),
        ("docs_project_requires_value", &["docs", "--spago-project"]),
    ];

    for (name, arguments) in cases {
        let output = workspace.command(arguments);
        assert!(!output.status.success(), "{name} unexpectedly succeeded");
        snapshot_output(name, &output);
    }
}

#[test]
fn lsp_options_require_the_explicit_subcommand() {
    let workspace = TestWorkspace::empty();
    let cases: &[(&str, &[&str])] = &[
        ("root_stdio", &["--stdio"]),
        ("root_stdio_before_lsp", &["--stdio", "lsp"]),
        ("root_lsp_log", &["--lsp-log", "off", "lsp"]),
        ("root_query_log", &["--query-log", "off", "lsp"]),
        ("root_checking_log", &["--checking-log", "off", "lsp"]),
        ("root_config", &["--config", "{}", "lsp"]),
        ("root_config_file", &["--config-file", "missing.json", "lsp"]),
    ];

    for (name, arguments) in cases {
        let output = workspace.command(arguments);
        assert_eq!(output.status.code(), Some(2), "{name}");
        assert!(output.stdout.is_empty(), "{name} wrote stdout");
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
fn lsp_configuration_options_reject_invalid_arguments() {
    let workspace = TestWorkspace::empty();
    let cases: &[(&str, &[&str])] = &[
        ("config_requires_value", &["lsp", "--config"]),
        ("config_file_requires_value", &["lsp", "--config-file"]),
        ("config_conflicts_with_file", &["lsp", "--config", "{}", "--config-file", "missing.json"]),
        (
            "config_file_conflicts_with_literal",
            &["lsp", "--config-file", "missing.json", "--config", "{}"],
        ),
        ("duplicate_config_file", &["lsp", "--config-file", "one", "--config-file", "two"]),
        ("removed_source_command", &["lsp", "--source-command", "custom"]),
        ("removed_diagnostics_on_open", &["lsp", "--diagnostics-on-open", "false"]),
        ("removed_diagnostics_on_save", &["lsp", "--diagnostics-on-save", "false"]),
        ("removed_diagnostics_on_change", &["lsp", "--diagnostics-on-change"]),
    ];

    for (name, arguments) in cases {
        let output = workspace.command(arguments);
        assert_eq!(output.status.code(), Some(2), "{name}");
        assert!(output.stdout.is_empty(), "{name} wrote stdout");
        snapshot_output(name, &output);
    }
}

#[test]
fn lsp_rejects_invalid_json_configuration_before_starting() {
    let workspace = TestWorkspace::empty();
    let cases = [
        ("empty_json", ""),
        ("malformed_json", "{"),
        ("multiline_eof", "{\n  \"diagnostics\": {\n    \"onSave\":\n"),
        ("trailing_json", "{} false"),
        ("wrong_top_level", "false"),
        ("unknown_setting", r#"{"unknown":true}"#),
        ("unknown_diagnostic", r#"{"diagnostics":{"onOpened":false}}"#),
        ("wrong_diagnostic_type", r#"{"diagnostics":{"onSave":"false"}}"#),
        ("unknown_source_kind", r#"{"sources":{"kind":"unknown"}}"#),
        ("missing_source_program", r#"{"sources":{"kind":"command"}}"#),
        ("empty_source_program", r#"{"sources":{"kind":"command","program":""}}"#),
        ("blank_source_program", r#"{"sources":{"kind":"command","program":" \t\n\u3000"}}"#),
        (
            "invalid_source_arguments",
            r#"{"sources":{"kind":"command","program":"custom","arguments":[1]}}"#,
        ),
        (
            "multiline_program_before_arguments",
            "{\n  \"sources\": {\n    \"kind\": \"command\",\n    \"program\": \" \",\n    \"arguments\": [\"sources\"]\n  }\n}",
        ),
        (
            "multiline_program_before_kind",
            "{\n  \"sources\": {\n    \"program\": \" \",\n    \"kind\": \"command\"\n  }\n}",
        ),
        (
            "multiline_invalid_arguments",
            "{\n  \"sources\": {\n    \"arguments\": [\"λ\", 12, \"sources\"],\n    \"program\": \"custom\",\n    \"kind\": \"command\"\n  }\n}",
        ),
        (
            "multiline_unicode_unknown_key",
            "{\r\n\t\"sources\": {\"kind\": \"command\", \"program\": \"λ\"},\r\n\t\"diagnostics\": {\"λ\": true}\r\n}",
        ),
        ("unicode_syntax_error", "{\n\t\"diagnostics\": {\"onSave\": λ}\n}"),
        ("wrong_diagnostic_object", r#"{"diagnostics":{"onSave":{}}}"#),
        ("escaped_diagnostic_key", r#"{"diagnostics":{"on\u004fpened":true}}"#),
        ("duplicate_diagnostic", r#"{"diagnostics":{"onSave":null,"onSave":true}}"#),
        (
            "duplicate_program",
            r#"{"sources":{"kind":"command","program":"first","program":"second"}}"#,
        ),
        (
            "null_source_arguments",
            r#"{"sources":{"kind":"command","program":"custom","arguments":null}}"#,
        ),
    ];

    for (name, content) in cases {
        workspace.write("config/settings.json", content);
        for (transport, arguments) in [
            ("inline", vec!["lsp", "--config", content]),
            ("file", vec!["lsp", "--config-file", "config/settings.json"]),
        ] {
            let output = workspace.command(&arguments);
            assert_eq!(output.status.code(), Some(2), "{name}: {transport}");
            assert!(output.stdout.is_empty(), "{name}: {transport} wrote stdout");
            snapshot_output(&format!("config_{name}_{transport}"), &output);
        }
    }
}

#[test]
fn lsp_reports_configuration_file_read_errors() {
    let workspace = TestWorkspace::empty();
    std::fs::write(workspace.path().join("invalid-utf8.json"), [0xff]).unwrap();
    let cases = [("missing.json", "missing"), ("invalid-utf8.json", "invalid_utf8")];
    for (path, name) in cases {
        let output = workspace.command(&["lsp", "--config-file", path]);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        insta::with_settings!({filters => vec![
            (r"No such file or directory \(os error 2\)|The system cannot find the file specified\. \(os error 2\)", "[FILE NOT FOUND]"),
        ]}, {
            snapshot_output(&format!("config_file_{name}"), &output);
        });
    }
}

#[test]
fn typescript_output_resolves_relative_to_the_working_directory() {
    let workspace = TestWorkspace::empty();
    let output =
        workspace.command_in("project", &["docs", "typescript", "--output", "../generated"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(workspace.path().join("generated/docs-schema.ts").is_file());
}

#[test]
fn v2_exposes_ported_commands_only() {
    let workspace = TestWorkspace::empty();
    for (name, arguments) in [
        ("v2_help_root", &["--help"][..]),
        ("v2_help_new", &["new", "--help"][..]),
        ("v2_help_add", &["add", "--help"][..]),
        ("v2_help_build", &["build", "--help"][..]),
        ("v2_help_lsp", &["lsp", "--help"][..]),
    ] {
        let output = workspace.v2_command(arguments);
        assert!(output.status.success(), "{name}");
        assert!(!output.stdout.is_empty(), "{name} did not write stdout");
        assert!(output.stderr.is_empty(), "{name} wrote stderr");
        snapshot_output(name, &output);
    }

    for command in ["compile", "watch", "docs"] {
        let output = workspace.v2_command(&[command]);
        assert_eq!(output.status.code(), Some(2), "{command}");
        assert!(output.stdout.is_empty(), "{command} wrote stdout");
    }

    let output = workspace.v2_command(&["add"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    snapshot_output("v2_add_requires_dependencies", &output);
}

#[test]
fn v2_rejects_invalid_lsp_configuration_before_starting() {
    let workspace = TestWorkspace::empty();
    let cases: &[(&str, &[&str])] = &[
        ("v2_config_invalid", &["lsp", "--config", "{"]),
        ("v2_config_conflict", &["lsp", "--config", "{}", "--config-file", "missing.json"]),
        ("v2_config_file_missing", &["lsp", "--config-file", "missing.json"]),
    ];
    for (name, arguments) in cases {
        let output = workspace.v2_command(arguments);
        assert_eq!(output.status.code(), Some(2), "{name}");
        assert!(output.stdout.is_empty(), "{name} wrote stdout");
        insta::with_settings!({filters => vec![
            (r"No such file or directory \(os error 2\)|The system cannot find the file specified\. \(os error 2\)", "[FILE NOT FOUND]"),
        ]}, {
            snapshot_output(name, &output);
        });
    }
}
