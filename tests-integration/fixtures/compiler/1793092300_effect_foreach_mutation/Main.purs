module Main where

import Prelude

import Control.Monad.ST as ST
import Control.Monad.ST.Ref as STRef
import Effect (Effect, foreachE)
import Effect.Ref (Ref)
import Effect.Ref as Ref

foreign import push :: Array Int -> Int -> Effect Unit
foreign import poke :: Array Int -> Int -> Int -> Effect Unit

visit :: Array Int -> Effect (Array Int)
visit values = do
  seen <- Ref.new []
  foreachE values \value -> do
    when (value == 1) (poke values 2 99)
    push values value
    Ref.modify_ (_ <> [ value ]) seen
  Ref.read seen

record :: Ref (Array Int) -> Int -> Effect Unit
record seen value = Ref.modify_ (_ <> [ value ]) seen

visitWithFunction :: Array Int -> Effect (Array Int)
visitWithFunction values = do
  seen <- Ref.new []
  foreachE values (record seen)
  Ref.read seen

sumState :: Array Int -> Int
sumState values = ST.run do
  total <- STRef.new 0
  ST.foreach values \value -> void (STRef.modify (_ + value) total)
  STRef.read total
