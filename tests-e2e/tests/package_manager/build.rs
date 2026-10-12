use std::path::Path;
use std::process::Output;
use std::time::Duration;

use super::support::{TestWorkspace, assert_success, normalized_manifest};

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
fn writes_a_build_manifest_for_the_generated_modules() {
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

main :: Int
main = library
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

foreign import library :: Int
"#,
    );
    workspace.write(
        "packages/library/src/Library.js",
        r#"export const library = 42;
"#,
    );

    let output = workspace.command(&["build", "--quiet"]);

    assert_success(&output);
    insta::with_settings!({omit_expression => true}, {
        insta::assert_snapshot!("manifest_workspace", normalized_manifest(&workspace, "output"));
    });
}

#[test]
fn records_fetched_packages_as_not_editable() {
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

    let output = workspace.command(&["build", "--quiet"]);

    assert_success(&output);
    let manifest: serde_json::Value =
        serde_json::from_str(&normalized_manifest(&workspace, "output")).unwrap();
    insta::with_settings!({omit_expression => true}, {
        insta::assert_snapshot!(
            "manifest_registry_packages",
            serde_json::to_string_pretty(&manifest["packages"]).unwrap()
        );
    });
    // The generated JavaScript imports `unit` from the module that defines it.
    assert_eq!(manifest["modules"]["Main"]["package"], "application");
    assert_eq!(manifest["modules"]["Main"]["dependencies"], serde_json::json!(["Data.Unit"]));
    assert_eq!(manifest["modules"]["Data.Unit"]["package"], "prelude");
    assert_eq!(manifest["modules"]["Data.Unit"]["javascript"], "Data.Unit/index.js");
    assert_eq!(manifest["modules"]["Data.Unit"]["foreign"], "Data.Unit/foreign.js");
    assert!(manifest["modules"].get("Prim").is_none());
}

#[test]
fn records_a_local_package_outside_the_workspace() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "project/spago.yaml",
        r#"workspace:
  extraPackages:
    shared:
      path: ../shared
package:
  name: application
  dependencies: [shared]
"#,
    );
    workspace.write(
        "project/src/Main.purs",
        r#"module Main where

import Shared (shared)

main :: Int
main = shared
"#,
    );
    workspace.write(
        "shared/spago.yaml",
        r#"package:
  name: shared
  dependencies: []
"#,
    );
    workspace.write(
        "shared/src/Shared.purs",
        r#"module Shared where

shared :: Int
shared = 1
"#,
    );

    let output = workspace.command_in("project", &["build", "--quiet"]);

    assert_success(&output);
    insta::with_settings!({omit_expression => true}, {
        insta::assert_snapshot!(
            "manifest_local_package_outside_the_workspace",
            normalized_manifest(&workspace, "project/output")
        );
    });
}

#[test]
fn writes_the_manifest_into_a_custom_output_directory() {
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
"#,
    );

    let output = workspace.command_in("src", &["build", "--quiet", "--output", "generated"]);

    assert_success(&output);
    assert!(!workspace.path().join("output").exists());
    insta::with_settings!({omit_expression => true}, {
        insta::assert_snapshot!(
            "manifest_custom_output",
            normalized_manifest(&workspace, "src/generated")
        );
    });
}

#[test]
fn replaces_the_manifest_exactly_when_the_output_changes() {
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
value = 42
"#,
    );
    let manifest = workspace.path().join("output/iris-manifest.json");
    let modified = |path: &Path| std::fs::metadata(path).unwrap().modified().unwrap();

    assert_success(&workspace.command(&["build", "--quiet"]));
    let first_manifest = workspace.read("output/iris-manifest.json");
    let first_modified = modified(&manifest);

    std::thread::sleep(Duration::from_millis(50));
    assert_success(&workspace.command(&["build", "--quiet"]));
    assert_eq!(modified(&manifest), first_modified, "an unchanged build keeps the manifest");

    std::thread::sleep(Duration::from_millis(50));
    workspace.write(
        "src/Main.purs",
        r#"module Main where

value :: Int
value = 43
"#,
    );
    assert_success(&workspace.command(&["build", "--quiet"]));
    assert!(modified(&manifest) > first_modified, "a code-only edit replaces the manifest");
    assert_eq!(workspace.read("output/iris-manifest.json"), first_manifest);
}

#[test]
fn keeps_the_output_and_manifest_of_the_last_successful_build_on_errors() {
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
value = 42
"#,
    );
    let manifest = workspace.path().join("output/iris-manifest.json");
    assert!(!manifest.exists());

    assert_success(&workspace.command(&["build", "--quiet"]));
    let manifest_after_success = workspace.read("output/iris-manifest.json");
    let module_after_success = workspace.read("output/Main/index.js");

    workspace.write(
        "src/Main.purs",
        r#"module Main where

value :: Int
value = missing
"#,
    );
    workspace.write(
        "src/Extra.purs",
        r#"module Extra where
"#,
    );
    let failed = workspace.command(&["build", "--quiet", "--no-diagnostics"]);
    assert_eq!(failed.status.code(), Some(1));
    assert_eq!(workspace.read("output/iris-manifest.json"), manifest_after_success);
    assert_eq!(workspace.read("output/Main/index.js"), module_after_success);
    assert!(!workspace.path().join("output/Extra").exists());

    let resilient = workspace.command(&["build", "--quiet", "--no-diagnostics", "--resilient"]);
    assert_eq!(resilient.status.code(), Some(1));
    insta::with_settings!({omit_expression => true}, {
        insta::assert_snapshot!("manifest_resilient", normalized_manifest(&workspace, "output"));
    });
}

#[test]
fn records_the_runtime_module_when_a_module_needs_it() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "spago.yaml",
        r#"workspace: {}
package:
  name: application
  dependencies: []
"#,
    );
    // The instance member is the constrained function applied to the instance itself, which
    // the generated JavaScript evaluates when the module loads, so the binding is lazy.
    workspace.write(
        "src/Main.purs",
        r#"module Main where

class Container f where
  size :: forall a. f a -> Int
  describe :: forall a. f a -> Int

data Box a = Box a

instance Container Box where
  size _ = 1
  describe = describeContainer

describeContainer :: forall f a. Container f => f a -> Int
describeContainer container = size container
"#,
    );

    let output = workspace.command(&["build", "--quiet"]);

    assert_success(&output);
    let manifest: serde_json::Value =
        serde_json::from_str(&workspace.read("output/iris-manifest.json")).unwrap();
    assert_eq!(manifest["runtime"], "runtime.js");
    assert!(workspace.path().join("output/runtime.js").is_file());
}
