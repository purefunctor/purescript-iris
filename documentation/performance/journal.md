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

### S1: dense `AstId` maps for lowered expressions and types

2026-11-17, branch `s1-dense-ast-id-tables`. `stabilizing::AstIdMap` stores each value in a vector
and finds it through one slot per stabilized node, presized from the `StabilizedModule`, so inserts
never rehash. `LoweredTree` expressions and types and `LoweringGraphNodes` expression and type nodes
use it. Lowering, local references, and lowering snapshots now list entries in insertion order
instead of hash order. Changes are against `5db492475`:

| Stage | Instructions | Allocations | Allocated bytes |
| --- | ---: | ---: | ---: |
| lowered | -9.447% | +0.771% | -1.063% |
| bracketed | -25.303% | +0.000% | +0.000% |
| sectioned | -27.631% | +0.000% | +0.000% |
| checked | -0.232% | +0.000% | +0.000% |
| build | -0.918% | +0.052% | -0.079% |
| edit_comment | -0.035% | +0.002% | +0.033% |
| edit_body | -0.190% | +0.001% | +0.013% |
| edit_interface | -1.563% | +0.032% | -0.481% |

Allocations rise by one slot vector per map per module. Allocator instructions in `lowered` rise
from 12,308,603 to 17,377,806: the presized slot vectors are zeroed, and the value vectors grow by
`realloc`, which copies where a hash table rehashed entry by entry outside the allocator. Interned
counts and generated output are unchanged.

## Measured and rejected

Measured on 2026-11-17 while building `AstIdMap`, with instructions that still included the
allocator; changes are against `fcc43af16` on `core`.

- Slots without stored keys, iterated in key order by scanning every slot: `bracketed` +100.9% and
  `sectioned` +41.3% instructions, because iteration visits every node of the module.
- Slot vectors grown on demand instead of presized: same instructions as presized, but allocated
  bytes +20.0% in `lowered` and +4.9% in `checked`.
- Binder maps (`LoweredTree::binders`, `LoweringGraphNodes::binder_node`,
  `CheckedNodeTypes::binders`): binders are 7% of node IDs, so the slot vector costs 4 bytes per
  node for little gain. With slot vectors grown on demand, their inserts went from 2,097,671 to
  2,076,551 instructions in lowering and from 1,806,059 to 1,665,241 in checking. They stay
  `FxHashMap`, as do the sparser `AstId` maps.

## Not tried

The work areas in the performance plan (K1 to K5, S1 to S3, L1, R1, F1, J1, E1, B1, I1) start here
until they are measured.
