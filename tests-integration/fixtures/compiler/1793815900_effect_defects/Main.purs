module Main (run, launchDefect) where

import Prelude

import Data.Either (Either(..))
import Effect (Effect)
import Effect.Exception (message)
import Iris.Effect.Async (Async)
import Iris.Effect.Async as Async
import Iris.Effect.Fiber (Exit(..))

type Report = String -> Effect Unit

foreign import boom :: String -> Effect Unit

foreign import throwNull :: Effect Unit

foreign import throwHostile :: Effect Unit

foreign import setTimeout :: Int -> Effect Unit -> Effect Int

foreign import clearTimeout :: Int -> Effect Unit

delay :: Int -> Async String () Unit
delay milliseconds = Async.async \callback -> do
  identifier <- setTimeout milliseconds (callback (Right unit))
  pure (Async.liftEffect (clearTimeout identifier))

describe :: forall value. Exit String value -> String
describe = case _ of
  Succeeded _ -> "succeeded"
  Failed error -> "failed " <> error
  Died defect -> "died " <> message defect

outcome :: forall value. Report -> String -> Async String () value -> Async String () Unit
outcome report label program = do
  fiber <- Async.fork program
  exit <- Async.await fiber
  Async.liftEffect (report (label <> ": " <> describe exit))

program :: Report -> Async String () Unit
program report = do
  dying <- Async.fork (Async.finally (say "released") (Async.liftEffect (boom "boom")))
  other <- Async.fork (delay 10 *> say "sibling completed")
  _ <- Async.await other
  outcome report "await" (Async.join dying)
  outcome report "attempt" (Async.attempt (Async.join dying))
  outcome report "cleanup over typed" (Async.finally (Async.liftEffect (boom "cleanup")) (Async.fail "typed"))
  outcome report "body over cleanup" (Async.finally (Async.liftEffect (boom "cleanup")) (Async.liftEffect (boom "body")))
  killed <- Async.fork (Async.finally (Async.liftEffect (boom "finalizer")) (delay 1000))
  Async.kill "killed" killed
  killedExit <- Async.await killed
  say ("kill with failing finalizer: " <> describe killedExit)
  outcome report "null" (Async.liftEffect throwNull)
  outcome report "hostile" (Async.liftEffect throwHostile)
  say "parent continues"
  where
  say message = Async.liftEffect (report message)

run :: Report -> Effect Unit
run report = void do
  Async.launch { onUncaught: \error -> report ("uncaught " <> error) } {} (program report)

launchDefect :: Report -> Effect Unit
launchDefect report = void do
  Async.launch { onUncaught: \error -> report ("uncaught " <> error) } {} (Async.liftEffect (boom "root"))
