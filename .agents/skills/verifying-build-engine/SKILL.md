---
name: verifying-build-engine
description:
  "Model-checks the query engine's states and transitions in serial and parallel modes with TLA+.
  Covers claiming, verification, waiting, cycle detection, cancellation, and input writes. Use when
  changing compiler-core/building/src/engine.rs, engine/promise.rs, or engine/graph.rs, or when
  investigating hangs, spurious cycles, spurious cancellations, or stale results from the build
  engine."
---

# Verifying the build engine

`compiler-core/building/verification/QueryEngine.tla` is a TLA+ model of the query engine's state
machine. TLC checks every interleaving of up to three snapshots within small bounds. The model does
not check the Rust code. It checks the design that the Rust code follows, one locked step at a time.

## When to run it

Run it when a change touches a function in the coverage record, or changes which lock or atomic
makes a step indivisible. Changes to a query body, such as `resolved` or `checked`, do not need it
unless they change how queries call each other.

## Run

From the repository root:

```bash
compiler-core/building/verification/check.sh
compiler-core/building/verification/check.sh --quick
```

The full run takes about 8 minutes. `--quick` runs `Serial`, `Parallel`, and the finding configs in
about 2.5 minutes.

The script uses `.agents/scripts/tlc.sh` to install Temurin 21.0.12 through mise and download
SHA-256-checked `tla2tools.jar` v1.7.4 into `target/tla`. Do not pin v1.8.0. It is a rolling
prerelease, and GitHub replaces its jar in place. Each config runs on a copy of the model in a
temporary directory, so TLC writes no trace files into the repository. Logs remain in
`target/tla/runs/building/`.

A clean run ends with `All results as expected` and exit status 0. A `FAIL` line means a property
failed, and the script prints the trace. A `MISSED` line means TLC did not catch a mutation.

To read a full counterexample, copy the model and a config to a scratch directory and run TLC there:

```bash
scratch="$(mktemp -d)"
cp compiler-core/building/verification/{QueryEngine.tla,SpuriousCycle.cfg} "$scratch"
(cd "$scratch" && mise exec java@temurin-21.0.12+8.0.LTS -- java -cp "$OLDPWD/target/tla/tla2tools-1.7.4.jar" \
  tlc2.TLC -config SpuriousCycle.cfg -workers auto QueryEngine.tla)
```

## Configurations

| Config               | Snapshots | Queries | Payloads | Writes | Panics | Expected result                                                                           |
| -------------------- | --------- | ------- | -------- | ------ | ------ | ----------------------------------------------------------------------------------------- |
| `Serial.cfg`         | 1         | A, B    | 0, 1, 2  | 2      | 0      | Passes every property                                                                     |
| `Parallel.cfg`       | 2         | A, B    | 0, 1, 2  | 1      | 0      | Passes every property                                                                     |
| `ParallelWrites.cfg` | 2         | A, B    | 0, 1     | 2      | 0      | Passes the safety invariants, `CancellationsAreExplained`, and `CyclesAreReal`            |
| `ParallelChain.cfg`  | 3         | A, B, C | 1        | 0      | 0      | Passes the safety invariants, `CancellationsAreExplained`, and `CyclesAreReal`            |
| `SpuriousCycle.cfg`  | 1         | A, B    | 0, 1, 2  | 2      | 0      | Passes `CyclesAreReal` through the import flip of finding 1                               |
| `SpuriousCancel.cfg` | 2         | A, B    | 1        | 2      | 0      | Passes `CancellationsAreExplained` through the import flip of finding 1, seen by a waiter |
| `PanicHang.cfg`      | 2         | A, B    | 1        | 0      | 1      | Passes every property after a compute panics                                              |

The safety invariants are the first nine properties in the next section. `SpuriousCycle` and
`SpuriousCancel` check one property each over the smallest bounds that reached finding 1.

Each query has a payload, which stands for its source text, and at most one import of another query.
Its value is the set of `<<query, payload is nonzero>>` pairs that its imports reach. An edit from
payload 1 to payload 2 changes the text but not the value, which tests early cutoff. An import edit
can create or remove a real cycle. Flipping "A imports B" to "B imports A" takes two writes in the
model, as in the code, because `set_input` changes one key per revision.

## Properties

Each property is one sentence, followed by what users see if it breaks.

- `InProgressHasLiveOwner`: every `InProgress` entry belongs to a running frame of its owner. If it
  breaks, nothing resolves the entry, and every later demand for it hangs.
- `OwnerFramesHoldTheirQuery`: a frame that claimed a query still holds that query's `InProgress`
  entry. If it breaks, two threads compute one query and drop each other's waiters.
