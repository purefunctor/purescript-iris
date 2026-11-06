module Main where

import Prelude

import Effect (Effect, untilE, whileE)
import Effect.Ref as Ref

untilLimit :: { limit :: Int } -> Effect Int
untilLimit configuration = do
  counter <- Ref.new 0
  let
    limit = configuration.limit
  untilE do
    current <- Ref.modify (_ + 1) counter
    pure (current >= limit)
  Ref.read counter

whileLimit :: { limit :: Int } -> Effect Int
whileLimit configuration = do
  counter <- Ref.new 0
  let
    limit = configuration.limit
  whileE
    ( do
        current <- Ref.read counter
        pure (current < limit)
    )
    (Ref.modify_ (_ + 1) counter)
  Ref.read counter
