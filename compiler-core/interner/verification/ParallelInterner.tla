--------------------------- MODULE ParallelInterner ---------------------------
EXTENDS Naturals, Sequences, FiniteSets

CONSTANTS Threads, Values, Location, OperationsPerThread, Mutant

Mutants == {
    "none",
    "trust_tag",
    "get_trust_tag",
    "skip_locked_probe",
    "unlocked_insert",
    "reuse_observed_table",
    "empty_grow",
    "relaxed_slot",
    "relaxed_generation",
    "publish_before_write",
    "return_before_write"
}

ASSUME Mutant \in Mutants
ASSUME Location \in [Values -> [shard : Nat, tag : Nat]]

NoId == 0

Shards == {Location[value].shard : value \in Values}

Operations == [kind : {"intern", "get"}, value : Values]

Capacity(generation) == generation - 1

VARIABLES
    arena,
    tables,
    generation,
    generationView,
    locked,
    lockView,
    view,
    pc,
    program,
    step,
    observed,
    candidate,
    snapshot,
    results

variables == <<arena, tables, generation, generationView, locked, lockView,
               view, pc, program, step, observed, candidate, snapshot, results>>

Maximum(set) == CHOOSE element \in set : \A other \in set : element >= other

Value(thread) == program[thread][step[thread]].value

Kind(thread) == program[thread][step[thread]].kind

Shard(thread) == Location[Value(thread)].shard

Tag(thread) == Location[Value(thread)].tag

Table(thread) == tables[Shard(thread)][observed[thread]]

ProbeSlots(thread) == {slot \in Table(thread) : slot.tag = Tag(thread)}

ProbeMatch(thread, trustTag) ==
    LET matches == {slot.id : slot \in {slot \in ProbeSlots(thread) :
                       arena[slot.id].value = Value(thread) \/ trustTag}}
    IN IF matches = {} THEN NoId ELSE Maximum(matches)

AcquireSlots(slots, known) == known \cup UNION {slot.view : slot \in slots}

AcquireArena(ids, known) == known \cup UNION {arena[id].view : id \in ids}

GenerationAcquire(thread) ==
    IF Mutant = "relaxed_generation"
    THEN view[thread]
    ELSE view[thread] \cup generationView[Shard(thread)]

Unreachable(thread, known) ==
    /\ pc' = [pc EXCEPT ![thread] = "unreachable"]
    /\ view' = [view EXCEPT ![thread] = known]
    /\ UNCHANGED <<arena, tables, generation, generationView, locked, lockView,
                   program, step, observed, candidate, snapshot, results>>

Advance(thread) ==
    /\ step' = [step EXCEPT ![thread] = @ + 1]
    /\ pc' = [pc EXCEPT ![thread] =
                IF step[thread] = OperationsPerThread THEN "done" ELSE "idle"]
    /\ candidate' = [candidate EXCEPT ![thread] = NoId]
    /\ snapshot' = [snapshot EXCEPT ![thread] = {}]

Return(thread, id, known) ==
    /\ results' = results \cup
           {[kind |-> Kind(thread), value |-> Value(thread), id |-> id,
             before |-> snapshot[thread], published |-> id = NoId \/ id \in known]}
    /\ view' = [view EXCEPT ![thread] = known]
    /\ Advance(thread)

Unlock(thread, known) ==
    /\ locked' = [locked EXCEPT ![Shard(thread)] = FALSE]
    /\ lockView' = [lockView EXCEPT ![Shard(thread)] = known]

LoadTable(thread) ==
    /\ pc[thread] = "idle"
    /\ observed' = [observed EXCEPT ![thread] = generation[Shard(thread)]]
    /\ view' = [view EXCEPT ![thread] = GenerationAcquire(thread)]
    /\ snapshot' = [snapshot EXCEPT ![thread] =
                      IF Kind(thread) = "get"
                      THEN {result.id : result \in
                               {result \in results :
                                   result.kind = "intern" /\ result.value = Value(thread)}}
                      ELSE {}]
    /\ pc' = [pc EXCEPT ![thread] = "probe"]
    /\ UNCHANGED <<arena, tables, generation, generationView, locked, lockView,
                   program, step, candidate, results>>

