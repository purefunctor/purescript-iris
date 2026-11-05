module Iris.Effect.Fiber
  ( Step(..)
  , Opaque
  , BracketConditions
  , Canceler
  , Runtime
  , newRuntime
  , Exit(..)
  , Fiber
  , makeFiber
  , joinFiber
  , killFiber
  , bindStep
  , catchStep
  , bracketStep
  , mapStep
  , forkStep
  , deferStep
  ) where

import Prelude

import Data.Either (Either(..))
import Data.List (List(..))
import Data.List as List
import Data.Maybe (Maybe(..), fromMaybe, isJust, isNothing)
import Effect (Effect, untilE)
import Effect.Exception (Error, throwException)
import Effect.Ref (Ref)
import Effect.Ref as Ref
import Effect.Uncurried (EffectFn3, runEffectFn3)
import Unsafe.Coerce (unsafeCoerce)

data Step error value
  = Pure value
  | Throw error
  | Sync (Effect value)
  | Fallible (Effect (Either error value))
  | Run (Effect (Step error value))
  | Async ((Either error value -> Effect Unit) -> Effect (Canceler error))
  | Bind (Step error Opaque) (Opaque -> Step error value)
  | Catch (Step Opaque value) (Opaque -> Step error value)
  | Bracket (Step error Opaque) (BracketConditions error Opaque value) (Opaque -> Step error value)
  | Fork (Step Opaque Opaque) (Fiber Opaque Opaque -> value)

type BracketConditions error resource result =
  { killed :: resource -> Step error Unit
  , failed :: error -> resource -> Step error Unit
  , died :: Error -> resource -> Step error Unit
  , completed :: result -> resource -> Step error Unit
  }

data Exit error value = Succeeded value | Failed error | Died Error

type Canceler error = Step error Unit

data Opaque

toOpaque :: forall value. value -> Opaque
toOpaque = unsafeCoerce

opaqueStep :: forall error value. Step error value -> Step Opaque Opaque
opaqueStep = unsafeCoerce

bindStep :: forall error a b. Step error a -> (a -> Step error b) -> Step error b
bindStep step continuation = Bind (unsafeCoerce step) (unsafeCoerce continuation)

catchStep ::
  forall inner error value. Step inner value -> (inner -> Step error value) -> Step error value
catchStep step handler = Catch (unsafeCoerce step) (unsafeCoerce handler)

bracketStep ::
  forall error resource value.
  Step error resource ->
  BracketConditions error resource value ->
  (resource -> Step error value) ->
  Step error value
bracketStep acquire conditions use =
  Bracket (unsafeCoerce acquire) (unsafeCoerce conditions) (unsafeCoerce use)

mapStep :: forall error a b. (a -> b) -> Step error a -> Step error b
mapStep function step = case step of
  Pure value -> Pure (function value)
  _ -> bindStep step (Pure <<< function)

forkStep ::
  forall error other value forked.
  Step error forked -> (Fiber error forked -> value) -> Step other value
forkStep child continuation = Fork (unsafeCoerce child) (unsafeCoerce continuation)

deferStep :: forall error value. (Unit -> Step error value) -> Step error value
deferStep thunk = Run (map thunk (pure unit))

foreign import tryHost :: forall result. EffectFn3 (Error -> result) result (Effect Unit) result

tryEffect :: Effect Unit -> Effect (Maybe Error)
tryEffect effect = runEffectFn3 tryHost Just Nothing effect

type Continuation = Opaque -> Step Opaque Opaque

data Attempt
  = AttemptCatch (Opaque -> Step Opaque Opaque) Boolean
  | AttemptResume (List Continuation) Boolean
  | AttemptBracket (BracketConditions Opaque Opaque Opaque) (Opaque -> Step Opaque Opaque) Boolean
  | AttemptRelease (BracketConditions Opaque Opaque Opaque) Opaque Boolean
  | AttemptFinalizer (Step Opaque Opaque)
  | AttemptFinalized (Maybe Opaque) (Maybe Opaque) (Maybe Error)

