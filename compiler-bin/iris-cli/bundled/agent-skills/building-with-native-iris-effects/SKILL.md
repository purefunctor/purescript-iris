---
name: building-with-native-iris-effects
description: "Guides agents writing Iris applications and libraries with native Sync and Async effects, typed Abort handling, JavaScript FFI, fibers, and interruption-safe resources. Use when working with Iris.Effect, Iris.Effect.Sync, Iris.Effect.Async, or Prim.Effect."
---

# Building with native Iris effects

Use this skill to write Iris application and library code with the native effect system. Do not
use it as a guide to compiler internals.

## Choose the computation type

```purescript
import Iris.Effect (Async, Fiber, Promise, Sync)
import Iris.Effect.Async as Async
import Iris.Effect.Compat as Compat
import Iris.Effect.Sync as Sync
import Prim.Effect (Abort)
import Prelude
```

The computation types take the effect set first and the result value last:

```purescript
Sync effects value
Async effects value
Fiber value
```

- Choose `Sync` for work that completes synchronously without suspension.
- Choose `Async` for callbacks, cooperative scheduling, fibers, interruption, and
  interruption-safe resource management.
- Use `Async.lift` to execute a `Sync` action inside `Async` while preserving its effects.
- There is no synchronous operation for waiting on an `Async` computation.
- Native `Sync` and `Async` are not the ecosystem's `Effect` and `Aff`. Use the explicit
  compatibility operations below at their boundaries.

Use qualified `Sync.do` and `Async.do` notation. Do not assume ordinary Prelude `Monad`
operations sequence these computations.

## Effect sets and compiler evidence

Effect sets have kind `Effects`. Their members have kind `Type`:

```purescript
[]
[Abort DatabaseError]
[Abort DatabaseError, Abort ValidationError]
[Abort DatabaseError | effects]
```

The final form has an open tail named `effects`. Effect sets are not value arrays or labeled
record rows. Ordering is immaterial and duplicates normalize away.

A declaration is an upper bound on permitted effects. An expression requiring fewer effects can
inhabit a declaration allowing more:

```purescript
newtype DatabaseError = DatabaseError Int
newtype ValidationError = ValidationError Int

possibleFailures :: Async [Abort DatabaseError, Abort ValidationError] Int
possibleFailures = Async.abort (ValidationError 13)
```

`[]` means that the computation has no tracked native effect requirements. It does not make a
dishonest foreign implementation safe, prevent JavaScript defects, or prevent fiber interruption.

Generic library signatures can import the compiler-solved classes from `Prim.Effect`:

| Constraint | Meaning |
|---|---|
| `Union left right combined` | `combined` is the union of both sets. |
| `Remove effect input remaining` | `remaining` is `input` without `effect`. |
| `Subset required allowed` | Every required effect is permitted by `allowed`. |
| `AbortIdentity error` | Supplies the type identity for selective abort handling. |
| `Runnable effects` | A closed set contains no unhandled `Abort` effect and may start a fiber. |

`Union` determines its third argument from its first two. `Remove` determines its third argument
from the effect and input set.

Let Iris solve these constraints. A generic helper may need to retain them in its signature, but
application code must not write instances, construct dictionaries, fabricate abort identities, or
coerce away an effect requirement. `Subset` proves permission; it does not execute or handle an
effect.

## Sequence computations

Both `Sync` and `Async` export the same basic combinators:

| Operation | Argument order and behavior |
|---|---|
| `pure value` | Return `value` with effect set `[]`. |
| `bind action continuation` | Run `action`, pass its result to `continuation`, and union their effects. |
| `discard action continuation` | Sequence an ignored result and union both effects. |
| `map function action` | Transform the result without changing the effect set. |
| `apply functionAction argumentAction` | Run the function action first, then the argument action. |
| `abort error` | Abort with requirement `[Abort error]`. |
| `catchAbort program handler` | Handle one abort type and preserve every other requirement. |

`apply` is sequential for `Async`; it does not start work in parallel. A discarded action does not
need to return `{}`.

