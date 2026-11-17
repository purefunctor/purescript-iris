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

### E1: fat LTO with one codegen unit for shipped binaries

2026-11-17, branch `e1-release-profile`, harness `5db492475`. The `dist` profile inherits
`release` and sets `lto = "fat"` and `codegen-units = 1`; the release and canary workflows build
`iris` with it. The `release` profile is unchanged, so `just perf` does not see this change: both
sides were measured from one checkout, built with `--release` and with `--profile dist`.

| Stage | `release` | `dist` | Change |
| --- | ---: | ---: | ---: |
| checked | 1,033,371,438 | 960,210,006 | -7.08% |
| build | 2,200,956,841 | 2,035,143,506 | -7.53% |
| edit_body | 687,728,679 | 644,181,260 | -6.33% |
| edit_interface | 789,562,325 | 735,226,744 | -6.88% |

Every stage improves, from -1.4% (parsed) to -29.8% (stabilized). Allocator instructions are
unchanged, since LTO does not reach mimalloc. Generated output is identical, interned counts are
unchanged, and LTO removes 114 of 1,893,894 allocations. Wall time for `build` on `acme` is 0.889
of `release` on one thread and 0.945 on all threads. Peak memory is unchanged.

Against `release` with one codegen unit per crate, which `just perf` builds, `dist` lowers `build`
by 3.95% and `checked` by 3.46%, so one codegen unit and LTO each account for about half the gain.

The shipped `iris` binary building `core` on one thread runs 2,391,980,640 instructions instead of
2,574,164,577 (-7.1%). On `acme` its wall time is 0.907 of `release` on one thread and 0.931 on
eight. The binary is 18.3 MB instead of 25.6 MB. A cold build of `iris` takes 2m47s instead of
1m05s on 8 vCPUs.

Profile-guided optimisation has been measured on Linux x64 but is not in the release workflows.
On top of `dist`, with a profile from running `measure --timing` on `acme`, it lowers `build` on
`core` by 16.79% (2,035,158,378 to 1,693,558,524), `checked` by 11.71%, and `edit_body` by
18.70%, with identical output. It also removes 15,005 allocations in `javascript`. Wall time for
`build` on `acme` is 0.874 on one thread and 0.935 on all threads. For the shipped `iris` binary on
`core`, the instruction count drops from 2,391,980,640 to 2,015,991,661 (-15.7%) with an `acme`
profile, and to 2,009,174,847 with a `core` profile, so training on the measured corpus does not
inflate the result. Each release target needs its own instrumented build and training run.

### K3: capability bundle validation

2026-11-17, branch `k3-bundle-validation-fast-path`, one commit on `5db492475` that changes only
`checking/src/core/constraint/bundles.rs`. Only `checked` and the edit stages run this code; every
other stage moved by at most 0.007%, and the output digest is unchanged. On `core`:

| Stage | Instructions | Allocator instructions | Allocations | Allocated bytes |
| --- | ---: | ---: | ---: | ---: |
| checked | -4.181% | -5.829% | -6.551% | -3.890% |
| build | -1.924% | -2.342% | -3.963% | -0.965% |
| edit_body | -1.354% | -2.589% | -2.704% | -1.199% |
| edit_interface | -1.172% | -2.633% | -2.433% | -0.947% |

Interned types fell by 1.992% (types with unification variables by 3.494%), because validation no
longer interns synonym expansions.

- Skip capability bundle validation for types without bundles. `bundles::validate` expanded every
  synonym on every node of each of the 6,743 top-level source types, although `core` uses no
  `Iris.Effect` bundles. It now first walks the type for the `Bundle` constructor, following solved
  unification variables and visiting each synonym's expansion once, and returns the type unchanged
  when the constructor is absent. Its inclusive cost went from 6.8% to 2.0% of `checked`
  (call counts and inclusive costs here measured at `fcc43af16` with a Callgrind probe before the
  harness existed).

## Measured and rejected

Nothing yet.

## Not tried

The work areas in the performance plan (K1 to K5, S1 to S3, L1, R1, F1, J1, B1, I1) start here
until they are measured.

### K3: capability bundle validation

From K3:

- Memoising per synonym whether its expansion reaches `Bundle`. The bundle walk visits 728
  synonym expansions on `core`, about 1% of `checked`, because each top-level type walks them
  again. The memo needs storage in `CheckState` or `CheckedSynonym`.
- A `TypeFlags` bit for types that may contain a bundle. The interner cannot tell which constructor
  is `Iris.Effect.Bundle`, which each engine resolves in `IrisEffectCore`, and every synonym and
  unification variable would have to set the bit, which leaves the walk above.
