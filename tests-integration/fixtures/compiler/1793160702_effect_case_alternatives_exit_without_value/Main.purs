module Main where

import Prelude

import Effect (Effect, foreachE, whileE)
import Effect.Ref as Ref

describe :: Ref.Ref Int -> Ref.Ref (Array String) -> Effect Unit
describe source log = do
  value <- Ref.read source
  case value of
    0 -> Ref.modify_ (_ <> [ "zero" ]) log
    1 -> Ref.modify_ (_ <> [ "one" ]) log
    _ -> Ref.modify_ (_ <> [ "many" ]) log

describeGuarded :: Ref.Ref Int -> Ref.Ref (Array String) -> Effect Unit
describeGuarded source log = do
  value <- Ref.read source
  case value of
    n
      | n > 10 -> Ref.modify_ (_ <> [ "big" ]) log
      | n > 0 -> Ref.modify_ (_ <> [ "small" ]) log
    _ -> Ref.modify_ (_ <> [ "none" ]) log

describeEach :: Array Int -> Effect (Array String)
describeEach values = do
  log <- Ref.new []
  foreachE values \value -> case value of
    0 -> Ref.modify_ (_ <> [ "zero" ]) log
    1 -> Ref.modify_ (_ <> [ "one" ]) log
    _ -> Ref.modify_ (_ <> [ "many" ]) log
  Ref.read log

describeCountdown :: Int -> Effect (Array String)
describeCountdown start = do
  counter <- Ref.new start
  log <- Ref.new []
  whileE (map (_ >= 0) (Ref.read counter)) do
    current <- Ref.read counter
    Ref.write (current - 1) counter
    case current of
      0 -> Ref.modify_ (_ <> [ "zero" ]) log
      1 -> Ref.modify_ (_ <> [ "one" ]) log
      _ -> Ref.modify_ (_ <> [ "many" ]) log
  Ref.read log