data Status = Suspended | Running | Runnable | Pending | Return | Completed

type Join = Exit Opaque Opaque -> Effect Unit

type JoinEntry = { identifier :: Int, join :: Join }

type Scheduler =
  { front :: Ref (List (Effect Unit))
  , back :: Ref (List (Effect Unit))
  , draining :: Ref Boolean
  , escaped :: Ref (Maybe Error)
  }

newtype Runtime = Runtime { scheduler :: Scheduler }

newRuntime :: Effect Runtime
newRuntime = do
  front <- Ref.new Nil
  back <- Ref.new Nil
  draining <- Ref.new false
  escaped <- Ref.new Nothing
  pure (Runtime { scheduler: { front, back, draining, escaped } })

enqueue :: Scheduler -> Effect Unit -> Effect Unit
enqueue scheduler thunk = do
  back <- Ref.read scheduler.back
  Ref.write (Cons thunk back) scheduler.back
  drainUnlessDraining scheduler

prioritize :: Scheduler -> Effect Unit -> Effect Unit
prioritize scheduler thunk = do
  front <- Ref.read scheduler.front
  Ref.write (Cons thunk front) scheduler.front

drainUnlessDraining :: Scheduler -> Effect Unit
drainUnlessDraining scheduler = do
  draining <- Ref.read scheduler.draining
  unless draining (drain scheduler)

drain :: Scheduler -> Effect Unit
drain scheduler = do
  Ref.write true scheduler.draining
  Ref.write Nothing scheduler.escaped
  untilE do
    front <- Ref.read scheduler.front
    case front of
      Cons thunk rest -> do
        Ref.write rest scheduler.front
        escaped <- tryEffect thunk
        recordFirstEscape scheduler.escaped escaped
        pure false
      Nil -> do
        back <- Ref.read scheduler.back
        case back of
          Nil -> pure true
          _ -> do
            Ref.write Nil scheduler.back
            Ref.write (List.reverse back) scheduler.front
            pure false
  Ref.write false scheduler.draining
  rethrowEscaped scheduler.escaped

recordFirstEscape :: Ref (Maybe Error) -> Maybe Error -> Effect Unit
recordFirstEscape recorded escaped = when (isJust escaped) do
  previous <- Ref.read recorded
  when (isNothing previous) (Ref.write escaped recorded)

rethrowEscaped :: Ref (Maybe Error) -> Effect Unit
rethrowEscaped recorded = do
  escaped <- Ref.read recorded
  case escaped of
    Just error -> throwException error
    Nothing -> pure unit

newtype Fiber error value =
  Fiber
    { runtime :: Runtime
    , status :: Ref Status
    , runnableResult :: Ref (Either Opaque Opaque)
    , pendingCanceler :: Ref (Maybe (Canceler Opaque))
    , exit :: Ref (Exit Opaque Opaque)
    , current :: Ref (Step Opaque Opaque)
    , returnValue :: Ref (Maybe Opaque)
    , failure :: Ref (Maybe Opaque)
    , defect :: Ref (Maybe Error)
    , interrupt :: Ref (Maybe Opaque)
    , continuations :: Ref (List Continuation)
    , attempts :: Ref (List Attempt)
    , bracketCount :: Ref Int
    , joins :: Ref (List JoinEntry)
    , joinIdentifier :: Ref Int
    , runTick :: Ref Int
    }

noResult :: Either Opaque Opaque
noResult = Right (toOpaque unit)

noExit :: Exit Opaque Opaque
noExit = Succeeded (toOpaque unit)

noStep :: Step Opaque Opaque
noStep = Pure (toOpaque unit)

orUnit :: Maybe Opaque -> Opaque
orUnit = fromMaybe (toOpaque unit)

