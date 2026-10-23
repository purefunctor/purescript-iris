---
name: verifying-parallel-interner
description:
  "Model-checks the lock-free parallel interner (compiler-core/interner/src/parallel.rs) with
  TLA+/TLC. Use when changing intern_with_metadata, intern_at_collision, publish_collision, get,
  scan_overflow, arena_value, the Interner fields, or the boxcar, papaya, or parking_lot
  dependencies of the interner crate, or when a type checker interner shows duplicate Ids, missed
  lookups, or an 'is not a valid index' panic."
---

# Verifying the parallel interner

`compiler-core/interner/src/parallel.rs` backs every `CoreInterners` table in
`compiler-frontend/checking/src/interners.rs`. Threads intern concurrently without a global lock. A
race can give one type two `TypeId`s, or give two types one `TypeId`. The type checker compares
types by `Id`, so either race gives wrong answers without an error. Unit tests rarely reach these
interleavings. The TLA+ model in `compiler-core/interner/verification/` checks every interleaving of
a bounded set of threads.

## Properties

`ParallelInterner.tla` defines each property as a TLA+ invariant, except `TableIsWriteOnce`, which
is an action property. TLC also reports a deadlock if an operation can get stuck, for example on the
`overflow` lock.

| Property                             | Statement                                                                                                                                                 | Harm when broken                                                                                                          |
| ------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `EqualValuesShareId`                 | Every `Id` returned for equal values is the same `Id`.                                                                                                    | Equal types compare unequal.                                                                                              |
| `DistinctValuesNeverShareId`         | Distinct values never receive the same `Id`.                                                                                                              | Different types compare equal.                                                                                            |
| `IdNamesItsValue`                    | The arena slot of every returned `Id` holds the value it was returned for.                                                                                | `interner[id]` reads the wrong type.                                                                                      |
| `GetFindsReturnedInterns`            | `get(v)` returns the `Id` of `v` whenever an `intern(v)` returned before the `get` started.                                                               | Lookups miss interned types.                                                                                              |
| `ReturnedIdsArePublished`            | Every returned `Id` names an arena slot whose write the returning thread has released.                                                                    | A thread that receives the `Id` can read an unwritten slot.                                                               |
| `ArenaReadsArePublished`             | Every `arena_value` read inside the interner happens after an acquire that synchronizes with the slot's write.                                            | `arena_value` hits `unreachable!("invariant violated: ...")`.                                                             |
| `OverflowIsCoherent`                 | Every `overflow` entry has a table entry for its hash and holds a value different from that entry's value. No two entries for one hash hold equal values. | `EqualValuesShareId` and `GetFindsReturnedInterns` depend on this lemma. A failure places the bug in the collision paths. |
| `TableIsWriteOnce` (action property) | `table[hash]` never changes once it is set.                                                                                                               | A replaced entry leaves already returned `Id`s unreachable from the table, and duplicates follow.                         |
| `ViewsHoldOnlyWrittenSlots`          | Checks the model's memory abstraction.                                                                                                                    | A failure is a modelling error, not a code bug.                                                                           |

## Check a change

From the repository root, run:

```bash
compiler-core/interner/verification/check.sh
```

The script uses `.agents/scripts/tlc.sh` to install Temurin 21.0.12 through mise and download
SHA-256-checked `tla2tools.jar` 1.7.4 to `target/tla/`. Each check runs on a copy of the models in a
temporary directory. The run takes about 90 seconds. A good run prints three `pass` lines and one
`caught` line per mutant and property pair, then ends with `All results as expected` and exits with
status 0.

- `pass` means that no reachable state of the unmutated model violates any property in that scope.
- `caught` means that a broken variant of the model fails the property that guards it, when TLC
  checks that property alone.
- `MISSED` means that the check no longer detects that bug. Treat it as seriously as a failing
  baseline.

The other modes are `check.sh baseline`, `check.sh mutants`, and `check.sh matrix`. The `matrix`
mode checks every mutant against every property separately and takes about 15 minutes.

TLC writes its logs, including counterexample traces, to `target/tla/runs/interner/<run>.log`.

## Decide whether the change touches the verified code

Compare the covered files with the sync commit:

```bash
git diff 753da5fcaa8edd7a04013dc5ddbe86d451e8627a -- \
  compiler-core/interner/src/parallel.rs compiler-core/interner/Cargo.toml
git diff 753da5fcaa8edd7a04013dc5ddbe86d451e8627a -- compiler-core/interner/src/lib.rs | rg -A6 'fn id_from_index'
git diff 753da5fcaa8edd7a04013dc5ddbe86d451e8627a -- Cargo.lock | rg -A3 'name = "(boxcar|papaya|seize|parking_lot)"'
```

