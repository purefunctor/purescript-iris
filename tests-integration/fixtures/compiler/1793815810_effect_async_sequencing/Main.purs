module Main (runOrdered, provided) where

import Prelude

import Effect (Effect)
import Iris.Effect.Async (Async)
import Iris.Effect.Async as Async
import Iris.Effect.Sync as Sync

foreign import step :: String -> Int -> Effect Int

foreign import complete :: Int -> Effect Unit

foreign import failure :: String -> Effect Unit

foreign import mark :: forall value. String -> value -> value

throughMonad :: forall monad. Monad monad => monad Int -> monad Int
throughMonad action = do
  value <- action
  pure (value * 3)

offset :: forall error services. Async error (offset :: Int | services) Int
offset = Async.sync (Sync.make \environment -> pure environment.offset)

program :: forall services. Int -> Async String (offset :: Int | services) Int
program input = do
  added <- offset
  first <- Async.liftEffect (step "first" input)
  adjusted <- throughMonad (pure (first + added))
  result <- (\left right -> left - right)
    <$> Async.liftEffect (step "left" adjusted)
    <*> Async.liftEffect (step "right" added)
  if result > 0 then pure result
  else Async.fail "negative result"

runOrdered :: Unit -> Effect Unit
runOrdered _ = do
  let
    environment = mark "environment" { offset: 7 }
    stored = mark "program" { action: program 11 }
  _ <- Async.launch { onUncaught: failure } environment do
    result <- stored.action
    Async.liftEffect (complete result)
  pure unit

provided :: Effect Unit
provided = do
  _ <- Async.launch { onUncaught: failure } {} do
    result <- Async.provide { offset: 3 } (program 5)
    Async.liftEffect (complete result)
  pure unit