- `WaitEdgesMatchWaiters`: a snapshot has a `SnapshotGraph` edge exactly while it is registered as a
  waiter. If it breaks, cycle detection reads a wrong graph.
- `WaitForGraphIsAcyclic`: no group of snapshots waits on each other in a loop. If it breaks, the
  engine deadlocks. A correct `add_edge` already guarantees this in `CreateFuture`, so only the
  `add_edge` mutations fail it. `WaitEdgesMatchWaiters` and `BlockedWaitersCanBeWoken` guard against
  the other deadlocks.
- `BlockedWaitersCanBeWoken`: a blocked `Future::wait` either has a result or waits on an entry
  whose owner is still running. If it breaks, the request or build hangs, and every later input
  write blocks behind the waiter's read lock.
- `DroppedSnapshotsLeaveNoTrace`: a snapshot that finished normally owns no entry and waits on
  nothing.
- `FreshCacheIsCorrect`: a `Computed` entry with `built == revision` equals a from-scratch
  evaluation of the current inputs. If it breaks, users see stale diagnostics or output after an
  edit.
- `OkResultsAreCorrect`: every `Ok` that a caller receives equals a from-scratch evaluation.
- `UnverifiableStaysInternal`: no demand returns `Unverifiable` to its caller. If it breaks, users
  see an internal error from the engine.
- `CancellationsAreExplained`: a query returns `Cancelled` only when a write or cancel is pending,
  or when the program has a real cycle. If it breaks, the LSP drops a round of diagnostics for no
  reason.
- `CyclesAreReal`: a query returns `Cycle` only when the current inputs have a cycle. If it breaks,
  users see a cycle error for a valid program.
- `DemandsComplete`: every demand eventually returns. This is a liveness property, checked with weak
  fairness for each snapshot.

## Findings

The model found two bugs, and both are fixed. Their configs now run with `expect_pass`, and a
mutation restores each bug. If the model finds a new bug, add a config that reproduces it, run it
with `expect_violation` until the code is fixed, and do not weaken the model to make it pass.

1. **Spurious `Cycle` after an import flip.** Fixed. `SpuriousCycle.cfg` and `SpuriousCancel.cfg`
   reproduced it.
   1. In revision 0, B imports A, and the engine has computed both.
   2. Two edits make A import B and remove B's import of A.
   3. Demanding A recomputes A, which demands B.
   4. B is stale, so `query_core` marks it `InProgress`, and `verify_core` re-runs B's old
      dependencies. Those include A.
   5. A is `InProgress` on the same snapshot, so `create_future` fails `add_edge(self, self)`.
   6. B and A cleaned up and returned `Err(Cycle)`. The next demand succeeded. A waiter on another
      snapshot returned `Cancelled`.

   The fix has three parts, and a mutation restores each one:
   1. `create_future` returns `Unverifiable` instead of waiting when any frame on the snapshot's
      stack is verifying. This replaces the same-snapshot `Cycle` in step 5. It also stops a
      verifying snapshot from waiting on a second snapshot that recomputes from current inputs and
      closes the wait loop, which made the second snapshot report a spurious `Cycle`. Mutation:
      `VerifyingDemandBlocks`.
   2. Old dependencies can form a cycle, or reach work of other snapshots, that the current inputs
      do not need. `query_core` treats `Unverifiable` from `verify_core` as "cannot verify" and
      recomputes the query from its current inputs, which finds a real cycle again. Other errors
      still propagate. Because of part 1, `verify_core` never returns `Cycle`. Mutation:
      `UnverifiablePropagates`.
   3. When a query fails, the cleanup in `QueryEngine::query` hands its `InProgress` entry to one of
      its waiters instead of discarding it. It moves the other waiters' graph edges to that heir,
      and `Promise::transfer` wakes the heir, which computes the query itself in `wait_core`.
      Without this, failures that part 1 recovers from still make other snapshots' waiters return
      `Cancelled`. Mutation: `CleanupDropsWaiters`.

   A simpler third part, where a waiter whose producer failed starts its demand again, fails
   `DemandsComplete` in `Parallel`. When the program has a real cycle, new snapshots can claim the
   entry before the woken waiter does, forever. The hand-off leaves no window to race for.

   `test_flipped_import_is_not_a_cycle` and `test_failed_query_hands_off_to_waiter` are the
   regression tests.

2. **A panic leaves `InProgress` forever.** Fixed. `QueryEngine::query` now catches the unwind, runs
   the same cleanup as for `Err`, and resumes the panic, so every frame on the unwinding stack
   releases the entry it owns. The cleanup hands each entry to one of its waiters, which computes
   the query itself. If that compute panics too, its entry passes to the next waiter, so a query
   that always panics fails once for each waiting request instead of hanging any of them.
   `PanicHang.cfg` now passes, and the `PanicSkipsCleanup` mutation restores the old behaviour.
   `test_panicking_query_releases_in_progress` is the regression test.

