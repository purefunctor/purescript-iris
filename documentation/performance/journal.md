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

### F1: classify known terms once per application head

2026-11-17, branch `f1-known-terms`, commit "Classify known terms once per application head",
measured with `just perf 5db492475`. The functional backend tested each application head against
dozens of `(module name, item name)` pairs, each test looking up the module name again. Each head
is now classified once into a `KnownTerm` in `functional/src/convert/known.rs`.

| Stage | Instructions | Allocations | Allocated bytes |
| --- | ---: | ---: | ---: |
| functional | 337,198,820 → 185,378,031 (−45.0%) | +0.000% | +0.000% |
| build | 2,070,079,664 → 1,918,257,928 (−7.3%) | +0.000% | +0.000% |
| edit_comment | 213,809,651 → 121,490,178 (−43.2%) | +0.000% | +0.000% |
| edit_body | 642,719,323 → 549,088,169 (−14.6%) | +0.000% | +0.000% |
| edit_interface | 742,119,949 → 648,484,347 (−12.6%) | +0.000% | +0.000% |

Interned counts and generated output are unchanged.

### I1: read dependency module names from indexed modules

2026-11-17, branch `f1-known-terms`, commit "Read dependency module names from indexed modules",
measured with `just perf` against the F1 commit. `functional` read a dependency's `content` and
`parsed` tree to get its module name, so every edit re-ran `functional` in every importer. It now
reads `IndexedModule::name`.

| Stage | Instructions | Allocations | Allocated bytes |
| --- | ---: | ---: | ---: |
| functional | 185,378,031 → 181,874,304 (−1.9%) | −0.107% | −0.680% |
| build | 1,918,258,361 → 1,914,753,739 (−0.2%) | −0.021% | −0.079% |
| edit_comment | 121,490,178 → 13,802,136 (−88.6%) | −96.575% | −94.772% |

Interned counts and generated output are unchanged.

## Measured and rejected

### F1: cache known-term classification per module

2026-11-17, on top of the F1 commit, measured with `just perf` against it. A
`RefCell<FxHashMap<(FileId, TermItemId), Option<KnownTerm>>>` in the conversion context memoised
each term's classification.

| Stage | Instructions | Allocations | Allocated bytes |
| --- | ---: | ---: | ---: |
| functional | 185,378,031 → 183,823,538 (−0.8%) | +0.189% | +0.513% |
| edit_comment | 121,490,182 → 120,454,193 (−0.9%) | +0.142% | +0.569% |

Rejected: allocated bytes regress beyond the 0.5% gate for under 1% of the stage's instructions.
Classifying a head is one module-name lookup and a string match, cheap enough without a table.

## Not tried

The work areas in the performance plan (K1 to K5, S1 to S3, L1, R1, J1, E1, B1, I1) start here
until they are measured.
