---
name: verifying-parallel-interner
description:
  "Model-checks the parallel interner (compiler-core/interner/src/parallel.rs) with
  TLA+/TLC. Use when changing intern_with_metadata, get, probe, Shard::table, Shard::grow,
  Location::of, arena_value, the Interner or Shard fields, or the boxcar or parking_lot
  dependencies of the interner crate, or when a type checker interner shows duplicate Ids, missed
  lookups, or an 'is not a valid index' panic."
---

# Verifying the parallel interner

`compiler-core/interner/src/parallel.rs` backs every `CoreInterners` table in
`compiler-frontend/checking/src/interners.rs`. Values are split across shards by hash. Each shard
holds an open-addressing table of `(tag, Id)` words that threads probe without a lock; inserts and
resizes take the shard's lock. A race can give one type two `TypeId`s, or give two types one
`TypeId`. The type checker compares
types by `Id`, so either race gives wrong answers without an error. Unit tests rarely reach these
interleavings. The TLA+ model in `compiler-core/interner/verification/` checks every interleaving of
a bounded set of threads.

## Properties

`ParallelInterner.tla` defines each property as a TLA+ invariant. TLC also reports a deadlock if an
operation can get stuck, for example on a shard lock.

| Property                     | Statement                                                                                                                                   | Harm when broken                                                                                                              |
| ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| `EqualValuesShareId`         | Every `Id` returned for equal values is the same `Id`.                                                                                      | Equal types compare unequal.                                                                                                  |
| `DistinctValuesNeverShareId` | Distinct values never receive the same `Id`.                                                                                                | Different types compare equal.                                                                                                |
| `IdNamesItsValue`            | The arena slot of every returned `Id` holds the value it was returned for.                                                                  | `interner[id]` reads the wrong type.                                                                                          |
| `GetFindsReturnedInterns`    | `get(v)` returns the `Id` of `v` whenever an `intern(v)` returned before the `get` started.                                                 | Lookups miss interned types.                                                                                                  |
| `ReturnedIdsArePublished`    | Every returned `Id` names an arena slot whose write the returning thread has released.                                                      | A thread that receives the `Id` can read an unwritten slot.                                                                   |
| `ArenaReadsArePublished`     | Every `arena_value` read inside the interner happens after an acquire that synchronizes with the slot's write.                              | `arena_value` hits `unreachable!("invariant violated: ...")`.                                                                 |
| `TablesAreCoherent`          | Every table entry sits in the shard and carries the tag of the value it names. No table of a shard holds two entries for equal values.      | `EqualValuesShareId` depends on this lemma. A failure places the bug in the locked insert path.                               |
| `CurrentTableHoldsInterns`   | Every returned `Id` has an entry in the current table of its shard.                                                                         | A value inserted into a retired table, or lost by a resize, is interned again under a new `Id`. A failure places the bug in growth. |
| `ViewsHoldOnlyWrittenSlots`  | Checks the model's memory abstraction.                                                                                                      | A failure is a modelling error, not a code bug.                                                                               |

## Check a change

From the repository root, run:

```bash
compiler-core/interner/verification/check.sh
```

The script uses `.agents/scripts/tlc.sh` to run Temurin 25 from the `verification` Nix shell and download
SHA-256-checked `tla2tools.jar` 1.7.4 to `target/tla/`. Each check runs on a copy of the models in a
temporary directory. The run takes about 2 minutes. A good run prints three `pass` lines and one
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
git diff 08b67bb5670d2f060815cb181924ec3d09548fd9 -- \
  compiler-core/interner/src/parallel.rs compiler-core/interner/Cargo.toml
