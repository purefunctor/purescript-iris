---
name: workflow-regression-tests
description: "Workflow for producing auditable Git or jj history for a known compiler bug fix. Use when a compiler fix needs a failing integration-test fixture followed by a fix commit that updates the same compiler, lowering, resolving, LSP, or source map report."
---

# Workflow: Regression Tests

Use this when the fix is known and needs an auditable regression test. The Jujutsu workflow can split a fix that is already present; the Git workflow starts before the fix is applied and moves forward through ordinary commits.

The goal is an auditable two-commit history:

1. **Failing fixture commit**: add the fixture and accept a snapshot that captures the current undesirable behavior.
2. **Fix commit**: keep the compiler fix and update the same snapshot so the undesirable behavior disappears or changes to the desired output.

This makes the regression visible in version control before the fix removes it.

## Historical pattern

Historical examples in this repo follow this shape:

- `Add failing test case for bare row tail syntax` → `Fix inference for bare row tails in syntax`
- `Add failing test case for constrained pattern scrutinee` → `Fix constrained pattern scrutinee checking`
- `Add failing test case for open row matching` → `Fix open row tail instance matching`

The fix commit usually touches compiler code plus the same `.snap`. Depending on the integration-test category, the snapshot diff might remove a `Diagnostics` block, replace an incorrect inferred result, update lowered/resolved bindings, or change an LSP hover/completion/definition report.

## Choose the integration-test category

Use the category that owns the observable regression:

| Category    | Alias | Use when the regression is visible in                                                            |
| ----------- | ----- | ------------------------------------------------------------------------------------------------ |
| `compiler`  | `c`   | Types, diagnostics, semantic recovery, functional conversion, generated JavaScript, or execution |
| `lowering`  | `l`   | Lowered core output, equation/binder shape, or source-to-core links                              |
| `resolving` | `r`   | Name resolution, imports, exports, qualification, re-exports, or resolver diagnostics            |
| `lsp`       | -     | Hover, definition, completion, import edits, or editor-facing source positions                   |
| `sourcemap` | -     | The source positions that generated JavaScript maps back to                                      |

Use the `workflow-integration-tests` skill for fixture authoring details. Use the command reference at `.agents/skills/workflow-integration-tests/reference/compiler-scripts.md` for runner syntax, filters, snapshots, and trace debugging.

Before either workflow, choose small, coherent units of behavior as required by
`tests-integration/AGENTS.md`. Keep the setup and expected results easy to hold in
working memory. Simple related variants or controls may stay together; a shared
bug, root cause, or fix alone does not justify a large collection of examples.
A failing-fixture commit may contain several focused fixtures; preserve those
boundaries and their meaningful context in the fix commit.

## Jujutsu workflow

### 1. Preserve the existing fix commit

If the current commit contains the intended fix, give it a clear description first:

```bash
jj describe -m "Fix <bug>"
```

Then insert a parent commit before it for the failing fixture:

```bash
jj new -B @ -m "Add failing test case for <bug>"
```

Use the existing commit message style when a more specific noun reads better, such as `Add <bug> regression fixture`.

### 2. Add the regression fixture

Create a fixture in the selected category with a descriptive name:

```bash
just t <category> --create "<descriptive name>"
```

Write a focused PureScript fixture that reproduces one behavior. Use `Main.purs` by default; add supporting modules only when imports, exports, qualification, or LSP candidates are part of the regression.

Snapshot expectations differ by category:

- `compiler` enters through `Main.purs`; its checking report contains types only,
  diagnostics cover all fixture-owned modules with stable relative paths, its
  semantic report contains recovery trees, and its functional report contains a
  successful tree or explicit rejection.
- `lsp` snapshots `Main.purs` only.
- `sourcemap` enters through `Main.purs`; its report lists the generated JavaScript with the
  source position of each mapped column.
- `lowering` and `resolving` snapshot every `.purs` file in the fixture.

Accept the snapshot in the failing fixture commit:

```bash
just t <category> NNN --accept
```

The snapshot should intentionally encode the bug: the wrong error, missing type, bad constraint, incorrect semantic tree, incorrect lowered output, wrong resolution target, incorrect LSP response, unexpected `???`, or other undesirable current behavior. Do not try to make this commit green by changing compiler behavior; its purpose is to record the failure before the fix.

### 3. Return to the fix commit and update the snapshot

Move back to the child commit containing the fix:

```bash
jj edit <fix-change-id>
```

Run the same fixture and inspect the snapshot change:

```bash
just t <category> NNN --diff
```

Accept the updated snapshot:

```bash
just t <category> NNN --accept
```

The diff should show the undesirable behavior being removed or replaced by the correct behavior. This is commonly a deleted `Diagnostics` section or a targeted type/constraint change in the same snapshot added by the previous commit.

## Git-only workflow

The Git workflow is strictly forward-moving: start from the buggy compiler state, commit the failing fixture, then apply and commit the fix. Do not use rebase, reset, or temporary history manipulation to manufacture this order after the fix is committed.

If the fix is already present in the working tree or local history, stop and decide with the user whether to preserve the existing order or start again from the buggy base. Do not rewrite or discard their work implicitly.

### 1. Commit the failing fixture

Start from a clean feature branch where the bug still reproduces. Create the fixture and accept the snapshot of the undesirable behavior:

```bash
git status --short
just t <category> --create "<descriptive name>"
just t <category> NNN --accept
```

Review and commit only the new fixture:

```bash
git diff -- tests-integration/fixtures/<category>/NNN_<name>
git add tests-integration/fixtures/<category>/NNN_<name>
git commit -m "Add failing test case for <bug>"
```

### 2. Apply the fix and update the snapshot

Implement the compiler fix only after the failing fixture commit exists. Run the same fixture and inspect the change before accepting it:

```bash
just t <category> NNN --diff
just t <category> NNN --accept
```

Review and commit the compiler changes together with the updated snapshot:

```bash
git diff -- <compiler-paths> tests-integration/fixtures/<category>/NNN_<name>
git add <compiler-paths> tests-integration/fixtures/<category>/NNN_<name>
git commit -m "Fix <bug>"
```

## Snapshot review checklist

Before finishing, verify:

- The failing fixture commit snapshot captures the bug clearly.
- The fix commit snapshot changes only what the fix should change.
- Unexpected `???` does not appear unless it is the regression being documented.
- Error kinds, locations, inferred types, constraints, semantic nodes, lowered bindings, resolved references, and LSP payloads are intentional for the selected category.
- Each fixture covers a small, coherent unit that is easy to understand as a whole. Split overloaded fixtures before accepting snapshots or committing, while preserving meaningful context.
- For `lowering` and `resolving`, every changed module snapshot in the fixture is expected.

## Useful commands

```bash
just t <category> NNN           # Run fixture
just t <category> NNN --diff    # Inspect snapshot diff
just t <category> NNN --accept  # Accept fixture snapshot
just t <category> NNN --reject  # Reject fixture snapshot
```
