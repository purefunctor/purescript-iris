module Main (run) where

import Prelude

import Data.Either (Either(..))
import Effect (Effect)
import Iris.Effect.Async (Async)
import Iris.Effect.Async as Async

type Report = String -> Effect Unit

newtype Code = Code Int

say :: forall error. Report -> String -> Async error () Unit
say report message = Async.liftEffect (report message)

describe :: Either String Int -> String
describe = case _ of
  Left error -> "left " <> error
  Right value -> "right " <> show value

program :: Report -> Async Code () Unit
program report = do
  recovered <- Async.catch (say report "before fail" *> Async.fail "boom" *> say report "after fail" *> pure 0) \error -> do
    say report ("caught " <> error)
    pure 7
  say report ("recovered " <> show recovered)
  mapped <- Async.attempt (Async.mapError (\error -> "mapped " <> error) (Async.fail "inner" :: Async String () Int))
  say report (describe mapped)
  rethrown <- Async.attempt (Async.catch (Async.fail "first") \error -> Async.fail ("handled " <> error))
  say report (describe rethrown)
  succeeded <- Async.attempt (pure 3)
  say report (describe succeeded)
  Async.mapError Code (Async.fail 42 :: Async Int () Unit)
  say report "after uncaught"

run :: Report -> Effect Unit
run report = void do
  Async.launch { onUncaught: \(Code code) -> report ("uncaught " <> show code) } {} (program report)
