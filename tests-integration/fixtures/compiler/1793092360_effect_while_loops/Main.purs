module Main where

import Prelude

import Control.Monad.ST as ST
import Control.Monad.ST.Ref as STRef
import Effect (Effect, untilE, whileE)
import Effect.Ref as Ref

foreign import hasNext :: Effect Boolean
foreign import next :: Effect Unit

countdown :: Int -> Effect (Array Int)
countdown start = do
  counter <- Ref.new start
  seen <- Ref.new []
  whileE (map (_ > 0) (Ref.read counter)) do
    current <- Ref.read counter
    Ref.modify_ (_ <> [ current ]) seen
    Ref.write (current - 1) counter
  Ref.read seen

countUntil :: Int -> Effect Int
countUntil limit = do
  counter <- Ref.new 0
  untilE do
    count <- Ref.modify (_ + 1) counter
    pure (count >= limit)
  Ref.read counter

drain :: Effect Unit
drain = whileE hasNext next

doubleUntil :: Int -> Int
doubleUntil limit = ST.run do
  value <- STRef.new 1
  ST.while (map (_ < limit) (STRef.read value)) (STRef.modify (_ * 2) value)
  STRef.read value
