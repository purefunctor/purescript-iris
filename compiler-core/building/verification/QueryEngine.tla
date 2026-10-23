---------------------------- MODULE QueryEngine ----------------------------
EXTENDS Naturals, Sequences, FiniteSets

CONSTANTS
    Snapshots,
    Queries,
    Payloads,
    NoSnapshot,
    MaxWrites,
    MaxPanics,
    Mutation

ASSUME Mutation \in {"None", "TwoOwners", "CleanupIgnoresOwnership", "EdgeAlwaysAdded",
                     "AddEdgeChecksOneHop", "EagerCycle", "EdgeNotRemoved", "WaitersNotWoken",
                     "ChangedAlwaysPrevious", "VerifyIgnoresInputs", "PanicSkipsCleanup",
                     "UnverifiablePropagates", "VerifyingDemandBlocks", "CleanupDropsWaiters"}

VARIABLES
    content,
    inputChanged,
    revision,
    cancelled,
    writer,
    writes,
    panics,
    state,
    graph,
    promise,
    stack,
    ret,
    alive,
    retired,
    cancelSeen,
    outcome

engine == <<state, graph, promise>>
threads == <<stack, ret, alive, retired, cancelSeen, outcome>>
inputs == <<content, inputChanged, revision, cancelled, writer, writes, panics>>
vars == <<inputs, engine, threads>>

Max(a, b) == IF a >= b THEN a ELSE b
Range(sequence) == {sequence[i] : i \in DOMAIN sequence}
RemoveAt(sequence, i) == SubSeq(sequence, 1, i - 1) \o SubSeq(sequence, i + 1, Len(sequence))

Weight(payload) == IF payload = 0 THEN 0 ELSE 1
Imports(q) == {<<>>} \cup {<<r>> : r \in Queries \ {q}}
Contents(q) == [imports : Imports(q), payload : Payloads]

InputKey(q) == [kind |-> "input", q |-> q]
DerivedKey(q) == [kind |-> "derived", q |-> q]

Ok(value) == [ok |-> TRUE, value |-> value, error |-> "none"]
Err(error) == [ok |-> FALSE, value |-> {}, error |-> error]

RECURSIVE Eval(_, _, _)
Eval(inputContent, q, visiting) ==
    IF q \in visiting
    THEN [cycle |-> TRUE, value |-> {}]
    ELSE LET children == {Eval(inputContent, r, visiting \cup {q}) : r \in Range(inputContent[q].imports)}
         IN IF \E child \in children : child.cycle
            THEN [cycle |-> TRUE, value |-> {}]
            ELSE [cycle |-> FALSE,
                  value |-> {<<q, Weight(inputContent[q].payload)>>}
                            \cup UNION {child.value : child \in children}]

NotComputed ==
    [kind |-> "NotComputed", owner |-> NoSnapshot, waiters |-> {},
     value |-> {}, built |-> 0, changed |-> 0, deps |-> <<>>]
InProgress(owner) ==
    [kind |-> "InProgress", owner |-> owner, waiters |-> {},
     value |-> {}, built |-> 0, changed |-> 0, deps |-> <<>>]
Computed(value, built, changed, deps) ==
    [kind |-> "Computed", owner |-> NoSnapshot, waiters |-> {},
     value |-> value, built |-> built, changed |-> changed, deps |-> deps]

NoPromise == [kind |-> "None", value |-> {}]
EmptyPromise == [kind |-> "Empty", value |-> {}]
FullPromise(value) == [kind |-> "Full", value |-> value]
DeadPromise == [kind |-> "Dead", value |-> {}]
OwnedPromise == [kind |-> "Owned", value |-> {}]

NewFrame(q) ==
    [q |-> q, pc |-> "enter", owner |-> FALSE, verifying |-> FALSE,
     deps |-> <<>>, todo |-> <<>>, current |-> DerivedKey(q), latest |-> 0,
     hasPrevious |-> FALSE, previousValue |-> {}, previousBuilt |-> 0,
     previousChanged |-> 0, oldDeps |-> <<>>, acc |-> {}, payload |-> 0]

NoOutcome ==
    [q |-> NoSnapshot, result |-> Err("none"),
     expected |-> [cycle |-> FALSE, value |-> {}], cancelSeen |-> FALSE]

