module Main (runOrdered, provided) where

import Prelude

import Effect (Effect)
import Iris.Effect (class Has)
import Iris.Effect.Async (Async)
import Iris.Effect.Async as Async

newtype Counter = Counter Int

foreign import step :: String -> Int -> Effect Int

foreign import complete :: Int -> Effect Unit

foreign import failure :: String -> Effect Unit

foreign import mark :: forall value. String -> value -> value

throughMonad :: forall monad. Monad monad => monad Int -> monad Int
throughMonad action = do
  value <- action
  pure (value * 3)

program :: forall services. Has Counter services => Int -> Async String services Int
program input = do
  Counter offset <- Async.service @Counter
  first <- Async.liftEffect (step "first" input)
  adjusted <- throughMonad (pure (first + offset))
  result <- (\left right -> left - right)
    <$> Async.liftEffect (step "left" adjusted)
    <*> Async.liftEffect (step "right" offset)
  if result > 0 then pure result
  else Async.fail "negative result"

runOrdered :: Unit -> Effect Unit
runOrdered _ = do
  let
    environment = mark "environment" { choice: Counter 7 }
    stored = mark "program" { action: program 11 }
  _ <- Async.launch { onUncaught: failure } environment do
    result <- stored.action
    Async.liftEffect (complete result)
  pure unit

provided :: Effect Unit
provided = do
  _ <- Async.launch { onUncaught: failure } {} do
    result <- Async.provide { renamed: Counter 3 } (program 5)
    Async.liftEffect (complete result)
  pure unit
