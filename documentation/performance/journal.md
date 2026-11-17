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

### J1

2026-11-17, branch `j1-javascript-rendering`, five commits against `5db492475`. Stages
`javascript` and `source_map`; interned counts unchanged; output digest unchanged.

| Stage | Instructions | Allocator instructions | Allocations | Allocated bytes |
| --- | ---: | ---: | ---: | ---: |
| javascript | 200,574,794 (-12.260%) | 24,710,284 (-25.952%) | 166,269 (-2.635%) | 81,540,845 (-16.667%) |
| source_map | 42,494,939 (-7.244%) | 847,214 (-82.341%) | 5,685 (-32.313%) | 11,583,958 (-34.827%) |
| build | 2,039,063,630 (-1.498%) | 216,520,051 (-5.505%) | 1,886,566 (-0.381%) | 422,374,151 (-5.057%) |

- Program text gives every generated node a two-byte position on one line instead of its own
  line, so Oxc's source-map builder no longer builds and searches a line table with one entry
  per node. This is most of the `javascript` change.
- Origin mappings and source mappings are reserved up front, program text names are stored
  apart from positions, and the origin position table is sized by the mapping count.
- What remains in `javascript` is mostly Oxc's code generation and its source-map builder
  (scanning output between mappings and hashing token names); in `source_map` it is syntax tree
  access in `first_token_start` and the origin position table.

## Measured and rejected

### J1

- 2026-11-17: borrowing mapping names instead of cloning them, in `finish_with_mappings` and
  in `source_map`. Instructions rose 0.14% in `javascript` and 1.1% in `source_map`, measured
  with an earlier copy of the harness.
- 2026-11-17: boxing the generated module's files instead of `Arc`. Gated `javascript`
  instructions fell 1.9%, but the shrinking reallocation moved the copy into the allocator
  (+11% allocator instructions); the stage total fell 0.5%. Measured with an earlier copy of the
  harness.
- 2026-11-17: reusing the previous origin's position in `source_map` before the hash
  lookup. About 1% of the stage, measured with a probe before the harness existed.

## Not tried

The work areas in the performance plan (K1 to K5, S1 to S3, L1, R1, F1, J1, E1, B1, I1) start here
until they are measured.

### J1

- Fewer `format!` calls in rendering. They are about 2% of `javascript` on `core`.
