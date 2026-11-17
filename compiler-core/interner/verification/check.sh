#!/usr/bin/env bash
set -euo pipefail

verification_directory="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$verification_directory/../../../.agents/scripts/tlc.sh" "$verification_directory" MCParallelInterner.tla

properties=(
  EqualValuesShareId
  DistinctValuesNeverShareId
  IdNamesItsValue
  GetFindsReturnedInterns
  ReturnedIdsArePublished
  ArenaReadsArePublished
  TablesAreCoherent
  CurrentTableHoldsInterns
  ViewsHoldOnlyWrittenSlots
)

scopes=(
  "colliding-pairs 3 2 2"
  "colliding-triples 4 1 3"
  "two-shards 3 1 4"
)

mutants=(
  "trust_tag colliding-pairs DistinctValuesNeverShareId"
  "get_trust_tag colliding-pairs DistinctValuesNeverShareId"
  "get_trust_tag colliding-pairs IdNamesItsValue"
  "skip_locked_probe colliding-pairs EqualValuesShareId"
  "skip_locked_probe colliding-pairs TablesAreCoherent"
  "unlocked_insert colliding-pairs EqualValuesShareId"
  "reuse_observed_table colliding-pairs EqualValuesShareId"
  "reuse_observed_table colliding-pairs CurrentTableHoldsInterns"
  "empty_grow colliding-pairs EqualValuesShareId"
  "empty_grow colliding-pairs GetFindsReturnedInterns"
  "relaxed_slot colliding-pairs ArenaReadsArePublished"
  "relaxed_generation colliding-pairs ArenaReadsArePublished"
  "publish_before_write colliding-pairs ArenaReadsArePublished"
  "return_before_write colliding-pairs ReturnedIdsArePublished"
)

scope_of() {
  local wanted="$1" scope name
  for scope in "${scopes[@]}"; do
    read -r name _ <<< "$scope"
    if [[ "$name" == "$wanted" ]]; then
      echo "$scope"
      return
    fi
  done
  echo "unknown scope $wanted" >&2
  exit 2
}

write_config() {
  local name="$1" mutant="$2" threads="$3" operations="$4" values="$5"
  shift 5
  local thread_names="" index
  for ((index = 1; index <= threads; index++)); do
    thread_names+="${thread_names:+, }t$index"
  done
  local config="$scratch_directory/$name.cfg"
  {
    echo "CONSTANTS"
    echo "    Threads = {$thread_names}"
    echo "    Values <- MCValues"
    echo "    Location <- MCLocation"
    echo "    OperationsPerThread = $operations"
    echo "    Mutant <- MCMutant"
    echo "    MutantName = \"$mutant\""
    echo "    ValueCount = $values"
    echo "SYMMETRY ThreadSymmetry"
    echo "SPECIFICATION Spec"
    for property in "$@"; do
      echo "INVARIANT $property"
    done
  } > "$config"
  echo "$config"
}

check_baseline() {
  for scope in "${scopes[@]}"; do
    read -r name threads operations values <<< "$scope"
    local config
    config="$(write_config "baseline-$name" none "$threads" "$operations" "$values" "${properties[@]}")"
    expect_pass "baseline-$name" "$config"
  done
}

check_mutants() {
  for entry in "${mutants[@]}"; do
    read -r mutant scope expected <<< "$entry"
    local name threads operations values config
    read -r name threads operations values <<< "$(scope_of "$scope")"
    config="$(write_config "mutant-$mutant-$expected" "$mutant" "$threads" "$operations" "$values" "$expected")"
    expect_violation "mutant-$mutant-$expected" "$config" \
      "^Error: Invariant $expected is violated"
  done
}

check_matrix() {
  printf '%-24s' "mutant"
  for invariant in "${properties[@]}"; do
    printf ' %s' "$invariant"
  done
  echo
  local seen=" "
  for entry in "${mutants[@]}"; do
    read -r mutant scope _ <<< "$entry"
    [[ "$seen" == *" $mutant "* ]] && continue
    seen="$seen$mutant "
    local name threads operations values
    read -r name threads operations values <<< "$(scope_of "$scope")"
    printf '%-24s' "$mutant"
    for invariant in "${properties[@]}"; do
      local config outcome
      config="$(write_config "matrix-$mutant-$invariant" "$mutant" "$threads" "$operations" "$values" "$invariant")"
      outcome="$(run_tlc "$config" "matrix-$mutant-$invariant")"
      local mark="-"
      [[ "$outcome" == "$invariant" ]] && mark="caught"
      [[ "$outcome" == "error" ]] && mark="error"
      printf " %-${#invariant}s" "$mark"
    done
    echo
  done
}

ensure_tools
case "${1:-all}" in
  baseline) check_baseline ;;
  mutants) check_mutants ;;
  matrix) check_matrix; exit ;;
  all)
    check_baseline
    check_mutants
    ;;
  *) echo "usage: $0 [all|baseline|mutants|matrix]" >&2; exit 2 ;;
esac
finish_checks
