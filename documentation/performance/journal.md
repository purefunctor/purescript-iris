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

### K4: type traversal and unification (2026-11-17, branch `k4-type-traversal`)

Checker changes in `checking/src/core/{fold,walk,normalise,substitute}.rs` and the zonk cache in
`checking/src/state.rs`, measured commit by commit with harness `5db492475`. Instructions exclude
the allocator. Interned counts and the output digest are unchanged by every commit, and no stage a
commit does not run moved by 0.02% or more.

| Commit | `checked` | `build` | `edit_comment` | `edit_body` | `edit_interface` |
| --- | ---: | ---: | ---: | ---: | ---: |
| Inline the flag check of type folds and walks | -1.14% | -0.53% | -0.02% | -0.79% | -0.69% |
| Skip redundant writes when compressing unification chains | -0.38% | -0.17% | -0.00% | -0.28% | -0.24% |
| Expand types with one lookup per reduction step | -0.85% | -0.39% | -0.01% | -0.61% | -0.52% |
| Store substitutions in a small inline map | -1.00% | -0.45% | -0.01% | -0.53% | -0.46% |
| Compress unification chains without recording them | -1.39% | -0.63% | -0.01% | -1.01% | -0.87% |
| Read expansion flags only for applications and rows | -0.29% | -0.13% | -0.01% | -0.21% | -0.18% |
| Reserve the zonk cache from the module's unification count | -0.73% | -0.33% | -0.01% | -0.54% | -0.46% |
| All seven | -5.64% | -2.60% | -0.08% | -3.90% | -3.38% |

Across all seven, `checked` went from 953,215,935 to 899,418,758 instructions, its allocations from
1,145,505 to 1,101,415 (-3.85%), its allocated bytes from 110,385,845 to 105,113,905 (-4.78%), and
its allocator instructions from 89,807,690 to 84,426,715 (-5.99%). The small substitution maps
account for most of the allocation change.

Code generation can move stages a change never runs. Creating the zonk cache with
`FxHashMap::with_capacity_and_hasher` added a caller to hashbrown's `fallible_with_capacity`, which
LLVM then stopped inlining in other crates; under harness `9615ae229` that cost `resolved` +0.77%
and `bracketed` +1.21%. Reserving on an empty map leaves every other stage unchanged.

## Measured and rejected

K4 experiments, measured with earlier harness revisions (before `9615ae229` unless noted), so only
the percentages are comparable.

- Replacing `ReductionContext` in `normalise_head` with a direct loop over the type's variant:
  `checked` +0.10%.
- Moving `rule_simplify_rows` out of line from `normalise_head`: +0.44% when the call tests for a
  row, +0.15% when the caller tests for the row first. Every unsolved unification variable pays
  for the call, and the smaller frame does not make up for it.
- Marking `lookup_zonk_cache` and `insert_zonk_cache` `#[inline]` on top of the presized cache:
  `checked` +0.07%.
- Creating the zonk cache with a capacity instead of reserving it (harness `9615ae229`): `checked`
  -0.42% instead of -0.71%, and `resolved` and `bracketed` regress (see above).

## Not tried

The work areas in the performance plan (K1 to K5, S1 to S3, L1, R1, F1, J1, E1, B1, I1) start here
until they are measured.

From K4, with the reason each was not worth trying on `core`:

- Memoising synonym expansion per module: about 3,000 expansions with substitutions across the
  build and edit stages, a third of them repeating a type and 72% containing unification
  variables, so a memo saves at most about 0.8% of `checked` and needs storage in `CheckState`.
- Instantiating all leading binders at once in `subtype_core` and `unify_core`: 241 of about
  72,000 top-level substitution folds start in `unification.rs`; most start in
  `instantiate_callable_foralls` (36,483), which already substitutes once.
- Returning synonyms by reference: cloning `CheckedSynonym` costs 0.13% of `checked`.
- Skipping `promote_type` or the kind check in `solve_core`: `promote_type` already skips solutions
  without variables, and no flag shows that the kind check (1.5% of `checked`) cannot fail.
- Folding rows without copying their fields: only 852 row folds on `core`.
- Of what remains in substitution and zonking, interning is 96M and 61M instructions (K1).
