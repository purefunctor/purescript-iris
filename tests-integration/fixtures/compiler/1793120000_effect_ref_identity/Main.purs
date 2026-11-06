module Main where

import Prelude

import Data.Array as Array
import Effect (Effect, forE)
import Effect.Ref (Ref)
import Effect.Ref as Ref

foreign import sameReference :: Ref Int -> Ref Int -> Boolean

foreign import incrementForeign :: Ref Int -> Effect Unit

fresh :: Effect (Ref Int)
fresh = Ref.new 0

pair :: Effect { same :: Boolean, left :: Int, right :: Int }
pair = do
  left <- Ref.new 0
  right <- Ref.new 0
  Ref.write 1 left
  leftValue <- Ref.read left
  rightValue <- Ref.read right
  pure { same: sameReference left right, left: leftValue, right: rightValue }

allocated :: Int -> Effect (Array (Ref Int))
allocated count = do
  references <- Ref.new []
  forE 0 count \index -> do
    reference <- Ref.new index
    Ref.modify_ (flip Array.snoc reference) references
  Ref.read references

throughCallback :: Ref Int -> (Ref Int -> Effect Unit) -> Effect Int
throughCallback reference callback = do
  callback reference
  Ref.read reference

incrementTwice :: Ref Int -> Effect Unit
incrementTwice reference = do
  Ref.modify_ (_ + 1) reference
  Ref.modify_ (_ + 1) reference

throughForeign :: Effect Int
throughForeign = do
  reference <- Ref.new 1
  incrementForeign reference
  Ref.modify (_ * 10) reference

shared :: Effect { first :: Int, second :: Int }
shared = do
  reference <- Ref.new 0
  let
    holder = { counter: reference }
  Ref.write 5 holder.counter
  first <- Ref.read reference
  _ <- Ref.modify (_ + 1) reference
  second <- Ref.read holder.counter
  pure { first, second }
