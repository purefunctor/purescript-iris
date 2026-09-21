module Iris.Effect.Compat where

import Effect (Effect)
import Iris.Effect (Sync)

foreign import liftEffect :: forall value. Effect value -> Sync [] value

foreign import liftEffectAs
  :: forall @effect value
   . Effect value
  -> Sync [effect] value
