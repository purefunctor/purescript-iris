------------------------- MODULE MCParallelInterner -------------------------
EXTENDS ParallelInterner, TLC

CONSTANTS MutantName, ValueCount

MCMutant == MutantName

MCValues ==
    CASE ValueCount = 2 -> {"x", "y"}
      [] ValueCount = 3 -> {"x", "y", "w"}
      [] ValueCount = 4 -> {"x", "y", "u", "v"}

MCLocation == [value \in MCValues |->
    CASE value = "u" -> [shard |-> 1, tag |-> 2]
      [] value = "v" -> [shard |-> 2, tag |-> 1]
      [] OTHER -> [shard |-> 1, tag |-> 1]]

ThreadSymmetry == Permutations(Threads)

=============================================================================