makeFiber :: forall error value. Runtime -> Step error value -> Effect (Fiber error value)
makeFiber runtime step = do
  status <- Ref.new Suspended
  runnableResult <- Ref.new noResult
  pendingCanceler <- Ref.new Nothing
  exit <- Ref.new noExit
  current <- Ref.new (opaqueStep step)
  returnValue <- Ref.new Nothing
  failure <- Ref.new Nothing
  defect <- Ref.new Nothing
  interrupt <- Ref.new Nothing
  continuations <- Ref.new Nil
  attempts <- Ref.new Nil
  bracketCount <- Ref.new 0
  joins <- Ref.new Nil
  joinIdentifier <- Ref.new 0
  runTick <- Ref.new 0
  pure
    ( Fiber
        { runtime
        , status
        , runnableResult
        , pendingCanceler
        , exit
        , current
        , returnValue
        , failure
        , defect
        , interrupt
        , continuations
        , attempts
        , bracketCount
        , joins
        , joinIdentifier
        , runTick
        }
    )

joinFiber ::
  forall error value. Fiber error value -> (Exit error value -> Effect Unit) -> Effect (Effect Unit)
joinFiber (Fiber fiber) callback = do
  let
    Runtime runtime = fiber.runtime
  canceler <- onComplete (Fiber fiber) (unsafeCoerce callback)
  status <- Ref.read fiber.status
  case status of
    Suspended -> do
      prioritize runtime.scheduler (run (Fiber fiber))
      drainUnlessDraining runtime.scheduler
    _ -> pure unit
  pure canceler

killFiber :: forall error value. error -> Fiber error value -> Effect Unit -> Effect (Effect Unit)
killFiber error (Fiber fiber) callback = do
  status <- Ref.read fiber.status
  case status of
    Completed -> do
      callback
      pure (pure unit)
    _ -> do
      canceler <- onComplete (Fiber fiber) \_ -> callback
      let
        markInterrupted = do
          interrupt <- Ref.read fiber.interrupt
          when (isNothing interrupt) (Ref.write (Just (toOpaque error)) fiber.interrupt)
        abandon = do
          Ref.write Return fiber.status
          Ref.write Nothing fiber.returnValue
          Ref.write Nothing fiber.failure
      case status of
        Suspended -> do
          Ref.write (Just (toOpaque error)) fiber.interrupt
          Ref.write (Failed (toOpaque error)) fiber.exit
          Ref.write noStep fiber.current
          Ref.write Completed fiber.status
          run (Fiber fiber)
        Pending -> do
          markInterrupted
          bracketCount <- Ref.read fiber.bracketCount
          when (bracketCount == 0) do
            pendingCanceler <- Ref.read fiber.pendingCanceler
            Ref.write Nothing fiber.pendingCanceler
            case pendingCanceler of
              Just pending -> pushAttempt (Fiber fiber) (AttemptFinalizer (opaqueStep pending))
              Nothing -> pure unit
            abandon
            runTick <- Ref.read fiber.runTick
            Ref.write (runTick + 1) fiber.runTick
            run (Fiber fiber)
        Runnable -> do
          markInterrupted
          bracketCount <- Ref.read fiber.bracketCount
          when (bracketCount == 0) abandon
        _ -> markInterrupted
      pure canceler

onComplete :: forall error value. Fiber error value -> Join -> Effect (Effect Unit)
onComplete (Fiber fiber) join = do
  status <- Ref.read fiber.status
  case status of
    Completed -> do
      exit <- Ref.read fiber.exit
      join exit
      pure (pure unit)
    _ -> do
      identifier <- Ref.read fiber.joinIdentifier
      Ref.write (identifier + 1) fiber.joinIdentifier
      joins <- Ref.read fiber.joins
      Ref.write (Cons { identifier, join } joins) fiber.joins
      pure do
        remaining <- Ref.read fiber.joins
        Ref.write (List.filter (\entry -> entry.identifier /= identifier) remaining) fiber.joins

pushAttempt :: forall error value. Fiber error value -> Attempt -> Effect Unit
pushAttempt (Fiber fiber) attempt = do
  attempts <- Ref.read fiber.attempts
  Ref.write (Cons attempt attempts) fiber.attempts