Top(s) == stack[s][Len(stack[s])]

Record(frame, key) ==
    IF frame.verifying \/ key \in Range(frame.deps) THEN frame.deps ELSE Append(frame.deps, key)

SetTop(s, frame) == stack' = [stack EXCEPT ![s][Len(stack[s])] = frame]

Call(s, parent, q) ==
    LET recorded == [parent EXCEPT !.deps = Record(parent, DerivedKey(q))]
    IN stack' = [stack EXCEPT ![s] = Append([@ EXCEPT ![Len(stack[s])] = recorded], NewFrame(q))]

ReturnOn(base, s, result) ==
    LET rest == SubSeq(stack[s], 1, Len(stack[s]) - 1)
        q == Top(s).q
    IN IF rest = <<>>
       THEN /\ stack' = [base EXCEPT ![s] = <<>>]
            /\ alive' = [alive EXCEPT ![s] = FALSE]
            /\ outcome' = [outcome EXCEPT ![s] =
                   [q |-> q, result |-> result, expected |-> Eval(content, q, {}),
                    cancelSeen |-> cancelSeen[s]]]
            /\ UNCHANGED ret
       ELSE /\ stack' = [base EXCEPT ![s] = rest]
            /\ ret' = [ret EXCEPT ![s] = result]
            /\ UNCHANGED <<alive, outcome>>

Return(s, result) == ReturnOn(stack, s, result)

RECURSIVE Reaches(_, _, _)
Reaches(from, target, fuel) ==
    IF from = target THEN TRUE
    ELSE IF fuel = 0 \/ graph[from] = NoSnapshot THEN FALSE
    ELSE Reaches(graph[from], target, fuel - 1)

AddEdgeRejects(from, to) ==
    /\ Mutation # "EdgeAlwaysAdded"
    /\ \/ Reaches(to, from, IF Mutation = "AddEdgeChecksOneHop" THEN 1 ELSE Cardinality(Snapshots))
       \/ Mutation = "EagerCycle" /\ graph[to] # NoSnapshot

Release(q, resolved, self) ==
    LET waiters == state[q].waiters
        owner == state[q].owner
    IN /\ promise' = [w \in Snapshots |->
                          IF w \in waiters THEN resolved
                          ELSE IF w = self THEN NoPromise
                          ELSE promise[w]]
       /\ graph' = [w \in Snapshots |->
                        IF w \in waiters /\ graph[w] = owner /\ Mutation # "EdgeNotRemoved"
                        THEN NoSnapshot ELSE graph[w]]

FulfillAndStore(s, q, value, built, changed, deps) ==
    /\ Release(q, FullPromise(value), s)
    /\ state' = [state EXCEPT ![q] = Computed(value, built, changed, deps)]

HandOff(q, heir, self) ==
    LET waiters == state[q].waiters
        owner == state[q].owner
    IN /\ state' = [state EXCEPT ![q] = [InProgress(heir) EXCEPT !.waiters = waiters \ {heir}]]
       /\ promise' = [w \in Snapshots |->
                          IF w = heir THEN OwnedPromise
                          ELSE IF w = self THEN NoPromise
                          ELSE promise[w]]
       /\ graph' = [w \in Snapshots |->
                        IF w \in waiters /\ graph[w] = owner /\ Mutation # "EdgeNotRemoved"
                        THEN IF w = heir THEN NoSnapshot ELSE heir
                        ELSE graph[w]]

MarkOwner(heir) == [stack EXCEPT ![heir][Len(stack[heir])].owner = TRUE]

Fail(s, error) ==
    LET frame == Top(s)
        q == frame.q
        heirs == state[q].waiters \ {s}
    IN IF \/ frame.owner
          \/ Mutation = "CleanupIgnoresOwnership" /\ state[q].kind = "InProgress"
       THEN IF heirs = {} \/ Mutation \in {"WaitersNotWoken", "CleanupDropsWaiters"}
            THEN /\ Release(q, IF Mutation = "WaitersNotWoken" THEN EmptyPromise ELSE DeadPromise, s)
                 /\ state' = [state EXCEPT ![q] = NotComputed]
                 /\ Return(s, Err(error))
            ELSE \E heir \in heirs :
                   /\ HandOff(q, heir, s)
                   /\ ReturnOn(MarkOwner(heir), s, Err(error))
       ELSE /\ promise' = [promise EXCEPT ![s] = NoPromise]
            /\ UNCHANGED <<state, graph>>
            /\ Return(s, Err(error))

