use super::support::{TestWorkspace, assert_success};

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
    assert_success(&output);
    assert!(output.stdout.ends_with(b"\n"));
    insta::assert_snapshot!(String::from_utf8_lossy(&output.stdout), @"first,second");
    assert!(workspace.path().join("generated/Configured/index.js").is_file());
    workspace.assert_spago_calls("", &[&["fetch", "-p", "application"]]);
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

    assert_eq!(output.status.code(), Some(7));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).starts_with("Node.js exited with status "));
}
