use std::fs;

use super::support::{TestWorkspace, assert_success};

#[test]
fn creates_a_spago_project_with_the_latest_supported_package_set() {
    let workspace = TestWorkspace::empty();
    let output = workspace.command(&["new", "--name", "example"]);
    assert_success(&output);
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Created package `example` with package set 81.1.0.\n\
         Run `iris build` to get started.\n"
    );
    assert!(output.stderr.is_empty());
    workspace
        .assert_spago_calls("", &[&["registry", "package-sets", "--latest", "--json", "--quiet"]]);
    fs::remove_file(workspace.path().join("spago-calls")).unwrap();

    insta::assert_snapshot!(workspace.summary(), @r#"
    --- .gitignore
    .spago/
    output/
    --- spago.yaml
    package:
      name: example
      dependencies:
        - console
        - effect
        - prelude
      test:
        main: Test.Main
        dependencies:
          - assert
    workspace:
      packageSet:
        registry: 81.1.0
    --- src/Main.purs
    module Main where

    import Prelude

    import Effect (Effect)
    import Effect.Console (log)

    main :: Effect Unit
    main = do
      log "🍝"
    --- test/Test/Main.purs
    module Test.Main where

    import Prelude

    import Effect (Effect)
    import Effect.Class.Console (log)

    main :: Effect Unit
    main = do
      log "🍕"
      log "You should add some tests."
    "#);
}

#[test]
fn builds_a_new_project() {
    let workspace = TestWorkspace::empty();
    let new_output = workspace.command(&["new", "--name", "example"]);
    assert_success(&new_output);

    let build_output = workspace.command(&["build", "--quiet"]);
    assert_success(&build_output);
    assert!(workspace.path().join("output/Main/index.js").is_file());
    workspace.assert_spago_calls(
        "",
        &[
            &["registry", "package-sets", "--latest", "--json", "--quiet"],
            &["fetch", "-p", "example"],
        ],
    );
}

#[test]
fn does_not_create_a_partial_project_when_package_set_discovery_fails() {
    let workspace = TestWorkspace::empty();
    workspace.set_env("IRIS_E2E_SPAGO_FAIL", "registry unavailable");

    let output = workspace.command(&["new", "--name", "example"]);
    assert!(!output.status.success());
    assert!(!workspace.path().join("spago.yaml").exists());
    assert!(!workspace.path().join("src/Main.purs").exists());
    workspace
        .assert_spago_calls("", &[&["registry", "package-sets", "--latest", "--json", "--quiet"]]);
}

#[test]
fn does_not_overwrite_existing_project_files() {
    let workspace = TestWorkspace::empty();
    workspace.write("src/Main.purs", "original");

    let output = workspace.command(&["new", "--name", "example"]);
    assert!(!output.status.success());
    assert_eq!(workspace.read("src/Main.purs"), "original");
    assert!(!workspace.path().join("spago.yaml").exists());
}

#[test]
fn does_not_create_a_partial_project_when_a_source_directory_is_a_file() {
    let workspace = TestWorkspace::empty();
    workspace.write("src", "original");

    let output = workspace.command(&["new", "--name", "example"]);
    assert!(!output.status.success());
    assert_eq!(workspace.read("src"), "original");
    assert!(!workspace.path().join("spago.yaml").exists());
}

#[test]
fn does_not_create_a_nested_workspace() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "spago.yaml",
        r#"workspace: {}
package:
  name: root
"#,
    );

    let output = workspace.command_in("packages/application", &["new", "--name", "application"]);
    assert!(!output.status.success());
    assert!(!workspace.path().join("packages/application/spago.yaml").exists());
}