## Mutations

`check.sh` runs these broken variants and requires TLC to catch each one. They show that the
properties can fail.

| `Mutation`                | What it breaks in the code                                                                                                    | Caught by                                                             |
| ------------------------- | ----------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------- |
| `TwoOwners`               | `query_core` re-checks the state under `upgradable_read`. The mutation overwrites an `InProgress` entry instead of joining it | `OwnerFramesHoldTheirQuery` in `Parallel`                             |
| `CleanupIgnoresOwnership` | The `LocalState::is_in_progress` guard in the cleanup in `QueryEngine::query`                                                 | `OwnerFramesHoldTheirQuery` in `Parallel`                             |
| `EdgeAlwaysAdded`         | `SnapshotGraph::add_edge` rejects edges that close a loop                                                                     | `WaitForGraphIsAcyclic` in `Serial`                                   |
| `AddEdgeChecksOneHop`     | `add_edge` follows the whole chain of successors                                                                              | `WaitForGraphIsAcyclic` in `ParallelChain`                            |
| `EagerCycle`              | `add_edge` rejects only real loops, not every owner that is itself waiting                                                    | `CyclesAreReal` in `ParallelChain`. Two snapshots cannot catch it     |
| `EdgeNotRemoved`          | `remove_waiter_edges` in `fulfill_and_store` and in the cleanup                                                               | `WaitEdgesMatchWaiters` in `Parallel`                                 |
| `WaitersNotWoken`         | The cleanup transfers the entry to a waiter and wakes it                                                                      | `BlockedWaitersCanBeWoken` in `Parallel`                              |
| `ChangedAlwaysPrevious`   | `compute_core` sets `changed: revision` when the value differs                                                                | `FreshCacheIsCorrect` in `Serial`                                     |
| `VerifyIgnoresInputs`     | `verify_core` reads `InputState::changed`                                                                                     | `FreshCacheIsCorrect` in `Serial`                                     |
| `UnverifiablePropagates`  | `query_core` recomputes when `verify_core` returns `Unverifiable`                                                             | `UnverifiableStaysInternal` in `Serial`                               |
| `VerifyingDemandBlocks`   | `create_future` returns `Unverifiable` under a verifying frame                                                                | `CyclesAreReal` in `ParallelWrites`                                   |
| `CleanupDropsWaiters`     | The cleanup in `QueryEngine::query` hands the entry to a waiter                                                               | `CancellationsAreExplained` in `SpuriousCancel`                       |
| `PanicSkipsCleanup`       | `catch_unwind` in `QueryEngine::query` runs the cleanup before it resumes the panic                                           | `InProgressHasLiveOwner` or `BlockedWaitersCanBeWoken` in `PanicHang` |

## Mapping to code

Each TLA+ action is one step that the code makes indivisible with a lock. The code locations match
the sync commit.

| Model                                                     | Code                                                                                                                                                          | Why the step is indivisible                                                                                                                                                                                                                                                                    |
| --------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `QueryCoreReadPath`                                       | The cancel check and the `shard.read()` block in `query_core`                                                                                                 | The thread holds the shard read lock across `create_future`. The owner needs the write lock to fulfill or clean up                                                                                                                                                                             |
| `QueryCoreUpgradablePath`, `Claim`                        | The `shard.upgradable_read()` block in `query_core`                                                                                                           | A shard has one upgradable holder at a time, which upgrades to write for the insert                                                                                                                                                                                                            |
| `CreateFuture`                                            | `create_future`, which returns `Unverifiable` without touching the graph while a frame is verifying                                                           | It takes the graph `Mutex` and then the waiters `Mutex`, both under the shard lock                                                                                                                                                                                                             |
| `FutureWait`                                              | `promise::Future::wait`, and `wait_core`, which computes the query when the promise was transferred                                                           | The slot `Mutex` and `Condvar`                                                                                                                                                                                                                                                                 |
| `VerifyCore`, `VerifyCoreReturn`                          | `verify_core`, and the match on its result in `query_core`, which recomputes on `Unverifiable`. `QueryFrame::record` records nothing while `verifying` is set | `verify_core` reads or demands each dependency separately and never stops early. The dependencies sit in an `FxHashSet` with no fixed order, so the model tries every order                                                                                                                    |
| `ComputeCore`, `ComputeChildren`, `ComputeChildrenReturn` | `compute_core` and the nested demands in the query body                                                                                                       | The body runs without locks. Only the store takes a lock                                                                                                                                                                                                                                       |
| `FulfillAndStore`, `Release`                              | `fulfill_and_store`, `remove_waiter_edges`                                                                                                                    | The shard write lock, with the graph lock inside it. Woken waiters run only after the lock drops, so other threads see only the end state                                                                                                                                                      |
| `Fail`, `HandOff`, `MarkOwner`                            | The cleanup after `catch_unwind` in `QueryEngine::query` and `hand_off_waiter_edges`                                                                          | The shard write lock, with the graph lock inside it. The entry names the heir as its owner before the lock drops. The model also marks the heir's waiting frame as owner in the same step. The code marks it in `wait_core` when the heir wakes, and the heir's thread runs nothing in between |
| `ComputePanics`, `UnwindCleanup`                          | Unwinding out of `compute`, caught by `catch_unwind` in each `QueryEngine::query` frame                                                                       | Each frame takes the shard write lock for its own entry, innermost first, and hands it off. A snapshot that joins an outer entry between two releases is handed that entry or keeps waiting on its heir, the same as a waiter that joined before the panic                                     |
| `TakeSnapshot`                                            | `QueryEngine::snapshot`, and `lock_shared_recursive` in `QueryControlGuard::new`                                                                              | It blocks only while a writer holds the lock, so the model allows it while a writer waits                                                                                                                                                                                                      |
| `RequestWrite`, `CommitWrite`                             | `set_input_transaction` sets `cancelled`, takes `query_lock.write()`, bumps `revision`, and clears `cancelled`                                                | The write lock waits for every snapshot to drop                                                                                                                                                                                                                                                |
| `RequestWrite`, `CancelWithoutWrite`                      | `request_cancel`, and `set_input_transaction` when the new value equals the old one                                                                           | The same lock, without a revision bump                                                                                                                                                                                                                                                         |

