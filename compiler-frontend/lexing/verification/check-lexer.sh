#!/usr/bin/env bash
set -euo pipefail

verification_directory="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$verification_directory/../../../.agents/scripts/tlc.sh" "$verification_directory" Lexer.tla

properties=(
  NeverPanics
  LexTerminates
  TokensCoverSource
  AnnotationsAreTrivia
  PositionsMatchOffsets
)

scenarios=(
  "comments 9"
  "strings 8"
  "numbers 6"
  "names 6"
  "all 4"
)

mutants=(
  "block_comment_takes_after_delimiter comments 9 AnnotationsAreTrivia"
  "underscore_takes_nothing names 2 LexTerminates"
  "operator_annotation_from_parenthesis names 3 TokensCoverSource"
  "qualifier_keeps_position names 3 PositionsMatchOffsets"
  "raw_string_opens_with_four_quotes strings 5 NeverPanics"
  "parenthesis_ignores_line_comment names 3 AnnotationsAreTrivia"
  "parenthesis_position_after_qualifier names 3 PositionsMatchOffsets"
)

write_config() {
  local name="$1" mutant="$2" scenario="$3" length="$4"
  shift 4
  local config="$scratch_directory/$name.cfg" property
  {
    echo "CONSTANTS"
    echo "    Scenario = \"$scenario\""
    echo "    MaxLength = $length"
    echo "    Mutant = \"$mutant\""
    echo "SPECIFICATION Spec"
    echo "CHECK_DEADLOCK FALSE"
    for property in "$@"; do
      echo "INVARIANT $property"
    done
  } > "$config"
  echo "$config"
}

check_baseline() {
  local entry scenario length config
  for entry in "${scenarios[@]}"; do
    read -r scenario length <<< "$entry"
    config="$(write_config "lexer-$scenario" none "$scenario" "$length" "${properties[@]}")"
    expect_pass "lexer-$scenario" "$config"
  done
}

check_mutants() {
  local entry mutant scenario length expected config
  for entry in "${mutants[@]}"; do
    read -r mutant scenario length expected <<< "$entry"
    config="$(write_config "lexer-mutant-$mutant" "$mutant" "$scenario" "$length" "$expected")"
    expect_violation "lexer-mutant-$mutant" "$config" "^Error: Invariant $expected is violated"
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
