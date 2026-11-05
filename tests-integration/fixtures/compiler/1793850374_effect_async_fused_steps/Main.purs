module Main (runLowered, runGeneric) where

import Prelude

import Data.Either (Either(..))
import Effect (Effect)
import Iris.Effect (class Has)
import Iris.Effect.Async (Async)
import Iris.Effect.Async as Async

newtype Offset = Offset Int

foreign import record :: Int -> Effect Int

foreign import later :: (Int -> Effect Unit) -> Effect Unit

foreign import complete :: Int -> Effect Unit

foreign import failed :: String -> Effect Unit

pause :: forall services. Async String services Int
pause = Async.async \callback -> do
  later (callback <<< Right)
  pure (pure unit)

lowered :: forall services. Has Offset services => Async String services Unit
lowered = do
  Offset offset <- Async.service @Offset
  first <- Async.liftEffect (record (offset + 1))
  second <- Async.liftEffect (record (first + 1))
  third <- pause
  fourth <- Async.liftEffect (record (second + third))
  Async.liftEffect (complete fourth)

generic :: forall monad. Monad monad => (Int -> monad Int) -> monad Int -> (Int -> monad Unit) -> monad Unit
generic step suspend finish = do
  first <- step 4
  second <- step (first + 1)
  third <- suspend
  fourth <- step (second + third)
  finish fourth

runLowered :: Effect Unit
runLowered = void (Async.launch { onUncaught: failed } { offset: Offset 3 } lowered)

runGeneric :: Effect Unit
runGeneric = void do
  Async.launch { onUncaught: failed } {} do
    generic (Async.liftEffect <<< record) pause (Async.liftEffect <<< complete)