```purescript
module Main where

import Iris.Effect (Async, Fiber, Sync)
import Iris.Effect.Async as Async
import Iris.Effect.Sync as Sync
import Prelude

foreign import mark :: String -> Int -> Sync [] Int
foreign import delayed :: Int -> Async [] Int

syncProgram :: Sync [] Int
syncProgram = Sync.do
  first <- mark "first" 20
  second <- mark "second" 22
  Sync.pure (first + second)

asyncProgram :: Async [] Int
asyncProgram = Async.do
  value <- Async.lift syncProgram
  Async.yield
  delayed value

main :: Sync [] (Fiber Int)
main = Async.run asyncProgram
```

## Handle typed aborts selectively

`Abort error` is identified by its error type, not its message, payload, or JavaScript
representation.

```purescript
newtype DatabaseError = DatabaseError Int
newtype ValidationError = ValidationError Int

syncFailure :: Sync [Abort DatabaseError] Int
syncFailure = Sync.abort (DatabaseError 7)

syncRecovered :: Sync [] Int
syncRecovered =
  Sync.catchAbort syncFailure \(DatabaseError code) -> Sync.pure code

asyncFailures :: Async [Abort DatabaseError, Abort ValidationError] Int
asyncFailures = Async.abort (ValidationError 13)

asyncPartiallyHandled :: Async [Abort ValidationError] Int
asyncPartiallyHandled =
  Async.catchAbort asyncFailures \(DatabaseError _) -> Async.pure 0

asyncRecovered :: Async [] Int
asyncRecovered =
  Async.catchAbort asyncPartiallyHandled \(ValidationError code) ->
    Async.pure code
```

The database handler does not intercept the validation abort. The final result is `13`, not `0`.

For both computation types, `catchAbort`:

1. Takes the program first and the handler second.
2. Selects the handled error from the handler's argument type.
3. Removes `Abort error` from the program's effect set.
4. Unions the remaining effects with the handler's effects.
5. Requires the handler and program to return the same result type.

Annotate the handler argument when the error type would otherwise be ambiguous. Catching an absent
type in a closed set is allowed and does not catch an unrelated abort. Effects introduced by the
handler remain in the output type. An abort raised by the handler is not caught again by the same
`catchAbort`.

Distinct newtypes retain distinct abort identities even when their runtime payloads match. Type
synonyms do not create nominally distinct identities; use a newtype when errors must be distinct.
Concrete structural types, including closed records, are supported.

A helper polymorphic in its error must retain `AbortIdentity error`. Do not expect a concrete
identity for an unconstrained type variable.

JavaScript exceptions are defects, not typed aborts. `catchAbort` does not catch arbitrary thrown
exceptions or interruption. Lifting a native `Sync.abort` does preserve its typed abort behavior.

## Lift ecosystem Effect operations

`Iris.Effect.Compat` is an opt-in bridge to the ecosystem's thunk-based `Effect` type:

```purescript
Compat.liftEffect
  :: forall value
   . Effect value
  -> Sync [] value

Compat.liftEffectAs
  :: forall @effect value
   . Effect value
  -> Sync [effect] value
```

Both operations preserve laziness: the `Effect` thunk runs only when the resulting `Sync` action
runs. `liftEffect` deliberately records no native requirement. Use `liftEffectAs` when defining a
tracked library operation, and define the effect label in that ordinary library module:

```purescript
module Console (Console, log) where

import Effect.Console as Effect.Console
import Iris.Effect (Sync)
import Iris.Effect.Compat as Compat
import Prelude

foreign import data Console :: Type

log :: String -> Sync [Console] Unit
log message = Compat.liftEffectAs @Console (Effect.Console.log message)
```

The type application classifies an operation; it does not inspect its implementation or convert
exceptions into typed aborts. Do not claim a narrower effect than the operation actually requires.
Iris intentionally does not ship capability-specific modules such as this example; libraries own
their labels and APIs.

## Construct asynchronous computations

```purescript
Async.lift
  :: forall effects value
   . Sync effects value
  -> Async effects value

Async.defer
  :: forall effects value
   . ({} -> Async effects value)
  -> Async effects value

Async.yield :: Async [] {}

Async.fromPromise
  :: forall effects value
   . Sync effects (Promise value)
  -> Async effects value
```

- `lift` runs the synchronous thunk when execution reaches it.
- `defer` delays construction of the next computation. Its factory receives `{}` and does not
  start another fiber.
