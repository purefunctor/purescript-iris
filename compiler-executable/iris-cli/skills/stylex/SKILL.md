---
name: stylex
description: Write StyleX styles in PureScript with Iris through the built-in Iris.StyleX, Iris.StyleX.When, and Iris.StyleX.Types modules, which Iris compiles to `@stylexjs/stylex` calls. Covers translating StyleX's JavaScript API and examples into PureScript, the placement rules Iris enforces, what only the StyleX plugin checks, and bundler settings for Iris output. Use when writing or changing styles in a module that imports Iris.StyleX, when Iris or the StyleX plugin reports an error about StyleX code from PureScript, or when adding StyleX to a project built with Iris.
---

# StyleX in PureScript with Iris

This skill covers what is specific to writing StyleX in PureScript. StyleX itself decides what
properties, conditions, merging, variables, and themes mean, and its guidance changes with its
releases, so read StyleX's own agent guides before writing styles unless they are already in
context. Use the `@stylexjs/stylex` version from the project's `package.json` or lockfile:

```bash
curl -fsSL https://raw.githubusercontent.com/facebook/stylex/<version>/packages/docs/static/llm/stylex-authoring.md
curl -fsSL https://raw.githubusercontent.com/facebook/stylex/<version>/packages/docs/static/llm/stylex-installation.md
```

`<version>` is a tag such as `0.19.0`, or `main` for the latest. The API reference is at
https://stylexjs.com/docs/api. Prefer any StyleX skill you already have for StyleX semantics, and
use this skill to express its advice in PureScript.

`Iris.StyleX`, `Iris.StyleX.When`, and `Iris.StyleX.Types` are built into Iris; do not add a
Spago dependency for them. `purs` cannot build a project that imports them. Iris compiles each
call to a direct `@stylexjs/stylex` call in `output/`, which the StyleX bundler plugin compiles to
CSS. With `iris watch` running, `iris watch query module Iris.StyleX` lists exact signatures and
`iris watch query javascript Main` shows what the plugin receives; see `iris skills get watch`.

## Translate StyleX JavaScript

```purescript
import Iris.StyleX as StyleX
import Iris.StyleX.When as When
import Iris.StyleX.Types as Types
```

| StyleX JavaScript | PureScript |
|---|---|
| `stylex.create({ root: { … } })` | `StyleX.create { root: { … } }` |
| `{...stylex.props(styles.a, styles.b)}` | `StyleX.props [ styles.a, styles.b ]`, a `{ className :: String }` record |
| `stylex.props(styles.row, marker)` | `StyleX.props [ styles.row, StyleX.markerStyle marker ]` |
| `create({ bar: (width, height) => ({ width, height }) })` | `create { bar: \size -> { width: size.width, height: size.height } }` |
| `stylex.props(styles.base, styles.bar(100, 40))` | `StyleX.props [ StyleX.dynamicStyle styles.base, styles.bar { width: 100, height: 40 } ]` |
| `active && styles.active` | `StyleX.conditional active styles.active` |
| `variant === 'primary' ? styles.primary : styles.secondary` | A function or `case` returning `StyleX.Style` |
| `{ default: 'blue', ':hover': 'red' }` | `{ default: "blue", ":hover": "red" }` |
| `{ default: 0, [stylex.when.ancestor(':hover')]: 1 }` | `StyleX.conditionalValue 0 [ When.ancestor ":hover" 1 ]` |
| `[stylex.when.ancestor(':focus', marker)]: 1` | `When.ancestorMarker ":focus" marker 1` |
| `{ default: 8, [breakpoints.small]: 4 }` | `StyleX.conditionalValue 8 [ StyleX.conditionalCase breakpoints.small 4 ]` |
| `stylex.defaultMarker()`, `stylex.defineMarker()` | `StyleX.defaultMarker`, `StyleX.defineMarker`, values rather than calls |
| `stylex.firstThatWorks('sticky', 'fixed')` | `StyleX.firstThatWorks [ "sticky", "fixed" ]` |
| `stylex.types.color('red')` | `Types.color "red"` |
| `stylex.keyframes`, `defineVars`, `defineConsts`, `createTheme`, `viewTransitionClass`, `positionTry`, `attrs` | The same name under `StyleX` |
| `tokens.stylex.js` exporting `defineVars` or `defineConsts` | An exported top-level value in any module, with the bundler setting below |

- Quote record labels that are not PureScript identifiers: `":hover"`, `"::placeholder"`,
  `"@media (min-width: 768px)"`, `"WebkitBackdropFilter"`.
- Values are strings, `Int`s, or `Number`s; write `0.5` rather than `.5`. The default and cases
  of a `conditionalValue` share one type, so write `StyleX.conditionalValue 0.5 [ When.ancestor
  ":hover" 1.0 ]` rather than mixing `0.5` with `1`.
- Record labels must be literal, so write StyleX's computed keys as `conditionalValue` cases:
  `{ default: 8, [breakpoints.small]: 4 }` becomes `StyleX.conditionalValue 8
  [ StyleX.conditionalCase breakpoints.small 4 ]`, which also accepts literal condition strings
  and mixes with `When` cases. Constants and variables also work as values:
  `{ color: palette.brand }`.
- StyleX's `null` cannot be written. `Data.Nullable.null` is an imported value, which Iris rejects
  inside `create`; give the property a real value or leave it out of that style.
