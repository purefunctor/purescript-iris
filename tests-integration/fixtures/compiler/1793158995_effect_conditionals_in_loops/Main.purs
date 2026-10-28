module Main where

import Prelude

import Control.Monad.ST as ST
import Control.Monad.ST.Ref as STRef
import Effect (Effect, forE)
import Effect.Ref as Ref

loopBody :: Int -> Effect Int
loopBody limit = do
  total <- Ref.new 0
  forE 0 limit \index -> do
    if index < 3 then Ref.modify_ (_ + index) total else pure unit
    Ref.modify_ (_ + 100) total
  Ref.read total

nestedConditionals :: Int -> Effect Int
nestedConditionals count = do
  ref <- Ref.new 0
  label <-
    if count > 0 then
      if count > 10 then do
        Ref.write 2 ref
        pure 20
      else do
        Ref.write 1 ref
        pure 10
    else
      pure 0
  Ref.modify_ (_ + label) ref
  Ref.read ref

tailConditional :: Int -> Int
tailConditional count = ST.run do
  ref <- STRef.new count
  if count > 0 then STRef.modify (_ * 2) ref else STRef.read ref
