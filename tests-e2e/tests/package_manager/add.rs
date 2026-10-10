use std::path::Path;

use super::support::{TestWorkspace, assert_success, install_spago_launcher};

#[test]
fn adds_a_workspace_dependency_with_real_spago() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "spago.yaml",
        r#"workspace: {}
"#,
    );
    workspace.write(
        "packages/application/spago.yaml",
        r#"package:
  name: application
  dependencies: []
"#,
    );
    workspace.write(
        "packages/library/spago.yaml",
        r#"package:
  name: library
  dependencies: []
"#,
    );

    let output = workspace.command(&["add", "--package", "application", "library"]);
    assert_success(&output);
    assert!(workspace.path().join("spago.lock").is_file());
    workspace.assert_spago_calls("", &[&["fetch", "-p", "application", "library"]]);

    insta::assert_snapshot!(workspace.read("packages/application/spago.yaml"), @r#"
    package:
      name: application
      dependencies:
        - library: "*"
    "#);
}

#[test]
fn adds_a_workspace_dependency_with_local_spago() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "spago.yaml",
        r#"workspace: {}
"#,
    );
    workspace.write(
        "packages/application/spago.yaml",
        r#"package:
  name: application
  dependencies: []
"#,
    );
    workspace.write(
        "packages/library/spago.yaml",
        r#"package:
  name: library
  dependencies: []
"#,
    );

    install_spago_launcher(
        &workspace.path().join("node_modules/.bin"),
        Path::new(env!("CARGO_BIN_EXE_spago-e2e")),
    );
    let mut command = workspace.command_builder("packages/application", &["add", "library"]);
    command.env_remove("IRIS_SPAGO");
    let output = command.output().unwrap();
    assert_success(&output);
    assert!(workspace.path().join("spago.lock").is_file());
    workspace
        .assert_spago_calls("packages/application", &[&["fetch", "-p", "application", "library"]]);

    insta::assert_snapshot!(workspace.read("packages/application/spago.yaml"), @r#"
    package:
      name: application
      dependencies:
        - library: "*"
    "#);
}
