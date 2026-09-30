# Iris test-pruning campaign

Campaign mode reviews one subsystem's whole test surface, such as checking,
code generation, or LSP. Include the unit tests, integration fixtures, e2e
scenarios, package-corpus cases, and support that protect that owner, not just
files bearing its name. The value bar, retention bar, and evidence requirements
in [SKILL.md](SKILL.md) apply to every lane.

Use this only for a requested broad audit. A localized change does not require
a campaign. Finish each step's criterion before moving to the next.

## 1. Baseline

Pin the baseline revision and record test, fixture, golden, and support line
counts separately from production. Record pass/fail and pending-snapshot state
for every in-scope suite. Keep preparation or environment failures separate from
compiler defects. An accepted snapshot of known buggy behavior is still a bug,
not proof that the behavior is correct.

Done when every in-scope suite has a recorded baseline result and limitations.

## 2. Lanes and inventory

Split along production owner boundaries, not filename prefixes. For checking,
possible lanes include unification, instances, functional dependencies,
expressions, and recovery; use actual ownership rather than a prescribed list.
Include contracts exercised at shared core, integration, editor, and CLI
boundaries. Assign each test declaration, fixture report, and relevant scenario
one audit lane, while recording any cross-lane contracts it protects.

Done when the complete in-scope surface has an owner.

## 3. Read-only ledger

Read every assigned test and its parameter cases, production owners and entry
points, overlapping proof, history, and CI routing. Use read-only workers for
independent lanes when available and useful; otherwise review serially. Record
each declaration or fixture contract with one mark:

- `R`: retain, naming the independent contract and regression it detects.
- `F`: retain the contract but repair an assertion or input that fails to
  exercise it, such as a negative case rejected by an earlier unrelated phase.
- `C`: consolidate, naming the owner that must absorb the assertion first.
- `D`: delete, naming the remaining proof or why no contract exists.

Split table rows or fixture reports when they need different marks. Judge the
assertions and observations, not names. Compiler reports of types, diagnostics,
recovery, and functional conversion are not interchangeable.

Done when every in-scope contract has a mark and the candidate evidence required
by `SKILL.md`.

## 4. Layer plan

Review the ledger a second time for redundant layers rather than treating it as
a deletion list. Name a keeper for every contract. Prefer the real owning API,
fresh generated program, or real process with isolated external resources over
a mocked collaborator. Preserve local algorithm tests with distinct invariants.

Done when each lane names retired files, keeper contracts, assertions to move,
and test-only seams unlocked; uncertain candidates remain retained.

## 5. Cutover

Edit lane by lane. Serialize shared harness and support edits through one owner.
Move proof before deleting weaker suites, then remove unlocked test-only seams.
Check datatest discovery and CI routing; update explicit inventories only where
they exist. Regenerate expectations through their owning commands, never by hand.
Record durable ownership lessons in the relevant `AGENTS.md` only when warranted
by demonstrated mistakes, not as a generic policy rewrite.

Done when each lane plan is applied and its keeper checks pass.

## 6. Preservation review

Compare deleted coverage against keepers by boundary group, independently when
reviewers are available. Look for lost sole proof, vacuous assertions, unreachable
negative cases, and snapshots that merely bless a changed result.

For each restored contract, make a deliberate, narrowly scoped mutation of the
production owner and confirm the keeper detects it. For snapshot proof, an
unaccepted mismatch must expose the relevant semantic change; for runtime proof,
the assertion must fail for the intended reason. Do not accept mutation output.
Restore source byte for byte and remove only pending artifacts created by that
mutation; preserve pre-existing user work. Rerun the keeper after restoration.

Done when every finding is restored or rejected with source evidence and every
restored contract has a caught mutation. Report unavailable independent review
as a limitation rather than claiming it happened.

## 7. Product defects

A retained baseline failure or known incorrect snapshot is a bug report, not a
deletion candidate. Repair it only within authorized scope, in a separate atomic
change, following `workflow-regression-tests`. Preserve before/after proof through
the owning compiler fixture or real user flow. Do not rewrite existing history
or discard someone else's work to manufacture a failing baseline.

Done when each authorized repair has a buggy control and a verified candidate;
unrelated or unauthorized defects remain named follow-ups.

## 8. Reconcile and hand off

If upstream changes during the campaign, reconcile without rewriting published
history. A file deleted by the campaign may receive a new regression upstream;
port that contract into its keeper rather than losing it. Rerun affected suites
and any relevant runtime, e2e, or compatibility proof on the reconciled revision.
Before a PR push, run all affected integration categories unfiltered with no
pending snapshots, plus the root formatting and license gates.

Hand off the `SKILL.md` report plus baseline/final counts, lanes and keepers,
retired layers, preservation gaps and mutation results, product-defect controls,
review limitations, and actual commit/PR/merge state. Land only as authorized.
