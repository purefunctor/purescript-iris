use super::support::TestWorkspace;

#[test]
fn runs_the_configured_module_with_node() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "spago.yaml",
        r#"workspace: {}
package:
  name: application
  dependencies: []
  run:
    main: Configured
"#,
    );
    workspace.write(
        "src/Configured.purs",
        r#"module Configured where

data Unit = Unit
foreign import data Effect :: Type -> Type
foreign import main :: Effect Unit
"#,
    );
    workspace.write(
        "src/Configured.js",
        r#"export const main = () => console.log(process.argv.slice(2).join(","));
"#,
    );

    let output =
        workspace.command(&["run", "--output", "generated", "--quiet", "--", "first", "second"]);
    insta::assert_snapshot!(workspace.report(&output), @"
    status: 0
    --- stdout
    first,second
    --- stderr
    ");
    assert!(workspace.path().join("generated/Configured/index.js").is_file());
    insta::assert_snapshot!(workspace.spago_calls(), @r#".: spago "fetch" "-p" "application""#);
}

#[test]
fn runs_output_modules_as_esm_in_a_commonjs_workspace() {
    let workspace = TestWorkspace::empty();
    workspace.write("package.json", "{\"type\":\"commonjs\"}\n");
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
foreign import data Effect :: Type -> Type
foreign import main :: Effect Unit
"#,
    );
    workspace.write("src/Main.js", "export const main = () => console.log(\"loaded\");\n");

    let output = workspace.command(&["run", "--quiet"]);

    insta::assert_snapshot!(workspace.report(&output), @"
    status: 0
    --- stdout
    loaded
    --- stderr
    ");
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
"#,
    );
    workspace.write(
        "src/Main.purs",
        r#"module Main where

data Unit = Unit

foreign import data Effect :: Type -> Type

foreign import fail :: Unit -> Effect Unit

validate' :: Unit -> Effect Unit
validate' unit = fail unit

main :: Effect Unit
main = validate' Unit
"#,
    );
    workspace.write("src/Main.js", "export const fail = () => { throw new Error(\"failed\"); };\n");

    let output = workspace.command(&["run", "--quiet"]);

    assert!(!output.status.success());
    insta::assert_snapshot!(workspace.workspace_stack_frames(&output), @"
    at fail ([WORKSPACE]/output/Main/foreign.js:1:35)
    at validate' ([WORKSPACE]/src/Main.purs:10:18)
    at <anonymous> ([WORKSPACE]/src/Main.purs:13:8)
    at async [WORKSPACE]/[eval1]:4:16
    ");
}

#[test]
fn preserves_the_node_exit_code() {
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
foreign import data Effect :: Type -> Type
foreign import main :: Effect Unit
"#,
    );
    workspace.write("src/Main.js", "export const main = () => { process.exitCode = 7; };\n");

    let output = workspace.command(&["run", "--quiet"]);

    insta::assert_snapshot!(workspace.report(&output), @"
    status: 7
    --- stdout
    --- stderr
    Node.js exited with status exit code: 7
    ");
}

#[test]
fn refuses_to_run_a_main_module_that_is_not_part_of_the_build() {
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
foreign import data Effect :: Type -> Type
foreign import main :: Effect Unit
"#,
    );
    workspace.write(
        "src/Main.js",
        r#"export const main = () => console.log("ran");
"#,
    );
    workspace.write(
        "src/Other.purs",
        r#"module Other where
"#,
    );
    let ran = workspace.command(&["run", "--quiet"]);
    insta::assert_snapshot!(workspace.report(&ran), @"
    status: 0
    --- stdout
    ran
    --- stderr
    ");

    std::fs::remove_file(workspace.path().join("src/Main.purs")).unwrap();
    let output = workspace.command(&["run", "--quiet"]);

    insta::assert_snapshot!(workspace.report(&output), @"
    status: 1
    --- stdout
    --- stderr
    module Main is not part of the build
    ");
    assert!(workspace.path().join("output/Main/index.js").is_file());
}
