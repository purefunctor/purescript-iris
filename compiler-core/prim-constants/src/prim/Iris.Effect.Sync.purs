module Iris.Effect.Sync where

import Iris.Effect (Sync)
import Prim.Effect (Abort, class AbortIdentity, class Remove, class Union)

foreign import pure :: forall value. value -> Sync [] value

foreign import bind
  :: forall left right combined value result
   . Union left right combined
  => Sync left value
  -> (value -> Sync right result)
  -> Sync combined result

foreign import discard
  :: forall left right combined ignored result
   . Union left right combined
  => Sync left ignored
  -> (ignored -> Sync right result)
  -> Sync combined result

foreign import map
  :: forall effects value result
   . (value -> result)
  -> Sync effects value
  -> Sync effects result

foreign import apply
  :: forall left right combined value result
   . Union left right combined
  => Sync left (value -> result)
  -> Sync right value
  -> Sync combined result

foreign import abort
  :: forall error value
   . AbortIdentity error
  => error
  -> Sync [Abort error] value

foreign import catchAbort
  :: forall error input remaining handled output value
   . AbortIdentity error
  => Remove (Abort error) input remaining
  => Union remaining handled output
  => Sync input value
  -> (error -> Sync handled value)
  -> Sync output value
