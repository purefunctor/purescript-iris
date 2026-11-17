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

### R1

Stabilizing and resolving memory, branch `r1-memory`, 2026-11-17, `just perf` against each commit's
parent on harness `5db492475`. Live heap is what each stage retains on `acme` (7,374 files, one
thread), measured with a counting allocator.

| Commit | Stage | Instructions | Allocated bytes | Live heap on `acme` |
| --- | --- | ---: | ---: | ---: |
| Size stabilized tables for the nodes they keep | stabilized | -1.01% | -29.18% | 192.0 → 90.5 MB |
| Allocate import lists with room for one import | resolved | -0.53% | -9.65% | 155.7 → 139.1 MB |
| Share export tables between resolved imports | resolved | -35.56% | -51.17% | 139.1 → 49.4 MB |
| Share class member lists between resolved modules | resolved | -28.63% | -22.05% | 49.4 → 42.8 MB |

- The stabilized tables were sized for every node, but Annotation, QualifiedName, and LabelName
  nodes, more than half of the tree, never get an `AstId`.
- Most of the resolved memory was copies: every import copied the imported module's whole export
  table, every visible class copied its members, and each import list reserved room for four
  imports.
- Sharing export tables costs `lowered` +0.52% and `functional` +0.25% in instructions: names
  looked up through an explicit import probe the shared table and then the import's listed set.
  Build instructions fall 0.24%, `edit_body` 0.84%, and `edit_interface` 0.74%.
- `indexed` was not changed; its 72 MB on `acme` is the items, names, and import lists each module
  owns, with no copies.
- The whole branch against `5db492475`: build instructions -0.45% and allocated bytes -3.63% on
  `core`; peak resident memory on `acme` 2,941.5 → 2,684.2 MB on one thread and 3,147.6 →
  2,898.6 MB on eight; `resolved` wall time on one thread 153.8 → 79.2 ms.

### S3: lexing and parsing

2026-11-17, branch `s3-lexing-parsing` on `5db492475`. Only `parsed` changes beyond 0.5%; interned
counts and the output digest are unchanged. Each commit against its parent, `parsed` stage:

| Commit | Instructions | Allocator instructions | Allocations |
| --- | ---: | ---: | ---: |
| Check ASCII before Unicode symbol tables in is_operator | -6.877% | +0.000% | +0.000% |
| Decode each character once in Lexer::take_while | -1.500% | +0.000% | +0.000% |
| Scan ASCII names by byte with a lookup table | -5.852% | +0.000% | +0.000% |
| Reserve parser events by the observed events per token | -0.032% | -10.431% | -7.699% |
| Find the end of line comments with str::find | -2.060% | +0.000% | +0.000% |

The branch against `5db492475`:

| Stage | Instructions | Allocator instructions | Allocations | Allocated bytes |
| --- | ---: | ---: | ---: | ---: |
| parsed | 187,390,649 (-15.447%) | 28,297,631 (-10.430%) | 10,166 (-7.699%) | 75,502,329 (-6.405%) |
| build | 2,035,846,785 (-1.654%) | 225,583,348 (-1.326%) | 1,892,932 (-0.045%) | 439,706,315 (-1.161%) |

Within `parsed`, lexing drops by about 39% and layout is unchanged. Parser events average 5.2 per
token on `core` and peak at 6.6 per file, which sets the reservation at six. What remains in
`parsed` is mostly tree construction in `parsing::builder`, which belongs to S2.

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

### R1

- Listed import items keyed by name (`FxHashMap<SmolStr, (FileId, ItemId)>`) instead of identity:
  `lowered` -0.44% but `functional` +0.61%, which iterates every export of every import, and
  5.8 MB more on `acme`. Measured on a pre-release harness copy, before allocator instructions
  were split out of the gate.
- `#[inline]` hints on the import lookups: no effect under the default release profile; only
  `#[inline(always)]` inlines them into the per-import loop. A single probe site per lookup,
  with listed entries stored in the export table's value type, was not inlined either.

### S3: lexing and parsing

2026-11-17. Measured with a Callgrind probe of `lexing::lex` alone on `core`, against the kept
change it would replace:

- `Lexer::take_while` over `char_indices` with a local position: lexing rose from 69.6 to about 77
  million instructions.
- ASCII name bytes checked with range comparisons instead of the lookup table: 71.4 million against
  69.6 million.
- Byte scanning ASCII whitespace in `take_annotation_whitespace`: 55.9 million against 53.6
  million.

Profiled and left alone: the parser's `failed_alternatives` map (about 900 lookups on `core`), the
parser's fuel checks in `nth` (about 5 million instructions), and `Layout::take_token`, whose cost
is spread across `Insert` construction, stack matching, and `insert_sep` with no single hotspot.

### K1: sharded type interner

- K1, 64 shards, each a `Mutex<hashbrown::HashTable>`: `checked` on `core` -8.7% instructions, but
  checking `acme` at 8 threads took about 1,170 ms against papaya's 1,000 ms.
- K1, 64 shards with lock-free probes: -5.7% instructions, still about 1,240 ms against 1,000 ms at 8
  threads. 65,794 of 3,391,966 locked inserts (1.9%) found the lock held, and parked threads do not
  show in sampling profiles. Moving the lock to its own cache line did not help. 1024 shards bring
  contention to about 3,600 inserts.

## Not tried

The work areas in the performance plan (K1 to K5, S1, S2, L1, R1, J1, B1, I1) start here
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
