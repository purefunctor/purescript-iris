use super::support::TestWorkspace;

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
    insta::assert_snapshot!(workspace.report(&output), @"
    status: 0
    --- stdout
    tests ran: from-manifest
    --- stderr
    ");
    insta::assert_snapshot!(workspace.spago_calls(), @r#".: spago "fetch" "-p" "application""#);
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

    assert!(!output.status.success());
    insta::assert_snapshot!(workspace.workspace_stack_frames(&output), @"
    at fail ([WORKSPACE]/output/Test.Main/foreign.js:1:35)
    at <anonymous> ([WORKSPACE]/test/Test/Main.purs:10:8)
    at async [WORKSPACE]/[eval1]:4:16
    ");
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
    insta::assert_snapshot!(workspace.report(&output), @"
    status: 1
    --- stdout
    --- stderr
    no selected packages contain tests
    ");
    insta::assert_snapshot!(workspace.spago_calls(), @"");
}
