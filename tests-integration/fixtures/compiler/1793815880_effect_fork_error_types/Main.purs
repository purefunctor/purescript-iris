module Main (run) where

import Prelude

import Data.Either (Either(..))
import Effect (Effect)
import Iris.Effect.Async (Async)
import Iris.Effect.Async as Async

type Report = String -> Effect Unit

newtype OuterError = OuterError Int

foreign import setTimeout :: Int -> Effect Unit -> Effect Int

foreign import clearTimeout :: Int -> Effect Unit

delay :: forall error. Int -> Async error () Unit
delay milliseconds = Async.async \callback -> do
  identifier <- setTimeout milliseconds (callback (Right unit))
  pure (Async.liftEffect (clearTimeout identifier))

describe :: Either String Int -> String
describe = case _ of
  Left error -> "failed " <> error
  Right value -> "succeeded " <> show value

describeOuter :: Either OuterError Int -> String
describeOuter = case _ of
  Left (OuterError code) -> "failed outer " <> show code
  Right value -> "succeeded " <> show value

innerRelease :: Async String () Int
innerRelease = Async.bracket (pure unit) (\_ -> Async.fail "release") \_ -> delay 1000 *> pure 9

innerCanceler :: Async String () Int
innerCanceler = Async.async \_ -> pure (Async.fail "cleanup")

children :: Report -> Async String () Int
children report = do
  _ <- Async.fork (Async.fail "detached child failure")
  failing <- Async.fork (delay 10 *> Async.fail "joined child failure")
  succeeding <- Async.fork (delay 10 *> pure 5)
  guarded <- Async.fork innerRelease
  delay 20
  Async.kill "killed" guarded
  joinedFailing <- Async.attempt (Async.join failing)
  joinedSucceeding <- Async.attempt (Async.join succeeding)
  joinedGuarded <- Async.attempt (Async.join guarded)
  Async.liftEffect (report (describe joinedFailing <> ", " <> describe joinedSucceeding <> ", " <> describe joinedGuarded))
  Async.fail "parent failure"

program :: Report -> Async OuterError () Unit
program report = do
  mapped <- Async.fork (Async.catch innerRelease \_ -> Async.fail (OuterError 1))
  cancelled <- Async.fork (Async.catch innerCanceler \_ -> Async.fail (OuterError 2))
  delay 20
  Async.kill (OuterError 7) mapped
  Async.kill (OuterError 8) cancelled
  joinedMapped <- Async.attempt (Async.join mapped)
  joinedCancelled <- Async.attempt (Async.join cancelled)
  Async.liftEffect (report (describeOuter joinedMapped <> ", " <> describeOuter joinedCancelled))
  _ <- Async.catch (children report) \error -> do
    Async.liftEffect (report ("caught " <> error))
    Async.fail (OuterError 99)
  pure unit

run :: Report -> Effect Unit
run report = void do
  Async.launch { onUncaught: \(OuterError code) -> report ("uncaught " <> show code) } {} (program report)
