------------------------- MODULE MCParallelInterner -------------------------
EXTENDS ParallelInterner, TLC

CONSTANTS MutantName, ValueCount

MCMutant == MutantName

MCValues ==
    CASE ValueCount = 2 -> {"x", "y"}
      [] ValueCount = 3 -> {"x", "y", "w"}
      [] ValueCount = 4 -> {"x", "y", "u", "v"}

MCHash == [value \in MCValues |-> IF value \in {"u", "v"} THEN 2 ELSE 1]

ThreadSymmetry == Permutations(Threads)

=============================================================================
