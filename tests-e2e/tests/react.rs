use std::path::Path;
use std::process::Command;

#[path = "support.rs"]
mod support;

use support::{TestWorkspace, assert_success};

#[test]
fn renders_react_with_stable_keyed_component_identity() {
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
        "src/Support.purs",
        r#"module Support (decorate) where

decorate :: String -> String
decorate value = value
"#,
    );
    workspace.write(
        "src/Main.iris",
        r#"module Main (view, list) where

import Iris.React as React
import Support (decorate)
import Widgets as UI

foreign import counter :: React.Component (name :: String)

item = React.component \props -> <Counter name={props.name} />

list children = React.fragment children

view reversed =
  <section data-testid="panel" data-reversed={if reversed then "true" else "false"} title="Iris React">
    <>Heading: {if reversed then
        <><Item key="second" name="second" /><Item key="first" name="first" /></>
      else
        <><Item key="first" name="first" /><Item key="second" name="second" /></>}
      {React.text (decorate " · ready")}<UI.button label="qualified" /></>
  </section>
"#,
    );
    workspace.write(
        "src/Widgets.iris",
        r#"module Widgets (button) where

import Iris.React as React

button = React.component \props -> <span data-qualified="true">{React.text props.label}</span>
"#,
    );
    workspace.write(
        "src/Main.js",
        r#"import React, { useState } from "react";

export function counter({ name }) {
  const [count, setCount] = useState(0);
  return React.createElement(
    "button",
    { "data-name": name, onClick: () => setCount((value) => value + 1) },
    `${name}:${count}`,
  );
}
"#,
    );

    let output = workspace.command(&["build", "--quiet"]);
    assert_success(&output);

    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let script = manifest.join("tools/verify-react.mjs");
    let verification =
        Command::new("node").arg(script).arg(workspace.path().join("output")).output().unwrap();
    assert_success(&verification);
    workspace.assert_spago_calls("", &[&["fetch", "-p", "application"]]);
}
