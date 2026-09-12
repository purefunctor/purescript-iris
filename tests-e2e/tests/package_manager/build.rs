use super::support::{TestWorkspace, assert_success};

fn diagnostic_settings(workspace: &TestWorkspace) -> insta::Settings {
    let mut settings = insta::Settings::clone_current();
    settings.set_strip_ansi_escape_codes(true);
    let workspace_path = std::fs::canonicalize(workspace.path()).unwrap();
    let workspace_path = workspace_path.to_string_lossy();
    let workspace_path = regex::escape(workspace_path.trim_start_matches(r"\\?\"));
    settings.add_filter(&format!(r"{workspace_path}[/\\]"), "");
    settings.add_filter(r"src\\Main\.purs", "src/Main.purs");
    settings.add_filter(
        concat!(
            r"(?m)^(?:Reading Spago workspace configuration\.\.\.",
            r"|Refreshing the Registry Index\.\.\.",
            r"|Cloning https://github\.com/purescript/registry-index\.git",
            r"|Cloning https://github\.com/purescript/registry\.git",
            r"|✓ Selecting package to build: application",
            r#"|Adding dependency ranges to the config in "spago.yaml""#,
            r"|Downloading dependencies\.\.\.",
            r"|No lockfile found, generating it\.\.\.",
            r"|Lockfile written to spago.lock\. Please commit this file\.)\r?\n(?:\r?\n)*",
        ),
        "",
    );
    settings
}

#[test]
fn builds_a_single_package_with_real_spago() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "spago.yaml",
        r#"workspace: {}
package:
  name: application
  dependencies: []
"#,
    );
    workspace.write(
        "src/Main.purs",
        r#"module Main where

value = 42
"#,
    );

    let output = workspace.command(&["build", "--quiet"]);
    assert_success(&output);
    assert!(workspace.path().join("spago.lock").is_file());
    assert!(workspace.path().join("output/Main/index.js").is_file());
    workspace.assert_spago_calls(
        "",
        &[&["fetch", "-p", "application"], &["sources", "--json", "-p", "application"]],
    );
}

#[test]
fn builds_resilient_output_despite_diagnostics() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "spago.yaml",
        r#"workspace: {}
package:
  name: application
  dependencies: []
"#,
    );
    workspace.write(
        "src/Main.purs",
        r#"module Main where

usable = 42

broken = missing
"#,
    );

    let output = workspace.command(&["build", "--quiet", "--resilient"]);
    assert!(
        !output.status.success(),
        "command unexpectedly succeeded\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    let _settings = diagnostic_settings(&workspace).bind_to_scope();
    insta::assert_snapshot!("resilient_source_diagnostics", stderr);

    let generated = workspace.read("output/Main/index.js");
    assert!(generated.contains("Generated code reached a source error"));
    assert!(generated.contains("export const broken"));
    assert!(generated.contains("export const usable = 42 | 0;"));
    workspace.assert_spago_calls(
        "",
        &[&["fetch", "-p", "application"], &["sources", "--json", "-p", "application"]],
    );
}

#[test]
fn gates_initializer_cycle_output_on_resilience() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "spago.yaml",
        r#"workspace: {}
package:
  name: application
  dependencies: []
"#,
    );
    workspace.write(
        "src/Main.purs",
        r#"module Main where

first :: Int
first = second

second :: Int
second = first
"#,
    );

    let strict = workspace.command(&["build", "--quiet"]);
    assert!(!strict.status.success());
    let stderr = String::from_utf8_lossy(&strict.stderr);
    let _settings = diagnostic_settings(&workspace).bind_to_scope();
    insta::assert_snapshot!("strict_initializer_cycle_diagnostics", stderr);
    assert!(!workspace.path().join("output/Main/index.js").exists());

    let resilient = workspace.command(&["build", "--quiet", "--resilient"]);
    assert!(!resilient.status.success());
    let stderr = String::from_utf8_lossy(&resilient.stderr);
    insta::assert_snapshot!("resilient_initializer_cycle_diagnostics", stderr);
    let generated = workspace.read("output/Main/index.js");
    assert!(generated.contains("Top-level value initializer cycle"));
    assert!(!generated.contains("@__PURE__"));
    workspace.assert_spago_calls(
        "",
        &[
            &["fetch", "-p", "application"],
            &["sources", "--json", "-p", "application"],
            &["fetch", "-p", "application"],
            &["sources", "--json", "-p", "application"],
        ],
    );
}

#[test]
fn reports_backend_failures_as_diagnostics() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "spago.yaml",
        r#"workspace: {}
package:
  name: application
  dependencies: []
"#,
    );
    workspace.write(
        "src/Main.purs",
        r#"module Main where

import Iris.StyleX (Props, Style, props)

partialProps :: Style -> Props
partialProps = props
"#,
    );

    let output = workspace.command(&["build", "--quiet"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    let _settings = diagnostic_settings(&workspace).bind_to_scope();
    insta::assert_snapshot!("backend_failure_diagnostics", stderr);
    assert!(!workspace.path().join("output/Main/index.js").exists());
    workspace.assert_spago_calls(
        "",
        &[&["fetch", "-p", "application"], &["sources", "--json", "-p", "application"]],
    );
}

#[test]
fn builds_the_whole_workspace_from_a_root_package_subdirectory() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "spago.yaml",
        r#"workspace: {}
package:
  name: application
  dependencies: [library]
"#,
    );
    workspace.write(
        "src/Application.purs",
        "module Application where\n\nimport Library (library)\n\napplication = library\n",
    );
    workspace.write(
        "packages/library/spago.yaml",
        r#"package:
  name: library
  dependencies: []
"#,
    );
    workspace.write("packages/library/src/Library.purs", "module Library where\n\nlibrary = 42\n");

    let output = workspace.command_in("src", &["build", "--quiet"]);
    assert_success(&output);
    assert!(workspace.path().join("output/Application/index.js").is_file());
    assert!(workspace.path().join("output/Library/index.js").is_file());
    workspace.assert_spago_calls("src", &[&["fetch"], &["sources", "--json"]]);
}
