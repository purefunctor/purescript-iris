---
name: auditing-tests
description: "Gates new or changed Iris tests and audits low-value, implementation-coupled, or duplicative coverage and test-only production seams. Use when writing or reviewing tests, sweeping a compiler subsystem, or pruning fixtures and test support in purescript-iris."
license: MIT
---

# Auditing Iris Tests

Three modes, one value bar. **Authoring** gates new or changed tests at write
time. **Audit** investigates a few high-confidence candidates for consolidation
or deletion. **Campaign** reviews one subsystem's whole test surface; read
[CAMPAIGN.md](CAMPAIGN.md) before starting one. Optimize for confidence, not
deletion counts or coverage percentages.

Adapted from OpenClaw's
[test-audit skill](https://github.com/openclaw/openclaw/tree/main/.agents/skills/test-audit).
The upstream license is preserved in [LICENSE](LICENSE).

Read root and scoped `AGENTS.md` files before working. This skill adds a test
value gate, not a replacement for Iris's ownership rules or verification policy.
Use the existing `workflow-integration-tests`, `workflow-regression-tests`, and
`running-compatibility-checks` skills for their respective workflows.

## Authoring gate

Before adding or changing a test, answer four questions:

1. What observable behavior, invariant, or independent contract does it protect?
2. What credible regression makes it fail or changes its observed report?
3. Why does existing coverage not already catch that failure? Each contract has
   one primary owner at the strongest useful boundary. Another layer needs a
   distinct risk, such as a local algorithm invariant versus its integration
   into compilation, or a process lifecycle failure compiler APIs cannot reach.
   Prefer extending a fixture or table over duplicating the same scenario.
4. Does it need an export, flag, wrapper, or injection hook no production caller
   needs? If so, test through the real owning boundary instead.

A missing answer means the test is not ready. Check it against the junk patterns
below; a match fails the gate unless the retention bar identifies an independent
contract. Behavior-preserving internal refactoring should not break a behavioral
test. Explicit representation or output contracts may legitimately constrain
refactoring; name those contracts instead of treating all snapshots as junk.

For a bug regression, demonstrate the undesirable behavior before the repair
and the intended behavior afterward at the owning boundary. Follow
`workflow-regression-tests` for Iris's failing-fixture/fix history. A deliberately
accepted baseline snapshot may pass while recording a bug: the decisive evidence
is the reviewed before/after report, not merely the runner's exit status. Runtime
regressions must exercise fresh generated code and fail for the intended reason
on the buggy compiler. Do not replay one bug at every layer it crosses.

## Choose the Iris owner

| Owner | Contract and suitable proof |
|-------|-----------------------------|
| Subsystem unit tests beside Rust code | Small algorithms and data structures using constructed local data, such as functional-dependency closure or pattern-matrix operations |
| `tests-integration` | Source-file behavior through compiler APIs and the existing fixture harness; do not build another compiler pipeline in a unit test |
| `compiler` fixtures | Checked types/kinds, diagnostics, semantic recovery, functional conversion, generated JavaScript, and optional runtime verification |
| `lowering`, `resolving`, `lsp` fixtures | The corresponding lowered/source-link, name/import/export, or editor-analysis contract |
| `tests-e2e` | Real CLI, Spago, filesystem, shell/process, watch/run, and development-environment behavior through temporary workspaces |
| `tests-compatibility` | Real package-set compatibility and benchmarks; extract a focused compiler regression into `tests-integration` |
| `tests-support` | Registry preparation, resolution, digest verification, extraction, and cache-publication invariants, not compiler semantics |

Compiler fixtures enter through `Main.purs`. Their checking, diagnostics,
semantic, and functional reports protect different observations; sharing one
input does not make those reports redundant. Keep generated goldens limited to
reachable fixture-owned modules. Use `verify.mjs` only when execution is the
contract; it must test fresh output, never tracked goldens. Use real registry
modules and the existing `replacements.json` mechanism for deliberate substitutes.

## Junk patterns

- Assertion-free coverage probes, self-comparisons, and identity copiers.
- Expected values computed by the helper, renderer, or compiler under test.
- Copied inventories, manifests, export lists, or exact source/import greps
  without an independent contract.
- Private predicate or call-shape tests duplicated by stronger boundary proof.
- Multiple invocations of the same contract without distinct failure modes.
- Source-compilation unit tests duplicating the integration fixture pipeline.
- Handwritten library stand-ins where real prepared registry modules belong.
- Mocks that implement the behavior being asserted or stand in identically for
  APIs with different contracts.
- Fixtures that supply the ordering, persistence, or evidence the owner should
  produce, or assert a store the exercised path never writes.
- Capability tests that restate declared flags rather than exercising delivery.
- Tests that exist only to preserve test-only exports, globals, or wrappers;
  production code whose only callers are tests.
- Negative controls that pass because of an unrelated parser, resolver, checker,
  or environment failure before reaching the intended contract.
- Names promising more than the inputs and assertions exercise.
- Snapshot acceptance without checking types, diagnostics, locations, recovery,
  generated code, or runtime results against the intended semantics.

## Value and retention bar

Keep tests that independently protect public APIs, language semantics, compiler
representations with an intentional contract, LSP payloads, configuration,
storage, security, platforms, defaults, generated code, packages, releases, or
architecture. Also retain observable call ordering, credible regressions, and
source inspection when it is the cheapest independent guard and survives an
identifier-only refactor.

Static, slow, snapshot-based, or implementation-adjacent is not a deletion
reason. A test that resembles implementation may still be its independent
contract. Prove redundancy before removing it. Passing snapshots do not prove
semantic correctness. A retained baseline failure may be a product defect;
reproduce it instead of deleting the evidence or accepting it away.

## Read-only discovery and candidate evidence

Keep discovery read-only and report evidence before editing. For each candidate,
read the complete test or fixture, production owner, entry points, callers,
callees, sibling implementations, overlapping coverage, relevant history, and
CI routing. Inspect dependency source or types when a claim depends on them.
Consult `.github/workflows/checks.yml`, `.github/workflows/platform-tests.yml`,
and `.buildkite/` as relevant; do not infer CI coverage from crate membership.

Record every field before deleting or consolidating a candidate:

- Exact test declaration or fixture/report and its location.
- Failure it can actually detect, not merely its name or intended purpose.
- Non-test callers of its production or support seam.
- Stronger remaining owner-boundary proof, or why no contract needs proof.
- Relevant history and why the test or seam exists.
- Production or test-support deletion unlocked.
- Risk and focused validation command.

A missing field means the candidate is not ready. Prefer a few well-supported
candidates to a speculative inventory. For broad discovery, split lanes by
production owner and give workers disjoint scope when delegation is available
and useful; a small audit needs no delegation ceremony.

## Edit shape

Choose one coherent owner-boundary batch. Move retained contracts into their
canonical owners before removing weaker proof. Delete obsolete test-only
exports, globals, wrappers, and dead paths rather than preserving aliases.
Consolidate repeated assertions at a shared owner when their risks are identical.

Prefer simpler production code, not a target LOC reduction. Do not add replacement
tests that restate implementation, weaken assertions, or turn uncertain
candidates into cleanup to inflate deletion counts. Leave unrelated bugs alone;
report them as follow-ups unless their repair is authorized.

## Validation

Do not edit source, fixtures, or expectations during a test run in the checkout.
Choose checks by affected behavior and preserve root verification gates:

1. Check changed Rust crates with `cargo check -p <crate-name> --tests` and run
   their unit tests with `cargo nextest run -p <crate-name>`.
2. Iterate on fixtures with `just t <category> <filters>`. Before pushing a
   change affecting integration tests, run every affected category without
   filters and confirm it passes with no pending snapshots.
3. Never edit `.snap` files or JavaScript goldens by hand. Review diffs with
   `just t <category> <filters> --diff`, accept intended snapshots with
   `just t <category> <filters> --accept`, and regenerate JavaScript through
   `just t compiler <filters> --update-output`. Inspect every expectation.
4. For CLI/environment changes, run `just e2e-prepare` and the relevant
   `just e2e` tests; preserve the shared harness's cross-platform assertions.
5. For compatibility impact, follow `running-compatibility-checks` and use
   `just compatibility <base-ref>` when justified. Do not substitute package
   corpus results for focused regression proof or confuse them with benchmarks.
6. For removed source greps or plan assertions, execute the script or operation
   owning the real contract. Run `just format` for Rust changes and
   `git diff --check`. Before a PR push, run `just format` and `just licenses`
   and fold resulting changes into their relevant commits as repository policy
   requires.
7. Review the final diff for lost contracts and accidental snapshot acceptance.
   Use `git diff --numstat` to report production/tooling separately from tests,
   fixtures, goldens, and support. State which checks actually ran and any limits.

## Landing and handoff

Commit, push, open a PR, or merge only as authorized. Follow the root commit and
PR conventions; do not import another project's review bots or landing scripts.
Keep one coherent audit batch reviewable. After landing, refresh the baseline
before starting another batch.

Report removed low-value categories, owner simplifications, retained false
positives and their contracts, proof actually run, production versus test/support
LOC, delivery state, and named follow-ups. Campaigns also use the handoff in
[CAMPAIGN.md](CAMPAIGN.md).
