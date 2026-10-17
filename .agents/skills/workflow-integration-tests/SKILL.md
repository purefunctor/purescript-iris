---
name: workflow-integration-tests
description: "Workflow for adding and updating Iris integration-test fixtures for unified compiler, lowering, resolving, LSP, and source map behavior. Use when creating compiler integration tests, reviewing snapshots, generated JavaScript, or source maps, or using `just t <category>`."
---

# Workflow: Iris Integration Tests

Use the command reference at `reference/compiler-scripts.md` for test runner syntax, snapshot workflows, filters, and trace debugging.

**Language:** Fixtures use PureScript syntax, not Haskell.

## Choose the category first

| Category    | Alias | Use for                                                                                           | Harness pattern                                              |
| ----------- | ----- | ------------------------------------------------------------------------------------------------- | ------------------------------------------------------------ |
| `compiler`  | `c`   | Types, diagnostics, semantic recovery, functional conversion, generated JavaScript, and execution | `Main.purs`, named reports, `output/`, optional `verify.mjs` |
| `lowering`  | `l`   | Lowered core output, binding/equation structure, source-to-core name links                        | every `.purs` file                                           |
| `resolving` | `r`   | Name resolution, imports, exports, qualification, duplicate-name diagnostics                      | every `.purs` file                                           |
| `lsp`       | -     | Hover, definition, completion, import edits, source locations in LSP reports                      | `Main.purs` only                                             |
| `sourcemap` | -     | Source positions that generated JavaScript maps back to                                           | `Main.purs`, `Main.snap`, optional foreign `Main.js`         |

When a behavior spans phases, test the earliest category that directly owns the behavior. Add a later-phase fixture only if the later report is the clearest way to make the regression reviewable.

## Creating a Test

### 1. Create fixture directory

Before creating fixtures, describe the focused behavior and expected results. Choose
small, coherent units that a reader can hold in working memory, and give each fixture
a descriptive name. Read `tests-integration/AGENTS.md` for the split criteria. A few
simple related examples may belong together; sharing a feature, bug, or fix alone
does not justify combining cases.

```bash
just t <category> --create "descriptive name"
```

The CLI picks the next timestamped fixture number, creates the folder under `tests-integration/fixtures/<category>/`, and writes a `Main.purs` template.

Tests are auto-discovered by the datatest harnesses in `tests-integration/tests/`.

### 2. Write focused PureScript modules

Keep each fixture about one small, coherent unit of behavior. Use a small `Main.purs` by default, and add supporting modules when needed to demonstrate that behavior. Split when the source and expected results become difficult to understand together, not simply because an example could run independently.

#### Compiler fixtures

Use `tests-integration/fixtures/compiler/<fixture>/Main.purs` as the entry point.
`Main.checking.snap` contains checked types, kinds, and declaration metadata, without
diagnostics. `Main.diagnostics.snap` contains parser, checking, foreign, and backend
diagnostics for all fixture-owned modules with stable fixture-relative paths.
`Main.semantic.snap` contains checked trees, including recovery. `Main.functional.snap` contains a
successful functional tree or an explicit rejection.

Track generated JavaScript and adjacent FFI under `output/` only for
fixture-owned modules reachable from `Main.purs`. Add `verify.mjs` when Node
execution is part of the contract; it runs against a fresh full dependency
closure, never tracked goldens. Do not commit registry output, `runtime.js`, or a
fixture `package.json`.

Compiler fixtures use `tests-integration/packages.json`. Prefer real package
modules to stand-ins. Deliberately malformed package cases require a fixture-local
`replacements.json` mapping every replaced registry module to a nonempty reason;
Prim and fixture-module collisions cannot be replaced. Keep both ordinary and
malformed fixtures focused on one behavior.

A small checked/inferred pair may stay together when the comparison is easy to
understand. Split variants when their setup or expected results require separate
explanations. Do not grow a fixture into a catalogue of numbered examples or
separate sections of `verify.mjs` merely because they share a broad feature.

For semantic behavior, write the smallest source that distinguishes the recovery,
declaration, expression, binder, or evidence shape under test.

#### Lowering fixtures

Write source that exposes the lowered structure being tested. Prefer simple declarations whose snapshot makes binding, equation, or source-link changes obvious.

#### Resolving fixtures

Use descriptive module names when multiple modules participate, such as `Library.purs`, `ReExporter.purs`, and `Main.purs`. Because every `.purs` file in a resolving fixture is snapshotted, review all generated `.snap` files before accepting.

#### LSP fixtures

Use `Main.purs` as the scenario driver. Add supporting modules for imported symbols and completion candidates. LSP snapshots are generated from `Main.purs`; supporting modules influence the report but do not get their own LSP snapshots.

Multiple cursors may observe one coherent unit, such as document highlights across
a binding's use, signature, and definition. Keep that context together. A small
set of related hover examples can also remain together without a request dependency.
Do not split by cursor count; follow the examples in `tests-integration/AGENTS.md`.

#### Source map fixtures

Use `tests-integration/fixtures/sourcemap/<fixture>/Main.purs` as the entry point. `Main.snap`
prints each line of the module's generated `index.js`, followed by a caret under each mapped
column with the 1-based PureScript `line:column` that it maps to and the source text from there to
the end of its line, or `unmapped`. Add a foreign `Main.js` when the behavior needs foreign imports.
The harness fails a fixture whose modules have errors.