enterFinalizer :: forall error value. Fiber error value -> Effect Unit
enterFinalizer (Fiber fiber) = do
  returnValue <- Ref.read fiber.returnValue
  failure <- Ref.read fiber.failure
  defect <- Ref.read fiber.defect
  pushAttempt (Fiber fiber) (AttemptFinalized returnValue failure defect)
  Ref.write Nothing fiber.failure
  Ref.write Nothing fiber.defect
  bracketCount <- Ref.read fiber.bracketCount
  Ref.write (bracketCount + 1) fiber.bracketCount

die :: forall error value. Fiber error value -> Error -> Effect Unit
die (Fiber fiber) error = do
  Ref.write (Just error) fiber.defect
  Ref.write Return fiber.status
  Ref.write Nothing fiber.failure
  Ref.write Nothing fiber.returnValue

suspendContinuations :: forall error value. Fiber error value -> Effect Unit
suspendContinuations (Fiber fiber) = do
  interrupt <- Ref.read fiber.interrupt
  continuations <- Ref.read fiber.continuations
  case continuations of
    Nil -> pure unit
    _ -> pushAttempt (Fiber fiber) (AttemptResume continuations (isJust interrupt))
  Ref.write Nil fiber.continuations

run :: forall error value. Fiber error value -> Effect Unit
run (Fiber fiber) = do
  untilE do
    escaped <- tryEffect (runFromStatus (Fiber fiber))
    case escaped of
      Just defect -> do
        die (Fiber fiber) defect
        runTick <- Ref.read fiber.runTick
        Ref.write (runTick + 1) fiber.runTick
        pure false
      Nothing -> pure true
  status <- Ref.read fiber.status
  case status of
    Completed -> do
      exit <- Ref.read fiber.exit
      complete (Fiber fiber) exit
    _ -> pure unit

runFromStatus :: forall error value. Fiber error value -> Effect Unit
runFromStatus (Fiber fiber) = do
  status <- Ref.read fiber.status
  case status of
    Suspended -> do
      Ref.write Running fiber.status
      runSteps (Fiber fiber)
    Runnable -> do
      Ref.write Running fiber.status
      result <- Ref.read fiber.runnableResult
      Ref.write noResult fiber.runnableResult
      stop <- deliver (Fiber fiber) result
      unless stop (runSteps (Fiber fiber))
    Return -> do
      Ref.write Running fiber.status
      stop <- returnStep (Fiber fiber)
      unless stop (runSteps (Fiber fiber))
    Running -> pure unit
    Pending -> pure unit
    Completed -> pure unit

deliver :: forall error value. Fiber error value -> Either Opaque Opaque -> Effect Boolean
deliver (Fiber fiber) result = case result of
  Left error -> failWith (Fiber fiber) error
  Right value -> succeedWith (Fiber fiber) value

succeedWith :: forall error value. Fiber error value -> Opaque -> Effect Boolean
succeedWith (Fiber fiber) value = do
  continuations <- Ref.read fiber.continuations
  case continuations of
    Cons continuation rest -> do
      Ref.write rest fiber.continuations
      Ref.write (continuation value) fiber.current
      pure false
    Nil -> do
      Ref.write (Just value) fiber.returnValue
      returnStep (Fiber fiber)

failWith :: forall error value. Fiber error value -> Opaque -> Effect Boolean
failWith (Fiber fiber) error = do
  Ref.write (Just error) fiber.failure
  Ref.write Nothing fiber.returnValue
  returnStep (Fiber fiber)

continueWith :: forall error value. Fiber error value -> Step Opaque Opaque -> Effect Boolean
continueWith (Fiber fiber) step = do
  Ref.write step fiber.current
  pure false

pushContinuation :: forall error value. Fiber error value -> Continuation -> Effect Unit
pushContinuation (Fiber fiber) continuation = do
  continuations <- Ref.read fiber.continuations
  Ref.write (Cons continuation continuations) fiber.continuations

