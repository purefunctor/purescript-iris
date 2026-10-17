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
        r#"export const main = () => console.log(`tests ran: ${process.argv.slice(2).join(",")}`);
"#,
    );

    let output = workspace.command(&["test", "--quiet"]);
    assert_success(&output);
    assert!(output.stdout.ends_with(b"\n"));
    insta::assert_snapshot!(String::from_utf8_lossy(&output.stdout), @"tests ran: from-manifest");
    workspace.assert_spago_calls("", &[&["fetch", "-p", "application"]]);
}

#[test]
fn maps_stack_traces_to_purescript_sources() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "spago.yaml",
        r#"workspace: {}
package:
  name: application
  dependencies: []
  test:
    main: Test.Main
    dependencies: []
"#,
    );
    workspace.write(
        "test/Test/Main.purs",
        r#"module Test.Main where

data Unit = Unit

foreign import data Effect :: Type -> Type

foreign import fail :: Unit -> Effect Unit

main :: Effect Unit
main = fail Unit
"#,
    );
    workspace.write(
        "test/Test/Main.js",
        "export const fail = () => { throw new Error(\"failed\"); };\n",
    );

    let output = workspace.command(&["test", "--quiet"]);

    let stderr = String::from_utf8_lossy(&output.stderr).replace('\\', "/");
    assert!(!output.status.success());
    assert!(stderr.contains("test/Test/Main.purs:10:8"), "unmapped stack trace:\n{stderr}");
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
