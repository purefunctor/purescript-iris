module Main (run) where

import Prelude

import Effect (Effect)
import Iris.Effect.Async (Async)
import Iris.Effect.Async as Async

type Report = String -> Effect Unit

nestedForks :: Int -> Async String () Int
nestedForks depth = do
  value <- pure depth
  if value == 0 then pure 719
  else do
    child <- Async.fork (nestedForks (value - 1))
    Async.join child

countdown :: Int -> Async String () Int
countdown remaining =
  if remaining == 0 then pure 0
  else do
    value <- pure remaining
    next <- countdown (value - 1)
    pure (next + 1)

program :: Report -> Async String () Unit
program report = do
  forked <- nestedForks 10000
  Async.liftEffect (report ("nested forks " <> show forked))
  counted <- countdown 100000
  Async.liftEffect (report ("binds " <> show counted))

run :: Report -> Effect Unit
run report = void do
  Async.launch { onUncaught: \error -> report ("uncaught " <> error) } {} (program report)