git diff 08b67bb5670d2f060815cb181924ec3d09548fd9 -- compiler-core/interner/src/lib.rs | rg -A6 'fn id_from_index'
git diff 08b67bb5670d2f060815cb181924ec3d09548fd9 -- Cargo.lock | rg -A3 'name = "(boxcar|parking_lot)"'
```

If the diff touches a function in the coverage table, the model may no longer describe the code.
Update the model first, then run the check.

## Coverage record

Sync commit: `08b67bb5670d2f060815cb181924ec3d09548fd9`. At this commit, the model describes the
code below. Move the sync commit forward only after you confirm that the model still describes the
code at the new commit.

| Code in `parallel.rs`                                                                    | Model action or definition           | Atomic unit in the model                                                                                                   |
| ---------------------------------------------------------------------------------------- | ------------------------------------ | -------------------------------------------------------------------------------------------------------------------------- |
| `Shard::table` before the first `probe` in `intern_with_metadata` and `get`              | `LoadTable`                          | One Acquire load of `generation`. The `OnceLock` read of `tables[generation]` follows it and adds no edge the model needs. |
| The first `probe` in `intern_with_metadata` and `get`, with `arena_value(id) == value`   | `Probe`, `ProbeSlots`, `ProbeMatch`  | The probe of the observed table, without the lock. Each entry with the same tag is an Acquire load and an `arena_value` read. |
| `shard.occupied.0.lock()`                                                                | `Lock`                               | One parking_lot lock acquisition.                                                                                          |
| `Shard::table` and the second `probe`, under the lock                                    | `LockedProbe`                        | The reload of `generation` and the probe of the current table.                                                             |
| `self.arena.push((value, metadata))`                                                     | `ArenaPush`                          | boxcar reserve and write. No other thread can learn the index before `StoreSlot`, so the merge hides no interleaving.      |
| `table[index].store(encode_slot(tag, id), Ordering::Release)`                            | `StoreSlot`                          | One Release store into a vacant entry. Entries never change once set.                                                      |
| `Shard::grow`                                                                            | `Grow`                               | Relaxed copy into a new table, then a Release store of `generation`. Readers cannot reach the new table before that store. |
| Guard drop and return                                                                    | `InsertUnlock`                       | One unlock.                                                                                                                |
| `id_from_index` in `lib.rs`                                                              | Not modelled; `Len(arena) + 1`       | See the abstractions below.                                                                                                |
| `arena_value` and `Index`                                                                | The `ids \subseteq` checks and `Unreachable` | boxcar `get` returns `None` unless its Acquire load of `active` sees the Release store from `push`.               |

### Memory model

Each thread has a `view`. The view is the set of arena slots whose `active = true` store the thread
is guaranteed to observe. The view grows only along release/acquire edges:

- A thread's own arena write adds the slot.
- `StoreSlot` stores the inserter's view with the entry. A probe that loads the entry adds that view
  to its own.
- `Grow` stores the grower's view with `generation`. `LoadTable` and `LockedProbe` add it. Copied
  entries are stored Relaxed and carry no view of their own.
- Releasing a shard lock stores the releasing thread's view in `lockView`. The next thread to take
  that lock adds it.
- Reading an `active` slot adds the writer's view at the time of the write. This is the boxcar
  Acquire on `active`.

The model treats an `arena_value` read of a slot outside the reader's view as a fault. boxcar `get`
may return `None` for that slot, and the interner then panics.

### Abstractions and bounds

- Shards and tags are explicit, in `MCLocation` in `MCParallelInterner.tla`. Every scope forces two
  values onto one shard and one tag, so probes must compare arena values:
  - `colliding-pairs` runs 3 threads × 2 operations over 2 values with one shard and one tag.
  - `colliding-triples` runs 4 threads × 1 operation over 3 values with one shard and one tag.
  - `two-shards` runs 3 threads × 1 operation over 4 values. `u` shares the shard of `x` and `y`
    under another tag, and `v` has a shard of its own.

  Each operation is `intern` or `get` of any value, and TLC checks every combination. A pass covers
  these scopes only.

- A table is a set of `(tag, id)` entries, and a probe reads every entry with its tag at once. Linear
  probing stops at the first vacant entry, and an insert fills the first vacant entry of its probe
  sequence, so a probe that reads entries one at a time cannot miss an entry that this set read
  would find. The model does not check that argument.
- `Capacity(generation)` is `generation - 1`, so the first insert into a shard resizes it and the
  scopes reach probes of retired tables. The code resizes when a table is half full.
- The model does not cover weakened atomic orderings inside boxcar or parking_lot. TLC explores
  interleavings, and the `view` sets express only the release/acquire edges listed above. Use Loom
  or Miri for those orderings.
- No property bounds memory. Retired tables stay allocated until the interner is dropped, because a
  thread may still be probing them. Their total size is below the size of the current table.
- The model does not include metadata (`M`). The inserting thread's metadata is kept, and
  `CoreInterners` derives metadata from the value.
- The model assumes that an `Id` passed between threads outside the interner crosses a synchronizing
  edge, such as a channel, a lock, or a thread join. `ReturnedIdsArePublished` makes this assumption
  sufficient.
- The model does not include `id_from_index` in `lib.rs`, which converts an arena index into an
  `Id`, because its arenas are small. The conversion panics with `interner capacity overflow` once
  an index does not fit in an `Id`, which happens after `u32::MAX` pushes. The panic happens after
  `arena.push`, so the slot is wasted, and before the entry is stored. The shard guard is released
  during unwinding, and parking_lot does not poison it, so the interner stays usable for values
  interned before the panic.

### Mutants

The `Mutant` constant in `ParallelInterner.tla` selects a mutant. `check.sh` lists the scope and the
property that must catch each one. A mutant that two properties must catch appears twice.

| Mutant                 | Code change it stands for                                                                     |
| ---------------------- | --------------------------------------------------------------------------------------------- |
| `trust_tag`            | Return the `Id` of an entry with the same tag in `intern_with_metadata` without `arena_value`. |
| `get_trust_tag`        | The same in `get`.                                                                            |
| `skip_locked_probe`    | Insert after taking the lock without probing again.                                           |
| `unlocked_insert`      | Insert after a lock-free miss without taking the shard lock.                                  |
| `reuse_observed_table` | Under the lock, probe and insert into the table observed before the lock instead of reloading. |
| `empty_grow`           | Publish a new table without copying the entries of the old one.                               |
| `relaxed_slot`         | Store entries with `Ordering::Relaxed`.                                                       |
| `relaxed_generation`   | Store `generation` with `Ordering::Relaxed` in `grow`.                                        |
| `publish_before_write` | Store the entry before the arena write completes.                                             |
| `return_before_write`  | Return the `Id` before the arena write completes.                                             |

## When the check fails

1. Open `target/tla/runs/interner/<run>.log` and read the trace from `State 1`. Each state names the
   action it took, such as `<StoreSlot line ...>`, and shows `pc`, `tables`, `generation`, `arena`, and
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
   For example, if `intern_with_metadata` stops probing again under the lock, set `slots == {}` in
   `LockedProbe`. The baselines then fail `EqualValuesShareId`, which reports the bug.
2. If the change adds a guard or a new step, add a mutant that removes it. Add the mutant to
   `Mutants` in the model, to `mutants` in `check.sh` with its scope and the property that must
   catch it, and to the mutant table above.
3. Run `check.sh` until every baseline passes and every mutant is caught.
4. Update the coverage table, and move the sync commit to the commit that contains the code change.