- `yield` is an action value. Write `Async.yield`, not `Async.yield {}`.
- `fromPromise` runs its `Sync` factory when reached, immediately attaches settlement handlers,
  and suspends until the Promise settles.
- Async descriptions are reusable. Each `Async.run` starts a fresh execution.

Prefer passing a delayed Promise factory rather than an already-running Promise:

```purescript
foreign import request :: Effect (Promise Response)

response :: Async [] Response
response = Async.fromPromise (Compat.liftEffect request)
```

A rejected Promise is an untyped defect. Interruption stops waiting and ignores later settlement;
it cannot cancel the Promise or its underlying work. If the host operation supports cancellation,
use `Async.register` and return a real cancellation action instead.

Use `defer` to avoid eagerly constructing recursive computations:

```purescript
deep :: Int -> Int -> Async [] Int
deep remaining total =
  if remaining == 0 then Async.pure total
  else Async.defer \_ -> deep (remaining - 1) (total + 1)
```

The interpreter is stack-safe for deferred computation steps and periodically yields to the host.
It cannot preempt a long JavaScript call, synchronous loop, registration body, or lifted `Sync`
action. Split long work into deferred steps and yield where appropriate.

## Register callbacks through JavaScript FFI

The exact registration type is:

```purescript
Async.register
  :: forall effects value
   . ((Async effects value -> Sync [] {}) -> Sync [] (Async [] {}))
  -> Async effects value
```

Read it from the inside out:

- `resume` accepts an `Async` computation, not a bare result.
- `resume program` returns a `Sync [] {}` action that must be executed.
- The registration function returns a synchronous action that installs the callback and returns an
  asynchronous cancellation action.
- The cancellation action has type `Async [] {}`.

In PureScript, execute `resume program` within `Sync.do`. In JavaScript, execute the returned thunk
with `resume(program)()`.

The foreign definitions for the earlier example are:

```javascript
import * as runtime from "../runtime.js";

export const mark = (label) => (value) => () => {
  console.log(label);
  return value;
};

export const delayed = (value) =>
  runtime.asyncRegister((resume) => () => {
    const timeout = setTimeout(
      () => resume(runtime.asyncPure(value))(),
      0,
    );

    return runtime.asyncLift(() => {
      clearTimeout(timeout);
      return {};
    });
  });
```

- PureScript arguments are curried.
- `Sync effects value` is represented by a zero-argument JavaScript thunk. Put synchronous side
  effects inside that final thunk.
- `Async effects value` is a runtime computation description, not a Promise or a thunk returning a
  Promise.
- Registration side effects belong inside `(resume) => () => { ... }`, not at module initialization
  or computation construction.
- Return the cancellation description. Do not execute it during registration or return a raw
  JavaScript cleanup function.
- APIs specifying `{}` require an empty-record result, not Prelude `Unit`.

`../runtime.js` is the runtime emitted beside generated modules in the current output layout. Use
the runtime from the same Iris build. Do not construct instruction objects, depend on generated
dictionary layouts, copy runtime internals, or manufacture abort identity strings.

Registration is one-shot. Synchronous completion is supported; the first accepted resume wins;
duplicate and stale resumes are ignored. Normal completion discards the cancellation action without
running it. Interruption while suspended invalidates the callback and runs cancellation before
unwinding enclosing resources.

A cancellation action is not an always-run finalizer. If a host API retains resources after
successful completion, clean them up on success or transfer their ownership to `bracket`.

Do not throw from a host callback to report a typed failure. Resume with a computation constructed
through the typed PureScript API.

## Start, join, and interrupt fibers

```purescript
Async.run
  :: forall effects value
   . Runnable effects
  => Async effects value
  -> Sync effects (Fiber value)
Async.join :: forall value. Fiber value -> Async [] value
Async.interrupt :: forall value. Fiber value -> Async [] {}
```

- `run` preserves ordinary capability requirements in the returned `Sync` action.
- `Runnable` is compiler-solved for closed effect sets without `Abort`. Handle every typed abort
  inside the `Async` program before starting its fiber.
- Executing the `Sync` action returned by `run` starts a fresh fiber. It does not wait for the
  result, and synchronous work may run before the action returns.
- `join` waits for success and returns the target value. A target defect or interruption propagates
  to the joining computation.
