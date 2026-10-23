--------------------------- MODULE ParallelInterner ---------------------------
EXTENDS Naturals, Sequences, FiniteSets

CONSTANTS Threads, Values, Hash, OperationsPerThread, Mutant

Mutants == {
    "none",
    "trust_hash",
    "skip_occupied_compare",
    "keep_candidate",
    "skip_publish_collision",
    "overwrite_insert",
    "publish_skip_scan",
    "collision_skip_scan",
    "collision_unlocked_scan",
    "get_skip_overflow",
    "get_trust_hash",
    "scan_hash_only",
    "relaxed_table",
    "publish_before_write",
    "return_before_write"
}

ASSUME Mutant \in Mutants
ASSUME Hash \in [Values -> Nat]

NoId == 0

Hashes == {Hash[value] : value \in Values}

Operations == [kind : {"intern", "get"}, value : Values]

VARIABLES
    arena,
    table,
    tableView,
    overflow,
    locked,
    lockView,
    view,
    pc,
    program,
    step,
    candidate,
    snapshot,
    results

variables == <<arena, table, tableView, overflow, locked, lockView,
               view, pc, program, step, candidate, snapshot, results>>

Maximum(set) == CHOOSE element \in set : \A other \in set : element >= other

Value(thread) == program[thread][step[thread]].value

Kind(thread) == program[thread][step[thread]].kind

OverflowMatches(hash, value) ==
    {index \in DOMAIN overflow :
        /\ overflow[index][1] = hash
        /\ \/ arena[overflow[index][2]].value = value
           \/ Mutant = "scan_hash_only"}

ScanOverflow(hash, value) ==
    LET matches == OverflowMatches(hash, value)
    IN IF matches = {} THEN NoId ELSE overflow[Maximum(matches)][2]

ScanOverflowReads(hash, value) ==
    LET matches == OverflowMatches(hash, value)
        stop == IF matches = {} THEN 0 ELSE Maximum(matches)
    IN {overflow[index][2] :
            index \in {index \in DOMAIN overflow :
                          overflow[index][1] = hash /\ index >= stop}}

AcquireSlots(slots, known) == known \cup UNION {arena[slot].view : slot \in slots}

TableAcquire(thread, hash) ==
    IF table[hash] = NoId \/ Mutant = "relaxed_table"
    THEN view[thread]
    ELSE view[thread] \cup tableView[hash]

TablePublish(thread) == IF Mutant = "relaxed_table" THEN {} ELSE view[thread]

Unreachable(thread, known) ==
    /\ pc' = [pc EXCEPT ![thread] = "unreachable"]
    /\ view' = [view EXCEPT ![thread] = known]
    /\ UNCHANGED <<arena, table, tableView, overflow, locked, lockView,
                   program, step, candidate, snapshot, results>>

Advance(thread) ==
    /\ step' = [step EXCEPT ![thread] = @ + 1]
    /\ pc' = [pc EXCEPT ![thread] =
                IF step[thread] = OperationsPerThread THEN "done" ELSE "idle"]
    /\ candidate' = [candidate EXCEPT ![thread] = NoId]
    /\ snapshot' = [snapshot EXCEPT ![thread] = {}]

ReturnIntern(thread, id, known) ==
    /\ results' = results \cup
           {[kind |-> "intern", value |-> Value(thread), id |-> id,
             before |-> {}, published |-> id \in known]}
    /\ view' = [view EXCEPT ![thread] = known]
    /\ Advance(thread)

ReturnGet(thread, id, known, before) ==
    /\ results' = results \cup
           {[kind |-> "get", value |-> Value(thread), id |-> id,
             before |-> before, published |-> id = NoId \/ id \in known]}
    /\ view' = [view EXCEPT ![thread] = known]
    /\ Advance(thread)

