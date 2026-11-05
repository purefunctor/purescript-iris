module Main where

import Prelude

import Data.Either (Either(..))
import Effect (Effect)
import Iris.Effect (class Has)
import Iris.Effect.Async (Async)
import Iris.Effect.Async as Async

newtype Offset = Offset Int

foreign import record :: Int -> Effect Int

foreign import later :: (Int -> Effect Unit) -> Effect Unit

pause :: forall services. Async String services Int
pause = Async.async \callback -> do
  later (callback <<< Right)
  pure (pure unit)

program :: forall services. Has Offset services => Async String services Int
program = do
  Offset offset <- Async.service @Offset
  first <- Async.liftEffect (record offset)
  second <- pause
  if first > second then Async.fail "first is larger"
  else pure (first + second)
