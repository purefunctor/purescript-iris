#!/usr/bin/env bash
set -euo pipefail

verification_directory="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$verification_directory/../../../.agents/scripts/tlc.sh" "$verification_directory" QueryEngine.tla

quick=false
if [[ "${1:-}" == "--quick" ]]; then
  quick=true
fi

mutant() {
  local mutation="$1" config="$2" pattern="$3" path="$scratch_directory/mutant-$1-$2.cfg"
  sed "s/Mutation = \"None\"/Mutation = \"$mutation\"/" "$verification_directory/$config.cfg" > "$path"
  expect_violation "mutant-$mutation-$config" "$path" "$pattern"
}

ensure_tools
echo "Properties that must hold:"
configurations=(Serial Parallel)
if ! $quick; then
  configurations+=(ParallelWrites ParallelChain)
fi
configurations+=(SpuriousCycle SpuriousCancel PanicHang)
for config in "${configurations[@]}"; do
  expect_pass "$config" "$verification_directory/$config.cfg"
done

if ! $quick; then
  echo
  echo "Mutations that must be caught:"
  mutant TwoOwners Parallel "Invariant OwnerFramesHoldTheirQuery is violated"
  mutant CleanupIgnoresOwnership Parallel "Invariant (OwnerFramesHoldTheirQuery|InProgressHasLiveOwner|BlockedWaitersCanBeWoken) is violated"
  mutant EdgeAlwaysAdded Serial "Invariant WaitForGraphIsAcyclic is violated"
  mutant AddEdgeChecksOneHop ParallelChain "Invariant WaitForGraphIsAcyclic is violated"
  mutant EagerCycle ParallelChain "Invariant CyclesAreReal is violated"
  mutant EdgeNotRemoved Parallel "Invariant WaitEdgesMatchWaiters is violated"
  mutant WaitersNotWoken Parallel "Invariant BlockedWaitersCanBeWoken is violated"
  mutant ChangedAlwaysPrevious Serial "Invariant (FreshCacheIsCorrect|OkResultsAreCorrect) is violated"
  mutant VerifyIgnoresInputs Serial "Invariant (FreshCacheIsCorrect|OkResultsAreCorrect) is violated"
  mutant UnverifiablePropagates Serial "Invariant UnverifiableStaysInternal is violated"
  mutant VerifyingDemandBlocks ParallelWrites "Invariant CyclesAreReal is violated"
  mutant CleanupDropsWaiters SpuriousCancel "Invariant CancellationsAreExplained is violated"
  mutant PanicSkipsCleanup PanicHang "Invariant (BlockedWaitersCanBeWoken|InProgressHasLiveOwner) is violated"
fi

finish_checks
