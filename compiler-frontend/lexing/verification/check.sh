#!/usr/bin/env bash
set -euo pipefail

verification_directory="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$verification_directory/../../../.agents/scripts/tlc.sh" "$verification_directory" Layout.tla

baseline_depth="${LAYOUT_VERIFICATION_DEPTH:-4}"
mutant_depth="${LAYOUT_VERIFICATION_MUTANT_DEPTH:-3}"

invariants=(
  RootIsBottom
  OpenBlocksMatchStack
  FinishClosesEveryBlock
)

action_properties=(
  NeverPanics
  EndsCloseOpenBlocks
  SeparatorsInsideBlocks
  SourceTokensPreserved
)

mutants=(
  "in_skip_end OpenBlocksMatchStack"
  "start_without_token OpenBlocksMatchStack"
  "paren_pops_unconditionally RootIsBottom"
  "else_drops_token SourceTokensPreserved"
  "finish_skips_ends FinishClosesEveryBlock"
  "data_ignores_top_declaration SeparatorsInsideBlocks"
  "insert_start_reads_any_delimiter ErasureIsSound"
  "top_declaration_indexes_stack NeverPanics"
)

write_config() {
  local name="$1" mutant="$2" depth="$3"
  shift 3
  local config="$scratch_directory/$name.cfg" property
  {
    echo "CONSTANTS"
    echo "    MaxDepth = $depth"
    echo "    Mutant = \"$mutant\""
    echo "SPECIFICATION Spec"
    echo "VIEW View"
    echo "CHECK_DEADLOCK FALSE"
    for property in "$@"; do
      if [[ " ${action_properties[*]} " == *" $property "* ]]; then
        echo "PROPERTY $property"
      elif [[ " ${invariants[*]} " == *" $property "* ]]; then
        echo "INVARIANT $property"
      fi
    done
  } > "$config"
  echo "$config"
}

check_baseline() {
  local config
  config="$(write_config baseline none "$baseline_depth" "${invariants[@]}" "${action_properties[@]}")"
  expect_pass baseline "$config"
}

check_mutants() {
  local entry mutant expected config pattern
  for entry in "${mutants[@]}"; do
    read -r mutant expected <<< "$entry"
    config="$(write_config "mutant-$mutant" "$mutant" "$mutant_depth" "$expected")"
    if [[ "$expected" == "ErasureIsSound" ]]; then
      pattern="ErasureIsSound"
    else
      pattern="^Error: (Invariant|Action property) $expected (of [A-Za-z ]+)?is violated"
    fi
    run_tlc "$config" "mutant-$mutant" > /dev/null
    local log="$output_directory/mutant-$mutant.log"
    if grep -qE "$pattern" "$log"; then
      echo "caught    mutant-$mutant: $expected"
    else
      echo "MISSED    mutant-$mutant: expected $expected (see $log)"
      grep -m5 -E "violated|^Error|No error" "$log" || true
      failures=$((failures + 1))
    fi
  done
}

ensure_tools
case "${1:-all}" in
  baseline) check_baseline ;;
  mutants) check_mutants ;;
  all)
    check_baseline
    check_mutants
    ;;
  *) echo "usage: $0 [all|baseline|mutants]" >&2; exit 2 ;;
esac
finish_checks
