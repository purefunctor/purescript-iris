module Main where

import Prelude

import Data.Array as Array
import Data.Either (Either(..))
import Effect (Effect, forE, foreachE, untilE, whileE)
import Effect.Exception (Error, catchException, message, throw, try)
import Effect.Ref (Ref)
import Effect.Ref as Ref

type Observed value = { outcome :: Either String value, visited :: Array String }

describe :: forall value. Either Error value -> Either String value
describe = case _ of
  Left error -> Left (message error)
  Right value -> Right value

observe :: forall value. (Ref (Array String) -> Effect value) -> Effect (Observed value)
observe body = do
  visited <- Ref.new []
  outcome <- try (body visited)
  seen <- Ref.read visited
  pure { outcome: describe outcome, visited: seen }

record :: Ref (Array String) -> String -> Effect Unit
record visited label = Ref.modify_ (flip Array.snoc label) visited

forAborts :: Effect (Observed Int)
forAborts = observe \visited -> do
  forE 0 5 \index -> do
    when (index == 3) (throw "stop at 3")
    record visited (show index)
  pure 99

untilAborts :: Effect (Observed Int)
untilAborts = observe \visited -> do
  counter <- Ref.new 0
  untilE do
    count <- Ref.modify (_ + 1) counter
    record visited (show count)
    when (count == 2) (throw "stop at 2")
    pure (count >= 5)
  Ref.read counter

whileAborts :: Effect (Observed Unit)
whileAborts = observe \visited -> do
  counter <- Ref.new 0
  whileE (map (_ < 5) (Ref.read counter)) do
    count <- Ref.modify (_ + 1) counter
    when (count == 3) (throw "stop at 3")
    record visited (show count)

foreachAborts :: Effect (Observed Unit)
foreachAborts = observe \visited -> foreachE [ "a", "b", "c", "d" ] \element -> do
  when (element == "c") (throw "stop at c")
  record visited element

nestedAborts :: Effect (Observed Unit)
nestedAborts = observe \visited -> forE 0 3 \row -> do
  record visited ("row " <> show row)
  forE 0 3 \column -> do
    when (row == 1 && column == 1) (throw "stop at 1:1")
    record visited (show row <> ":" <> show column)

recovers :: Effect (Observed Unit)
recovers = observe \visited -> forE 0 4 \index ->
  catchException (\error -> record visited ("caught " <> message error)) do
    when (index == 1) (throw "one")
    record visited (show index)
