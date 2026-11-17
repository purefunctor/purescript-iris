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
| parsed | 221,380,316 | 31,837,808 | 11,014 | 80,669,001 |
| stabilized | 63,791,000 | 11,076,117 | 7,191 | 39,923,200 |
| indexed | 18,779,384 | 1,910,320 | 15,539 | 3,684,695 |
| resolved | 18,663,227 | 2,147,686 | 12,908 | 6,865,792 |
| lowered | 160,337,319 | 12,441,163 | 128,096 | 32,897,163 |
| grouped | 5,122,440 | 1,443,396 | 16,081 | 1,333,904 |
| bracketed | 2,904,939 | 719,864 | 5,620 | 585,316 |
| sectioned | 3,215,698 | 192,642 | 1,632 | 147,484 |
| checked | 952,317,281 | 90,724,839 | 1,145,505 | 110,385,845 |
| functional | 335,950,956 | 40,389,870 | 364,420 | 51,535,780 |
| javascript | 228,105,775 | 33,057,725 | 170,769 | 97,849,509 |
| source_map | 45,749,950 | 4,849,865 | 8,399 | 17,774,130 |
| documented | 10,414,366 | 890,574 | 6,606 | 1,221,168 |
| build | 2,066,732,651 | 231,681,869 | 1,893,780 | 444,872,987 |
| edit_comment | 213,137,671 | 22,029,221 | 204,181 | 28,268,955 |
| edit_body | 641,642,671 | 56,106,591 | 603,553 | 70,856,317 |
| edit_interface | 740,978,022 | 62,395,738 | 670,789 | 89,680,613 |

Instructions exclude the allocator; allocator instructions are everything reached through
mimalloc's `mi_malloc`, `mi_zalloc`, `mi_realloc`, and `mi_free` entry points.

After the build the type interner holds 278,862 types, 158,954 of them with unification variables.

Instruction counts depend on the Rust toolchain and the machine's instruction set, so compare only
numbers measured in the same run of `just perf`, never against this table.

## In place

Nothing yet.

## Measured and rejected

Nothing yet.

## Not tried

The work areas in the performance plan (K1 to K5, S1 to S3, L1, R1, F1, J1, E1, B1, I1) start here
until they are measured.
