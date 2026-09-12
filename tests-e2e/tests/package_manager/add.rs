use super::support::{IrisExecutable, TestWorkspace, assert_success};

#[test]
fn adds_a_workspace_dependency_with_real_spago() {
    let mut manifests = vec![];
    for executable in IrisExecutable::ALL {
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

        let output =
            workspace.command_for(executable, &["add", "--package", "application", "library"]);
        assert_success(&output);
        manifests.push(workspace.read("packages/application/spago.yaml"));
        assert!(workspace.path().join("spago.lock").is_file());
        workspace.assert_spago_calls("", &[&["fetch", "-p", "application", "library"]]);
    }
    assert!(manifests.windows(2).all(|pair| pair[0] == pair[1]));

    insta::assert_snapshot!(manifests[0], @r#"
    package:
      name: application
      dependencies:
        - library: "*"
    "#);
}
