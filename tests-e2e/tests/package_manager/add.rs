use super::support::{TestWorkspace, assert_success};

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