InternWithMetadata(thread) ==
    /\ pc[thread] = "idle"
    /\ Kind(thread) = "intern"
    /\ LET value == Value(thread)
           hash == Hash[value]
           id == table[hash]
           known == TableAcquire(thread, hash)
       IN IF id = NoId
          THEN /\ pc' = [pc EXCEPT ![thread] = "push"]
               /\ view' = [view EXCEPT ![thread] = known]
               /\ UNCHANGED <<arena, table, tableView, overflow, locked, lockView,
                              program, step, candidate, snapshot, results>>
          ELSE IF id \notin known
          THEN Unreachable(thread, known)
          ELSE LET seen == AcquireSlots({id}, known)
               IN IF arena[id].value = value \/ Mutant = "trust_hash"
                  THEN /\ ReturnIntern(thread, id, seen)
                       /\ UNCHANGED <<arena, table, tableView, overflow, locked,
                                      lockView, program>>
                  ELSE /\ pc' = [pc EXCEPT ![thread] = "intern_at_collision"]
                       /\ view' = [view EXCEPT ![thread] = seen]
                       /\ UNCHANGED <<arena, table, tableView, overflow, locked,
                                      lockView, program, step, candidate,
                                      snapshot, results>>

ArenaReserve(thread) ==
    /\ pc[thread] = "push"
    /\ arena' = Append(arena, [value |-> Value(thread), active |-> FALSE, view |-> {}])
    /\ candidate' = [candidate EXCEPT ![thread] = Len(arena) + 1]
    /\ pc' = [pc EXCEPT ![thread] =
                IF Mutant \in {"publish_before_write", "return_before_write"}
                THEN "try_insert"
                ELSE "push_write"]
    /\ UNCHANGED <<table, tableView, overflow, locked, lockView, view,
                   program, step, snapshot, results>>

WriteSlot(thread) ==
    LET slot == candidate[thread]
        written == view[thread] \cup {slot}
    IN /\ arena' = [arena EXCEPT ![slot] = [@ EXCEPT !.active = TRUE, !.view = written]]
       /\ view' = [view EXCEPT ![thread] = written]

ArenaWrite(thread) ==
    /\ pc[thread] = "push_write"
    /\ WriteSlot(thread)
    /\ pc' = [pc EXCEPT ![thread] = "try_insert"]
    /\ UNCHANGED <<table, tableView, overflow, locked, lockView,
                   program, step, candidate, snapshot, results>>

ArenaWriteAfterPublish(thread) ==
    /\ pc[thread] = "push_write_late"
    /\ WriteSlot(thread)
    /\ results' = results \cup
           {[kind |-> "intern", value |-> Value(thread), id |-> candidate[thread],
             before |-> {}, published |-> TRUE]}
    /\ Advance(thread)
    /\ UNCHANGED <<table, tableView, overflow, locked, lockView, program>>

TryInsert(thread) ==
    /\ pc[thread] = "try_insert"
    /\ LET slot == candidate[thread]
           hash == Hash[Value(thread)]
           current == table[hash]
           written == arena[slot].active
       IN IF current = NoId
          THEN /\ table' = [table EXCEPT ![hash] = slot]
               /\ tableView' = [tableView EXCEPT ![hash] = TablePublish(thread)]
               /\ IF written \/ Mutant = "return_before_write"
                  THEN /\ ReturnIntern(thread, slot, view[thread])
                       /\ UNCHANGED <<arena, overflow, locked, lockView, program>>
                  ELSE /\ pc' = [pc EXCEPT ![thread] = "push_write_late"]
                       /\ UNCHANGED <<arena, overflow, locked, lockView, view,
                                      program, step, candidate, snapshot, results>>
          ELSE IF ~written
          THEN /\ pc' = [pc EXCEPT ![thread] = "push_write"]
               /\ UNCHANGED <<arena, table, tableView, overflow, locked, lockView,
                              view, program, step, candidate, snapshot, results>>
          ELSE LET known == TableAcquire(thread, hash)
               IN IF current \notin known
                  THEN Unreachable(thread, known)
                  ELSE LET seen == AcquireSlots({current}, known)
                           returned ==
                               CASE Mutant = "skip_occupied_compare" -> current
                                 [] Mutant = "keep_candidate" -> slot
                                 [] Mutant = "skip_publish_collision" ->
                                        IF arena[current].value = arena[slot].value
                                        THEN current ELSE slot
                                 [] arena[current].value = arena[slot].value -> current
                                 [] OTHER -> NoId
                       IN /\ IF Mutant = "overwrite_insert"
                             THEN /\ table' = [table EXCEPT ![hash] = slot]
                                  /\ tableView' = [tableView EXCEPT ![hash] = view[thread]]
                             ELSE UNCHANGED <<table, tableView>>
                          /\ IF returned # NoId
                             THEN /\ ReturnIntern(thread, returned, seen)
                                  /\ UNCHANGED <<arena, overflow, locked, lockView,
                                                 program>>
                             ELSE /\ pc' = [pc EXCEPT ![thread] = "publish_collision"]
                                  /\ view' = [view EXCEPT ![thread] = seen]
                                  /\ UNCHANGED <<arena, overflow, locked, lockView,
                                                 program, step, candidate, snapshot,
                                                 results>>