If the diff touches a function in the coverage table, the model may no longer describe the code.
Update the model first, then run the check.

## Coverage record

Sync commit: `753da5fcaa8edd7a04013dc5ddbe86d451e8627a`. At this commit, the model describes the
code below. Move the sync commit forward only after you confirm that the model still describes the
code at the new commit.

| Code in `parallel.rs`                                                           | Model action or definition                                           | Atomic unit in the model                                                                                               |
| ------------------------------------------------------------------------------- | -------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| `intern_with_metadata`: `table.get(&hash)` and `arena_value(id) == &value`      | `InternWithMetadata`                                                 | One papaya `get` with Acquire, plus a read of the immutable slot.                                                      |
| `intern_with_metadata`: `self.arena.push(...)`                                  | `ArenaReserve`, then `ArenaWrite`                                    | The boxcar `push` does `inflight.fetch_add` with Relaxed, then writes the slot and does `active.store(true, Release)`. |
| `intern_with_metadata`: `table.try_insert(hash, candidate)` and both match arms | `TryInsert`                                                          | One papaya `try_insert`. It releases on success and acquires `current` on failure.                                     |
| `intern_at_collision`: lock, `scan_overflow`, early return                      | `InternAtCollision`                                                  | One `overflow.lock()` critical section up to the scan result.                                                          |
| `intern_at_collision`: `arena.push`, `overflow.push`, guard drop                | `CollisionPush`                                                      | The rest of the same critical section.                                                                                 |
| `publish_collision`                                                             | `PublishCollision`                                                   | One `overflow.lock()` critical section.                                                                                |
| `get`: `table.get(&hash)` and `arena_value(id) == value`                        | `Get`                                                                | Same as `InternWithMetadata`.                                                                                          |
| `get`: `overflow.lock()` and `scan_overflow`                                    | `GetOverflow`                                                        | One critical section.                                                                                                  |
| `scan_overflow`                                                                 | `ScanOverflow`, `ScanOverflowReads`                                  | A reverse scan that stops at the first match. Each inspected entry with the same hash is an `arena_value` read.        |
| `id_from_index` in `lib.rs`                                                     | Not modelled; `Len(arena) + 1` in `ArenaReserve` and `CollisionPush` | See the abstractions below.                                                                                            |
| `arena_value` and `Index`                                                       | The `id \notin known` checks and `Unreachable`                       | boxcar `get` returns `None` unless its Acquire load of `active` sees the Release store from `push`.                    |

### Memory model

Each thread has a `view`. The view is the set of arena slots whose `active = true` store the thread
is guaranteed to observe. The view grows only along release/acquire edges:

- A thread's own `ArenaWrite` adds the slot.
- papaya `try_insert` stores the inserter's view with the entry in `tableView`. A papaya `get` that
  sees the entry adds that view to its own.
- Releasing `overflow` stores the releasing thread's view in `lockView`. The next thread to take the
  lock adds that view to its own.
- Reading an `active` slot adds the writer's view at the time of the write. This is the boxcar
  Acquire on `active`.

The model treats an `arena_value` read of a slot outside the reader's view as a fault. boxcar `get`
may return `None` for that slot, and the interner then panics.

### Abstractions and bounds

- Hashes are explicit, in `MCHash` in `MCParallelInterner.tla`. Every scope forces collisions:
  - `colliding-pairs` runs 3 threads × 2 operations over 2 values with one hash.
  - `colliding-triples` runs 4 threads × 1 operation over 3 values with one hash. With fewer than 3
    colliding values, `overflow` holds at most one entry per hash, and `scan_hash_only` passes.
  - `two-hash-classes` runs 3 threads × 1 operation over 4 values in two hash classes. One shared
    `overflow` then holds entries for different hashes.

  Each operation is `intern` or `get` of any value, and TLC checks every combination. A pass covers
  these scopes only.

- The model treats papaya as a linearizable map that never removes or overwrites an entry. Inserts
  release and reads acquire. It does not model seize reclamation.
- The model takes papaya's `ResizeMode::Blocking` as an axiom. Readers and writers act only on the
  root table, and promotion happens only after every entry is copied. A resize therefore neither
  loses a key nor lets two `try_insert` calls both succeed.
- `CollisionPush` merges boxcar's reserve and write into one step. No other thread can learn that
  index before the thread releases the `overflow` lock, so the merge hides no interleaving.
- The model does not cover weakened atomic orderings inside boxcar, papaya, or parking_lot. TLC
  explores interleavings, and the `view` sets express only the release/acquire edges listed above.
  Use Loom or Miri for those orderings.
