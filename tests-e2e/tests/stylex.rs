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
        r#"module Tokens (await, constants, rowMarker, variables) where

import Iris.StyleX as StyleX

await :: StyleX.Style
await = StyleX.defaultMarker

constants = StyleX.defineConsts { spacing: "13px" }

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
import Tokens (await, constants, rowMarker, variables)

localColour = "purple"

accent = variables.accent

theme = StyleX.createTheme variables { accent: "white" }

animation = StyleX.keyframes { from: { opacity: 0.0 }, to: { opacity: 1.0 } }

position = StyleX.positionTry { top: "7px" }

transition = StyleX.viewTransitionClass { old: { opacity: 0.0 }, new: { opacity: 1.0 } }

styles = StyleX.create
  { root:
      { color: StyleX.conditionalValue "blue"
          [ When.ancestorMarker ":hover" rowMarker "red" ]
      , backgroundColor: localColour
      , borderColor: accent
      , animationName: animation
      , positionTryFallbacks: position
      , viewTransitionClass: transition
      }
  , animated: { animationName: StyleX.keyframes { from: { opacity: 0.2 }, to: { opacity: 0.8 } } }
  , row: { padding: 8 }
  }

inlined = let colour = "green" in StyleX.create { root: { color: colour } }

staticFunction :: String -> { root :: StyleX.Style }
staticFunction _ = StyleX.create { root: { color: "orange" } }

locallyAnimated :: String -> { root :: StyleX.Style }
locallyAnimated _ =
  let frames = StyleX.keyframes { from: { opacity: 0.3 }, to: { opacity: 0.7 } }
  in StyleX.create { root: { animationName: frames } }

locallyImported :: String -> { root :: StyleX.Style }
locallyImported _ =
  let spacing = constants.spacing
  in StyleX.create { root: { padding: spacing, margin: spacing } }

awaitProps = StyleX.props await

markedProps = StyleX.props [ styles.row, StyleX.markerStyle rowMarker ]

markedAttrs = StyleX.attrs [ StyleX.markerStyle rowMarker, styles.row ]

conditionalProps enabled = StyleX.props
  [ styles.row, StyleX.conditional enabled (StyleX.markerStyle rowMarker) ]
"#,
    );

    let output = workspace.command(&["build", "--quiet"]);
    assert_success(&output);
    let generated = workspace.read("output/Main/index.js");
    assert!(
        generated
            .contains("import { \"await\" as Tokens_await, constants as Tokens_constants, rowMarker as Tokens_rowMarker, variables as Tokens_variables }"),
        "unexpected generated JavaScript:\n{generated}"
    );

    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let script = manifest.join("tools/verify-stylex.mjs");
    let verification =
        Command::new("node").arg(script).arg(workspace.path().join("output")).output().unwrap();
    assert_success(&verification);
    workspace.assert_spago_calls("", &[&["fetch", "-p", "application"]]);
}