Verifying(s) == \E i \in DOMAIN stack[s] : stack[s][i].verifying

CreateFuture(s, frame, owner) ==
    IF Verifying(s) /\ Mutation # "VerifyingDemandBlocks"
    THEN Fail(s, "Unverifiable")
    ELSE IF AddEdgeRejects(s, owner)
    THEN Fail(s, "Cycle")
    ELSE /\ graph' = [graph EXCEPT ![s] = owner]
         /\ state' = [state EXCEPT ![frame.q].waiters = @ \cup {s}]
         /\ promise' = [promise EXCEPT ![s] = EmptyPromise]
         /\ SetTop(s, [frame EXCEPT !.pc = "wait"])
         /\ UNCHANGED <<ret, alive, outcome>>

Claim(s, frame) == state' = [state EXCEPT ![frame.q] = InProgress(s)]

QueryCoreReadPath(s) ==
    LET frame == Top(s)
        current == state[frame.q]
    IN /\ frame.pc = "enter"
       /\ IF cancelled
          THEN Fail(s, "Cancelled")
          ELSE CASE current.kind = "Computed" /\ current.built = revision ->
                        /\ Return(s, Ok(current.value))
                        /\ UNCHANGED engine
                 [] current.kind = "InProgress" ->
                        CreateFuture(s, frame, current.owner)
                 [] OTHER ->
                        /\ SetTop(s, [frame EXCEPT !.pc = "upgrade"])
                        /\ UNCHANGED <<engine, ret, alive, outcome>>

QueryCoreUpgradablePath(s) ==
    LET frame == Top(s)
        current == state[frame.q]
    IN /\ frame.pc = "upgrade"
       /\ CASE current.kind = "NotComputed"
               \/ (Mutation = "TwoOwners" /\ current.kind = "InProgress") ->
                   /\ Claim(s, frame)
                   /\ SetTop(s, [frame EXCEPT !.owner = TRUE, !.pc = "compute"])
                   /\ UNCHANGED <<graph, promise, ret, alive, outcome>>
            [] current.kind = "InProgress" ->
                   CreateFuture(s, frame, current.owner)
            [] current.kind = "Computed" /\ current.built = revision ->
                   /\ Return(s, Ok(current.value))
                   /\ UNCHANGED engine
            [] OTHER ->
                   /\ Claim(s, frame)
                   /\ SetTop(s, [frame EXCEPT
                          !.owner = TRUE, !.pc = "verify", !.verifying = TRUE,
                          !.hasPrevious = TRUE, !.previousValue = current.value,
                          !.previousBuilt = current.built, !.previousChanged = current.changed,
                          !.oldDeps = current.deps, !.todo = current.deps, !.latest = 0])
                   /\ UNCHANGED <<graph, promise, ret, alive, outcome>>

FutureWait(s) ==
    LET frame == Top(s)
    IN /\ frame.pc = "wait"
       /\ promise[s].kind \in {"Full", "Dead", "Owned"}
       /\ CASE promise[s].kind = "Full" ->
                 /\ promise' = [promise EXCEPT ![s] = NoPromise]
                 /\ Return(s, Ok(promise[s].value))
                 /\ UNCHANGED <<state, graph>>
            [] promise[s].kind = "Owned" ->
                 /\ promise' = [promise EXCEPT ![s] = NoPromise]
                 /\ SetTop(s, [frame EXCEPT !.pc = "compute"])
                 /\ UNCHANGED <<state, graph, ret, alive, outcome>>
            [] OTHER -> Fail(s, "Cancelled")

Observe(frame, changed, todo) ==
    [frame EXCEPT !.latest = Max(frame.latest, changed), !.todo = todo]

