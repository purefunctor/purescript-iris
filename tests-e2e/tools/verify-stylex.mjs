import fs from "node:fs";
import path from "node:path";
import process from "node:process";

import { transformSync } from "@babel/core";
import stylexPlugin from "@stylexjs/babel-plugin";

// The plugin resolves imports to real paths and hashes them relative to `rootDir`, so a
// symlinked temporary directory (macOS `/var`) must be canonical on both sides.
const outputRoot = fs.realpathSync(path.resolve(process.argv[2]));
const modules = ["Tokens", "Main"];
const styles = [];

for (const moduleName of modules) {
  const filename = path.join(outputRoot, moduleName, "index.js");
  const source = fs.readFileSync(filename, "utf8");
  const result = transformSync(source, {
    filename,
    babelrc: false,
    configFile: false,
    plugins: [
      [
        stylexPlugin,
        {
          dev: false,
          unstable_moduleResolution: {
            type: "commonJS",
            rootDir: outputRoot,
            themeFileExtension: "index",
          },
        },
      ],
    ],
  });

  const staticCalls = [
    "create",
    "keyframes",
    "createTheme",
    "defineConsts",
    "defineMarker",
    "defineVars",
    "positionTry",
    "viewTransitionClass",
  ];
  if (staticCalls.some((call) => result.code.includes(`$stylex.${call}(`))) {
    throw new Error(`${moduleName} retains uncompiled static StyleX calls`);
  }
  styles.push(...result.metadata.stylex);
}

const css = stylexPlugin.processStylexRules(styles, { useLayers: false });
for (const expected of [
  "--",
  ":where(",
  "color:red",
  "background-color:purple",
  "border-color:var(--",
  "padding:13px",
  "color:green",
  "color:orange",
  "@keyframes",
  "from{opacity:.2;}to{opacity:.8;}",
  "@position-try",
  "top:7px",
  "::view-transition-old",
]) {
  if (!css.includes(expected)) {
    throw new Error(`StyleX CSS does not contain ${JSON.stringify(expected)}:\n${css}`);
  }
}

// A cross-module reference must hash to the variable its defining module declares.
const accent = css.match(/(--[\w-]+):blue/)?.[1];
if (accent === undefined || !css.includes(`border-color:var(${accent})`)) {
  throw new Error(`border-color does not use the variable Tokens defines:\n${css}`);
}