- Interrupting a joiner stops that wait; it does not interrupt the target.
- `interrupt` requests target interruption and waits for cleanup. It returns `{}` rather than the
  target result or failure.
- Constructing `run`, `join`, or `interrupt` does not execute it.

Keep ownership of every started fiber. There is currently no automatic parent-child supervision.

From a JavaScript host, observe a root fiber explicitly:

```javascript
import * as Main from "./output/Main/index.js";

const fiber = Main.main();
const result = await fiber.promise;
console.log(result);
```

Awaiting `Main.main()` does not await the fiber because the fiber itself is not a Promise. The
current `iris run` and `iris test` launcher awaits `main()` but does not await a returned fiber's
`.promise`. Use an explicit host boundary when completion and defects must be observed.

## Protect resources

`bracket` takes acquire, release, then use:

```purescript
Async.bracket
  :: forall acquire use release acquired combined resource value
   . Union acquire use acquired
  => Union acquired release combined
  => Async acquire resource
  -> (resource -> Async release {})
  -> (resource -> Async use value)
  -> Async combined value
```

After successful acquisition, release runs when use succeeds, aborts, defects, or is interrupted.
Nested resources release in reverse acquisition order. A suspended registration's cancellation
action runs before enclosing resources release.

Acquisition, release, and cancellation are interruption-masked. Interruption waits for those steps
to finish. If acquisition succeeds while interruption is pending, the resource is released without
entering use. If acquisition fails before producing a resource, this bracket cannot release it;
acquisition must manage its own partial ownership.

Keep protected operations bounded and able to complete. Interruption cannot preempt synchronous
host work or break a permanently suspended protected step.

Prefer cleanup that completes successfully. An abort or defect escaping cleanup currently becomes
a cleanup defect rather than a catchable typed abort. `interrupt` waits for cleanup but does not
report the target's cleanup failure; observe the target separately when that outcome matters.

Own a child fiber with existing primitives:

```purescript
scopedProgram :: Async [] Int
scopedProgram =
  Async.bracket
    (Async.lift (Async.run asyncProgram))
    Async.interrupt
    Async.join
```

If the owner's use phase is interrupted, release interrupts the child and waits for its cleanup.

## Follow effect diagnostics

Treat effect signatures as checked budgets. For a `MissingEffects` diagnostic:

1. Read the missing types and the declared allowed set.
2. Follow the primary expression and related source locations that introduced each effect.
3. Handle the specific abort, propagate its requirement through the signature, or remove the
   operation.
4. Recheck the entry boundary: `Async.run` requires a closed, `Abort`-free effect set.

Diagnostics can report several missing effects together and identify the responsible expression,
including a final `do` expression or a local declaration. When precise provenance is unavailable,
the diagnostic points to the declaration.

Do not fabricate `Subset` evidence, use unsafe coercion, or lie in an FFI signature. For an
unresolved `Union`, `Remove`, `Subset`, or `AbortIdentity`, check argument order, handler type
annotations, open tails, and whether a generic helper must retain the constraint.

## Respect current limits

- Open-tail solving is conservative. Unions of unrelated open tails can remain unresolved. Removal
  can remain unresolved when the effect is not explicit in an open input. Subset checks involving
  unknown tails may require more information or a retained constraint.
- Arbitrary polymorphic or higher-rank error types do not automatically acquire an abort identity.
- Ordinary libraries define their own effect labels and tracked operations with `liftEffectAs`.
  `catchAbort` handles only its selected `Abort error`; capability labels are preserved to the root.
- There is no timeout, race, parallel traversal, generic defect catcher, user-controlled
  interruption mask, or dedicated structured-concurrency API.
- There is no `Sync.bracket`. Use `Async.bracket` and lift synchronous operations where necessary.
- Effect sets track declared requirements, not arbitrary JavaScript behavior. FFI must honor its
  signatures.

## Verify runtime behavior

Check more than compilation:

- Results and sequencing order match the intended behavior.
- Each typed handler catches only its selected error.
- Callback completion works synchronously and asynchronously.
- Duplicate or stale callbacks cannot complete a later suspension.
- Cancellation removes external work.
- Resources release on success, abort, defect, and interruption in ownership order.
- Long recursive async work yields control and can be interrupted.
- The host observes the root fiber's completion and failures.
