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
        r#"package:
  name: application
  dependencies: []
"#,
    );
    workspace.write(
        "packages/application/src/Main.purs",
        r#"module Main where

value = 42
"#,
    );
    workspace.write(
        "packages/library/spago.yaml",
        r#"package:
  name: library
  dependencies: []
"#,
    );
    workspace.write("packages/library/src/Library.purs", "module Library where\n");

    let output = workspace.command_in("packages/application/src", &["build", "--quiet"]);

    assert_success(&output);
    assert!(workspace.path().join("output/Main/index.js").is_file());
    assert!(!workspace.path().join("output/Library/index.js").exists());
    let manifest = read_manifest(&workspace);
    assert_eq!(object_keys(&manifest["packages"]), ["application"]);
    assert_eq!(object_keys(&manifest["modules"]), ["Main"]);
    workspace.assert_spago_calls("packages/application/src", &[&["fetch", "-p", "application"]]);
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
        r#"module Application where

import Library (library)

application = library
"#,
    );
    workspace.write(
        "packages/library/spago.yaml",
        r#"package:
  name: library
  dependencies: []
"#,
    );
    workspace.write(
        "packages/library/src/Library.purs",
        r#"module Library where

library = 42
"#,
    );

    let output = workspace.command_in("src", &["build", "--quiet"]);

    assert_success(&output);
    assert!(workspace.path().join("output/Application/index.js").is_file());
    assert!(workspace.path().join("output/Library/index.js").is_file());
    workspace.assert_spago_calls("src", &[&["fetch"]]);
}

#[test]
fn builds_with_the_registry_version_selected_by_spago() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "spago.yaml",
        r#"workspace:
  packageSet:
    registry: 64.10.0
package:
  name: application
  dependencies: [prelude]
"#,
    );
    workspace.write(
        "src/Main.purs",
        r#"module Main where

import Prelude

value = unit
"#,
    );
    workspace.write(".spago/p/prelude-999.0.0/src/Stale.purs", "module Stale where\n");

    let output = workspace.command(&["build", "--quiet"]);

    assert_success(&output);
    let lock: serde_json::Value = serde_json::from_str(&workspace.read("spago.lock")).unwrap();
    let selected_version = lock["packages"]["prelude"]["version"].as_str().unwrap();
    assert_ne!(selected_version, "999.0.0");
    assert!(workspace.path().join("output/Main/index.js").is_file());
    assert!(!workspace.path().join("output/Stale/index.js").exists());
    let manifest = read_manifest(&workspace);
    let prelude = &manifest["packages"]["prelude"];
    assert_eq!(prelude["editable"], false);
    let root = manifest_workspace(&workspace);
    assert_eq!(prelude["root"], format!("{root}/.spago/p/prelude-{selected_version}"));
    assert_eq!(manifest["modules"]["Prelude"]["package"], "prelude");
    assert_eq!(manifest["modules"]["Main"]["package"], "application");
    workspace.assert_spago_calls("", &[&["fetch", "-p", "application"]]);
}

#[test]
fn excludes_gitignored_packages_from_selection() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "spago.yaml",
        r#"workspace: {}
package:
  name: application
  dependencies: []
"#,
    );
    workspace.write(".gitignore", "ignored/\n");
    workspace.write(
        "ignored/spago.yaml",
        r#"package:
  name: ignored
  dependencies: []
"#,
    );

    let output = workspace.command(&["build", "--quiet", "--package", "ignored"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.ends_with(b"\n"));
    insta::assert_snapshot!(String::from_utf8_lossy(&output.stderr), @"workspace package 'ignored' was not found; available packages: application");
    workspace.assert_spago_calls("", &[]);
}

#[test]
fn excludes_nested_workspaces_from_selection() {
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
        "nested/spago.yaml",
        r#"workspace: {}
package:
  name: nested
  dependencies: []
"#,
    );
    workspace.write(
        "nested/packages/child/spago.yaml",
        r#"package:
  name: child
  dependencies: []
"#,
    );

    let output = workspace.command(&["build", "--quiet", "--package", "child"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.ends_with(b"\n"));
    insta::assert_snapshot!(String::from_utf8_lossy(&output.stderr), @"workspace package 'child' was not found; available packages: application");
    workspace.assert_spago_calls("", &[]);
}

#[test]
fn suppresses_diagnostics_without_changing_failure_status() {
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

broken = missing
"#,
    );

    let output = workspace.command(&["build", "--quiet", "--no-diagnostics"]);

    snapshot_output("suppressed_error_diagnostics", &output);
}

#[test]
fn reports_parse_errors_as_build_failures() {
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

value :: Int
value = if true then 1
"#,
    );

    let output = workspace.command(&["build", "--quiet"]);

    snapshot_output("parse_error_diagnostics", &output);
    assert!(!workspace.path().join("output/Main/index.js").exists());
    assert!(!workspace.path().join("output/iris-manifest.json").exists());
}

#[test]
fn writes_a_build_manifest_describing_the_generated_modules() {
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
        "src/Main.purs",
        r#"module Main where

import Library (library)

foreign import offset :: Int

main :: Int
main = library
"#,
    );
    workspace.write("src/Main.js", "export const offset = 1;\n");
    workspace.write(
        "packages/library/spago.yaml",
        r#"package:
  name: library
  dependencies: []
"#,
    );
    workspace.write(
        "packages/library/src/Library.purs",
        r#"module Library where

library :: Int
library = 42
"#,
    );

    let output = workspace.command(&["build", "--quiet"]);

    assert_success(&output);
    let version = workspace.command(&["--version"]);
    let version = String::from_utf8_lossy(&version.stdout);
    let version = version.trim_end().strip_prefix("iris ").unwrap();
    let manifest = read_manifest(&workspace);
    assert_eq!(manifest["iris"], version);
    let root = manifest_workspace(&workspace);
    assert_eq!(manifest["workspace_root"], root);
    let root_pattern = regex::escape(&root);
    let filters = vec![
        (r#""iris": "[^"]+""#, r#""iris": "[version]""#),
        (root_pattern.as_str(), "[workspace]"),
    ];
    insta::with_settings!({filters => filters}, {
        insta::assert_snapshot!(workspace.read("output/iris-manifest.json"));
    });
}

fn read_manifest(workspace: &TestWorkspace) -> serde_json::Value {
    serde_json::from_str(&workspace.read("output/iris-manifest.json")).unwrap()
}

/// The workspace root as the manifest spells absolute paths, with forward slashes.
fn manifest_workspace(workspace: &TestWorkspace) -> String {
    let root = dunce::canonicalize(workspace.path()).unwrap();
    root.to_str().unwrap().replace('\\', "/")
}

fn object_keys(value: &serde_json::Value) -> Vec<&str> {
    value.as_object().unwrap().keys().map(String::as_str).collect()
}
