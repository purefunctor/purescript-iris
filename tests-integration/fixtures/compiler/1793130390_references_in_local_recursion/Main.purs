module Main where

import Prelude

import Effect (Effect)
import Effect.Ref as Ref

countdown :: Int -> Effect Int
countdown start = do
  steps <- Ref.new 0
  let
    go :: Int -> Effect Unit
    go remaining =
      if remaining > 0 then do
        Ref.modify_ (_ + remaining) steps
        go (remaining - 1)
      else
        pure unit
  go start
  go 2
  Ref.read steps
