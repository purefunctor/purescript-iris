module Main where

import Prelude

import Effect (Effect)
import Effect.Ref as Ref
import Iris.Effect.Async (Async)
import Iris.Effect.Async as Async
import Iris.Effect.Fiber (Exit)
import Iris.Effect.Fiber as Fiber

whenConfigured :: { enabled :: Boolean, value :: Int } -> Ref.Ref Int -> Async String () Unit
whenConfigured configuration reference =
  when configuration.enabled (Async.liftEffect (Ref.write configuration.value reference))

unlessConfigured :: { enabled :: Boolean, value :: Int } -> Ref.Ref Int -> Async String () Unit
unlessConfigured configuration reference =
  unless configuration.enabled (Async.liftEffect (Ref.write configuration.value reference))

failWhen :: Boolean -> Async String () Unit
failWhen condition = when condition (Async.fail "failure")

run :: Async String () Unit -> (Exit String Unit -> Effect Unit) -> Effect Unit
run program callback = do
  runtime <- Fiber.newRuntime
  fiber <- Fiber.makeFiber runtime (Async.toStep {} program)
  void (Fiber.joinFiber fiber callback)
