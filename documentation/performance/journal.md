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

The work areas in the performance plan (K1 to K5, S1 to S3, L1, R1, J1, B1, I1) start here
until they are measured.

### K3: capability bundle validation

From K3:

- Memoising per synonym whether its expansion reaches `Bundle`. The bundle walk visits 728
  synonym expansions on `core`, about 1% of `checked`, because each top-level type walks them
  again. The memo needs storage in `CheckState` or `CheckedSynonym`.
- A `TypeFlags` bit for types that may contain a bundle. The interner cannot tell which constructor
  is `Iris.Effect.Bundle`, which each engine resolves in `IrisEffectCore`, and every synonym and
  unification variable would have to set the bit, which leaves the walk above.

### J1

- Fewer `format!` calls in rendering. They are about 2% of `javascript` on `core`.

### K5: skolem and exhaustiveness passes

- A type flag for rigid variables with a skolem scope. The skolem audit could then skip types
  without one instead of consulting its memo, and the scan for expected scopes over every
  expression of a module (about 0.28% of `checked`) could stop early. It needs a new bit in
  `TypeFlags` and the interner's flag computation.
- Reusing wildcard patterns per type in `CheckState::allocate_wildcard`. Each call interns a
  pattern, at about 300 instructions per call and 0.23% of `checked` on `core`.
- Deduplicating constructors in `exhaustive::collect_sigma` by a linear scan instead of a hash
  set. Not measured: it is quadratic in the number of distinct constructors, and `algorithm_u`
  already collects the sigma once per row, so large literal case tables would get much slower.
