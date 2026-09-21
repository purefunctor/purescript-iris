module Console (Console, log) where

import Effect (Effect)
import Iris.Effect (Sync)
import Iris.Effect.Compat as Compat
import Prelude (Unit)

foreign import data Console :: Type

foreign import logEffect :: String -> Effect Unit

log :: String -> Sync [Console] Unit
log message = Compat.liftEffectAs @Console (logEffect message)
