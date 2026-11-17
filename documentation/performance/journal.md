# Performance journal

Every performance experiment is recorded here, kept or not, with the numbers from `just perf`
(see the `measuring-performance` skill). Search this file before starting an experiment.

Each entry names the date, the commit or branch, the stages affected, and the change in
instructions, allocations, and interned counts on `core`, plus wall time on `acme` when the claim
depends on it.

## Baseline

Package set 82.1.1, `core` preset (295 files plus the `Prim` modules), Callgrind on one thread,
release profile with one codegen unit per crate, measured on 2026-11-17 at `fcc43af16` with the
harness applied:

| Stage | Instructions | Allocator instructions | Allocations | Allocated bytes |
| --- | ---: | ---: | ---: | ---: |
| parsed | 221,624,361 | 31,594,166 | 11,014 | 80,669,001 |
| stabilized | 63,884,120 | 10,982,997 | 7,191 | 39,923,200 |
| indexed | 18,850,428 | 1,841,462 | 15,539 | 3,684,695 |
| resolved | 18,664,683 | 2,146,426 | 12,908 | 6,865,792 |
| lowered | 160,477,926 | 12,309,041 | 128,096 | 32,897,163 |
| grouped | 5,154,766 | 1,411,070 | 16,081 | 1,333,904 |
| bracketed | 2,904,939 | 719,864 | 5,620 | 585,316 |
| sectioned | 3,215,698 | 192,642 | 1,632 | 147,484 |
| checked | 953,212,878 | 89,828,423 | 1,145,505 | 110,385,845 |
| functional | 337,197,555 | 39,192,301 | 364,420 | 51,535,780 |
| javascript | 228,601,229 | 32,562,232 | 170,769 | 97,849,509 |
| source_map | 45,813,646 | 4,786,169 | 8,399 | 17,774,130 |
| documented | 10,472,515 | 832,425 | 6,606 | 1,221,168 |
| build | 2,070,074,744 | 228,399,218 | 1,893,780 | 444,872,987 |
| edit_comment | 213,808,058 | 21,390,965 | 204,181 | 28,268,955 |
| edit_body | 642,717,690 | 55,063,579 | 603,553 | 70,856,317 |
| edit_interface | 742,111,293 | 61,291,999 | 670,789 | 89,680,613 |

Instructions exclude the allocator; allocator instructions are everything reached through
mimalloc's `mi_malloc`, `mi_zalloc`, `mi_realloc`, and `mi_free` entry points.

After the build the type interner holds 278,862 types, 158,954 of them with unification variables.

Instruction counts depend on the Rust toolchain and the machine's instruction set, so compare only
numbers measured in the same run of `just perf`, never against this table.

## In place

### R1

Stabilizing and resolving memory, branch `r1-memory`, 2026-11-17, `just perf` against each commit's
parent on harness `5db492475`. Live heap is what each stage retains on `acme` (7,374 files, one
thread), measured with a counting allocator.

| Commit | Stage | Instructions | Allocated bytes | Live heap on `acme` |
| --- | --- | ---: | ---: | ---: |
| Size stabilized tables for the nodes they keep | stabilized | -1.01% | -29.18% | 192.0 → 90.5 MB |
| Allocate import lists with room for one import | resolved | -0.53% | -9.65% | 155.7 → 139.1 MB |
| Share export tables between resolved imports | resolved | -35.56% | -51.17% | 139.1 → 49.4 MB |
| Share class member lists between resolved modules | resolved | -28.63% | -22.05% | 49.4 → 42.8 MB |

- The stabilized tables were sized for every node, but Annotation, QualifiedName, and LabelName
  nodes, more than half of the tree, never get an `AstId`.
- Most of the resolved memory was copies: every import copied the imported module's whole export
  table, every visible class copied its members, and each import list reserved room for four
  imports.
- Sharing export tables costs `lowered` +0.52% and `functional` +0.25% in instructions: names
  looked up through an explicit import probe the shared table and then the import's listed set.
  Build instructions fall 0.24%, `edit_body` 0.84%, and `edit_interface` 0.74%.
- `indexed` was not changed; its 72 MB on `acme` is the items, names, and import lists each module
  owns, with no copies.
- The whole branch against `5db492475`: build instructions -0.45% and allocated bytes -3.63% on
  `core`; peak resident memory on `acme` 2,941.5 → 2,684.2 MB on one thread and 3,147.6 →
  2,898.6 MB on eight; `resolved` wall time on one thread 153.8 → 79.2 ms.

## Measured and rejected

### R1

- Listed import items keyed by name (`FxHashMap<SmolStr, (FileId, ItemId)>`) instead of identity:
  `lowered` -0.44% but `functional` +0.61%, which iterates every export of every import, and
  5.8 MB more on `acme`. Measured on a pre-release harness copy, before allocator instructions
  were split out of the gate.
- `#[inline]` hints on the import lookups: no effect under the default release profile; only
  `#[inline(always)]` inlines them into the per-import loop. A single probe site per lookup,
  with listed entries stored in the export table's value type, was not inlined either.

## Not tried

The work areas in the performance plan (K1 to K5, S1 to S3, L1, R1, F1, J1, E1, B1, I1) start here
until they are measured.
