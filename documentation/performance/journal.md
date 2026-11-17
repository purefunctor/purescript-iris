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

### K5: skolem and exhaustiveness passes

2026-11-17, branch `k5-skolem-exhaustive`, five commits on `5db492475`. Only `checked` and the
edit stages run this code; every other stage moved by at most 0.008%, and interned counts and the
output digest are unchanged. Together, on `core`:

| Stage | Instructions | Allocator instructions | Allocations | Allocated bytes |
| --- | ---: | ---: | ---: | ---: |
| checked | -1.613% | -6.216% | -4.223% | -0.855% |
| build | -0.741% | -2.542% | -2.554% | -0.212% |
| edit_body | -1.495% | -5.149% | -4.885% | -0.875% |
| edit_interface | -1.294% | -5.282% | -4.396% | -0.692% |

Each commit, as its change in `checked` instructions and allocations from the commit before:

- Skip skolem type inspection while discovering scopes: -0.395%, -0.828%. Discovery records every
  scope it finds as already reported, so it could never report an escape.
- Memoise expected skolem mentions per audit: -0.607%, -2.648%. Judgments share most of their
  types, and each inspection walked them again with its own visited set.
- Skip exhaustiveness search after an irrefutable row: -0.209%, -0.566%. `algorithm_m` now runs
  for 1,510 of the 3,778 checks on `core`.
- Stream leading skolem scopes: -0.342%, -0.024%.
- Check sigma completeness against the constructor list: -0.070%, -0.206%.

After these, the skolem pass is about 1.4% of `checked` instructions (from 2.9%), and the
exhaustiveness checks about 1.7% (from 1.9%), measured by inclusive cost of their entry points.

## Measured and rejected

Nothing yet.

## Not tried

The work areas in the performance plan (K1 to K5, S1 to S3, L1, R1, F1, J1, E1, B1, I1) start here
until they are measured.

- A type flag for rigid variables with a skolem scope. The skolem audit could then skip types
  without one instead of consulting its memo, and the scan for expected scopes over every
  expression of a module (about 0.28% of `checked`) could stop early. It needs a new bit in
  `TypeFlags` and the interner's flag computation.
- Reusing wildcard patterns per type in `CheckState::allocate_wildcard`. Each call interns a
  pattern, at about 300 instructions per call and 0.23% of `checked` on `core`.
- Deduplicating constructors in `exhaustive::collect_sigma` by a linear scan instead of a hash
  set. Not measured: it is quadratic in the number of distinct constructors, and `algorithm_u`
  already collects the sigma once per row, so large literal case tables would get much slower.
