module Main where

import Prelude

import Data.Traversable (sequence)
import Effect (Effect, forE)
import Effect.Ref as Ref

sharedCounter :: Effect (Array Int)
sharedCounter = do
  counter <- Ref.new 0
  actions <- Ref.new []
  forE 0 3 \_ -> Ref.modify_ (_ <> [ Ref.modify (_ + 1) counter ]) actions
  Ref.read actions >>= sequence

ownCounters :: Effect (Array Int)
ownCounters = do
  actions <- Ref.new []
  forE 0 3 \index -> do
    counter <- Ref.new (index * 10)
    Ref.modify_ (_ <> [ Ref.modify (_ + 1) counter ]) actions
  bumps <- Ref.read actions
  _ <- sequence bumps
  sequence bumps
