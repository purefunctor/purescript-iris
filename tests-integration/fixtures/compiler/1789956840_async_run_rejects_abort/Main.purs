module Main where

import Iris.Effect (Async, Fiber, Sync)
import Iris.Effect.Async as Async
import Prim.Effect (Abort, class Subset)

newtype Failure = Failure Int

type FailureEffect = Abort Failure

direct :: Sync [Abort Failure] (Fiber Int)
direct = Async.run (Async.abort (Failure 1))

aliased :: Sync [FailureEffect] (Fiber Int)
aliased = Async.run (Async.abort (Failure 2))

unknown
  :: forall @effect value
   . Subset [effect] [effect]
  => Async [effect] value
  -> Sync [effect] (Fiber value)
unknown = Async.run
