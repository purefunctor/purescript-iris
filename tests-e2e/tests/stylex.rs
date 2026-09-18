use std::path::Path;
use std::process::Command;

#[path = "support.rs"]
mod support;

use support::{TestWorkspace, assert_success};

#[test]
fn emits_javascript_consumable_by_stylex_babel() {
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
        "src/Tokens.purs",
        r#"module Tokens (await, rowMarker, variables) where

import Iris.StyleX as StyleX

await :: StyleX.Style
await = StyleX.defaultMarker

variables = StyleX.defineVars { accent: "blue" }

rowMarker :: StyleX.Marker
rowMarker = StyleX.defineMarker
"#,
    );
    workspace.write(
        "src/Main.purs",
        r#"module Main where

import Iris.StyleX as StyleX
import Iris.StyleX.When as When
import Tokens (await, rowMarker, variables)

theme = StyleX.createTheme variables { accent: "white" }

styles = StyleX.create
  { root:
      { color: StyleX.conditionalValue "blue"
          [ When.ancestorMarker ":hover" rowMarker "red" ]
      }
  }

awaitProps = StyleX.props await
"#,
    );

    let output = workspace.command(&["build", "--quiet"]);
    assert_success(&output);
    let generated = workspace.read("output/Main/index.js");
    assert!(
        generated
            .contains("import { \"await\" as Tokens_await, rowMarker as Tokens_rowMarker, variables as Tokens_variables }"),
        "unexpected generated JavaScript:\n{generated}"
    );

    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let script = manifest.join("tools/verify-stylex.mjs");
    let verification =
        Command::new("node").arg(script).arg(workspace.path().join("output")).output().unwrap();
    assert_success(&verification);
    workspace.assert_spago_calls("", &[&["fetch", "-p", "application"]]);
}