VerifyCore(s) ==
    LET frame == Top(s)
    IN /\ frame.pc = "verify"
       /\ IF frame.todo = <<>>
          THEN IF frame.previousBuilt >= frame.latest
               THEN /\ FulfillAndStore(s, frame.q, frame.previousValue, revision,
                                       frame.previousChanged, frame.oldDeps)
                    /\ Return(s, Ok(frame.previousValue))
               ELSE /\ SetTop(s, [frame EXCEPT !.pc = "compute", !.verifying = FALSE])
                    /\ UNCHANGED <<engine, ret, alive, outcome>>
          ELSE \E i \in DOMAIN frame.todo :
                 LET dependency == frame.todo[i]
                     rest == RemoveAt(frame.todo, i)
                     traced == state[dependency.q]
                 IN /\ UNCHANGED <<engine, ret, alive, outcome>>
                    /\ IF dependency.kind = "input"
                       THEN SetTop(s, Observe(frame,
                                IF Mutation = "VerifyIgnoresInputs" THEN 0
                                ELSE inputChanged[dependency.q], rest))
                       ELSE IF traced.kind = "Computed" /\ traced.built = revision
                       THEN SetTop(s, Observe(frame, traced.changed, rest))
                       ELSE Call(s, [frame EXCEPT !.pc = "verify_return", !.current = dependency,
                                                  !.todo = rest], dependency.q)

VerifyCoreReturn(s) ==
    LET frame == Top(s)
        traced == state[frame.current.q]
    IN /\ frame.pc = "verify_return"
       /\ IF ~ret[s].ok /\ ret[s].error = "Unverifiable" /\ Mutation # "UnverifiablePropagates"
          THEN /\ SetTop(s, [frame EXCEPT !.pc = "compute", !.verifying = FALSE])
               /\ UNCHANGED <<engine, ret, alive, outcome>>
          ELSE IF ~ret[s].ok
          THEN Fail(s, ret[s].error)
          ELSE /\ SetTop(s, [Observe(frame, IF traced.kind = "Computed" THEN traced.changed ELSE 0,
                                     frame.todo) EXCEPT !.pc = "verify"])
               /\ UNCHANGED <<engine, ret, alive, outcome>>

ComputeCore(s) ==
    LET frame == Top(s)
        imports == content[frame.q].imports
    IN /\ frame.pc = "compute"
       /\ IF cancelled
          THEN Fail(s, "Cancelled")
          ELSE /\ SetTop(s, [frame EXCEPT
                     !.pc = "compute_children",
                     !.deps = Record(frame, InputKey(frame.q)),
                     !.payload = content[frame.q].payload,
                     !.todo = [i \in DOMAIN imports |-> DerivedKey(imports[i])],
                     !.acc = {}])
               /\ UNCHANGED <<engine, ret, alive, outcome>>

ComputeChildren(s) ==
    LET frame == Top(s)
        computed == {<<frame.q, Weight(frame.payload)>>} \cup frame.acc
    IN /\ frame.pc = "compute_children"
       /\ IF frame.todo = <<>>
          THEN IF frame.hasPrevious /\ computed = frame.previousValue
               THEN /\ FulfillAndStore(s, frame.q, frame.previousValue, revision,
                                       frame.previousChanged, frame.deps)
                    /\ Return(s, Ok(frame.previousValue))
               ELSE /\ FulfillAndStore(s, frame.q, computed, revision,
                                       IF Mutation = "ChangedAlwaysPrevious" /\ frame.hasPrevious
                                       THEN frame.previousChanged ELSE revision,
                                       frame.deps)
                    /\ Return(s, Ok(computed))
          ELSE /\ Call(s, [frame EXCEPT !.pc = "compute_return", !.current = Head(frame.todo)],
                       Head(frame.todo).q)
               /\ UNCHANGED <<engine, ret, alive, outcome>>

ComputeChildrenReturn(s) ==
    LET frame == Top(s)
    IN /\ frame.pc = "compute_return"
       /\ IF ~ret[s].ok
          THEN Fail(s, ret[s].error)
          ELSE /\ SetTop(s, [frame EXCEPT !.pc = "compute_children",
                                          !.acc = @ \cup ret[s].value,
                                          !.todo = Tail(@)])
               /\ UNCHANGED <<engine, ret, alive, outcome>>

ThreadStep(s) ==
    /\ alive[s]
    /\ stack[s] # <<>>
    /\ \/ QueryCoreReadPath(s)
       \/ QueryCoreUpgradablePath(s)
       \/ FutureWait(s)
       \/ VerifyCore(s)
       \/ VerifyCoreReturn(s)
       \/ ComputeCore(s)
       \/ ComputeChildren(s)
       \/ ComputeChildrenReturn(s)
    /\ UNCHANGED <<inputs, retired, cancelSeen>>

