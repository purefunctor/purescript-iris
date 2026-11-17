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

### K1 step 1: sharded type interner (2026-11-17, branch `interner-step1-new`)

`compiler-core/interner/src/parallel.rs` replaces papaya with 1024 shards of open-addressing
tables of `(tag, Id)` words: probes need no lock, inserts and resizes take the shard lock. The type
table is presized to 2^18 types. `just perf 5db492475`, `core`, Callgrind:

| Stage | Instructions | Allocator instructions | Allocations | Allocated bytes |
| --- | ---: | ---: | ---: | ---: |
| checked | 899,721,316 (-5.612%) | 76,850,365 (-14.388%) | 849,506 (-25.840%) | 103,063,629 (-6.633%) |
| build | 2,016,584,986 (-2.584%) | 216,374,412 (-5.312%) | 1,597,781 (-15.630%) | 437,550,771 (-1.646%) |
| edit_body | 637,431,744 (-0.823%) | 54,798,173 (-0.736%) | 603,425 (-0.021%) | 70,854,765 (-0.002%) |
| edit_interface | 736,828,907 (-0.712%) | 60,594,114 (-0.432%) | 670,789 (+0.000%) | 89,680,613 (+0.000%) |

Interned counts and the output digest are unchanged. Presizing alone is -1.307% of `checked`.

Wall time on `acme`, medians of 3 interleaved pairs, candidate over base:

| Threads | checked | build | Peak resident memory |
| ---: | ---: | ---: | ---: |
| 1 | 5,728 → 4,427 ms (0.776) | 11,603 → 9,873 ms (0.851) | 2,942 → 2,982 MB |
| 2 | 2,911 → 2,630 ms (0.863) | 6,088 → 5,672 ms (0.927) | 2,984 → 3,035 MB |
| 4 | 1,584 → 1,446 ms (0.913) | 3,299 → 3,128 ms (0.980) | 3,042 → 3,085 MB |
| 8 | 926 → 856 ms (0.940) | 1,859 → 1,761 ms (0.961) | 3,133 → 3,185 MB |

Dropping a `QueryEngine` after checking `acme` at 8 threads falls from about 370 ms to 49 ms,
because the tables no longer own one allocation per entry. Peak memory rises by about 50 MB: retired
tables stay allocated until the interner is dropped.

## Measured and rejected

- K1, 64 shards, each a `Mutex<hashbrown::HashTable>`: `checked` on `core` -8.7% instructions, but
  checking `acme` at 8 threads took about 1,170 ms against papaya's 1,000 ms.
- K1, 64 shards with lock-free probes: -5.7% instructions, still about 1,240 ms against 1,000 ms at 8
  threads. 65,794 of 3,391,966 locked inserts (1.9%) found the lock held, and parked threads do not
  show in sampling profiles. Moving the lock to its own cache line did not help. 1024 shards bring
  contention to about 3,600 inserts.

## Not tried

The work areas in the performance plan (K1 to K5, S1 to S3, L1, R1, F1, J1, E1, B1, I1) start here
until they are measured.