- No property bounds wasted slots, which are the candidates that lose a race.
- Some changes cannot be expressed as mutants: an unlocked scan in `publish_collision`, because it
  is one action; an `overflow` entry that names an unwritten slot, because `CollisionPush` merges
  the reserve and the write; and a weakened boxcar `active` ordering. `return_before_write` tests
  the model's own publication rule, not a realistic code change.
- The model does not include metadata (`M`). The winning thread's metadata is kept, and
  `CoreInterners` derives metadata from the value.
- The model assumes that an `Id` passed between threads outside the interner crosses a synchronizing
  edge, such as a channel, a lock, or a thread join. `ReturnedIdsArePublished` makes this assumption
  sufficient.
- The model does not include `id_from_index` in `lib.rs`, which converts an arena index into an
  `Id`, because its arenas are small. The conversion panics with `interner capacity overflow` once
  an index does not fit in an `Id`, which happens after `u32::MAX` pushes, wasted slots included.
  The panic happens after `arena.push`, so the slot is wasted, and before the candidate reaches
  `table` or `overflow`. The `overflow` guard is released during unwinding, and parking_lot does not
  poison it, so the interner stays usable for values interned before the panic.

### Mutants

The `Mutant` constant in `ParallelInterner.tla` selects a mutant. `check.sh` lists the scope and the
property that must catch each one. A mutant that two properties must catch appears twice.

| Mutant                    | Code change it stands for                                                                          |
| ------------------------- | -------------------------------------------------------------------------------------------------- |
| `trust_hash`              | Return the table `Id` without `arena_value(id) == &value`.                                         |
| `skip_occupied_compare`   | Return `*current` in the `Err` arm without comparing values.                                       |
| `keep_candidate`          | Return `candidate` in the `Err` arm.                                                               |
| `skip_publish_collision`  | Return `candidate` in the `Err` arm when the values differ, without calling `publish_collision`.   |
| `overwrite_insert`        | Use `table.insert` instead of `try_insert`.                                                        |
| `publish_skip_scan`       | Drop `scan_overflow` from `publish_collision`.                                                     |
| `collision_skip_scan`     | Drop `scan_overflow` from `intern_at_collision`.                                                   |
| `collision_unlocked_scan` | Scan and push under two separate `overflow.lock()` guards.                                         |
| `get_skip_overflow`       | Return `None` from `get` without scanning `overflow`.                                              |
| `get_trust_hash`          | Return the table `Id` from `get` without `arena_value(id) == value`.                               |
| `scan_hash_only`          | Make `scan_overflow` match on `overflow_hash == hash` without `arena_value(overflow_id) == value`. |
| `relaxed_table`           | Use a table with no release/acquire edge, such as a map of Relaxed atomics.                        |
| `publish_before_write`    | Publish the index in the table before the slot write completes.                                    |
| `return_before_write`     | Return the candidate `Id` before the slot write completes.                                         |

## When the check fails

1. Open `target/tla/runs/interner/<run>.log` and read the trace from `State 1`. Each state names the
   action it took, such as `<TryInsert line ...>`, and shows `pc`, `table`, `overflow`, `arena`, and
   `results`. Use the coverage table to map each action to its Rust function. `t1`, `t2`, and so on
   are threads, and `program` lists each thread's operations.
2. Replay the steps against `parallel.rs`. If the Rust code can take each modelled step in that
   order, the failure is a real bug. Report it as a sequence of steps in the code. Do not weaken the
   property or the model to make it pass.
3. If the Rust code cannot take one of the steps, the model is wrong. Common causes are a non-atomic
   region modelled as one action, a missing release/acquire edge, or a mutant branch that leaks into
   `Mutant = "none"`. Fix the model, and keep every mutant caught.
4. A `MISSED` mutant after a model change means that the model no longer reaches that path. Fix the
   model, not the `mutants` list in `check.sh`.

## When the change is intended

1. Edit the matching action in `ParallelInterner.tla` so that it describes the new code. Keep one
   action per region that the code executes indivisibly, and keep the names of the Rust functions.
   For example, if `publish_collision` stops calling `scan_overflow`, set `reads == {}` and
   `found == NoId` in `PublishCollision`. All three baselines then fail `EqualValuesShareId`, which
   reports the bug.
2. If the change adds a guard or a new step, add a mutant that removes it. Add the mutant to
   `Mutants` in the model, to `mutants` in `check.sh` with its scope and the property that must
   catch it, and to the mutant table above.
3. Run `check.sh` until every baseline passes and every mutant is caught.
4. Update the coverage table, and move the sync commit to the commit that contains the code change.