runSteps :: forall error value. Fiber error value -> Effect Unit
runSteps (Fiber fiber) = untilE do
  step <- Ref.read fiber.current
  case step of
    Bind inner continuation -> case inner of
      Pure value -> continueWith (Fiber fiber) (continuation value)
      Sync effect -> do
        value <- effect
        continueWith (Fiber fiber) (continuation value)
      Fallible effect -> do
        result <- effect
        case result of
          Left error -> failWith (Fiber fiber) error
          Right value -> continueWith (Fiber fiber) (continuation value)
      Throw error -> failWith (Fiber fiber) error
      Run effect -> do
        next <- effect
        case next of
          Pure value -> continueWith (Fiber fiber) (continuation value)
          Sync nextEffect -> do
            value <- nextEffect
            continueWith (Fiber fiber) (continuation value)
          Fallible nextEffect -> do
            result <- nextEffect
            case result of
              Left error -> failWith (Fiber fiber) error
              Right value -> continueWith (Fiber fiber) (continuation value)
          Throw error -> failWith (Fiber fiber) error
          _ -> do
            pushContinuation (Fiber fiber) continuation
            continueWith (Fiber fiber) next
      _ -> do
        pushContinuation (Fiber fiber) continuation
        continueWith (Fiber fiber) inner
    Pure value -> succeedWith (Fiber fiber) value
    Sync effect -> do
      value <- effect
      succeedWith (Fiber fiber) value
    Fallible effect -> do
      result <- effect
      case result of
        Left error -> failWith (Fiber fiber) error
        Right value -> succeedWith (Fiber fiber) value
    Throw error -> failWith (Fiber fiber) error
    Run effect -> do
      next <- effect
      continueWith (Fiber fiber) next
    Async register -> do
      localRunTick <- Ref.read fiber.runTick
      Ref.write Pending fiber.status
      Ref.write Nothing fiber.pendingCanceler
      let
        Runtime runtime = fiber.runtime
      canceler <- register \result -> do
        tick <- Ref.read fiber.runTick
        when (tick == localRunTick) do
          Ref.write (tick + 1) fiber.runTick
          enqueue runtime.scheduler do
            resumeTick <- Ref.read fiber.runTick
            when (resumeTick == localRunTick + 1) do
              Ref.write Nothing fiber.pendingCanceler
              Ref.write result fiber.runnableResult
              Ref.write Runnable fiber.status
              run (Fiber fiber)
      status <- Ref.read fiber.status
      case status of
        Pending -> do
          pendingCanceler <- Ref.read fiber.pendingCanceler
          when (isNothing pendingCanceler) (Ref.write (Just canceler) fiber.pendingCanceler)
        _ -> pure unit
      pure true
    Catch inner handler -> do
      interrupt <- Ref.read fiber.interrupt
      suspendContinuations (Fiber fiber)
      pushAttempt (Fiber fiber) (AttemptCatch handler (isJust interrupt))
      continueWith (Fiber fiber) (opaqueStep inner)
    Bracket acquire conditions use -> do
      bracketCount <- Ref.read fiber.bracketCount
      Ref.write (bracketCount + 1) fiber.bracketCount
      interrupt <- Ref.read fiber.interrupt
      suspendContinuations (Fiber fiber)
      pushAttempt (Fiber fiber) (AttemptBracket conditions use (isJust interrupt))
      continueWith (Fiber fiber) acquire
    Fork child continuation -> do
      forked <- makeFiber fiber.runtime child
      let
        Runtime runtime = fiber.runtime
      Ref.write (Right (continuation forked)) fiber.runnableResult
      Ref.write Runnable fiber.status
      prioritize runtime.scheduler (run (Fiber fiber))
      prioritize runtime.scheduler (run forked)
      drainUnlessDraining runtime.scheduler
      pure true

