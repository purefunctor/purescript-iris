module Main (run) where

import Prelude

import Data.Either (Either(..))
import Effect (Effect)
import Iris.Effect.Async (Async)
import Iris.Effect.Async as Async

type Report = String -> Effect Unit

describe :: Either String Int -> String
describe = case _ of
  Left error -> "failed " <> error
  Right value -> "succeeded " <> show value

guarded :: Async String () Unit -> Async String () Int -> Async String () (Either String Int)
guarded release body = Async.attempt (Async.bracket (pure unit) (\_ -> release) (\_ -> body))

program :: Report -> Async String () Unit
program report = do
  releaseFailed <- guarded (Async.fail "release") (pure 1)
  report' ("release fails: " <> describe releaseFailed)
  bodyFailed <- guarded (pure unit) (Async.fail "body")
  report' ("body fails: " <> describe bodyFailed)
  bothFailed <- guarded (Async.fail "release") (Async.fail "body")
  report' ("both fail: " <> describe bothFailed)
  neitherFailed <- guarded (pure unit) (pure 2)
  report' ("neither fails: " <> describe neitherFailed)
  where
  report' message = Async.liftEffect (report message)

run :: Report -> Effect Unit
run report = void do
  Async.launch { onUncaught: \error -> report ("uncaught " <> error) } {} (program report)
