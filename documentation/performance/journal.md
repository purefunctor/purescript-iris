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

### K2: constraint solving and instances, 2026-11-17, branch `k2-constraint-solving`

Each commit measured against its parent, `checked` on `core`, with the harness at `5db492475`.
No commit changed the interned counts beyond the first (278,862 to 278,846 types), and the
generated JavaScript and source maps are identical throughout. The whole branch takes `checked`
from 953,213,446 to 903,906,302 instructions (−5.17%) and 1,145,505 to 964,511 allocations
(−15.80%).

| Commit | Instructions | Allocations |
| --- | ---: | ---: |
| Decompose instance heads once at declaration | 953,213,446 → 925,802,724 (−2.88%) | 1,145,505 → 1,040,760 (−9.14%) |
| Store functional dependency positions in bit sets | 925,802,724 → 921,976,840 (−0.41%) | 1,040,760 → 983,578 (−5.49%) |
| Remove the write-only canonical constraint cache | 921,976,840 → 918,903,171 (−0.33%) | 983,578 → 982,314 (−0.13%) |
| Cache the type of each canonical constraint | 918,903,171 → 917,020,507 (−0.20%) | 982,314 → 983,386 (+0.11%) |
| Hoist functional dependency lookups out of candidate loops | 917,020,507 → 913,246,425 (−0.41%) | 983,386 → 973,284 (−1.03%) |
| Check constraint arguments for unknown types directly | 913,246,425 → 905,453,913 (−0.85%) | 973,284 → 972,801 (−0.05%) |
| Read superclasses without cloning the class | 905,453,913 → 903,906,302 (−0.17%) | 972,801 → 964,511 (−0.85%) |

The type cache was measured again after the unknown-type check removed its most frequent caller:
reverting it then costs +0.31% in `checked` (905,453,913 → 908,270,942), so it stays.

`derive::tools::solve_and_report_constraints` is the solver entry for the members of every
instance declaration, not only derived ones: 1,461 of its calls on `core` come from
`check_instance_member_groups` (8.9% of `checked` at the baseline) and 850 from derived members.

## Measured and rejected

- K2, 2026-11-17: caching superclass elaboration per set of given constraints. On `core`,
  `superclass_edges` runs 4,409 times; 1,640 calls have no roots, and the other 2,769 cover 2,145
  distinct (module, roots) pairs, so at most 624 calls (23%) would hit. Not built.
- K2, 2026-11-17: in `match_instance`, a closure-free `covers` for classes without functional
  dependencies and presized match vectors and substitution maps. Measured with a checked-only
  Callgrind probe that includes the allocator: +2.4M instructions with the presized vector, which
  allocates for candidates rejected at their first argument where the old code never allocated,
  and +1.9M without it.

## Not tried

- K2: an index of instances by class in `CheckedModule`. `collect_instance_chains` still costs
  about 31M instructions on `core` (17.7M from the solver, 13.6M from overlap validation): it
  rescans every instance of each touched module per (module, class) and sorts the candidates. The
  index needs `lib.rs` and the two instance declaration sites, and the module being checked still
  needs a scan until its instances are all declared.
- K2: reusing elaborated givens across nested implications that add no givens. Elaboration is about
  30M on `core`, but sharing it would share superclass evidence projections between scopes and
  change the evidence the backend sees.

The work areas in the performance plan (K1 to K5, S1 to S3, L1, R1, F1, J1, E1, B1, I1) start here
until they are measured.