## What the model leaves out

- The model has one lock per query instead of 16 shards. The protocol is the same.
- Derived queries always run through a snapshot. The root `QueryEngine` from `QueryControl::default`
  has no lock guard, but production code queries only through snapshots.
- The model covers content writes only. `remove_file` and `remove_foreign_file` take the same write
  lock and also discard entries. `remove_file` discards every `InProgress` entry, whoever owns it.
  Because the model leaves it out, the `ELSE 0` branch in `VerifyCoreReturn` never runs.
- The only errors are `Cycle`, `Unverifiable`, and `Cancelled`. `MissingContent` takes the same
  cleanup path.
- The model checks cancellation at query entry and before compute, as the code does. No property
  catches a deleted cancellation check, and the model cannot measure how long a compute runs before
  it stops.
- `FulfillAndStore` is one step, so the model cannot express fulfilling waiters before removing
  their edges. Check that order in code review.
- The script does not check that every write completes. New snapshots can keep a writer waiting
  forever, and the doc comment on `QueryEngine::snapshot` calls that caller misuse.
- The bounds are up to 3 snapshots, 3 queries, and 2 writes. A pass covers those bounds only.
- TLC does not read the Rust code. A code change that the model does not mirror is not verified.

## Coverage record

- **Sync commit:** `1d388327c562aeb67e5f971fb42d3eb338a3056e`
- **Covered code:**
  - `compiler-core/building/src/engine.rs`: `QueryEngine::query`, `fulfill_and_store`,
    `remove_waiter_edges`, `hand_off_waiter_edges`, `compute_core`, `verify_core`, `create_future`,
    `wait_core`, `query_core`, `set_input_transaction`, `snapshot`, `request_cancel`,
    `QueryControlGuard`, `QueryFrame::record`, `LocalState::is_verifying`
  - `compiler-core/building/src/engine/graph.rs`: `SnapshotGraph::add_edge`, `remove_edge`
  - `compiler-core/building/src/engine/promise.rs`: `Future::wait`, `Promise::fulfill`,
    `Promise::transfer`, `Drop for Promise`

To find changes since the sync commit:

```bash
git log --oneline 1d388327c562aeb67e5f971fb42d3eb338a3056e..HEAD -- compiler-core/building/src/engine.rs compiler-core/building/src/engine/
```

For each commit, check whether it changes a step in the mapping table. If it does, update the model
and run `check.sh`. Move the sync commit forward only after you confirm that the model describes the
code at the new commit.

## Real bug or modeling error?

When a property fails, read the trace as a sequence of steps from the mapping table, and check each
step against the code.

- If the code cannot take a step in the trace, the model is wrong. For example, the model may treat
  two steps as one that the code does not, or it may miss a lock. Fix the model and say what was
  wrong.
- If the code can take every step, the bug is real. Reproduce it with a scratch Rust test in
  `engine.rs` if you can, and report it with the trace. Do not change the property to make it pass.

When a code change is intentional, change the matching action first. If the change makes a mutation
impossible, remove that mutation or rewrite it to break the new code. Keep at least one mutation for
every property.
