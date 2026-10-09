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
const files = ["Tokens/index.stylex.js", "Tokens/index.js", "Main/index.js"];
const styles = [];
const transformed = new Map();

for (const file of files) {
  const filename = path.join(outputRoot, file);
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
          // The default that `@stylexjs/unplugin` passes; theme files need no further setup.
          unstable_moduleResolution: { type: "commonJS", rootDir: outputRoot },
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
    throw new Error(`${file} retains uncompiled static StyleX calls`);
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
  "margin:21px",
]) {
  if (!css.includes(expected)) {
    throw new Error(`StyleX CSS does not contain ${JSON.stringify(expected)}:\n${css}`);
  }
}

// A cross-module reference must hash to the variable its defining module declares.
const accent = css.match(/(--[\w-]+):blue/)?.[1];
if (
  accent === undefined ||
  !css.includes(`border-color:var(${accent})`) ||
  !css.includes(`{color:var(${accent})`)
) {
  throw new Error(`Styles do not use the variable Tokens defines:\n${css}`);
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
const theme = await import(pathToFileURL(path.join(outputRoot, "Tokens/index.stylex.js")));
assert.equal(tokens.variables, theme.variables);
const classNames = (value) => new Set(value.trim().split(/\s+/));
const gapClasses = classNames(tokens.gapProps.className);
assert.equal(gapClasses.size, 2);
for (const gapClass of gapClasses) {
  assert.ok(css.includes(`.${gapClass}`));
}
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
assert.deepEqual(main.dynamicProps, stylex.props(main.styles.sized({ width: 100 })));
assert.ok(Object.values(main.dynamicProps.style).includes("100px"));
assert.deepEqual(
  main.mixedDynamicProps,
  stylex.props(main.styles.row, main.styles.sized({ width: 40 })),
);
assert.deepEqual(main.dynamicAttrs, stylex.attrs(main.styles.sized({ width: 30 })));
assert.ok(main.dynamicAttrs.style.includes("30px"));
