# Native React JSX (experiment)

JSX is available in **`.iris` files**. Ordinary `.purs` files keep PureScript syntax,
including its operator lexing. Both extensions participate in the same module graph:
Iris discovers them in Spago package source directories, and either can import the other.
Do not put both `View.purs` and `View.iris` at the same source path: they share the
`View.js` / `View.jsx` foreign-module slot.

This is an Iris extension, not syntax understood by upstream PureScript, Spago's own
compiler, or PureScript formatters. Existing packages remain `.purs`; adopting `.iris`
makes that module Iris-specific. Editor clients also need to associate `.iris` with
their PureScript language mode; Iris's server recognizes the extension.

Iris supplies semantic highlighting for JSX without requiring a JSX TextMate grammar.
Intrinsic tags use `type`, component references use `variable`, qualified prefixes use
`namespace`, attributes use `property`, text uses `string`, and JSX delimiters use
`operator`. Expressions inside braces retain normal PureScript highlighting, including
nested JSX. Highlighting also works on incomplete tags while editing.

For VS Code, enable semantic highlighting and associate the extension in your settings:

```json
{
  "files.associations": { "*.iris": "purescript" },
  "editor.semanticHighlighting.enabled": true
}
```

## React imports are generated

Install `react` and `react-dom` in your application. This experiment is tested with
React 19.2. Import the built-in `Iris.React` module for its types and helpers; there is
no `Runtime.purs`, `Runtime.js`, or generated `Iris.React/index.js` to ship.

```purescript
module Main where

import Iris.React as React

badge :: React.Component (label :: String, children :: React.JSX)
badge = React.component \props ->
  <strong title={props.label}>{props.children}</strong>

view :: String -> React.JSX
view name =
  <main className="greeting">
    Hello {React.text name}!
    <Badge label="Made with Iris"><span>🌱</span></Badge>
  </main>
```

Build with `iris build`, then pass the exported `view` result to ReactDOM's `root.render`.
Like current Reason/ReScript React integration, Iris emits automatic-runtime calls:

```javascript
import { jsx as $jsx, jsxs as $jsxs } from "react/jsx-runtime";
// Single children use jsx; multiple static children use jsxs.
$jsx(badge, { label: "Made with Iris", children: $jsx("span", { children: "🌱" }) });
```

The output is executable ES2022 **`.js`, not preserved `<tag />` syntax**. No JSX
transform is required after Iris. A bundler still resolves React's package imports,
just as it resolves the generated StyleX import. Only required runtime names are imported.

## Components have checked record props and stable identity

`React.Component props` is opaque and nominal in its row parameter. A component tag
passes this value to React; it does not call the render function during JSX construction.
`React.component` wraps a one-record render function and must directly initialize a
non-recursive top-level value. Creating component functions during rendering changes
React identity and can reset hook state, so Iris rejects that use.

Unqualified lowercase tags are intrinsic strings, including `<my-widget />`.
`<Badge>` resolves `badge`; only the first character is lowercased (`<URL>` means `uRL`).
Qualified tags always resolve components: `<UI.button>` and `<UI.Button>` both mean
`UI.button`. Imported opaque components and component-valued parameters work too.

Custom props use ordinary PureScript row checking:

- Closed rows require all declared fields and reject extra or incorrectly typed fields.
- Open rows preserve row polymorphism: `Component (label :: String | props)` accepts
  extra fields without forgetting the required `label`.
- Bindings can express optional subsets with `Prim.Row.Union supplied rest AllowedProps`.
- Function-valued, constrained, and rank-polymorphic props use normal contextual checking.
  As with other generic function applications, give a render function an explicit
  signature when its props contain higher-rank or constrained fields.

Raw intrinsic attributes currently accept open records: **Iris does not validate HTML
attribute names, event types, or void-element children**. Use React spellings such as
`className`, and ordinary typed wrappers or bindings for stricter DOM APIs.

`key` must be a string and is passed separately to React, not exposed to the component's
props. It becomes the leading argument to the keyed element helper; JSX does not promise
JavaScript's attribute evaluation order. Element-construction helpers require
`Row.Lacks "key" props`. `ref` is an ordinary explicitly typed prop under the React 19
convention; Iris does not synthesize forwarding.

## Children and braces

No meaningful nested children means no `children` field. One child becomes a single
`React.JSX`; multiple children become a React-renderable array. A component requiring
`children :: React.JSX` therefore rejects `<Component />`. An explicit `children=` prop
cannot be combined with nested children. Duplicate attributes and mismatched tags are errors.

Literal text is a native React string. Use `React.text value` for dynamic strings,
`React.array nodes` for a dynamic array, `React.fragment nodes` for a fragment, and
`React.empty` for `null`. These helpers and `React.element` also work as first-class or
partially applied functions. There is no implicit conversion of arbitrary interpolated
values into JSX.

Attributes use `name="text"` or `name={expression}`. Quoted attributes use PureScript
string escapes; triple-quoted strings also work. Write `disabled={true}`, not `disabled`.
Braces contain PureScript: records, lambdas, sections, comments, `let`, `case`, `do`, and
nested JSX all retain their usual meaning and local layout.

Text preserves same-line spaces. Multiline text drops indentation and blank lines and
joins nonempty lines with spaces. Entities remain literal: `&amp;` is not decoded.
Use `{React.text "<"}` to insert a markup delimiter.

Parenthesize JSX used as a function argument: `render (<div />)` or `render $ <div />`.
A newline alone does not start a JSX operand. Complete operator names such as `(<>)`
and `(<$>)` keep their meaning; write `( <></> )` for a parenthesized empty fragment.

## Existing React bindings are a separate interoperability boundary

The props design follows React Basic's opaque nodes/components and the DOM bindings'
row-subset constraints. However, `React.Basic.JSX` and `React.Basic.ReactComponent` are
not the same nominal types as `Iris.React.JSX` and `Iris.React.Component`.
This experiment does not supply an interop package or make them implicitly equal.

External JavaScript components can be declared directly with `foreign import` and a
typed `React.Component` row. Keep their identity stable. React callbacks must execute
when JavaScript calls them: a PureScript `event -> Effect Unit` returns a thunk, which
React will not execute automatically. Use the existing `EffectFn` FFI conventions or
binding-library adapters rather than casting calling conventions.

Spreads, attribute puns, single-quoted attributes, component-tag rename/navigation,
and a compiler-owned hooks or complete DOM-props library are outside this experiment.

## Verify compilation and browser rendering

```sh
just t compiler jsx_
just e2e-prepare
just e2e --test react
```

Compiler fixtures check types, diagnostics, elaboration, and generated imports.
The browser test builds a mixed `.purs`/`.iris` project with the real CLI and runs its
fresh output in Vitest's Playwright Chromium mode. It checks DOM output, clicks,
rerendering, and keyed reordering while preserving a stateful child. It also checks
that static fragments avoid false key warnings without disabling dynamic-list validation.
