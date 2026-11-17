#!/usr/bin/env bash

set -euo pipefail

setup_failure() {
  printf 'performance: %s failed\n' "$1" >&2
  exit 2
}

usage() {
  cat <<'EOF'
Usage: run.sh [base-ref]

Compare the current checkout with base-ref using release-built measurement
harnesses. base-ref defaults to origin/main.

Environment:
  PERF_COUNTED_PRESETS    presets measured under Callgrind (default: core)
  PERF_TIMED_PRESET       preset for wall time, or empty to skip (default: acme)
  PERF_TIMED_RUNS         interleaved base and candidate runs per thread count (default: 3)
  PERF_TIMED_THREADS      thread counts for wall time; 0 uses every core (default: "1 0")
  PERF_PACKAGE_SET        package-set version passed to both revisions
  PERF_THRESHOLD_PERCENT  regression threshold for gated metrics (default: 0.5)
  PERFORMANCE_REPORT_DIR  where to write reports (default: target/performance-reports/<time>)
EOF
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  usage
  exit 0
fi

if [[ $# -gt 1 ]]; then
  usage >&2
  exit 2
fi

base_ref="${1:-origin/main}"
counted_presets=${PERF_COUNTED_PRESETS:-core}
timed_preset=${PERF_TIMED_PRESET-acme}
timed_runs=${PERF_TIMED_RUNS:-3}
timed_threads=${PERF_TIMED_THREADS:-1 0}
threshold=${PERF_THRESHOLD_PERCENT:-0.5}

workspace=$(git rev-parse --show-toplevel) || setup_failure "locating the workspace"
base_commit=$(git -C "$workspace" rev-parse --verify "${base_ref}^{commit}") || {
  printf 'performance: base revision not found: %s\n' "$base_ref" >&2
  exit 2
}
candidate_commit=$(git -C "$workspace" rev-parse --short HEAD)
candidate_label="HEAD $candidate_commit"
if [[ -n "$(git -C "$workspace" status --porcelain --untracked-files=no)" ]]; then
  candidate_label="$candidate_label with uncommitted changes"
fi

timestamp=$(date -u +%Y%m%dT%H%M%SZ)
report_dir=${PERFORMANCE_REPORT_DIR:-"$workspace/target/performance-reports/$timestamp-$$"}
base_workspace="$workspace/target/performance/base-worktree"
base_target="$workspace/target/performance/base-target"
head_target="$workspace/target/performance/head-target"

# With several codegen units per crate, recompiling a crate repartitions the generic code it
# instantiates and changes inlining in stages a change never runs. One unit per crate keeps
# instruction counts of untouched stages stable across rebuilds.
export CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1
corpus_dir="$workspace/target/compatibility"

cleanup() {
  git -C "$workspace" worktree remove --force "$base_workspace" >/dev/null 2>&1 || true
  rm -rf "$base_workspace"
}

trap cleanup EXIT
trap 'exit 130' INT TERM

mkdir -p "$report_dir" || setup_failure "creating the report directory"

if ! command -v valgrind >/dev/null; then
  printf 'performance: valgrind is required; install it with your package manager\n' >&2
  exit 2
fi

printf 'Performance base: %s (%s)\n' "$base_ref" "$base_commit"
printf 'Toolchain: %s\n' "$(cd "$workspace" && rustc -V)"
printf 'Building candidate harness in release mode...\n'
cargo build --manifest-path "$workspace/Cargo.toml" \
  --target-dir "$head_target" \
  --release \
  -p tests-compatibility || setup_failure "building the candidate harness"

printf 'Creating base worktree...\n'
cleanup
git -C "$workspace" worktree prune
git -C "$workspace" worktree add --detach "$base_workspace" "$base_commit" >/dev/null || \
  setup_failure "creating the base worktree"

printf 'Building base harness in release mode...\n'
cargo build --manifest-path "$base_workspace/Cargo.toml" \
  --target-dir "$base_target" \
  --release \
  -p tests-compatibility || setup_failure "building the base harness"

# Callgrind counts depend on the process's starting state, including the working directory and
# the lengths of its arguments and environment. Both sides therefore run from the same directory,
# with harness and output paths of equal length.
harness_dir="$workspace/target/performance/harness"
mkdir -p "$harness_dir/base" "$harness_dir/head" || setup_failure "creating the harness directory"
cp "$head_target/release/tests-compatibility" "$harness_dir/head/tests-compatibility" || \
  setup_failure "copying the candidate harness"
cp "$base_target/release/tests-compatibility" "$harness_dir/base/tests-compatibility" || \
  setup_failure "copying the base harness"
candidate_harness="$harness_dir/head/tests-compatibility"
base_harness="$harness_dir/base/tests-compatibility"

if ! "$base_harness" measure --help >/dev/null 2>&1; then
  printf 'performance: %s predates the measurement harness; choose a later base\n' "$base_ref" >&2
  exit 2
fi

common_options=(
  --registry-dir "$corpus_dir/registry"
  --index-dir "$corpus_dir/registry-index"
  --cache-dir "$corpus_dir"
)
if [[ -n "${PERF_PACKAGE_SET:-}" ]]; then
  common_options+=(--package-set "$PERF_PACKAGE_SET")
fi

measure() {
  local side=$1 output=$2
  shift 2
  local harness=$candidate_harness
  if [[ "$side" == base ]]; then
    harness=$base_harness
  fi
  (cd "$workspace" && "$harness" measure "${common_options[@]}" --json-output "$output" "$@") \
    >>"$report_dir/$side.log" 2>&1 || {
    printf 'performance: the %s measurement failed; inspect %s/%s.log\n' \
      "$side" "$report_dir" "$side" >&2
    exit 2
  }
}

# The registry and index are cloned only when prepare runs with their default relative paths,
# so prepare runs from the workspace without overriding them.
printf 'Preparing the corpus...\n'
prepare_options=(--cache-dir "$corpus_dir")
if [[ -n "${PERF_PACKAGE_SET:-}" ]]; then
  prepare_options+=(--package-set "$PERF_PACKAGE_SET")
fi
for preset in $counted_presets $timed_preset; do
  prepare_options+=(--preset "$preset")
done
(cd "$workspace" && "$candidate_harness" prepare "${prepare_options[@]}") \
  >>"$report_dir/prepare.log" 2>&1 || setup_failure "preparing the corpus; inspect $report_dir/prepare.log"

base_measurements=()
candidate_measurements=()

for preset in $counted_presets; do
  printf 'Counting instructions and allocations for %s under Callgrind...\n' "$preset"
  measure head "$report_dir/$preset-head.json" --preset "$preset"
  measure base "$report_dir/$preset-base.json" --preset "$preset"
  base_measurements+=(--base "$report_dir/$preset-base.json")
  candidate_measurements+=(--candidate "$report_dir/$preset-head.json")
done

if [[ -n "$timed_preset" && "$timed_runs" -gt 0 ]]; then
  for threads in $timed_threads; do
    for run in $(seq 1 "$timed_runs"); do
      printf 'Timing %s with %s threads, run %s of %s...\n' "$timed_preset" "$threads" "$run" "$timed_runs"
      sides=(base head)
      if (( run % 2 == 0 )); then
        sides=(head base)
      fi
      for side in "${sides[@]}"; do
        output="$report_dir/$timed_preset-$side-threads$threads-run$run.json"
        measure "$side" "$output" --preset "$timed_preset" --timing --threads "$threads"
        if [[ "$side" == base ]]; then
          base_measurements+=(--base "$output")
        else
          candidate_measurements+=(--candidate "$output")
        fi
      done
    done
  done
fi

printf 'Comparing measurements...\n'
set +e
"$candidate_harness" compare-performance \
  "${base_measurements[@]}" \
  "${candidate_measurements[@]}" \
  --base-label "$base_ref $(git -C "$workspace" rev-parse --short "$base_commit")" \
  --candidate-label "$candidate_label" \
  --threshold-percent "$threshold" \
  --json-output "$report_dir/comparison.json" \
  --summary-output "$report_dir/summary.md" \
  >"$report_dir/comparison.log" 2>&1
comparison_status=$?
set -e

if [[ "$comparison_status" != 0 && "$comparison_status" != 1 ]]; then
  printf 'performance: comparison failed with status %s; inspect %s/comparison.log\n' \
    "$comparison_status" "$report_dir" >&2
  exit 2
fi

printf '\n'
cat "$report_dir/summary.md"
printf '\nPerformance reports: %s\n' "$report_dir"
exit "$comparison_status"
