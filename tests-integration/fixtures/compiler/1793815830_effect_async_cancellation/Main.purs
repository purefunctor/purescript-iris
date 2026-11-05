module Main (run) where

import Prelude

import Data.Either (Either(..))
import Effect (Effect)
import Iris.Effect.Async (Async)
import Iris.Effect.Async as Async
import Iris.Effect.Fiber (Exit(..))

type Report = String -> Effect Unit

foreign import setTimeout :: Int -> Effect Unit -> Effect Int

foreign import clearTimeout :: Int -> Effect Unit

say :: Report -> String -> Async String () Unit
say report message = Async.liftEffect (report message)

sleep :: Report -> Int -> Async String () Unit
sleep report milliseconds = Async.async \callback -> do
  identifier <- setTimeout milliseconds (callback (Right unit))
  pure (say report "canceler" *> Async.liftEffect (clearTimeout identifier))

describe :: Exit String Unit -> String
describe = case _ of
  Succeeded _ -> "succeeded"
  Failed error -> "failed " <> error
  Died _ -> "died"

program :: Report -> Async String () Unit
program report = do
  sleeping <- Async.fork (sleep report 1000 *> say report "woke")
  say report "forked"
  Async.kill "stop" sleeping
  say report "killed"
  sleepingExit <- Async.await sleeping
  say report ("sleeping " <> describe sleepingExit)
  finished <- Async.fork (pure unit)
  _ <- Async.await finished
  Async.kill "too late" finished
  finishedExit <- Async.await finished
  say report ("finished " <> describe finishedExit)
  sleep report 20
  say report "done"

run :: Report -> Effect Unit
run report = void do
  Async.launch { onUncaught: \error -> report ("uncaught " <> error) } {} (program report)