Source map fixtures use `tests-integration/packages.json`, like compiler fixtures. Every generated
line appears in the report, so keep the module to the declarations that show the mapping under
test. Generated JavaScript itself belongs in compiler fixtures.

### 3. Run and review

```bash
just t <category> NNN MMM
```

When an intentional compiler change affects generated JavaScript, review the reported files and update only the relevant fixtures:

```bash
just t compiler NNN --update-output
```

Ordinary compiler runs must remain read-only. Do not set `IRIS_UPDATE_JAVASCRIPT_OUTPUT` directly; `compiler-scripts` owns that implementation detail.

### 4. Accept or reject snapshots

```bash
just t <category> NNN --diff         # Inspect a fixture diff
just t <category> NNN --accept       # Accept a specific fixture
just t <category> NNN --reject       # Reject a specific fixture
just t <category> --accept --confirm # Accept all pending snapshots
```

## Multi-File Tests

For imports, re-exports, or cross-module behavior:

```
tests-integration/fixtures/compiler/NNN_import_test/
├── Main.purs    # Scenario driver
├── Lib.purs     # Supporting module
├── Main.checking.snap
├── Main.diagnostics.snap
├── Main.semantic.snap
├── Main.functional.snap
└── output/      # Reachable fixture-owned JavaScript
```

**Lib.purs:**

```purescript
module Lib where

life :: Int
life = 42

data Maybe a = Just a | Nothing
```

**Main.purs:**

```purescript
module Main where

import Lib (life, Maybe(..))

test :: Maybe Int
test = Just life
```

- Module name must match filename
- Compiler fixtures enter through `Main.purs`; diagnostics cover all fixture-owned modules
- LSP fixtures snapshot only `Main.purs`
- Lowering and resolving fixtures snapshot every `.purs` file

## Snapshot Review Focus

### Compiler

Review each named report against its contract. Checking contains only types;
diagnostics cover all fixture-owned modules with stable paths; semantic preserves
useful recovery structure; and functional records success or explicit rejection.
Review every changed `output/` file for reachability and fixture ownership. When
`verify.mjs` exists, confirm it passes against fresh full generated output.

Checking output resembles:

```
Terms
functionName :: InferredOrCheckedType
...

Types
TypeName :: Kind
...

```

Check diagnostics and locations in `Main.diagnostics.snap`, not the types-only
checking report. In `Main.semantic.snap`, check declarations, finalized types and
kinds, binders, expressions, evidence, and recovery.

### Lowering

Check the lowered module report, especially declarations, binders, equations, and source links. Unexpected name-link changes are often as important as textual output changes.

### Resolving

Check local/imported/exported references, qualification, hidden imports, re-exports, and duplicate-name diagnostics. Multi-module resolving fixtures can update many snapshots; each changed module should be intentional.

### LSP

Check hover text, definitions, completions, edits, and reported positions. Review both the source excerpt and the resulting LSP payload.

### Source maps

Check that each mapping points at the construct that generated the code: the declaration,
equation, local binding, case alternative, guard, do or ado statement, or expression. Code
generated for one equation or case alternative must not map to a sibling. Imports, re-exports, and
the control flow of the `$tail_` dispatcher shared by mutually recursive top-level functions are
unmapped; treat other new `unmapped` lines, and mappings into unrelated source lines, as
regressions.

## Acceptance Criteria

Before accepting, verify:

1. **The category is appropriate**
   - Compiler owns types, diagnostics, semantic recovery, functional conversion,
     and generated or executed JavaScript
   - Lowering owns lowered core/source-link behavior
   - Resolving owns name/import/export behavior
   - LSP owns editor-facing reports
   - Source maps own the source positions that generated JavaScript maps back to

2. **The fixture is narrow**
   - One small, coherent unit whose source and expected results are easy to hold in working memory
   - Related examples and cursors stay together when they help explain that unit
   - Supporting declarations, modules, and assertions preserve meaningful context
   - Split overloaded fixtures before acceptance, commits, or handoff, not mechanically by count

3. **Snapshots are intentional**
   - Compiler reports and generated JavaScript changes are correct
   - Explicit signatures and inferred polymorphism match the scenario being tested
   - Semantic/lowering/resolving/LSP changes match the feature or bug being tested

4. **No unexpected `???`**
   - `test :: ???` - STOP: inference failure
   - `CannotUnify { ??? -> ???, Int }` - OK in error tests

5. **Errors appear where expected**
   - Confirm error kind matches (`NoInstanceFound`, `CannotUnify`)
   - Verify location points to correct declaration

6. **Polymorphism is appropriate in `Main.checking.snap`**
   - Type variables scoped correctly
   - Constraints propagate as expected

## Common Issues

| Symptom                           | Likely Cause                                                                                                                                      |
| --------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| `test :: ???`                     | Syntax error or undefined names                                                                                                                   |
| Unexpected monomorphism           | Missing polymorphic context                                                                                                                       |
| Wrong error location              | Check binder/expression placement                                                                                                                 |
| Missing types in snapshot         | Module header or imports incorrect                                                                                                                |
| Missing compiler diagnostics      | Confirm the source is fixture-owned and loaded; diagnostics include unreferenced supporting modules and use source paths, not module-header names |
| Extra resolving/lowering snapshot | Every `.purs` file is snapshotted in `resolving` and `lowering`                                                                                   |