InternAtCollision(thread) ==
    /\ pc[thread] = "intern_at_collision"
    /\ ~locked
    /\ LET value == Value(thread)
           hash == Hash[value]
           known == view[thread] \cup lockView
           reads == IF Mutant = "collision_skip_scan" THEN {}
                    ELSE ScanOverflowReads(hash, value)
           found == IF Mutant = "collision_skip_scan" THEN NoId
                    ELSE ScanOverflow(hash, value)
       IN IF ~(reads \subseteq known)
          THEN Unreachable(thread, known)
          ELSE LET seen == AcquireSlots(reads, known)
               IN IF found # NoId
                  THEN /\ lockView' = seen
                       /\ ReturnIntern(thread, found, seen)
                       /\ UNCHANGED <<arena, table, tableView, overflow, locked, program>>
                  ELSE /\ IF Mutant = "collision_unlocked_scan"
                          THEN /\ lockView' = seen
                               /\ UNCHANGED locked
                          ELSE /\ locked' = TRUE
                               /\ UNCHANGED lockView
                       /\ pc' = [pc EXCEPT ![thread] = "collision_push"]
                       /\ view' = [view EXCEPT ![thread] = seen]
                       /\ UNCHANGED <<arena, table, tableView, overflow, program,
                                      step, candidate, snapshot, results>>

CollisionPush(thread) ==
    /\ pc[thread] = "collision_push"
    /\ Mutant = "collision_unlocked_scan" => ~locked
    /\ LET value == Value(thread)
           hash == Hash[value]
           known == IF Mutant = "collision_unlocked_scan"
                    THEN view[thread] \cup lockView
                    ELSE view[thread]
           slot == Len(arena) + 1
           written == known \cup {slot}
       IN /\ arena' = Append(arena, [value |-> value, active |-> TRUE, view |-> written])
          /\ overflow' = Append(overflow, <<hash, slot>>)
          /\ locked' = FALSE
          /\ lockView' = written
          /\ ReturnIntern(thread, slot, written)
          /\ UNCHANGED <<table, tableView, program>>

PublishCollision(thread) ==
    /\ pc[thread] = "publish_collision"
    /\ ~locked
    /\ LET slot == candidate[thread]
           hash == Hash[Value(thread)]
           value == arena[slot].value
           known == view[thread] \cup lockView
           reads == IF Mutant = "publish_skip_scan" THEN {}
                    ELSE ScanOverflowReads(hash, value)
           found == IF Mutant = "publish_skip_scan" THEN NoId
                    ELSE ScanOverflow(hash, value)
       IN IF ~(reads \subseteq known)
          THEN Unreachable(thread, known)
          ELSE LET seen == AcquireSlots(reads, known)
               IN /\ lockView' = seen
                  /\ IF found # NoId
                     THEN /\ ReturnIntern(thread, found, seen)
                          /\ UNCHANGED overflow
                     ELSE /\ overflow' = Append(overflow, <<hash, slot>>)
                          /\ ReturnIntern(thread, slot, seen)
                  /\ UNCHANGED <<arena, table, tableView, locked, program>>

