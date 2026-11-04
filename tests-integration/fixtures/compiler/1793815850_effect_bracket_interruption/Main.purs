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

sleep :: Report -> String -> Int -> Async String () Unit
sleep report label milliseconds = Async.async \callback -> do
  identifier <- setTimeout milliseconds (callback (Right unit))
  pure (say report ("cancel " <> label) *> Async.liftEffect (clearTimeout identifier))

describe :: Exit String Unit -> String
describe = case _ of
  Succeeded _ -> "succeeded"
  Failed error -> "failed " <> error
  Died _ -> "died"

killed :: Report -> String -> Async String () Unit -> Async String () Unit
killed report label body = do
  fiber <- Async.fork body
  Async.kill ("kill " <> label) fiber
  exit <- Async.await fiber
  say report (label <> " " <> describe exit)

program :: Report -> Async String () Unit
program report = do
  killed report "during acquire" do
    Async.bracket (sleep report "acquire" 10 *> say report "acquired")
      (\_ -> say report "released")
      (\_ -> say report "used")
  killed report "during use" do
    Async.bracket (say report "acquired")
      (\_ -> sleep report "release" 10 *> say report "released")
      (\_ -> sleep report "use" 1000 *> say report "used")
  killed report "with failing release" do
    Async.bracket (pure unit)
      (\_ -> Async.fail "release failure")
      (\_ -> sleep report "use" 1000)

run :: Report -> Effect Unit
run report = void do
  Async.launch { onUncaught: \error -> report ("uncaught " <> error) } {} (program report)
