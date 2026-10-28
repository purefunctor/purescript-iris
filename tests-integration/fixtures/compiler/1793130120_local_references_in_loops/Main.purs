module Main where

import Prelude

import Control.Monad.ST as ST
import Control.Monad.ST.Ref as STRef
import Effect (Effect, forE, foreachE, untilE, whileE)
import Effect.Ref as Ref

counted :: Array Int -> Effect Int
counted values = do
  total <- Ref.new 0
  forE 0 3 \index -> Ref.modify_ (_ + index) total
  foreachE values \value -> Ref.modify_ (_ + value) total
  whileE (map (_ < 20) (Ref.read total)) (Ref.modify_ (_ + 4) total)
  untilE do
    Ref.modify_ (_ + 1) total
    map (_ >= 25) (Ref.read total)
  Ref.read total

doubledPerIteration :: Effect Int
doubledPerIteration = do
  total <- Ref.new 0
  forE 1 4 \index -> do
    local <- Ref.new index
    Ref.modify_ (_ * 2) local
    value <- Ref.read local
    Ref.modify_ (_ + value) total
  Ref.read total

stateCountdown :: Int -> Array Int
stateCountdown start = ST.run do
  remaining <- STRef.new start
  seen <- STRef.new []
  ST.while (map (_ > 0) (STRef.read remaining)) do
    value <- STRef.read remaining
    _ <- STRef.modify (_ <> [ value ]) seen
    STRef.modify (_ - 1) remaining
  STRef.read seen
