## Ownership

This crate owns source-based compiler integration tests through the public APIs and the combination
of compiler subsystems. Loading a file and observing its parsing, resolution, checked types,
diagnostics, semantic trees, generated code, or editor analysis belongs in the fixture harness.

Use the existing fixture loader, runners, and snapshot machinery. Do not reproduce this pipeline in
compiler-crate unit tests or add harness unit tests that merely load and compile another source
file. Local algorithm invariants belong beside their implementation; CLI and development-environment
contracts belong in `tests-e2e`.

## Running and reviewing

Integration tests are `cargo nextest` tests over insta snapshots. Runs write snapshots and generated
JavaScript in place, so git is the review tool: `git diff` shows what changed, `git restore` or
`git clean` discards it, and committing accepts it. Commit or stage unrelated expectation changes
before a run so the diff shows only what the run produced.

`just t <category> [filters...]` prepares registry packages, runs the category's targets with
`INSTA_UPDATE=always IRIS_UPDATE_JAVASCRIPT_OUTPUT=1`, and prints `git status` for this crate.
Filters are ordinary nextest filters, such as a fixture timestamp or name fragment. The direct
equivalent is:

```bash
just integration-prepare
INSTA_UPDATE=always IRIS_UPDATE_JAVASCRIPT_OUTPUT=1 \
  cargo nextest run -p tests-integration --test <target> [filters...]
```

A failing `verify.mjs` or harness check fails the run without writing goldens. Before pushing, run
every affected category without filters; each must pass and leave no uncommitted expectation
changes.

Fixtures live in `fixtures/<category>/<unix-timestamp>_<snake_case_name>/` and are discovered
automatically. Create one by creating the directory and its `Main.purs`, for example with
`$(date +%s)` as the prefix; delete one by deleting the directory. Fixture snapshots live beside
their sources; `lowering_unit` snapshots live in `tests/snapshots/`.

## Categories

| Category     | Alias | Targets                     | Owns                                                                                     | Reports                                                      |
| ------------ | ----- | --------------------------- | ---------------------------------------------------------------------------------------- | ------------------------------------------------------------ |
| `compiler`   | `c`   | `compiler`                  | Types, diagnostics, semantic recovery, functional conversion, generated JavaScript, execution | `Main.purs` entry, named reports, `output/`, optional `verify.mjs` |
| `lowering`   | `l`   | `lowering`, `lowering_unit` | Lowered core, binding and equation structure, source-to-core name links                  | One snapshot per `.purs` file                                |
| `resolving`  | `r`   | `resolving`                 | Name resolution, imports, exports, qualification, duplicate-name diagnostics             | One snapshot per `.purs` file                                |
| `lsp`        | -     | `lsp`                       | Hover, definition, completion, import edits, reported positions                          | `Main.purs` only                                             |
| `sourcemap`  | -     | `sourcemap`                 | Source positions that generated JavaScript maps back to                                  | `Main.snap`, optional foreign `Main.js`                      |
| `docs`       | -     | `docs`                      | Generated documentation                                                                  |                                                              |
| `formatting` | -     | `formatting`                | Formatter fixtures and corpus; see `CONTRIBUTING.md`                                     |                                                              |

When a behavior spans phases, test the earliest category that directly owns it. Add a later-phase
fixture only when its report is the clearest way to make the regression reviewable.

### Compiler

`Main.purs` is the entry point. `Main.checking.snap` contains checked types, kinds, and declaration
metadata without diagnostics. `Main.diagnostics.snap` contains parser, checking, foreign, and
backend diagnostics for every fixture-owned module, with stable fixture-relative paths.
`Main.semantic.snap` contains checked trees, including recovery. `Main.functional.snap` contains a
successful functional tree or an explicit rejection.

Track generated JavaScript and adjacent FFI under `output/` only for fixture-owned modules reachable
from `Main.purs`. Do not commit registry output, `runtime.js`, or a fixture `package.json`. Add
`verify.mjs` only when Node execution is the contract; it runs against a fresh full dependency
closure, never tracked goldens.

Compiler fixtures use `packages.json`. Use real registry modules rather than stand-ins.
Deliberately malformed cases need a fixture-local `replacements.json` mapping each replaced registry
module to a nonempty reason; Prim and fixture modules cannot be replaced.

Review each report against its contract. An unexpected `???` in checking output means inference
failed; `???` inside an error in an error test can be expected. Check error kinds and locations in
the diagnostics report, not the checking report.

### Lowering and resolving

Every `.purs` file is snapshotted, so review every changed module snapshot. Use descriptive module
names such as `Library.purs` and `ReExporter.purs` when several modules participate. Unexpected
name-link changes in lowering reports matter as much as textual changes.

### LSP