UnwindCleanup(s) ==
    LET owned == {stack[s][i].q : i \in {j \in DOMAIN stack[s] : stack[s][j].owner}}
        Heirs(q) == IF state[q].waiters = {} THEN {NoSnapshot} ELSE state[q].waiters
    IN \E heir \in [owned -> Snapshots \cup {NoSnapshot}] :
         /\ \A q \in owned : heir[q] \in Heirs(q)
         /\ LET heirs == {heir[q] : q \in owned} \ {NoSnapshot}
                Waited(w) == CHOOSE q \in owned : w \in state[q].waiters
            IN /\ state' = [q \in Queries |->
                               IF q \notin owned THEN state[q]
                               ELSE IF heir[q] = NoSnapshot THEN NotComputed
                               ELSE [InProgress(heir[q]) EXCEPT !.waiters = state[q].waiters \ {heir[q]}]]
               /\ promise' = [w \in Snapshots |-> IF w \in heirs THEN OwnedPromise ELSE promise[w]]
               /\ graph' = [w \in Snapshots |->
                               IF graph[w] = s /\ \E q \in owned : w \in state[q].waiters
                               THEN IF w \in heirs THEN NoSnapshot ELSE heir[Waited(w)]
                               ELSE graph[w]]
               /\ stack' = [w \in Snapshots |->
                               IF w = s THEN <<>>
                               ELSE IF w \in heirs
                               THEN [stack[w] EXCEPT ![Len(stack[w])].owner = TRUE]
                               ELSE stack[w]]

ComputePanics(s) ==
    /\ panics < MaxPanics
    /\ alive[s]
    /\ stack[s] # <<>>
    /\ Top(s).pc = "compute_children"
    /\ alive' = [alive EXCEPT ![s] = FALSE]
    /\ panics' = panics + 1
    /\ outcome' = [outcome EXCEPT ![s] = NoOutcome]
    /\ IF Mutation = "PanicSkipsCleanup"
       THEN /\ retired' = [retired EXCEPT ![s] = TRUE]
            /\ stack' = [stack EXCEPT ![s] = <<>>]
            /\ UNCHANGED engine
       ELSE /\ UnwindCleanup(s)
            /\ UNCHANGED retired
    /\ UNCHANGED <<content, inputChanged, revision, cancelled, writer, writes, ret, cancelSeen>>

TakeSnapshot(s) ==
    /\ ~alive[s]
    /\ ~retired[s]
    /\ \E q \in Queries :
         /\ stack' = [stack EXCEPT ![s] = <<NewFrame(q)>>]
         /\ alive' = [alive EXCEPT ![s] = TRUE]
         /\ cancelSeen' = [cancelSeen EXCEPT ![s] = cancelled]
         /\ outcome' = [outcome EXCEPT ![s] = NoOutcome]
    /\ UNCHANGED <<inputs, engine, ret, retired>>

RequestWrite ==
    /\ writer = "idle"
    /\ writes < MaxWrites
    /\ cancelled' = TRUE
    /\ writer' = "waiting"
    /\ cancelSeen' = [s \in Snapshots |-> cancelSeen[s] \/ alive[s]]
    /\ UNCHANGED <<content, inputChanged, revision, writes, panics,
                   engine, stack, ret, alive, retired, outcome>>

CommitWrite ==
    /\ writer = "waiting"
    /\ \A s \in Snapshots : ~alive[s]
    /\ \E q \in Queries :
         \E written \in Contents(q) \ {content[q]} :
           /\ content' = [content EXCEPT ![q] = written]
           /\ inputChanged' = [inputChanged EXCEPT ![q] = revision + 1]
    /\ revision' = revision + 1
    /\ cancelled' = FALSE
    /\ writer' = "idle"
    /\ writes' = writes + 1
    /\ UNCHANGED <<panics, engine, threads>>

CancelWithoutWrite ==
    /\ writer = "waiting"
    /\ \A s \in Snapshots : ~alive[s]
    /\ cancelled' = FALSE
    /\ writer' = "idle"
    /\ writes' = writes + 1
    /\ UNCHANGED <<content, inputChanged, revision, panics, engine, threads>>

