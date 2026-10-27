module Main where

import Prelude

import Control.Monad.ST (ST)
import Control.Monad.ST as ST
import Control.Monad.ST.Ref as STRef
import Effect (Effect)
import Effect.Ref as Ref
import Effect.Unsafe (unsafePerformEffect)

performed :: Int
performed = unsafePerformEffect do
  ref <- Ref.new 20
  Ref.modify (_ + 1) ref

doubled :: Int -> Int
doubled value = ST.run do
  ref <- STRef.new value
  STRef.modify (_ * 2) ref

offset :: Int -> Int
offset value = 1 + ST.run (pure value)

performedAction :: Effect Int -> Int
performedAction action = unsafePerformEffect action

runAction :: (forall region. ST region Int) -> Int
runAction action = ST.run action
