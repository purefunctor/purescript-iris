use super::support::{TestWorkspace, assert_success};

#[test]
fn tests_the_configured_module_with_node() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "spago.yaml",
        r#"workspace: {}
package:
  name: application
  dependencies: []
  test:
    main: Application.Test
    dependencies: []
    execArgs: [from-manifest]
"#,
    );
    workspace.write(
        "test/Application/Test.purs",
        r#"module Application.Test where

data Unit = Unit
foreign import data Effect :: Type -> Type
foreign import main :: Effect Unit
"#,
    );
    workspace.write(
        "test/Application/Test.js",
        "export const main = () => console.log(`tests ran: ${process.argv.slice(2).join(\",\")}`);\n",
    );

    let output = workspace.command(&["test", "--quiet"]);
    assert_success(&output);
    assert_eq!(String::from_utf8_lossy(&output.stdout), "tests ran: from-manifest\n");
    workspace.assert_spago_calls(
        "",
        &[&["fetch", "-p", "application"], &["sources", "--json", "-p", "application"]],
    );
}

#[test]
fn rejects_a_selected_package_without_tests_before_fetching() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "spago.yaml",
        r#"workspace: {}
package:
  name: application
  dependencies: []
"#,
    );
    workspace.write("src/Main.purs", "module Main where\n");

    let output = workspace.command(&["test", "--quiet"]);
    assert!(!output.status.success());
    workspace.assert_spago_calls("", &[]);
}
