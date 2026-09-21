module Iris.Effect.Async where

import Iris.Effect (Async, Fiber, Sync)
import Prim.Effect (Abort, class AbortIdentity, class Remove, class Union)

foreign import pure :: forall value. value -> Async [] value

foreign import bind
  :: forall left right combined value result
   . Union left right combined
  => Async left value
  -> (value -> Async right result)
  -> Async combined result

foreign import discard
  :: forall left right combined ignored result
   . Union left right combined
  => Async left ignored
  -> (ignored -> Async right result)
  -> Async combined result

foreign import map
  :: forall effects value result
   . (value -> result)
  -> Async effects value
  -> Async effects result

foreign import apply
  :: forall left right combined value result
   . Union left right combined
  => Async left (value -> result)
  -> Async right value
  -> Async combined result

foreign import lift :: forall effects value. Sync effects value -> Async effects value

foreign import defer
  :: forall effects value
   . ({} -> Async effects value)
  -> Async effects value

foreign import yield :: Async [] {}

foreign import register
  :: forall effects value
   . ((Async effects value -> Sync [] {}) -> Sync [] (Async [] {}))
  -> Async effects value

foreign import bracket
  :: forall acquire use release acquired combined resource value
   . Union acquire use acquired
  => Union acquired release combined
  => Async acquire resource
  -> (resource -> Async release {})
  -> (resource -> Async use value)
  -> Async combined value

foreign import run :: forall value. Async [] value -> Sync [] (Fiber value)

foreign import join :: forall value. Fiber value -> Async [] value

foreign import interrupt :: forall value. Fiber value -> Async [] {}

foreign import abort
  :: forall error value
   . AbortIdentity error
  => error
  -> Async [Abort error] value

foreign import catchAbort
  :: forall error input remaining handled output value
   . AbortIdentity error
  => Remove (Abort error) input remaining
  => Union remaining handled output
  => Async input value
  -> (error -> Async handled value)
  -> Async output value
