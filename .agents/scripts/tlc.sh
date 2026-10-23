#!/usr/bin/env bash

verification_directory="$1"
model="$2"
repository_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
java_version="temurin-21.0.12+8.0.LTS"
tla_version="1.7.4"
tla_sha256="936a262061c914694dfd669a543be24573c45d5aa0ff20a8b96b23d01e050e88"
tools_directory="$repository_root/target/tla"
tla_jar="$tools_directory/tla2tools-$tla_version.jar"
output_directory="$tools_directory/runs/$(basename "$(dirname "$verification_directory")")"
scratch_directory="$(mktemp -d)"
trap 'rm -rf "$scratch_directory"' EXIT
failures=0

ensure_tools() {
  mkdir -p "$tools_directory" "$output_directory"
  if [[ ! -f "$tla_jar" ]] || ! echo "$tla_sha256  $tla_jar" | sha256sum --check --status; then
    local download="$scratch_directory/tla2tools.jar"
    curl --proto '=https' --tlsv1.2 -fsSL -o "$download" \
      "https://github.com/tlaplus/tlaplus/releases/download/v$tla_version/tla2tools.jar"
    echo "$tla_sha256  $download" | sha256sum --check --status
    mv "$download" "$tla_jar"
  fi
  mise install --yes "java@$java_version" >/dev/null
}

run_tlc() {
  local config="$1" label="$2"
  local run="$scratch_directory/$label" log="$output_directory/$label.log"
  mkdir -p "$run"
  cp "$verification_directory"/*.tla "$run/"
  cp "$config" "$run/check.cfg"
  local status=0
  (cd "$run" && mise exec "java@$java_version" -- \
    java -XX:+UseParallelGC -cp "$tla_jar" tlc2.TLC -workers auto \
      -metadir "$run/states" -config check.cfg "$model") > "$log" 2>&1 || status=$?
  if [[ "$status" -eq 0 ]] && grep -q "Model checking completed. No error has been found." "$log"; then
    echo "pass"
  elif grep -qE "^Error: (Invariant|Action property) [A-Za-z]+ (of [A-Za-z ]+)?is violated" "$log"; then
    grep -m1 -oE "^Error: (Invariant|Action property) [A-Za-z]+" "$log" | awk '{print $NF}'
  elif grep -q "^Error: Temporal properties were violated." "$log"; then
    echo "temporal"
  elif grep -q "^Error: Deadlock reached." "$log"; then
    echo "deadlock"
  else
    echo "error"
  fi
}

expect_pass() {
  local label="$1" config="$2" outcome
  local log="$output_directory/$label.log"
  outcome="$(run_tlc "$config" "$label")"
  if [[ "$outcome" == "pass" ]]; then
    echo "pass      $label"
  else
    echo "FAIL      $label: $outcome (see $log)"
    sed -n '/violated\|^Error/,$p' "$log" | head -40 || true
    failures=$((failures + 1))
  fi
}

expect_violation() {
  local label="$1" config="$2" pattern="$3" outcome
  local log="$output_directory/$label.log"
  outcome="$(run_tlc "$config" "$label")"
  if [[ "$outcome" != "pass" && "$outcome" != "error" ]] && grep -qE "$pattern" "$log"; then
    echo "caught    $label: $(grep -m1 -E "$pattern" "$log")"
  else
    echo "MISSED    $label: $outcome (expected: $pattern; see $log)"
    grep -m5 -E "violated|^Error|No error" "$log" || true
    failures=$((failures + 1))
  fi
}

finish_checks() {
  echo
  if [[ "$failures" -ne 0 ]]; then
    echo "$failures unexpected result(s)"
    return 1
  fi
  echo "All results as expected"
}
