---
name: measuring-performance
description: "Measures Iris compiler performance against a base revision with Callgrind instruction counts, allocation counts, interned-type counts, output digests, and interleaved wall time. Use before and after any performance change, or when judging whether a change regresses performance."
---

# Measuring Performance

Performance is judged by numbers from the repository's harness, not by impressions or a single
timing.

## Run the comparison

From the repository root, on Linux with Valgrind installed:

```bash
just perf <base-ref>
```

The base defaults to `origin/main`. The candidate is the current checkout, including uncommitted
changes. The base is built in `target/performance/base-worktree` with its own target directory, so
repeated runs against the same base reuse its build. The base revision must contain the harness.
The command clones the registry and prepares the corpus under `target/compatibility/` itself.

Both sides build with whatever toolchain `rustc` resolves to in the workspace, which the run
prints. That is the stable Rust pinned in `flake.nix`, which the development shell puts first on
`PATH`; CI builds with the latest stable instead. Instruction counts differ
between toolchains, so compare only numbers taken on the same one.

The command prints a Markdown summary and writes `summary.md`, `comparison.json`, every
measurement, logs, and one Callgrind profile per stage under `target/performance-reports/`.

Environment variables adjust the run; `bash .agents/skills/measuring-performance/scripts/run.sh
--help` lists them. `PERF_TIMED_RUNS=0` skips wall time when only the gated metrics matter.

Exit statuses:

- `0`: no gated metric regressed beyond the threshold (0.5%)
- `1`: a gated metric regressed, or generated output changed; the reports are valid
- `2`: building, preparing, or measuring failed; read the logs in the report directory

CI's Performance job runs the gated `core` measurement on every push to `main`, comparing the
pushed commit with its first parent, which is the previous `main` for a merge commit. A failure
there reports a regression that has already landed, so run `just perf origin/main` before merging.

## What is measured

| Metric | Corpus | Role |
| --- | --- | --- |
| Instructions per stage outside the allocator, one thread, Callgrind | `core` | Gate |
| Instructions inside the allocator per stage | `core` | Reported |
| Allocations, reallocations, and bytes per stage | `core` | Gate |
| Interned types, types with unification variables, rows | `core` | Gate |
| Digest of generated JavaScript and source maps | `core` | Gate; must be identical |
| Wall time per stage, one thread and all threads, interleaved | `acme` | Reported |
| Peak resident memory | `acme` | Reported |

Stages are the build queries in pipeline order (`parsed` through `documented`), their sum
(`build`), and three edits to `Data.Maybe` that each run every query again from the fully built
engine, measuring the incremental path a language server takes:

- `edit_comment` appends a comment; nothing semantic changes.
- `edit_body` changes the body of `fromMaybe` without changing its type.
- `edit_interface` appends an exported declaration, which changes the module's interface.

The module is restored and rebuilt, unmeasured, between edits. The package set is pinned in the
harness (`DEFAULT_PERFORMANCE_PACKAGE_SET`) so numbers stay comparable over time.

Two runs of an unchanged revision differ by under 0.001% in instructions and not at all in
allocations or interned counts. Stages a change does not run stay within about 0.01%, for two
reasons that also apply when you measure by hand:

- Instructions exclude the allocator (everything reached through mimalloc's `mi_malloc`,
  `mi_zalloc`, `mi_realloc`, and `mi_free` entry points). Its page management depends on the
  heap that earlier stages leave behind and moves by up to 1% in stages a change never runs.
  Allocation counts gate the allocator instead.
- Both harnesses are built with `CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1`, in their own target
  directories under `target/performance/`. With the default 16 codegen units, recompiling a crate
  repartitions the generic code it instantiates, such as the query engine's, and inlining changes
  move unrelated stages by up to 0.2%.

When a stage your change does not run still moves, diff that stage's profiles from
`<preset>-base.callgrind/` and `<preset>-head.callgrind/` with `callgrind_annotate`. A function
that appears or disappears in one of them is an inlining decision that the change flipped, for
example by adding a caller to a shared generic; such a shift is real in the binary, so either
explain it in the commit or reshape the change so the other stages stay unchanged.

The `Prim` modules are measured files, so their parsing is counted in `parsed` rather than in the
first stage that imports them.

## Rules for performance changes

- One optimisation per commit. Put the `just perf` table for the affected stages (instructions,
  allocations, interned counts) in the commit body.
- Keep a change only if output is identical and no stage regresses by more than 0.5% in
  instructions without a stated reason.
- Instruction counts miss cache misses and contention. A claim about parallel scaling or memory
  needs the wall-time and peak-memory tables, from a quiet machine, since Valgrind serialises
  threads.
- Every new cache gets a check under `debug_assertions` that recomputes the uncached answer and
  compares, so the integration suite verifies it.
- Run the integration categories the change can affect; the output digest covers only the
  generated JavaScript of the `core` corpus.

## Profile one revision

To find where instructions go, measure one revision and read its per-stage profiles:

```bash
CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 cargo run --release -p tests-compatibility \
  --target-dir target/performance/head-target -- measure --preset core --json-output target/perf/core.json
callgrind_annotate --inclusive=yes target/perf/core.callgrind/checked.callgrind.out | head -60
```

`measure --timing --threads N --preset acme` measures wall time and memory without Valgrind.
`compare-performance --base <json>... --candidate <json>...` compares any set of measurements.
