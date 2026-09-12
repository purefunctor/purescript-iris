use super::support::{TestWorkspace, assert_success};

#[test]
fn creates_a_spago_project_without_running_spago() {
    let workspace = TestWorkspace::empty();
    let output = workspace.command(&["new", "--name", "example"]);
    assert_success(&output);
    workspace.assert_spago_calls("", &[]);

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
    workspace: {}
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
