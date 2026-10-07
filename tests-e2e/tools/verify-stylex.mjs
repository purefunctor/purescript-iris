import assert from "node:assert/strict";
import fs from "node:fs";
import { registerHooks } from "node:module";
import path from "node:path";
import process from "node:process";
import { pathToFileURL } from "node:url";

import { transformSync } from "@babel/core";
import stylexPlugin from "@stylexjs/babel-plugin";
import * as stylex from "@stylexjs/stylex";

// The plugin resolves imports to real paths and hashes them relative to `rootDir`, so a
// symlinked temporary directory (macOS `/var`) must be canonical on both sides.
const outputRoot = fs.realpathSync(path.resolve(process.argv[2]));
const modules = ["Tokens", "Main"];
const styles = [];
const transformed = new Map();

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
  transformed.set(filename, result.code);
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
  "padding:8px",
  "@media (max-width: 600px)",
  "padding:4px",
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

for (const [filename, code] of transformed) {
  fs.writeFileSync(filename, code);
}

const stylexUrl = import.meta.resolve("@stylexjs/stylex");
registerHooks({
  resolve(specifier, context, nextResolve) {
    return nextResolve(
      specifier === "@stylexjs/stylex" ? stylexUrl : specifier,
      context,
    );
  },
});

const tokens = await import(pathToFileURL(path.join(outputRoot, "Tokens/index.js")));
const main = await import(pathToFileURL(path.join(outputRoot, "Main/index.js")));
const classNames = (value) => new Set(value.trim().split(/\s+/));
const expected = classNames(stylex.props(main.styles.row, tokens.rowMarker).className);
const markerClass = stylex.props(tokens.rowMarker).className;
assert.ok(markerClass.length > 0);
assert.ok(css.includes(`.${markerClass}:hover`));
assert.ok(expected.has(markerClass));
assert.ok(expected.size > 1);
assert.deepEqual(classNames(main.markedProps.className), expected);
assert.deepEqual(classNames(main.markedAttrs.class), expected);
assert.deepEqual(classNames(main.conditionalProps(true).className), expected);
assert.deepEqual(
  classNames(main.conditionalProps(false).className),
  classNames(stylex.props(main.styles.row).className),
);