Probe(thread) ==
    /\ pc[thread] = "probe"
    /\ LET slots == ProbeSlots(thread)
           acquired == AcquireSlots(slots, view[thread])
           ids == {slot.id : slot \in slots}
           trustTag == IF Kind(thread) = "intern"
                       THEN Mutant = "trust_tag"
                       ELSE Mutant = "get_trust_tag"
           found == ProbeMatch(thread, trustTag)
       IN IF ~(ids \subseteq acquired)
          THEN Unreachable(thread, acquired)
          ELSE LET seen == AcquireArena(ids, acquired)
               IN IF found # NoId
                  THEN /\ Return(thread, found, seen)
                       /\ UNCHANGED <<arena, tables, generation, generationView,
                                      locked, lockView, program, observed>>
                  ELSE /\ pc' = [pc EXCEPT ![thread] =
                                   IF Mutant = "unlocked_insert"
                                   THEN "locked_probe"
                                   ELSE "lock"]
                       /\ view' = [view EXCEPT ![thread] = seen]
                       /\ UNCHANGED <<arena, tables, generation, generationView,
                                      locked, lockView, program, step, observed,
                                      candidate, snapshot, results>>

Lock(thread) ==
    /\ pc[thread] = "lock"
    /\ ~locked[Shard(thread)]
    /\ locked' = [locked EXCEPT ![Shard(thread)] = TRUE]
    /\ view' = [view EXCEPT ![thread] = @ \cup lockView[Shard(thread)]]
    /\ pc' = [pc EXCEPT ![thread] = "locked_probe"]
    /\ UNCHANGED <<arena, tables, generation, generationView, lockView, program,
                   step, observed, candidate, snapshot, results>>

LockedProbe(thread) ==
    /\ pc[thread] = "locked_probe"
    /\ LET current == IF Mutant = "reuse_observed_table"
                      THEN observed[thread]
                      ELSE generation[Shard(thread)]
           known == GenerationAcquire(thread)
           slots == IF Mutant = "skip_locked_probe" THEN {}
                    ELSE {slot \in tables[Shard(thread)][current] : slot.tag = Tag(thread)}
           acquired == AcquireSlots(slots, known)
           ids == {slot.id : slot \in slots}
           matches == {slot.id : slot \in {slot \in slots :
                          arena[slot.id].value = Value(thread)}}
           found == IF matches = {} THEN NoId ELSE Maximum(matches)
       IN IF ~(ids \subseteq acquired)
          THEN Unreachable(thread, acquired)
          ELSE LET seen == AcquireArena(ids, acquired)
               IN IF found # NoId \/ Kind(thread) = "get"
                  THEN /\ Unlock(thread, seen)
                       /\ Return(thread, found, seen)
                       /\ UNCHANGED <<arena, tables, generation, generationView,
                                      program, observed>>
                  ELSE /\ observed' = [observed EXCEPT ![thread] = current]
                       /\ pc' = [pc EXCEPT ![thread] = "push"]
                       /\ view' = [view EXCEPT ![thread] = seen]
                       /\ UNCHANGED <<arena, tables, generation, generationView,
                                      locked, lockView, program, step, candidate,
                                      snapshot, results>>

ArenaPush(thread) ==
    /\ pc[thread] = "push"
    /\ LET slot == Len(arena) + 1
           written == view[thread] \cup {slot}
           late == Mutant \in {"publish_before_write", "return_before_write"}
       IN /\ arena' = Append(arena,
                 IF late
                 THEN [value |-> Value(thread), active |-> FALSE, view |-> {}]
                 ELSE [value |-> Value(thread), active |-> TRUE, view |-> written])
          /\ candidate' = [candidate EXCEPT ![thread] = slot]
          /\ view' = [view EXCEPT ![thread] = IF late THEN @ ELSE written]
          /\ results' = IF Mutant = "return_before_write"
                        THEN results \cup
                                 {[kind |-> "intern", value |-> Value(thread), id |-> slot,
                                   before |-> {}, published |-> FALSE]}
                        ELSE results
          /\ pc' = [pc EXCEPT ![thread] = "store"]
          /\ UNCHANGED <<tables, generation, generationView, locked, lockView,
                         program, step, observed, snapshot>>

StoreSlot(thread) ==
    /\ pc[thread] = "store"
    /\ LET shard == Shard(thread)
           entry == [tag |-> Tag(thread), id |-> candidate[thread],
                     view |-> IF Mutant = "relaxed_slot" THEN {} ELSE view[thread]]
       IN tables' = [tables EXCEPT ![shard][observed[thread]] = @ \cup {entry}]
    /\ pc' = [pc EXCEPT ![thread] =
                IF arena[candidate[thread]].active THEN "grow" ELSE "late_write"]
    /\ UNCHANGED <<arena, generation, generationView, locked, lockView, view,
                   program, step, observed, candidate, snapshot, results>>

