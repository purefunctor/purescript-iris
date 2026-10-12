## Ownership

This crate owns source-based compiler integration tests through the public APIs and the combination
of compiler subsystems. Loading a file and observing its parsing, resolution, checked types,
diagnostics, semantic trees, generated code, or editor analysis belongs in the fixture harness.

Use the existing fixture loader, runners, and snapshot machinery. Do not reproduce this pipeline in
compiler-crate unit tests or add harness unit tests that merely load and compile another source
file. Local algorithm invariants belong beside their implementation; CLI and development-environment
contracts belong in `tests-e2e`.

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
- Use the unified `compiler` category for pipeline behavior and keep its `Main.purs` entry focused.
  Review the report that owns the contract: checking types, diagnostics for every fixture-owned
  module with stable relative paths, recovery semantic trees, or successful/explicitly rejected
  functional trees. Use runtime verification only when execution is the contract.
- Use real registry modules for library dependencies rather than handwritten stand-ins or vendored
  copies. Deliberately malformed or substituted registry modules must declare their reason through
  the existing fixture replacement mechanism.
- Keep goldens focused on reachable fixture-owned generated JavaScript. Optional `verify.mjs`
  verification must exercise a freshly generated full dependency closure, not tracked goldens.
- Verify harness changes through representative fixtures and affected category runs. A separate unit
  test is justified for an isolated harness algorithm only when it protects a distinct local
  invariant.

Use the `workflow-integration-tests` skill for fixture conventions and snapshot commands, and
`CONTRIBUTING.md` for package preparation and ownership. Preserve the root verification gates.
