module Main where

import Prelude

import Effect (Effect, forE, foreachE, whileE)
import Effect.Ref as Ref

total :: Array Int -> Effect Int
total values = do
  sum <- Ref.new 0
  forE 0 3 \index ->
    Ref.modify_ (_ + index) sum
  foreachE values \value ->
    Ref.write value sum
  whileE (map (_ < 10) (Ref.read sum)) do
    Ref.modify_ (_ + 1) sum
  Ref.read sum