`Main.purs` drives the scenario and is the only file snapshotted; supporting modules provide
imported symbols and completion candidates. Review the source excerpt and the payload together.

### Source maps

`Main.snap` prints each line of the module's generated `index.js`, then a caret under each mapped
column with the 1-based PureScript `line:column` it maps to and the source text from there to the
end of the line, or `unmapped`. A mapping that names a function shows the name in parentheses. The
harness fails a fixture whose modules have errors. Generated JavaScript itself belongs in compiler
fixtures.

Each mapping must point at the construct that generated the code: the declaration, equation, local
binding, case alternative, guard, do or ado statement, or expression. Code for one equation or case
alternative must not map to a sibling. A module import maps to the first import statement that
names the module, even one that imports nothing from it, and otherwise to the first statement that
re-exports one of its items, such as `import Prelude`, or that brings into scope a term or type
operator resolving to one of its items. A module that no import statement provides, such as one
reached only through an instance on an imported value's type, stays unmapped. The `./foreign.js`
and `./index.stylex.js` imports map to the first `foreign import` or theme definition they serve,
and the `@stylexjs/stylex` import maps to the first import of a StyleX module whose functions the
generated code calls. Re-exports, the `../runtime.js` import, and the control flow of the `$tail_`
dispatcher are unmapped; treat other new `unmapped` lines as regressions.

A function defined by a named declaration or local binding carries its PureScript name, and so does
each function it returns. A closure held by a record field carries the field's name. Names appear
on the header mappings: the `function` keyword, binding, or `return`, the parameters, and the call
of an immediately invoked arrow. The first body mapping is unnamed. Lambdas, generated helpers, and
functions that inlining separates from their binding carry no name.

## Fixture design

Each fixture must cover one small, coherent unit of behavior. The goal is to keep the setup,
behavior, and expected results easy for a reader to hold in working memory. A unit may include a
few closely related examples. It does not have to contain exactly one declaration, assertion,
variant, or cursor. Judge the source and its reports together, not by a fixed count or line limit.

Before writing source, describe the behavior and expected results briefly, and give the fixture a
descriptive name. Apply these criteria:

- Split when examples require separate explanations, substantial independent setup, or too much
  context to understand together. A shared feature, bug, root cause, or fix is not enough reason to
  combine them. In particular, a `compiler` fixture must not become a catalogue of examples under
  a broad label such as instance matching or error recovery.
- Keep a few simple variants or controls together when they make one behavior easier to understand.
  Being able to run an example independently is not itself a reason to split it. A small numeric
  and string type-literal hover fixture is a coherent unit, even though it has two queries.
- Preserve meaningful context and relationships. Multiple LSP cursors on the use, signature, and
  definition of one binding can demonstrate document highlighting together. Keep dependent request
  sequences, such as populating and reusing a completion cache, together too. Do not remove nearby
  declarations or modules merely because their cursors are removed; they may affect the results.
- Extend an existing fixture only while it remains small and coherent. Split it when another case
  would make it harder to understand as a whole. Existing broad fixtures are not precedent, and
  adding comments or sections does not solve excessive complexity. Do not sweep unrelated fixtures
  unless asked.

Review the source against these criteria before generating expectations, then review the reports
for the same clarity. Split overloaded fixtures before committing or handing off the work; do not
wait for the user to request it. Multiple compiler reports observing one unit need not be split.

Apply these rules to existing fixtures in small, coherent batches. For each batch, preserve the
original scenarios and required setup, regenerate expectations, and inspect every changed report
and generated file against the original behavior. Run the affected checks and commit that verified
batch before starting another. Do not bulk-split fixtures and postpone output review until the end.

- Avoid duplicating a scenario across categories merely to increase coverage.
- Keep goldens focused on reachable fixture-owned generated JavaScript.
- Verify harness changes through representative fixtures and affected category runs. A separate unit
  test is justified for an isolated harness algorithm only when it protects a distinct local
  invariant.

## Regression tests

A known bug fix lands as two commits so the regression is visible in history:

1. `Add failing test case for <bug>`: the fixture with expectations that record the current wrong
   behavior, such as a spurious diagnostic, a wrong type, or an incorrect LSP response. Do not
   change compiler behavior in this commit.
2. `Fix <bug>`: the compiler fix plus the same expectations updated, so the diff shows the wrong
   behavior removed or corrected and nothing else.

With Jujutsu, a fix that is already written can be split: describe the current change as the fix,
insert the fixture commit before it with `jj new -B @`, run the fixture there, then `jj edit` the
fix and run it again.

With Git, move forward only: start where the bug still reproduces, commit the fixture, then apply
and commit the fix. Do not rebase or reset to manufacture this order. If the fix is already present,
ask the user whether to keep the existing order or restart from the buggy base.
