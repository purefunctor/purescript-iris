use std::fs::{self, File};
use std::process::Output;
use std::time::{Duration, SystemTime};

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
    workspace.assert_spago_calls("packages/application/src", &[&["fetch"]]);
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
    workspace.assert_spago_calls("", &[&["fetch"]]);
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
}

#[test]
fn records_the_whole_workspace_whichever_package_is_built() {
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
        "packages/library/spago.yaml",
        r#"package:
  name: library
  dependencies: []
"#,
    );
    workspace.write(
        "packages/metadata/spago.yaml",
        r#"package:
  name: metadata
  dependencies: [library]
"#,
    );
    workspace.write(
        "packages/library/src/Library.purs",
        r#"module Library where

foreign import value :: Int
"#,
    );
    workspace.write(
        "packages/library/src/Library.js",
        r#"export const value = 42;
"#,
    );
    workspace.write(
        "packages/library/src/Library/Types.purs",
        r#"module Library.Types where
"#,
    );
    workspace.write(
        "src/Main.purs",
        r#"module Main where

import Library (value)

foreign import render :: Int -> Int
"#,
    );
    workspace.write(
        "src/Main.jsx",
        r#"export const render = (value) => value;
"#,
    );

    let output = workspace.command(&["build", "--quiet", "--package", "library"]);

    assert_success(&output);
    assert!(workspace.path().join("output/Library/foreign.js").is_file());
    assert!(!workspace.path().join("output/Main").exists());
    insta::with_settings!({omit_expression => true}, {
        insta::assert_snapshot!("build_manifest", workspace.normalized_manifest());
    });

    let library_manifest = workspace.read("output/manifest.json");
    assert_success(&workspace.command(&["build", "--quiet"]));
    assert_eq!(workspace.read("output/manifest.json"), library_manifest);
    let manifest = workspace.manifest().unwrap();
    let modules = manifest["modules"].as_object().unwrap().values();
    let files = modules.flat_map(|module| [&module["javascript"], &module["foreign"]]);
    for file in files.filter_map(serde_json::Value::as_str) {
        assert!(workspace.path().join("output").join(file).is_file(), "{file} does not exist");
    }
}

#[test]
fn replaces_the_manifest_only_after_successful_builds_that_change_the_output() {
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

import Library (value)

main = value
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

value = 1
"#,
    );
    let manifest_path = workspace.path().join("output/manifest.json");

    assert_success(&workspace.command(&["build", "--quiet"]));
    let previous = workspace.read("output/manifest.json");
    workspace.write(
        "packages/library/src/Library.purs",
        r#"module Library where

value = missing
"#,
    );
    let arguments = ["build", "--quiet", "--package", "library", "--no-diagnostics"];
    let failed = workspace.command(&arguments);
    assert_eq!(failed.status.code(), Some(1));
    assert_eq!(workspace.read("output/manifest.json"), previous);

    workspace.write(
        "packages/library/src/Library.purs",
        r#"module Library where

value = 1
"#,
    );
    assert_success(&workspace.command(&["build", "--quiet", "--package", "library"]));
    let old = SystemTime::UNIX_EPOCH + Duration::from_secs(86_400);
    let set_modified = |time| {
        File::options().write(true).open(&manifest_path).unwrap().set_modified(time).unwrap()
    };
    let modified = || fs::metadata(&manifest_path).unwrap().modified().unwrap();
    set_modified(old);
    assert_success(&workspace.command(&["build", "--quiet", "--package", "library"]));
    assert_eq!(modified(), old, "a build that changes nothing replaced the manifest");

    workspace.write(
        "packages/library/src/Library.purs",
        r#"module Library where

value = 2
"#,
    );
    assert_success(&workspace.command(&["build", "--quiet", "--package", "library"]));
    assert_eq!(workspace.read("output/manifest.json"), previous);
    assert_ne!(modified(), old, "a build that changed a module kept the manifest");
}

#[test]
fn removes_the_manifest_when_writing_the_output_fails() {
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

foreign import value :: Int
"#,
    );
    workspace.write(
        "src/Main.js",
        r#"export const value = 42;
"#,
    );
    assert_success(&workspace.command(&["build", "--quiet"]));
    assert!(workspace.manifest().is_some());

    let foreign_output = workspace.path().join("output/Main/foreign.js");
    fs::remove_file(&foreign_output).unwrap();
    fs::create_dir(&foreign_output).unwrap();
    let failed = workspace.command(&["build", "--quiet"]);
    assert_eq!(failed.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&failed.stderr).starts_with("failed to write "));
    assert!(workspace.manifest().is_none());

    fs::remove_dir(&foreign_output).unwrap();
    assert_success(&workspace.command(&["build", "--quiet"]));
    assert!(workspace.manifest().unwrap()["modules"]["Main"].is_object());
}

#[test]
fn rejects_a_module_name_that_two_workspace_files_declare() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "spago.yaml",
        r#"workspace: {}
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
"#,
    );
    workspace.write(
        "packages/tool/spago.yaml",
        r#"package:
  name: tool
  dependencies: []
"#,
    );
    workspace.write(
        "packages/tool/src/Main.purs",
        r#"module Main where
"#,
    );
    assert_success(&workspace.command(&["build", "--quiet", "--package", "library"]));
    let previous = workspace.read("output/manifest.json");

    workspace.write(
        "packages/tool/src/Clash.purs",
        r#"module Main where
"#,
    );
    workspace.write(
        "packages/library/src/Library.purs",
        r#"module Library where

value = 1
"#,
    );
    let output = workspace.command(&["build", "--quiet", "--package", "library"]);

    assert_eq!(output.status.code(), Some(1));
    let canonical = dunce::canonicalize(workspace.path()).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr)
        .replace(canonical.to_str().unwrap(), "[WORKSPACE]")
        .replace(workspace.path().to_str().unwrap(), "[WORKSPACE]")
        .replace('\\', "/");
    insta::assert_snapshot!(stderr, @"module Main is declared by both [WORKSPACE]/packages/tool/src/Clash.purs and [WORKSPACE]/packages/tool/src/Main.purs");
    assert!(!workspace.read("output/Library/index.js").contains("value"));
    assert_eq!(workspace.read("output/manifest.json"), previous);
}
