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

### B1: canonicalize build inputs in parallel (2026-11-17)

`iris_build::compile::build_initial` canonicalized every selected source and every package source
identity on one thread before planning, about 120 ms for the 7,374 `acme` sources in an orb, while
every other core idled. Both now use `par_iter`, whose indexed `collect` keeps the order the plan
reads.

Measured through `build_initial` on the `acme` `src` directories, with packages and dependencies
from each `purs.json`, against `1d5ee22ff` (whose `build_initial` binaries are byte-identical to
`5db492475`'s), 7 interleaved pairs at 8 threads (4 cores × 2 threads) and 2 at 1 thread:

| Threads | Plan ready, base → head | Build wall, median base → head | Median paired ratio |
| ---: | ---: | ---: | ---: |
| 8 | 173 → 87 ms | 2,197 → 2,069 ms | 0.942 |
| 1 | 184 → 174 ms | 12,419 → 12,320 ms | 0.992 |

`just perf 5db492475` does not reach `build_initial`: every gated metric is ±0.000% and the output
digest is unchanged. Its ungated `acme` build wall ratios were 1.023 at 1 thread and 1.078 at 8,
with base and candidate runs overlapping (8 threads: base 1,785–2,030 ms, candidate 1,826–2,189 ms)
on code the change does not touch; against `1d5ee22ff` the same ratios were 1.002 and 1.004.

## Measured and rejected

### B1: build sources in import order instead of on demand (2026-11-17)

At 8 threads, workers inside `build_initial` spend 6–8% of their time blocked in
`Future::wait` on a query another worker is computing, about 1,000 ms in 300 waits for `acme`.
`query_package` was changed to index each package's sources first and start a source only once the
sources it imports from the same package had finished, with sources in import cycles built
afterwards on demand. Blocked time fell to 35 ms in 20 waits, but build wall time did not change
(3 interleaved pairs: base 2,113–2,263 ms, head 2,171–2,238 ms) and utilisation stayed at 79%.
Applying the same schedule to all sources as one package, which also removes the package barriers
of `executor::execute_parallel`, did not change wall time either (base 2,018–2,240 ms, head
2,116–2,224 ms) and raised CPU time. The blocked workers had no other ready work: the limit is the
import graph's critical path, not the order in which ready sources start.

## Not tried

The work areas in the performance plan (K1 to K5, S1 to S3, L1, R1, F1, J1, E1, B1, I1) start here
until they are measured.