returnStep :: forall error value. Fiber error value -> Effect Boolean
returnStep (Fiber fiber) = do
  Ref.write Nil fiber.continuations
  untilE do
    attempts <- Ref.read fiber.attempts
    case attempts of
      Nil -> do
        defect <- Ref.read fiber.defect
        interrupt <- Ref.read fiber.interrupt
        failure <- Ref.read fiber.failure
        returnValue <- Ref.read fiber.returnValue
        let
          result = case defect, interrupt, failure of
            Just error, _, _ -> Died error
            _, Just error, _ -> Failed error
            _, _, Just error -> Failed error
            _, _, _ -> Succeeded (orUnit returnValue)
        Ref.write result fiber.exit
        Ref.write noStep fiber.current
        Ref.write Completed fiber.status
        pure true
      Cons attempt rest -> do
        Ref.write rest fiber.attempts
        interrupt <- Ref.read fiber.interrupt
        bracketCount <- Ref.read fiber.bracketCount
        failure <- Ref.read fiber.failure
        defect <- Ref.read fiber.defect
        let
          interruptedSince enqueuedInterrupted = isJust interrupt && not enqueuedInterrupted
          killed enqueuedInterrupted = interruptedSince enqueuedInterrupted && bracketCount == 0
        case attempt of
          AttemptCatch handler enqueuedInterrupted -> case failure of
            Just error | not (killed enqueuedInterrupted) -> do
              Ref.write Nothing fiber.failure
              Ref.write (handler error) fiber.current
              pure true
            _ -> pure false
          AttemptResume continuations enqueuedInterrupted ->
            if killed enqueuedInterrupted || isJust failure || isJust defect then pure false
            else do
              returnValue <- Ref.read fiber.returnValue
              case continuations of
                Cons continuation others -> do
                  Ref.write others fiber.continuations
                  Ref.write (continuation (orUnit returnValue)) fiber.current
                  pure true
                Nil -> pure false
          AttemptBracket conditions use enqueuedInterrupted -> do
            Ref.write (bracketCount - 1) fiber.bracketCount
            if isJust failure || isJust defect then pure false
            else do
              returnValue <- Ref.read fiber.returnValue
              let
                resource = orUnit returnValue
              pushAttempt (Fiber fiber) (AttemptRelease conditions resource enqueuedInterrupted)
              if not (interruptedSince enqueuedInterrupted) || bracketCount - 1 > 0 then do
                Ref.write (use resource) fiber.current
                pure true
              else pure false
          AttemptRelease conditions resource enqueuedInterrupted -> do
            returnValue <- Ref.read fiber.returnValue
            let
              finalizer = case defect, failure of
                _, _ | killed enqueuedInterrupted -> conditions.killed resource
                Just error, _ -> conditions.died error resource
                _, Just error -> conditions.failed error resource
                _, _ -> conditions.completed (orUnit returnValue) resource
            enterFinalizer (Fiber fiber)
            Ref.write (opaqueStep finalizer) fiber.current
            pure true
          AttemptFinalizer finalizer -> do
            enterFinalizer (Fiber fiber)
            Ref.write finalizer fiber.current
            pure true
          AttemptFinalized savedValue savedFailure savedDefect -> do
            Ref.write (bracketCount - 1) fiber.bracketCount
            if isJust savedDefect then do
              Ref.write savedDefect fiber.defect
              Ref.write Nothing fiber.returnValue
              Ref.write Nothing fiber.failure
            else if isJust defect then do
              Ref.write Nothing fiber.returnValue
              Ref.write Nothing fiber.failure
            else if isNothing savedFailure && isJust failure then
              Ref.write Nothing fiber.returnValue
            else do
              Ref.write savedValue fiber.returnValue
              Ref.write savedFailure fiber.failure
            pure false
  status <- Ref.read fiber.status
  pure case status of
    Completed -> true
    _ -> false

complete :: forall error value. Fiber error value -> Exit Opaque Opaque -> Effect Unit
complete (Fiber fiber) exit = do
  joins <- Ref.read fiber.joins
  Ref.write Nil fiber.joins
  escaped <- Ref.new Nothing
  remaining <- Ref.new (List.reverse joins)
  untilE do
    pending <- Ref.read remaining
    case pending of
      Cons entry rest -> do
        Ref.write rest remaining
        outcome <- tryEffect (entry.join exit)
        recordFirstEscape escaped outcome
        pure false
      Nil -> pure true
  rethrowEscaped escaped
