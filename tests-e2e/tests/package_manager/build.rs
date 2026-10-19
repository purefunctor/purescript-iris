use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, SystemTime};

use super::support::{TestWorkspace, assert_success};

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

    insta::assert_snapshot!(workspace.report(&output), @"
    status: 0
    --- stdout
    --- stderr
    ");
    insta::assert_snapshot!(workspace.metadata_modules().join("\n"), @"Main");
    insta::assert_snapshot!(workspace.spago_calls(), @r#"packages/application/src: spago "fetch" "-p" "application""#);
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

    insta::assert_snapshot!(workspace.report(&output), @"
    status: 0
    --- stdout
    --- stderr
    ");
    insta::assert_snapshot!(workspace.metadata_modules().join("\n"), @"
    Application
    Library
    ");
    insta::assert_snapshot!(workspace.spago_calls(), @r#"src: spago "fetch""#);
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

    insta::assert_snapshot!(workspace.report(&output), @"
    status: 0
    --- stdout
    --- stderr
    ");
    let lock: serde_json::Value = serde_json::from_str(&workspace.read("spago.lock")).unwrap();
    let selected_version = lock["packages"]["prelude"]["version"].as_str().unwrap();
    assert_ne!(selected_version, "999.0.0");
    assert!(workspace.path().join("output/Main/index.js").is_file());
    assert!(!workspace.path().join("output/Stale/index.js").exists());
    let mut prelude = workspace.module_metadata("Prelude").unwrap();
    let package = dunce::canonicalize(workspace.path()).unwrap();
    let package = package.join(format!(".spago/p/prelude-{selected_version}"));
    assert_eq!(Path::new(prelude["source"].as_str().unwrap()), package.join("src/Prelude.purs"));
    let source = workspace.normalize_output(prelude["source"].as_str().unwrap().as_bytes());
    prelude["source"] = source.into();
    prelude["iris"] = "[VERSION]".into();
    let prelude = serde_json::to_string_pretty(&prelude).unwrap();
    let selected_version = format!("prelude-{}", regex::escape(selected_version));
    insta::with_settings!({filters => vec![(selected_version.as_str(), "prelude-[SELECTED]")]}, {
        insta::assert_snapshot!(prelude, @r#"
        {
          "editable": false,
          "foreign": null,
          "iris": "[VERSION]",
          "package": "prelude",
          "source": "[WORKSPACE]/.spago/p/prelude-[SELECTED]/src/Prelude.purs",
          "version": 1
        }
        "#);
    });
    insta::assert_snapshot!(workspace.spago_calls(), @r#".: spago "fetch" "-p" "application""#);
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

    insta::assert_snapshot!(workspace.report(&output), @"
    status: 1
    --- stdout
    --- stderr
    workspace package 'ignored' was not found; available packages: application
    ");
    insta::assert_snapshot!(workspace.spago_calls(), @"");
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

    insta::assert_snapshot!(workspace.report(&output), @"
    status: 1
    --- stdout
    --- stderr
    workspace package 'child' was not found; available packages: application
    ");
    insta::assert_snapshot!(workspace.spago_calls(), @"");
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

    insta::assert_snapshot!(workspace.report(&output), @"
    status: 1
    --- stdout
    --- stderr
    ");
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

    insta::with_settings!({omit_expression => true}, {
        insta::assert_snapshot!("parse_error_diagnostics", workspace.report(&output));
    });
    assert!(!workspace.path().join("output/Main/index.js").exists());
}

#[test]
fn maps_node_stack_traces_to_purescript_sources() {
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

data Unit = Unit

foreign import fail :: Unit -> Unit

failure :: Unit
failure = fail Unit
"#,
    );
    workspace.write("src/Main.js", "export const fail = () => { throw new Error(\"failed\"); };\n");
    workspace.write("main.mjs", "import \"./output/Main/index.js\";\n");

    assert_success(&workspace.command(&["build", "--quiet"]));
    let source_map = workspace.read("output/Main/index.js.map");
    let source_map: serde_json::Value = serde_json::from_str(&source_map).unwrap();
    insta::assert_snapshot!(source_map["sources"], @r#"["../../src/Main.purs"]"#);
    let output = Command::new("node")
        .args(["--enable-source-maps", "main.mjs"])
        .current_dir(dunce::canonicalize(workspace.path()).unwrap())
        .output()
        .unwrap();

    assert!(!output.status.success());
    insta::assert_snapshot!(workspace.workspace_stack_frames(&output), @"
    at fail ([WORKSPACE]/output/Main/foreign.js:1:35)
    at <anonymous> ([WORKSPACE]/src/Main.purs:8:11)
    ");
}

#[test]
fn records_metadata_for_each_module_that_a_build_compiles() {
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

    insta::assert_snapshot!(workspace.report(&output), @"
    status: 0
    --- stdout
    --- stderr
    ");
    insta::assert_snapshot!(workspace.metadata_modules().join("\n"), @"
    Library
    Library.Types
    ");
    assert!(!workspace.path().join("output/Main").exists());

    assert_success(&workspace.command(&["build", "--quiet"]));
    insta::with_settings!({omit_expression => true}, {
        insta::assert_snapshot!("module_metadata", workspace.normalized_metadata());
    });
    insta::assert_snapshot!(workspace.spago_calls(), @r#"
    .: spago "fetch" "-p" "library"
    .: spago "fetch"
    "#);
}

#[test]
fn replaces_module_metadata_only_when_a_successful_build_changes_it() {
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

value = 1
"#,
    );
    assert_success(&workspace.command(&["build", "--quiet"]));

    workspace.write(
        "src/Main.purs",
        r#"module Main where

value = missing
"#,
    );
    workspace.write(
        "src/Extra.purs",
        r#"module Extra where
"#,
    );
    let failed = workspace.command(&["build", "--quiet", "--no-diagnostics"]);
    insta::assert_snapshot!(workspace.report(&failed), @"
    status: 1
    --- stdout
    --- stderr
    ");
    assert!(!workspace.path().join("output/Extra").exists());

    let metadata_path = workspace.path().join("output/Main/metadata.json");
    let old = SystemTime::UNIX_EPOCH + Duration::from_secs(86_400);
    File::options().write(true).open(&metadata_path).unwrap().set_modified(old).unwrap();
    workspace.write(
        "src/Main.purs",
        r#"module Main where

value = 2
"#,
    );
    assert_success(&workspace.command(&["build", "--quiet"]));
    assert!(workspace.read("output/Main/index.js").contains("value = 2"));
    assert!(workspace.module_metadata("Extra").is_some());
    let modified = fs::metadata(&metadata_path).unwrap().modified().unwrap();
    assert_eq!(modified, old, "a build that kept the metadata of Main rewrote it");

    let previous = fs::read_to_string(&metadata_path).unwrap();
    let mut reader = File::open(&metadata_path).unwrap();
    workspace.write(
        "src/Main.purs",
        r#"module Main where

foreign import value :: Int
"#,
    );
    workspace.write(
        "src/Main.js",
        r#"export const value = 3;
"#,
    );
    assert_success(&workspace.command(&["build", "--quiet"]));
    insta::assert_snapshot!(workspace.module_metadata("Main").unwrap()["foreign"], @r#""foreign.js""#);
    let mut read = String::new();
    reader.read_to_string(&mut read).unwrap();
    assert_eq!(read, previous, "a build rewrote the metadata of Main in place");
}
