module Main where

import Console (Console)
import Console as Console
import Effect (Effect)
import Iris.Effect (Async, Fiber, Promise, Sync)
import Iris.Effect.Async as Async
import Iris.Effect.Compat as Compat
import Iris.Effect.Sync as Sync
import Prim.Effect (Abort, class Runnable, class Subset)
import Prelude

newtype PromiseError = PromiseError Int

foreign import countedEffect :: Int -> Effect Int

foreign import delayedPromise :: Int -> Effect (Promise Int)

foreign import rejectedPromise :: Effect (Promise Int)

foreign import throwingPromise :: Effect (Promise Int)

foreign import pendingPromise :: Effect (Promise Int)

liftedEffect :: Sync [] Int
liftedEffect = Compat.liftEffect (countedEffect 17)

trackedConsole :: Sync [Console] Unit
trackedConsole = Console.log "tracked console"

program :: Async [Console] Int
program = Async.do
  Async.lift trackedConsole
  value <- Async.fromPromise (Compat.liftEffect (delayedPromise 41))
  Async.pure (value + 1)

main :: Sync [Console] (Fiber Int)
main = Async.run program

rejected :: Sync [] (Fiber Int)
rejected = Async.run (Async.fromPromise (Compat.liftEffect rejectedPromise))

thrown :: Sync [] (Fiber Int)
thrown = Async.run (Async.fromPromise (Compat.liftEffect throwingPromise))

pending :: Sync [] (Fiber Int)
pending = Async.run (Async.fromPromise (Compat.liftEffect pendingPromise))

factoryAbort :: Async [] Int
factoryAbort =
  Async.catchAbort
    (Async.fromPromise
      (Sync.abort (PromiseError 5) :: Sync [Abort PromiseError] (Promise Int)))
    \(PromiseError code) -> Async.pure code

runFactoryAbort :: Sync [] (Fiber Int)
runFactoryAbort = Async.run factoryAbort

runGeneric
  :: forall @effects value
   . Runnable effects
  => Subset effects effects
  => Async effects value
  -> Sync effects (Fiber value)
runGeneric = Async.run
