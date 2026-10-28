module Main where

import Prelude

import Data.Traversable (sequence)
import Effect (Effect, forE, foreachE)
import Effect.Ref as Ref

snapshots :: Effect (Array (Unit -> Int))
snapshots = do
  counter <- Ref.new 0
  closures <- Ref.new []
  forE 0 3 \_ -> do
    Ref.modify_ (_ + 10) counter
    value <- Ref.read counter
    Ref.modify_ (_ <> [ \_ -> value ]) closures
  Ref.read closures

previous :: Array Int -> Effect (Array (Unit -> Int))
previous values = do
  latest <- Ref.new 0
  closures <- Ref.new []
  foreachE values \value -> do
    old <- Ref.modify' (\state -> { state: value, value: state }) latest
    Ref.modify_ (_ <> [ \_ -> old ]) closures
  Ref.read closures

chained :: Effect Int
chained = do
  ref <- Ref.new (\_ -> 0)
  forE 0 3 \_ -> Ref.modify_ (\function -> \_ -> function unit + 1) ref
  function <- Ref.read ref
  pure (function unit)

readers :: Effect (Array Int)
readers = do
  counter <- Ref.new 0
  actions <- Ref.new []
  forE 0 3 \index -> do
    Ref.write index counter
    Ref.modify_ (_ <> [ Ref.read counter ]) actions
  Ref.write 42 counter
  Ref.read actions >>= sequence
