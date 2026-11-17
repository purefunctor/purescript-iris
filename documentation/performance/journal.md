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

### L1: lowering allocations and lookups

2026-11-17, branch `lowering-allocations`, four commits in `compiler-frontend/lowering/src`,
measured together with `just perf 5db492475`:

| Stage | Instructions | Allocator instructions | Allocations | Allocated bytes |
| --- | ---: | ---: | ---: | ---: |
| lowered | 149,695,312 (-6.722%) | 9,198,601 (-25.270%) | 90,625 (-29.252%) | 31,370,887 (-4.640%) |
| build | 2,059,296,199 (-0.521%) | 226,347,343 (-0.898%) | 1,856,309 (-1.979%) | 443,346,711 (-0.343%) |
| edit_interface | 734,435,621 (-1.035%) | 58,956,558 (-3.810%) | 650,939 (-2.959%) | 88,876,137 (-0.897%) |

Stages that do not lower move by at most 0.001%; interned counts and the output digest are
identical. Wall time on `acme` did not separate from noise (`lowered` ratios 0.952 on one thread
and 0.987 on eight, with `build` at 1.015 and 0.982 in the same runs), and peak resident memory
did not change.

| Commit | `lowered` instructions | Allocator instructions | Allocations |
| --- | ---: | ---: | ---: |
| Borrow qualified names from source text | -2.341% | -0.011% | -0.013% |
| Memoise module-level name lookups | -4.739% | +2.125% | +1.693% (temporary memo tables) |
| Collect sequences into Arc slices with one allocation | +0.849% | -26.814% | -30.421% |
| Reuse lowered names in scope tables | -0.580% | +0.000% | +0.000% |

The Arc collection commit moves some cost from the allocator into lowering; counting the
allocator, lowering still falls from 161.9M to 159.8M instructions. Memory retained by a lowered
module is unchanged by all four commits (15.8 MB on `core`), so the 364 MB `lowered` adds on
`acme` is what each lowered module keeps, not transient allocation.

## Measured and rejected

### L1

- The lookup memo on `hashbrown::HashTable`, keyed by borrowed `(qualifier, name)`: `grouped`
  moved by +0.33% without running changed code, because the new callers changed how hashbrown
  code shared with `group_module` was inlined. The kept memo uses std's `FxHashMap` instead, and
  also gave 0.1% fewer `lowered` instructions and 1.6% fewer allocated bytes.

Measured with a lowering-only Callgrind probe that counts `lowering::lower_module` including the
allocator, so these numbers are not comparable with `just perf` tables:

- Presizing the let-binding name map and group vector in `lower_equation_chunk`: +0.38%
  instructions for 26 fewer allocations on `core`.
- Four inline slots instead of eight when collecting Arc slices: -0.13% instructions,
  +0.95% allocations, +0.37% allocated bytes.

## Not tried

The work areas in the performance plan (K1 to K5, S1 to S3, L1, R1, F1, J1, E1, B1, I1) start here
until they are measured.

### L1

Remaining lowering cost on `core` after L1, from the lowering-only probe profile:

- `StabilizedModule::lookup_cst`, called once per lowered node: about 7% of `lowered`. A dense
  table from preorder index to `AstId` in the stabilized module would avoid hashing each node
  pointer; S2's preorder storage makes it possible, and lowering needs no change if
  `lookup_cst` keeps its signature.
- Syntax tree accessors (`node_sibling`, `children`, `cast`, `token`): about 17%; area S2.
- Inserts and rehashes into `LoweredTree` and `LoweringGraphNodes` maps: about 15%; area S1.
- Sorting in `index_references`: about 2%.