LateWrite(thread) ==
    /\ pc[thread] = "late_write"
    /\ LET slot == candidate[thread]
           written == view[thread] \cup {slot}
       IN /\ arena' = [arena EXCEPT ![slot] = [@ EXCEPT !.active = TRUE, !.view = written]]
          /\ view' = [view EXCEPT ![thread] = written]
    /\ pc' = [pc EXCEPT ![thread] = "grow"]
    /\ UNCHANGED <<tables, generation, generationView, locked, lockView,
                   program, step, observed, candidate, snapshot, results>>

Grow(thread) ==
    /\ pc[thread] = "grow"
    /\ LET shard == Shard(thread)
           current == observed[thread]
           table == tables[shard][current]
           copied == IF Mutant = "empty_grow" THEN {}
                     ELSE {[tag |-> slot.tag, id |-> slot.id, view |-> {}] : slot \in table}
       IN IF Cardinality(table) > Capacity(current)
          THEN /\ tables' = [tables EXCEPT ![shard] = Append(@, copied)]
               /\ generation' = [generation EXCEPT ![shard] = current + 1]
               /\ generationView' = [generationView EXCEPT ![shard] =
                                       IF Mutant = "relaxed_generation" THEN {}
                                       ELSE view[thread]]
          ELSE UNCHANGED <<tables, generation, generationView>>
    /\ pc' = [pc EXCEPT ![thread] = "unlock"]
    /\ UNCHANGED <<arena, locked, lockView, view, program, step, observed,
                   candidate, snapshot, results>>

InsertUnlock(thread) ==
    /\ pc[thread] = "unlock"
    /\ Unlock(thread, view[thread])
    /\ Return(thread, candidate[thread], view[thread])
    /\ UNCHANGED <<arena, tables, generation, generationView, program, observed>>

Init ==
    /\ arena = <<>>
    /\ tables = [shard \in Shards |-> <<{}>>]
    /\ generation = [shard \in Shards |-> 1]
    /\ generationView = [shard \in Shards |-> {}]
    /\ locked = [shard \in Shards |-> FALSE]
    /\ lockView = [shard \in Shards |-> {}]
    /\ view = [thread \in Threads |-> {}]
    /\ pc = [thread \in Threads |-> "idle"]
    /\ program \in [Threads -> [1..OperationsPerThread -> Operations]]
    /\ step = [thread \in Threads |-> 1]
    /\ observed = [thread \in Threads |-> 1]
    /\ candidate = [thread \in Threads |-> NoId]
    /\ snapshot = [thread \in Threads |-> {}]
    /\ results = {}

Finished ==
    /\ \A thread \in Threads : pc[thread] = "done"
    /\ UNCHANGED variables

Next ==
    \/ \E thread \in Threads :
           \/ LoadTable(thread)
           \/ Probe(thread)
           \/ Lock(thread)
           \/ LockedProbe(thread)
           \/ ArenaPush(thread)
           \/ StoreSlot(thread)
           \/ LateWrite(thread)
           \/ Grow(thread)
           \/ InsertUnlock(thread)
    \/ Finished

Spec == Init /\ [][Next]_variables

Issued == {result \in results : result.id # NoId}

EqualValuesShareId ==
    \A left, right \in Issued : left.value = right.value => left.id = right.id

DistinctValuesNeverShareId ==
    \A left, right \in Issued : left.value # right.value => left.id # right.id

IdNamesItsValue ==
    \A result \in Issued : arena[result.id].value = result.value

GetFindsReturnedInterns ==
    \A result \in results :
        result.kind = "get" /\ result.before # {} => result.id \in result.before

ReturnedIdsArePublished ==
    \A result \in Issued : result.published

ArenaReadsArePublished ==
    \A thread \in Threads : pc[thread] # "unreachable"

TablesAreCoherent ==
    \A shard \in Shards :
        \A index \in DOMAIN tables[shard] :
            \A slot, other \in tables[shard][index] :
                /\ Location[arena[slot.id].value] = [shard |-> shard, tag |-> slot.tag]
                /\ other.id # slot.id => arena[other.id].value # arena[slot.id].value

CurrentTableHoldsInterns ==
    \A result \in Issued :
        LET shard == Location[result.value].shard
        IN result.id \in {slot.id : slot \in tables[shard][generation[shard]]}

ViewsHoldOnlyWrittenSlots ==
    \A slot \in UNION ({view[thread] : thread \in Threads}
                       \cup {lockView[shard] : shard \in Shards}
                       \cup {generationView[shard] : shard \in Shards}
                       \cup {entry.view : entry \in
                                UNION {UNION {tables[shard][index] :
                                                 index \in DOMAIN tables[shard]} :
                                          shard \in Shards}}) :
        arena[slot].active

=============================================================================