Get(thread) ==
    /\ pc[thread] = "idle"
    /\ Kind(thread) = "get"
    /\ LET value == Value(thread)
           hash == Hash[value]
           id == table[hash]
           known == TableAcquire(thread, hash)
           before == {result.id : result \in
                         {result \in results :
                             result.kind = "intern" /\ result.value = value}}
       IN IF id = NoId
          THEN /\ ReturnGet(thread, NoId, known, before)
               /\ UNCHANGED <<arena, table, tableView, overflow, locked, lockView,
                              program>>
          ELSE IF id \notin known
          THEN Unreachable(thread, known)
          ELSE LET seen == AcquireSlots({id}, known)
               IN IF arena[id].value = value \/ Mutant = "get_trust_hash"
                  THEN /\ ReturnGet(thread, id, seen, before)
                       /\ UNCHANGED <<arena, table, tableView, overflow, locked,
                                      lockView, program>>
                  ELSE IF Mutant = "get_skip_overflow"
                  THEN /\ ReturnGet(thread, NoId, seen, before)
                       /\ UNCHANGED <<arena, table, tableView, overflow, locked,
                                      lockView, program>>
                  ELSE /\ pc' = [pc EXCEPT ![thread] = "get_overflow"]
                       /\ view' = [view EXCEPT ![thread] = seen]
                       /\ snapshot' = [snapshot EXCEPT ![thread] = before]
                       /\ UNCHANGED <<arena, table, tableView, overflow, locked,
                                      lockView, program, step, candidate, results>>

GetOverflow(thread) ==
    /\ pc[thread] = "get_overflow"
    /\ ~locked
    /\ LET value == Value(thread)
           hash == Hash[value]
           known == view[thread] \cup lockView
           reads == ScanOverflowReads(hash, value)
       IN IF ~(reads \subseteq known)
          THEN Unreachable(thread, known)
          ELSE LET seen == AcquireSlots(reads, known)
               IN /\ lockView' = seen
                  /\ ReturnGet(thread, ScanOverflow(hash, value), seen, snapshot[thread])
                  /\ UNCHANGED <<arena, table, tableView, overflow, locked, program>>

Init ==
    /\ arena = <<>>
    /\ table = [hash \in Hashes |-> NoId]
    /\ tableView = [hash \in Hashes |-> {}]
    /\ overflow = <<>>
    /\ locked = FALSE
    /\ lockView = {}
    /\ view = [thread \in Threads |-> {}]
    /\ pc = [thread \in Threads |-> "idle"]
    /\ program \in [Threads -> [1..OperationsPerThread -> Operations]]
    /\ step = [thread \in Threads |-> 1]
    /\ candidate = [thread \in Threads |-> NoId]
    /\ snapshot = [thread \in Threads |-> {}]
    /\ results = {}

Finished ==
    /\ \A thread \in Threads : pc[thread] = "done"
    /\ UNCHANGED variables

Next ==
    \/ \E thread \in Threads :
           \/ InternWithMetadata(thread)
           \/ ArenaReserve(thread)
           \/ ArenaWrite(thread)
           \/ ArenaWriteAfterPublish(thread)
           \/ TryInsert(thread)
           \/ InternAtCollision(thread)
           \/ CollisionPush(thread)
           \/ PublishCollision(thread)
           \/ Get(thread)
           \/ GetOverflow(thread)
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

OverflowIsCoherent ==
    \A index \in DOMAIN overflow :
        LET hash == overflow[index][1]
            slot == overflow[index][2]
        IN /\ table[hash] # NoId
           /\ arena[slot].value # arena[table[hash]].value
           /\ \A other \in DOMAIN overflow :
                  other # index /\ overflow[other][1] = hash =>
                      arena[overflow[other][2]].value # arena[slot].value

TableIsWriteOnce ==
    [][\A hash \in Hashes : table[hash] # NoId => table'[hash] = table[hash]]_variables

ViewsHoldOnlyWrittenSlots ==
    \A slot \in UNION ({view[thread] : thread \in Threads}
                       \cup {tableView[hash] : hash \in Hashes}
                       \cup {lockView}) :
        arena[slot].active

=============================================================================