- StyleX's dynamic styles take one parameter; pass several values as a record and read its
  fields, as in the table. Annotate the parameter, as in `\(size :: { width :: Int })`, or its
  field types stay open. Calling one gives a `DynamicStyle`, and `props` on dynamic styles
  returns `DynamicProps`, `{ className :: String, style :: InlineStyle }`. `InlineStyle` holds
  StyleX's CSS variables: pass it to the UI library's `style` attribute unchanged. Include static
  styles in the same array with `StyleX.dynamicStyle styles.base`.
- `StyleXStyles` props become `StyleX.Style` arguments: pass styles between components as values.

## Write components

```purescript
styles = StyleX.create
  { button: { borderRadius: 8, color: { default: "var(--text)", ":hover": "var(--accent)" } }
  , small: { fontSize: 13, height: 28 }
  , large: { fontSize: 15, height: 44 }
  , label: { fontWeight: 600 }
  }

styleProps = StyleX.recordProps styles

sizeStyle :: Size -> StyleX.Style
sizeStyle = case _ of
  Small -> styles.small
  Large -> styles.large

DOM.span styleProps.label [ DOM.text label ]
DOM.button
  { className:
      (StyleX.props [ styles.button, sizeStyle size, StyleX.conditional active styles.label ])
        .className
  , onClick: handler_ onPress
  }
  children
```

- Leave `styles` unannotated; `create` infers a record of `StyleX.Style` with the same labels.
- `recordProps styles` expands at compile time into a record of `props` results, one per label.
  Use it for elements with a single style, and `props` for compositions and conditional styles.
- `props` takes a `Style`, an `Array Style`, a `Marker`, a `DynamicStyle`, or an
  `Array DynamicStyle`. Pass the result as an element's props when it needs nothing else;
  otherwise take `.className`. `attrs` and `recordAttrs` are the same for renderers that take an
  HTML `class` attribute and return `Attrs`.
- A named `Marker` joins an array of styles through `StyleX.markerStyle marker`, which compiles
  away; pass the marker itself to `When.*Marker`. `StyleX.defaultMarker` is already a `Style`.
- Styles shared by several components can live in their own module and be imported.

## Rules Iris enforces

Every function from these modules must be called directly with all of its arguments; `f $ x` and
`x # f` count. They cannot be passed around as values or partially applied, and their
declarations have no runtime value. Iris reports violations as `FunctionalCodegen` errors that
begin `Cannot generate JavaScript for module`.

| Function | Rule |
|---|---|
| `defineVars`, `defineConsts`, `defineMarker` | Must be the entire body of an exported, non-recursive top-level value |
| `createTheme`, `viewTransitionClass`, `positionTry` | Must be the entire body of a non-recursive top-level value |
| `keyframes` | The entire body of a non-recursive value or `let` binding, or inside `create`, `defineVars`, `createTheme`, or `viewTransitionClass` |
| `Types.*` | Only inside `defineVars` or `createTheme` |
| `When.*`, `conditionalCase` | Only as elements written directly in a `conditionalValue` case array; `conditionalCase` keys must be static |
| `conditionalValue` | Only inside `create` |
| `firstThatWorks` | Only inside `create`, `keyframes`, `positionTry`, or `viewTransitionClass`, with a non-empty array literal |

The StyleX plugin evaluates the arguments of `create`, `keyframes`, `defineVars`,
`defineConsts`, `createTheme`, `positionTry`, and `viewTransitionClass` at build time, so Iris
requires them to be statically known:

- Build them from literals, top-level values and `let` bindings of the same module, and other
  modules' `defineVars`, `defineConsts`, and `defineMarker` exports.
- Function parameters, `props` results, function calls, and other imported values are rejected,
  even when reached through an alias. `defineConsts` cannot use imported values at all.
- A `create` call may sit inside a function, as long as its arguments are static.
- A dynamic namespace takes one named parameter, not a destructuring pattern, and must use it.
  Its body is a record literal whose values are built from the parameter, its fields, literals,
  records, arrays, and operators. Compute anything else, such as `show` or `if`, in the caller
  and pass it in. Condition keys inside it stay static.

`createTheme` overrides are type-checked against the `defineVars` fields of the same name, and
`Types` functions against their argument types; see their signatures with `iris watch query`.

## What only the StyleX plugin checks

StyleX validates CSS property names and values, which Iris does not: `{ color: true }` builds
with Iris and fails in the plugin. After changing styles, check the bundler or dev server output
as well as the Iris build.

## Bundler setup

Follow StyleX's installation guide for the bundler, then add what Iris output needs:

- Generated modules import `@stylexjs/stylex`, so the project depends on it at runtime.
- Iris writes each module to `output/<Module>/index.js`, but the plugin accepts `defineVars`,
  `defineConsts`, `defineMarker`, and `createTheme` only in theme files, `*.stylex.js` by default.
  Mark Iris output as theme files; `@stylexjs/unplugin` forwards this option to the Babel plugin:

  ```js
  unstable_moduleResolution: {
    type: "commonJS",
    rootDir: process.cwd(),
    themeFileExtension: "index",
  },
  ```

  Without it, those calls fail with `Unable to generate hash` or `Only static values are allowed`,
  and imports between modules in static calls fail with `Could not resolve the path to the
  imported file`. When calling the Babel plugin yourself, pass real file paths: the plugin
  resolves imports through symlinks, so a symlinked filename gives a variable a different hash in
  the module that imports it.
- Run `iris watch` beside the dev server, with the same `--output` the bundler imports from.
- Vite may miss `@stylexjs/stylex` in its first scan of generated modules; add it to
  `optimizeDeps.include` if the dev server reloads on first navigation.

Iris is tested with `@stylexjs/babel-plugin` 0.19.0, and the Iris website builds with
`@stylexjs/unplugin` 0.19.0.