Init ==
    /\ content \in {initial \in [Queries -> UNION {Contents(q) : q \in Queries}] :
                       \A q \in Queries : initial[q] \in Contents(q)}
    /\ inputChanged = [q \in Queries |-> 0]
    /\ revision = 0
    /\ cancelled = FALSE
    /\ writer = "idle"
    /\ writes = 0
    /\ panics = 0
    /\ state = [q \in Queries |-> NotComputed]
    /\ graph = [s \in Snapshots |-> NoSnapshot]
    /\ promise = [s \in Snapshots |-> NoPromise]
    /\ stack = [s \in Snapshots |-> <<>>]
    /\ ret = [s \in Snapshots |-> Err("none")]
    /\ alive = [s \in Snapshots |-> FALSE]
    /\ retired = [s \in Snapshots |-> FALSE]
    /\ cancelSeen = [s \in Snapshots |-> FALSE]
    /\ outcome = [s \in Snapshots |-> NoOutcome]

Next ==
    \/ \E s \in Snapshots : TakeSnapshot(s) \/ ThreadStep(s) \/ ComputePanics(s)
    \/ RequestWrite
    \/ CommitWrite
    \/ CancelWithoutWrite

Spec ==
    /\ Init
    /\ [][Next]_vars
    /\ \A s \in Snapshots : WF_vars(ThreadStep(s))

OwnerFrame(s, q) == \E i \in DOMAIN stack[s] : stack[s][i].q = q /\ stack[s][i].owner

InProgressHasLiveOwner ==
    \A q \in Queries :
        state[q].kind = "InProgress" => OwnerFrame(state[q].owner, q)

OwnerFramesHoldTheirQuery ==
    \A s \in Snapshots : \A i \in DOMAIN stack[s] :
        stack[s][i].owner =>
            /\ state[stack[s][i].q].kind = "InProgress"
            /\ state[stack[s][i].q].owner = s

WaitEdgesMatchWaiters ==
    \A s \in Snapshots :
        /\ graph[s] # NoSnapshot =>
               \E q \in Queries : /\ state[q].kind = "InProgress"
                                  /\ state[q].owner = graph[s]
                                  /\ s \in state[q].waiters
        /\ \A q \in Queries :
               (state[q].kind = "InProgress" /\ s \in state[q].waiters) => graph[s] = state[q].owner

WaitForGraphIsAcyclic ==
    \A s \in Snapshots : graph[s] # NoSnapshot => ~Reaches(graph[s], s, Cardinality(Snapshots))

BlockedWaitersCanBeWoken ==
    \A s \in Snapshots :
        (stack[s] # <<>> /\ Top(s).pc = "wait") =>
            \/ promise[s].kind \in {"Full", "Dead", "Owned"}
            \/ \E q \in Queries : /\ state[q].kind = "InProgress"
                                  /\ s \in state[q].waiters
                                  /\ alive[state[q].owner]
                                  /\ OwnerFrame(state[q].owner, q)

DroppedSnapshotsLeaveNoTrace ==
    \A s \in Snapshots :
        (~alive[s] /\ ~retired[s]) =>
            /\ graph[s] = NoSnapshot
            /\ \A q \in Queries :
                   state[q].kind = "InProgress" => state[q].owner # s /\ s \notin state[q].waiters

FreshCacheIsCorrect ==
    \A q \in Queries :
        (state[q].kind = "Computed" /\ state[q].built = revision) =>
            LET expected == Eval(content, q, {})
            IN ~expected.cycle /\ state[q].value = expected.value

OkResultsAreCorrect ==
    \A s \in Snapshots :
        outcome[s].result.ok =>
            ~outcome[s].expected.cycle /\ outcome[s].result.value = outcome[s].expected.value

CyclesAreReal ==
    \A s \in Snapshots :
        outcome[s].result.error = "Cycle" => outcome[s].expected.cycle

CancellationsAreExplained ==
    \A s \in Snapshots :
        outcome[s].result.error = "Cancelled" => outcome[s].cancelSeen \/ outcome[s].expected.cycle

UnverifiableStaysInternal ==
    \A s \in Snapshots : outcome[s].result.error # "Unverifiable"

DemandsComplete == \A s \in Snapshots : alive[s] ~> ~alive[s]

=============================================================================
